use crate::app_state::AppState;
use crate::auth::{write_audit_log, AuthUser, InstanceRole};
use crate::entities::{metric_points, ts_instances};
use crate::error::{ApiResult, AppError};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use serde::{Deserialize, Serialize};
use tss_tsproto::{
    create_driver, EditServerInfoParams, ProtocolKind, ServerInfoSnapshot, TsConnectionConfig,
    VirtualServerSummary,
};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/instances", get(list_instances).post(create_instance))
        .route("/instances/test-connection", post(test_connection))
        .route(
            "/instances/{id}",
            get(get_instance).put(update_instance).delete(delete_instance),
        )
        .route("/instances/{id}/reconnect", post(reconnect_instance))
        .route(
            "/instances/{id}/servers",
            get(list_virtual_servers).post(create_virtual_server),
        )
        .route(
            "/instances/{id}/servers/{sid}/start",
            post(start_virtual_server),
        )
        .route(
            "/instances/{id}/servers/{sid}/stop",
            post(stop_virtual_server),
        )
        .route(
            "/instances/{id}/servers/{sid}",
            axum::routing::delete(delete_virtual_server),
        )
        .route(
            "/instances/{id}/servers/{sid}/info",
            get(get_server_info).put(edit_server_info),
        )
        .route("/instances/{id}/servers/{sid}/metrics", get(get_metrics))
}

#[derive(Serialize)]
pub struct InstanceView {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub host: String,
    pub port: i32,
    pub username: String,
    pub default_virtual_server_id: i32,
    pub tls_verify: bool,
    pub enabled: bool,
    pub status: String,
    pub last_error: Option<String>,
    pub my_role: String,
    pub created_at: String,
}

#[derive(Deserialize)]
struct CreateInstanceReq {
    name: String,
    protocol: String,
    host: String,
    port: u16,
    username: String,
    secret: String,
    #[serde(default = "default_sid")]
    default_virtual_server_id: i32,
    #[serde(default = "default_true")]
    tls_verify: bool,
    #[serde(default = "default_true")]
    enabled: bool,
}

#[derive(Deserialize)]
struct UpdateInstanceReq {
    name: Option<String>,
    protocol: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    username: Option<String>,
    secret: Option<String>,
    default_virtual_server_id: Option<i32>,
    tls_verify: Option<bool>,
    enabled: Option<bool>,
}

#[derive(Deserialize)]
struct TestConnReq {
    protocol: String,
    host: String,
    port: u16,
    username: String,
    secret: String,
    #[serde(default = "default_sid_u32")]
    virtual_server_id: u32,
    #[serde(default = "default_true")]
    tls_verify: bool,
}

#[derive(Deserialize)]
struct CreateVirtualServerReq {
    name: String,
    #[serde(default = "default_slots")]
    max_clients: u32,
    port: Option<u16>,
}

#[derive(Deserialize)]
struct MetricsQuery {
    #[serde(default = "default_range")]
    range: String,
}

#[derive(Serialize)]
struct MetricPointView {
    recorded_at: String,
    clients_online: i32,
    max_clients: i32,
    channels_online: i32,
    avg_ping_ms: f64,
    packet_loss_pct: f64,
    bw_in_bps: i64,
    bw_out_bps: i64,
}

fn default_sid() -> i32 {
    1
}
fn default_sid_u32() -> u32 {
    1
}
fn default_true() -> bool {
    true
}
fn default_slots() -> u32 {
    32
}
fn default_range() -> String {
    "1h".to_string()
}

async fn to_instance_view(
    state: &AppState,
    m: ts_instances::Model,
    role: InstanceRole,
) -> InstanceView {
    let live = state.hub.get_instance_status(&m.id).await;
    let (status, last_error) = match live {
        Some((s, e)) => (s, e),
        None => (m.status, m.last_error),
    };
    InstanceView {
        id: m.id,
        name: m.name,
        protocol: m.protocol,
        host: m.host,
        port: m.port,
        username: m.username,
        default_virtual_server_id: m.default_virtual_server_id,
        tls_verify: m.tls_verify,
        enabled: m.enabled,
        status,
        last_error,
        my_role: role.as_str().to_string(),
        created_at: m.created_at.to_rfc3339(),
    }
}

async fn list_instances(
    State(state): State<AppState>,
    auth: AuthUser,
) -> ApiResult<Json<Vec<InstanceView>>> {
    let all = ts_instances::Entity::find()
        .order_by_asc(ts_instances::Column::CreatedAt)
        .all(&state.db)
        .await?;

    let mut out = Vec::new();
    for m in all {
        if let Some(role) = auth.role_for_instance(&state.db, &m.id).await? {
            out.push(to_instance_view(&state, m, role).await);
        }
    }
    Ok(Json(out))
}

async fn create_instance(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateInstanceReq>,
) -> ApiResult<(StatusCode, Json<InstanceView>)> {
    auth.require_super_admin()?;
    let proto = ProtocolKind::parse(&req.protocol)
        .ok_or_else(|| AppError::Validation("protocol must be raw_tcp, ssh, or web_query".into()))?;
    if req.name.trim().is_empty() || req.host.trim().is_empty() {
        return Err(AppError::Validation("Instance name and host are required".into()));
    }

    let now = Utc::now().fixed_offset();
    let enc_secret = state.crypto.encrypt_secret(&req.secret)?;
    let model = ts_instances::ActiveModel {
        id: Set(Uuid::now_v7().to_string()),
        name: Set(req.name.trim().to_string()),
        protocol: Set(proto.as_str().to_string()),
        host: Set(req.host.trim().to_string()),
        port: Set(req.port as i32),
        username: Set(req.username.trim().to_string()),
        encrypted_secret: Set(enc_secret),
        default_virtual_server_id: Set(req.default_virtual_server_id.max(1)),
        tls_verify: Set(req.tls_verify),
        enabled: Set(req.enabled),
        status: Set(if req.enabled { "connecting" } else { "disabled" }.to_string()),
        last_error: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&state.db)
    .await?;

    if model.enabled {
        let _ = state.hub.start_instance(model.clone()).await;
    }

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&model.id),
        None,
        "instance.create",
        &model.name,
        serde_json::json!({ "protocol": model.protocol, "host": model.host, "port": model.port }),
        &auth.ip_addr,
    )
    .await;

    let view = to_instance_view(&state, model, InstanceRole::SuperAdmin).await;
    Ok((StatusCode::CREATED, Json(view)))
}

async fn test_connection(
    auth: AuthUser,
    Json(req): Json<TestConnReq>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_super_admin()?;
    let protocol = ProtocolKind::parse(&req.protocol)
        .ok_or_else(|| AppError::Validation("Invalid protocol".into()))?;
    let cfg = TsConnectionConfig {
        protocol,
        host: req.host.trim().to_string(),
        port: req.port,
        username: req.username.trim().to_string(),
        secret: req.secret,
        virtual_server_id: req.virtual_server_id.max(1),
        tls_verify: req.tls_verify,
    };
    let drv = create_driver(&cfg)?;
    let res = drv.test_connection().await;
    let servers = if res.is_ok() {
        drv.list_virtual_servers().await.unwrap_or_default()
    } else {
        Vec::new()
    };
    let _ = drv.shutdown().await;
    res?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "virtual_servers": servers
    })))
}

async fn get_instance(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<InstanceView>> {
    let role = auth
        .require_instance_role(&state.db, &id, InstanceRole::Auditor)
        .await?;
    let model = ts_instances::Entity::find_by_id(&id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Instance not found".into()))?;
    Ok(Json(to_instance_view(&state, model, role).await))
}

async fn update_instance(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateInstanceReq>,
) -> ApiResult<Json<InstanceView>> {
    auth.require_super_admin()?;
    let model = ts_instances::Entity::find_by_id(&id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Instance not found".into()))?;
    let mut active: ts_instances::ActiveModel = model.into();
    if let Some(v) = req.name {
        active.name = Set(v.trim().to_string());
    }
    if let Some(v) = req.protocol {
        let p = ProtocolKind::parse(&v)
            .ok_or_else(|| AppError::Validation("Invalid protocol".into()))?;
        active.protocol = Set(p.as_str().to_string());
    }
    if let Some(v) = req.host {
        active.host = Set(v.trim().to_string());
    }
    if let Some(v) = req.port {
        active.port = Set(v as i32);
    }
    if let Some(v) = req.username {
        active.username = Set(v.trim().to_string());
    }
    if let Some(v) = req.secret.filter(|s| !s.is_empty()) {
        active.encrypted_secret = Set(state.crypto.encrypt_secret(&v)?);
    }
    if let Some(v) = req.default_virtual_server_id {
        active.default_virtual_server_id = Set(v.max(1));
    }
    if let Some(v) = req.tls_verify {
        active.tls_verify = Set(v);
    }
    if let Some(v) = req.enabled {
        active.enabled = Set(v);
    }
    active.updated_at = Set(Utc::now().fixed_offset());
    let updated = active.update(&state.db).await?;

    if updated.enabled {
        let _ = state.hub.start_instance(updated.clone()).await;
    } else {
        state.hub.stop_and_remove_instance(&updated.id).await;
    }

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&updated.id),
        None,
        "instance.update",
        &updated.name,
        serde_json::json!({}),
        &auth.ip_addr,
    )
    .await;

    Ok(Json(
        to_instance_view(&state, updated, InstanceRole::SuperAdmin).await,
    ))
}

async fn delete_instance(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    auth.require_super_admin()?;
    let model = ts_instances::Entity::find_by_id(&id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Instance not found".into()))?;
    state.hub.stop_and_remove_instance(&id).await;
    ts_instances::Entity::delete_by_id(&id)
        .exec(&state.db)
        .await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        None,
        "instance.delete",
        &model.name,
        serde_json::json!({}),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn reconnect_instance(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<InstanceView>> {
    let role = auth
        .require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    let model = ts_instances::Entity::find_by_id(&id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Instance not found".into()))?;
    state.hub.start_instance(model.clone()).await?;
    Ok(Json(to_instance_view(&state, model, role).await))
}

async fn list_virtual_servers(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<VirtualServerSummary>>> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Auditor)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    Ok(Json(drv.list_virtual_servers().await?))
}

async fn create_virtual_server(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<CreateVirtualServerReq>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    auth.require_super_admin()?;
    let drv = state.hub.driver_for(&id).await?;
    let sid = drv
        .create_virtual_server(req.name.trim(), req.max_clients, req.port)
        .await?;
    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "virtual_server.create",
        req.name.trim(),
        serde_json::json!({ "max_clients": req.max_clients, "port": req.port }),
        &auth.ip_addr,
    )
    .await;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "sid": sid }))))
}

async fn start_virtual_server(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    drv.start_virtual_server(sid).await?;
    let _ = state.hub.refresh_virtual_server(&id, sid, false).await;
    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "virtual_server.start",
        &format!("sid:{sid}"),
        serde_json::json!({}),
        &auth.ip_addr,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn stop_virtual_server(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    drv.stop_virtual_server(sid).await?;
    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "virtual_server.stop",
        &format!("sid:{sid}"),
        serde_json::json!({}),
        &auth.ip_addr,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_virtual_server(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
) -> ApiResult<StatusCode> {
    auth.require_super_admin()?;
    let drv = state.hub.driver_for(&id).await?;
    drv.delete_virtual_server(sid).await?;
    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "virtual_server.delete",
        &format!("sid:{sid}"),
        serde_json::json!({}),
        &auth.ip_addr,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_server_info(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
) -> ApiResult<Json<ServerInfoSnapshot>> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Auditor)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    Ok(Json(drv.get_server_info(sid).await?))
}

async fn edit_server_info(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
    Json(req): Json<EditServerInfoParams>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    drv.edit_server_info(sid, &req).await?;
    let _ = state.hub.refresh_virtual_server(&id, sid, false).await;
    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "virtual_server.edit",
        &format!("sid:{sid}"),
        serde_json::to_value(&req).unwrap_or_default(),
        &auth.ip_addr,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_metrics(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
    Query(q): Query<MetricsQuery>,
) -> ApiResult<Json<Vec<MetricPointView>>> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Auditor)
        .await?;

    let hours = match q.range.as_str() {
        "1h" => 1,
        "24h" => 24,
        "7d" => 24 * 7,
        "30d" => 24 * 30,
        _ => 1,
    };
    let since = Utc::now().fixed_offset() - Duration::hours(hours);

    let rows = metric_points::Entity::find()
        .filter(metric_points::Column::InstanceId.eq(&id))
        .filter(metric_points::Column::VirtualServerId.eq(sid as i32))
        .filter(metric_points::Column::RecordedAt.gte(since))
        .order_by_asc(metric_points::Column::RecordedAt)
        .all(&state.db)
        .await?;

    // Downsample uniformly if more than 240 points
    let max_points = 240usize;
    let total = rows.len();
    let step = if total > max_points {
        total / max_points
    } else {
        1
    };

    let out = rows
        .into_iter()
        .step_by(step.max(1))
        .map(|r| MetricPointView {
            recorded_at: r.recorded_at.to_rfc3339(),
            clients_online: r.clients_online,
            max_clients: r.max_clients,
            channels_online: r.channels_online,
            avg_ping_ms: r.avg_ping_ms,
            packet_loss_pct: r.packet_loss_pct,
            bw_in_bps: r.bw_in_bps,
            bw_out_bps: r.bw_out_bps,
        })
        .collect();

    Ok(Json(out))
}
