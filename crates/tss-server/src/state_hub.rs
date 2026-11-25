use crate::crypto::CryptoContext;
use crate::entities::{metric_points, ts_instances};
use crate::error::{ApiResult, AppError};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tokio::sync::{broadcast, watch, RwLock};
use tracing::warn;
use tss_tsproto::{
    create_driver, ChannelNode, OnlineClient, ProtocolKind, ServerInfoSnapshot, TsConnectionConfig,
    TsNotifyEvent, TsProtocolDriver,
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct LiveVirtualServerState {
    pub server_info: Option<ServerInfoSnapshot>,
    pub channels: Vec<ChannelNode>,
    pub clients: Vec<OnlineClient>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WsEnvelope {
    pub event: String,
    pub timestamp: String,
    pub instance_id: String,
    pub virtual_server_id: u32,
    pub payload: serde_json::Value,
}

struct InstanceRuntime {
    driver: Arc<dyn TsProtocolDriver>,
    default_sid: u32,
    status: String,
    last_error: Option<String>,
    live_servers: BTreeMap<u32, LiveVirtualServerState>,
    shutdown_tx: watch::Sender<bool>,
}

pub struct StateHub {
    db: DatabaseConnection,
    crypto: Arc<CryptoContext>,
    runtimes: Arc<RwLock<HashMap<String, InstanceRuntime>>>,
    ws_tx: broadcast::Sender<WsEnvelope>,
}

impl StateHub {
    pub fn new(db: DatabaseConnection, crypto: Arc<CryptoContext>) -> Arc<Self> {
        let (ws_tx, _) = broadcast::channel(512);
        Arc::new(Self {
            db,
            crypto,
            runtimes: Arc::new(RwLock::new(HashMap::new())),
            ws_tx,
        })
    }

    pub fn subscribe_ws(&self) -> broadcast::Receiver<WsEnvelope> {
        self.ws_tx.subscribe()
    }

    pub async fn bootstrap_all(self: &Arc<Self>) -> ApiResult<()> {
        let instances = ts_instances::Entity::find()
            .filter(ts_instances::Column::Enabled.eq(true))
            .all(&self.db)
            .await?;

        for inst in instances {
            if let Err(e) = self.start_instance(inst.clone()).await {
                warn!(
                    "Failed to attach runtime for {}: {:?}",
                    inst.name,
                    e.status_and_meta()
                );
            }
        }
        Ok(())
    }

    pub async fn start_instance(self: &Arc<Self>, inst: ts_instances::Model) -> ApiResult<()> {
        self.stop_instance_runtime_only(&inst.id).await;

        if !inst.enabled {
            return Ok(());
        }

        let protocol = ProtocolKind::parse(&inst.protocol)
            .ok_or_else(|| AppError::Validation(format!("Unknown protocol '{}'", inst.protocol)))?;
        let secret = self.crypto.decrypt_secret(&inst.encrypted_secret)?;
        let cfg = TsConnectionConfig {
            protocol,
            host: inst.host.clone(),
            port: inst.port as u16,
            username: inst.username.clone(),
            secret,
            virtual_server_id: inst.default_virtual_server_id.max(1) as u32,
            tls_verify: inst.tls_verify,
        };
        let driver = create_driver(&cfg)?;
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        {
            let mut map = self.runtimes.write().await;
            map.insert(
                inst.id.clone(),
                InstanceRuntime {
                    driver: driver.clone(),
                    default_sid: cfg.virtual_server_id,
                    status: "connecting".to_string(),
                    last_error: None,
                    live_servers: BTreeMap::new(),
                    shutdown_tx,
                },
            );
        }

        let _ = self
            .refresh_virtual_server(&inst.id, cfg.virtual_server_id, true)
            .await;

        let hub = self.clone();
        let instance_id = inst.id.clone();
        let default_sid = cfg.virtual_server_id;
        tokio::spawn(async move {
            hub.run_supervisor(instance_id, default_sid, driver, shutdown_rx)
                .await;
        });

        Ok(())
    }

    async fn stop_instance_runtime_only(&self, instance_id: &str) {
        let removed = self.runtimes.write().await.remove(instance_id);
        if let Some(rt) = removed {
            let _ = rt.shutdown_tx.send(true);
            let _ = rt.driver.shutdown().await;
        }
    }

    pub async fn stop_and_remove_instance(&self, instance_id: &str) {
        self.stop_instance_runtime_only(instance_id).await;
    }

    pub async fn driver_for(&self, instance_id: &str) -> ApiResult<Arc<dyn TsProtocolDriver>> {
        let guard = self.runtimes.read().await;
        guard
            .get(instance_id)
            .map(|rt| rt.driver.clone())
            .ok_or_else(|| {
                AppError::NotFound("Instance runtime is not active or does not exist".to_string())
            })
    }

    pub async fn get_live_state(
        &self,
        instance_id: &str,
        sid: u32,
    ) -> Option<LiveVirtualServerState> {
        let guard = self.runtimes.read().await;
        guard
            .get(instance_id)
            .and_then(|rt| rt.live_servers.get(&sid).cloned())
    }

    pub async fn get_instance_status(&self, instance_id: &str) -> Option<(String, Option<String>)> {
        let guard = self.runtimes.read().await;
        guard
            .get(instance_id)
            .map(|rt| (rt.status.clone(), rt.last_error.clone()))
    }

    pub async fn all_active_instance_sids(&self) -> Vec<(String, u32, String)> {
        let guard = self.runtimes.read().await;
        guard
            .iter()
            .map(|(id, rt)| (id.clone(), rt.default_sid, rt.status.clone()))
            .collect()
    }

    pub async fn refresh_virtual_server(
        &self,
        instance_id: &str,
        sid: u32,
        record_metric: bool,
    ) -> ApiResult<LiveVirtualServerState> {
        let driver = self.driver_for(instance_id).await?;
        let info_res = driver.get_server_info(sid).await;
        let channels_res = driver.list_channels(sid).await;
        let clients_res = driver.list_clients(sid).await;

        match (info_res, channels_res, clients_res) {
            (Ok(info), Ok(channels), Ok(clients)) => {
                let voice_count = clients.iter().filter(|c| c.client_type == 0).count() as i32;
                let now_str = Utc::now().to_rfc3339();
                let live = LiveVirtualServerState {
                    server_info: Some(info.clone()),
                    channels: channels.clone(),
                    clients: clients.clone(),
                    updated_at: now_str.clone(),
                };
                self.set_instance_status(instance_id, "online", None).await;
                {
                    let mut guard = self.runtimes.write().await;
                    if let Some(rt) = guard.get_mut(instance_id) {
                        rt.live_servers.insert(sid, live.clone());
                    }
                }
                if record_metric {
                    let pt = metric_points::ActiveModel {
                        id: Set(Uuid::now_v7().to_string()),
                        instance_id: Set(instance_id.to_string()),
                        virtual_server_id: Set(sid as i32),
                        clients_online: Set(voice_count),
                        max_clients: Set(info.max_clients as i32),
                        channels_online: Set(channels.len() as i32),
                        avg_ping_ms: Set(info.total_ping_ms),
                        packet_loss_pct: Set(info.total_packetloss_pct),
                        bw_in_bps: Set(info.bw_in_bps as i64),
                        bw_out_bps: Set(info.bw_out_bps as i64),
                        recorded_at: Set(Utc::now().fixed_offset()),
                    };
                    if let Err(e) = pt.insert(&self.db).await {
                        warn!("Failed to record metric_point: {e}");
                    }
                }
                let _ = self.ws_tx.send(WsEnvelope {
                    event: "state.delta".to_string(),
                    timestamp: now_str,
                    instance_id: instance_id.to_string(),
                    virtual_server_id: sid,
                    payload: serde_json::json!({
                        "kind": "sync",
                        "server_info": info,
                        "channels": channels,
                        "clients": clients,
                        "status": "online"
                    }),
                });
                Ok(live)
            }
            (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => {
                let msg = e.to_string();
                self.set_instance_status(instance_id, "offline", Some(msg.clone()))
                    .await;
                let _ = self.ws_tx.send(WsEnvelope {
                    event: "instance.status".to_string(),
                    timestamp: Utc::now().to_rfc3339(),
                    instance_id: instance_id.to_string(),
                    virtual_server_id: sid,
                    payload: serde_json::json!({
                        "status": "offline",
                        "error": msg
                    }),
                });
                Err(e.into())
            }
        }
    }

    async fn set_instance_status(&self, instance_id: &str, status: &str, err: Option<String>) {
        let mut changed = false;
        {
            let mut guard = self.runtimes.write().await;
            if let Some(rt) = guard.get_mut(instance_id) {
                if rt.status != status || rt.last_error != err {
                    rt.status = status.to_string();
                    rt.last_error = err.clone();
                    changed = true;
                }
            }
        }
        if changed {
            if let Ok(Some(model)) = ts_instances::Entity::find_by_id(instance_id)
                .one(&self.db)
                .await
            {
                let mut active: ts_instances::ActiveModel = model.into();
                active.status = Set(status.to_string());
                active.last_error = Set(err);
                active.updated_at = Set(Utc::now().fixed_offset());
                let _ = active.update(&self.db).await;
            }
        }
    }

    async fn run_supervisor(
        self: Arc<Self>,
        instance_id: String,
        default_sid: u32,
        driver: Arc<dyn TsProtocolDriver>,
        mut shutdown_rx: watch::Receiver<bool>,
    ) {
        let mut events = driver.subscribe_events();
        let poll_secs = if driver.supports_push_events() { 15 } else { 3 };
        let mut poll_tick = tokio::time::interval(std::time::Duration::from_secs(poll_secs));
        let mut metric_tick = tokio::time::interval(std::time::Duration::from_secs(30));
        poll_tick.tick().await;
        metric_tick.tick().await;

        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        break;
                    }
                }
                evt = events.recv() => {
                    match evt {
                        Ok(TsNotifyEvent::TextMessage { .. }) => {}
                        Ok(_) => {
                            let _ = self.refresh_virtual_server(&instance_id, default_sid, false).await;
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            let _ = self.refresh_virtual_server(&instance_id, default_sid, false).await;
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                _ = poll_tick.tick() => {
                    let _ = self.refresh_virtual_server(&instance_id, default_sid, false).await;
                }
                _ = metric_tick.tick() => {
                    let _ = self.refresh_virtual_server(&instance_id, default_sid, true).await;
                }
            }
        }
    }
}
