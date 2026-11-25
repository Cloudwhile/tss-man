use crate::app_state::AppState;
use crate::auth::write_audit_log;
use crate::entities::{automation_rules, backup_policies, snapshots, ts_instances, webhook_endpoints};
use crate::error::{ApiResult, AppError};
use chrono::{DateTime, FixedOffset, Utc};
use cron::Schedule;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::warn;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotSummary {
    pub server_name: String,
    pub max_clients: u32,
    pub channel_count: usize,
    pub group_count: usize,
    pub channel_names: Vec<String>,
    pub group_names: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AfkMoverConfig {
    target_channel_id: u32,
    max_idle_secs: u64,
    #[serde(default)]
    excluded_group_ids: Vec<u32>,
    #[serde(default)]
    only_when_muted_or_away: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct TimedBroadcastConfig {
    cron_expr: String,
    message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct PacketLossAlertConfig {
    threshold_pct: f64,
    #[serde(default = "default_cooldown")]
    cooldown_secs: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct OfflineAlertConfig {
    #[serde(default = "default_cooldown")]
    cooldown_secs: i64,
}

fn default_cooldown() -> i64 {
    300
}

pub fn normalize_cron(expr: &str) -> String {
    let trimmed = expr.trim();
    if trimmed.split_whitespace().count() == 5 {
        format!("0 {trimmed}")
    } else {
        trimmed.to_string()
    }
}

pub fn validate_cron(expr: &str) -> ApiResult<Schedule> {
    let norm = normalize_cron(expr);
    Schedule::from_str(&norm)
        .map_err(|e| AppError::Validation(format!("Invalid cron expression '{expr}': {e}")))
}

fn is_cron_due(expr: &str, last_run: Option<DateTime<FixedOffset>>, created_at: DateTime<FixedOffset>) -> bool {
    let Ok(schedule) = Schedule::from_str(&normalize_cron(expr)) else {
        return false;
    };
    let anchor = last_run.unwrap_or(created_at).with_timezone(&Utc);
    let now = Utc::now();
    schedule
        .after(&anchor)
        .next()
        .map(|next| next <= now)
        .unwrap_or(false)
}

pub async fn capture_snapshot_record(
    state: &AppState,
    instance_id: &str,
    sid: u32,
    name: &str,
    trigger_kind: &str,
    created_by: Option<&str>,
) -> ApiResult<snapshots::Model> {
    let driver = state.hub.driver_for(instance_id).await?;
    let snap = driver.create_snapshot(sid).await?;
    let info = driver.get_server_info(sid).await?;
    let channels = driver.list_channels(sid).await?;
    let groups = driver.list_server_groups(sid).await?;

    let summary = SnapshotSummary {
        server_name: info.name,
        max_clients: info.max_clients,
        channel_count: channels.len(),
        group_count: groups.len(),
        channel_names: channels.into_iter().map(|c| c.channel_name).collect(),
        group_names: groups.into_iter().map(|g| g.name).collect(),
    };
    let summary_json = serde_json::to_string(&summary)
        .map_err(|e| AppError::Internal(format!("Serialize snapshot summary failed: {e}")))?;
    let raw_payload = serde_json::to_string(&snap)
        .map_err(|e| AppError::Internal(format!("Serialize snapshot payload failed: {e}")))?;
    let size_bytes = raw_payload.len() as i64;
    let encrypted_payload = state.crypto.encrypt_secret(&raw_payload)?;

    let model = snapshots::ActiveModel {
        id: Set(Uuid::now_v7().to_string()),
        instance_id: Set(instance_id.to_string()),
        virtual_server_id: Set(sid as i32),
        name: Set(name.chars().take(128).collect()),
        trigger_kind: Set(trigger_kind.to_string()),
        encrypted_payload: Set(encrypted_payload),
        summary_json: Set(summary_json),
        size_bytes: Set(size_bytes),
        created_by: Set(created_by.map(str::to_string)),
        created_at: Set(Utc::now().fixed_offset()),
    }
    .insert(&state.db)
    .await?;

    Ok(model)
}

pub async fn dispatch_webhook_event(
    state: &AppState,
    event_name: &str,
    instance_id: &str,
    instance_name: &str,
    sid: u32,
    severity: &str,
    details: serde_json::Value,
) {
    let Ok(endpoints) = webhook_endpoints::Entity::find()
        .filter(webhook_endpoints::Column::Enabled.eq(true))
        .all(&state.db)
        .await
    else {
        return;
    };

    let timestamp = Utc::now().to_rfc3339();
    let payload = serde_json::json!({
        "event": event_name,
        "timestamp": timestamp,
        "instance_id": instance_id,
        "instance_name": instance_name,
        "virtual_server_id": sid,
        "severity": severity,
        "details": details,
    });
    let raw_body = payload.to_string();

    for ep in endpoints {
        let subscribed = ep.events_csv.trim() == "*"
            || ep
                .events_csv
                .split(',')
                .any(|s| s.trim().eq_ignore_ascii_case(event_name));
        if !subscribed {
            continue;
        }
        let status_label = deliver_single_webhook(state, &ep, event_name, &timestamp, &raw_body).await;
        let mut active: webhook_endpoints::ActiveModel = ep.into();
        active.last_delivery_status = Set(Some(status_label));
        let _ = active.update(&state.db).await;
    }
}

pub async fn deliver_single_webhook(
    state: &AppState,
    ep: &webhook_endpoints::Model,
    event_name: &str,
    timestamp: &str,
    raw_body: &str,
) -> String {
    let mut req = state
        .http_client
        .post(&ep.url)
        .header("Content-Type", "application/json")
        .header("X-TSS-Event", event_name)
        .header("X-TSS-Timestamp", timestamp);

    if let Some(enc) = ep.encrypted_signing_secret.as_deref() {
        if let Ok(secret) = state.crypto.decrypt_secret(enc) {
            let sig = state.crypto.hmac_sha256_hex(&secret, raw_body.as_bytes());
            req = req.header("X-TSS-Signature-256", format!("sha256={sig}"));
        }
    }

    match req
        .body(raw_body.to_string())
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await
    {
        Ok(resp) => format!("HTTP {}", resp.status().as_u16()),
        Err(e) => format!("ERR: {e}"),
    }
}

pub async fn execute_automation_rule(
    state: &AppState,
    rule: &automation_rules::Model,
    manual_trigger: bool,
) -> ApiResult<serde_json::Value> {
    let sid = rule.virtual_server_id.max(1) as u32;
    let inst_name = ts_instances::Entity::find_by_id(&rule.instance_id)
        .one(&state.db)
        .await?
        .map(|m| m.name)
        .unwrap_or_else(|| rule.instance_id.clone());

    match rule.rule_kind.as_str() {
        "afk_mover" => {
            let cfg: AfkMoverConfig = serde_json::from_str(&rule.config_json)
                .map_err(|e| AppError::Validation(format!("Invalid afk_mover config: {e}")))?;
            let driver = state.hub.driver_for(&rule.instance_id).await?;
            let clients = driver.list_clients(sid).await?;
            let mut moved = Vec::new();
            let threshold_ms = cfg.max_idle_secs * 1000;
            for cl in clients {
                if cl.client_type != 0 || cl.cid == cfg.target_channel_id {
                    continue;
                }
                if cl
                    .server_groups
                    .iter()
                    .any(|g| cfg.excluded_group_ids.contains(g))
                {
                    continue;
                }
                if cfg.only_when_muted_or_away && !cl.away && !cl.output_muted {
                    continue;
                }
                if cl.idle_time_ms >= threshold_ms {
                    driver
                        .move_client(sid, cl.clid, cfg.target_channel_id, None)
                        .await?;
                    moved.push(cl.nickname);
                }
            }
            if !moved.is_empty() || manual_trigger {
                let mut active: automation_rules::ActiveModel = rule.clone().into();
                active.last_triggered_at = Set(Some(Utc::now().fixed_offset()));
                let _ = active.update(&state.db).await;
                if !moved.is_empty() {
                    let _ = state.hub.refresh_virtual_server(&rule.instance_id, sid, false).await;
                    write_audit_log(
                        &state.db,
                        None,
                        "automation-bot",
                        Some(&rule.instance_id),
                        Some(sid as i32),
                        "automation.afk_mover",
                        &rule.name,
                        serde_json::json!({ "moved_clients": moved }),
                        "scheduler",
                    )
                    .await;
                }
            }
            Ok(serde_json::json!({ "rule_kind": "afk_mover", "moved_count": moved.len(), "moved_clients": moved }))
        }
        "timed_broadcast" => {
            let cfg: TimedBroadcastConfig = serde_json::from_str(&rule.config_json)
                .map_err(|e| AppError::Validation(format!("Invalid timed_broadcast config: {e}")))?;
            let driver = state.hub.driver_for(&rule.instance_id).await?;
            driver.send_text_message(sid, 3, sid, &cfg.message).await?;
            let mut active: automation_rules::ActiveModel = rule.clone().into();
            active.last_triggered_at = Set(Some(Utc::now().fixed_offset()));
            let _ = active.update(&state.db).await;
            write_audit_log(
                &state.db,
                None,
                "automation-bot",
                Some(&rule.instance_id),
                Some(sid as i32),
                "automation.timed_broadcast",
                &rule.name,
                serde_json::json!({ "message": cfg.message }),
                "scheduler",
            )
            .await;
            Ok(serde_json::json!({ "rule_kind": "timed_broadcast", "sent": true }))
        }
        "alert_offline" => {
            let cfg: OfflineAlertConfig = serde_json::from_str(&rule.config_json)
                .unwrap_or(OfflineAlertConfig { cooldown_secs: 300 });
            let status = state
                .hub
                .get_instance_status(&rule.instance_id)
                .await
                .map(|(s, _)| s)
                .unwrap_or_else(|| "offline".to_string());
            let in_cooldown = !manual_trigger
                && rule
                    .last_triggered_at
                    .map(|t| (Utc::now().fixed_offset() - t).num_seconds() < cfg.cooldown_secs)
                    .unwrap_or(false);
            if (status == "offline" || manual_trigger) && !in_cooldown {
                dispatch_webhook_event(
                    state,
                    "alert.instance_offline",
                    &rule.instance_id,
                    &inst_name,
                    sid,
                    "critical",
                    serde_json::json!({ "status": status, "rule": rule.name }),
                )
                .await;
                let mut active: automation_rules::ActiveModel = rule.clone().into();
                active.last_triggered_at = Set(Some(Utc::now().fixed_offset()));
                let _ = active.update(&state.db).await;
                return Ok(serde_json::json!({ "rule_kind": "alert_offline", "dispatched": true }));
            }
            Ok(serde_json::json!({ "rule_kind": "alert_offline", "dispatched": false }))
        }
        "alert_packet_loss" => {
            let cfg: PacketLossAlertConfig = serde_json::from_str(&rule.config_json)
                .map_err(|e| AppError::Validation(format!("Invalid alert_packet_loss config: {e}")))?;
            let loss = state
                .hub
                .get_live_state(&rule.instance_id, sid)
                .await
                .and_then(|l| l.server_info)
                .map(|i| i.total_packetloss_pct)
                .unwrap_or(0.0);
            let in_cooldown = !manual_trigger
                && rule
                    .last_triggered_at
                    .map(|t| (Utc::now().fixed_offset() - t).num_seconds() < cfg.cooldown_secs)
                    .unwrap_or(false);
            if (loss >= cfg.threshold_pct || manual_trigger) && !in_cooldown {
                dispatch_webhook_event(
                    state,
                    "alert.high_packet_loss",
                    &rule.instance_id,
                    &inst_name,
                    sid,
                    "warning",
                    serde_json::json!({
                        "packet_loss_pct": loss,
                        "threshold_pct": cfg.threshold_pct,
                        "rule": rule.name
                    }),
                )
                .await;
                let mut active: automation_rules::ActiveModel = rule.clone().into();
                active.last_triggered_at = Set(Some(Utc::now().fixed_offset()));
                let _ = active.update(&state.db).await;
                return Ok(serde_json::json!({ "rule_kind": "alert_packet_loss", "dispatched": true, "packet_loss_pct": loss }));
            }
            Ok(serde_json::json!({ "rule_kind": "alert_packet_loss", "dispatched": false, "packet_loss_pct": loss }))
        }
        other => Err(AppError::Validation(format!("Unknown rule_kind '{other}'"))),
    }
}

pub fn start_scheduler(state: AppState) {
    let prev_statuses = Arc::new(Mutex::new(HashMap::<String, String>::new()));
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(15));
        tick.tick().await;
        loop {
            tick.tick().await;
            if let Err(e) = run_scheduler_pass(&state, &prev_statuses).await {
                warn!("Scheduler pass error: {:?}", e.status_and_meta());
            }
        }
    });
}

async fn run_scheduler_pass(
    state: &AppState,
    prev_statuses: &Arc<Mutex<HashMap<String, String>>>,
) -> ApiResult<()> {
    // 1. Evaluate backup policies
    let policies = backup_policies::Entity::find()
        .filter(backup_policies::Column::Enabled.eq(true))
        .all(&state.db)
        .await?;

    for pol in policies {
        if !is_cron_due(&pol.cron_expr, pol.last_run_at, pol.created_at) {
            continue;
        }
        let snap_name = format!("Scheduled {}", Utc::now().format("%Y-%m-%d %H:%M UTC"));
        let status = match capture_snapshot_record(
            state,
            &pol.instance_id,
            pol.virtual_server_id.max(1) as u32,
            &snap_name,
            "scheduled",
            None,
        )
        .await
        {
            Ok(_) => {
                // Prune old scheduled snapshots beyond retention_count
                let all_sched = snapshots::Entity::find()
                    .filter(snapshots::Column::InstanceId.eq(&pol.instance_id))
                    .filter(snapshots::Column::VirtualServerId.eq(pol.virtual_server_id))
                    .filter(snapshots::Column::TriggerKind.eq("scheduled"))
                    .order_by_desc(snapshots::Column::CreatedAt)
                    .all(&state.db)
                    .await?;
                let keep = pol.retention_count.max(1) as usize;
                for old in all_sched.into_iter().skip(keep) {
                    let _ = snapshots::Entity::delete_by_id(old.id).exec(&state.db).await;
                }
                "ok".to_string()
            }
            Err(e) => format!("failed: {}", e.status_and_meta().3),
        };
        let mut active: backup_policies::ActiveModel = pol.into();
        active.last_run_at = Set(Some(Utc::now().fixed_offset()));
        active.last_status = Set(Some(status));
        let _ = active.update(&state.db).await;
    }

    // 2. Detect status transitions & run automation rules
    let active_list = state.hub.all_active_instance_sids().await;
    {
        let mut prev = prev_statuses.lock().await;
        for (id, _sid, cur) in &active_list {
            prev.entry(id.clone()).or_insert_with(|| cur.clone());
        }
    }

    let rules = automation_rules::Entity::find()
        .filter(automation_rules::Column::Enabled.eq(true))
        .all(&state.db)
        .await?;

    for rule in rules {
        match rule.rule_kind.as_str() {
            "timed_broadcast" => {
                if let Ok(cfg) = serde_json::from_str::<TimedBroadcastConfig>(&rule.config_json) {
                    if is_cron_due(&cfg.cron_expr, rule.last_triggered_at, rule.created_at) {
                        let _ = execute_automation_rule(state, &rule, false).await;
                    }
                }
            }
            _ => {
                let _ = execute_automation_rule(state, &rule, false).await;
            }
        }
    }

    Ok(())
}
