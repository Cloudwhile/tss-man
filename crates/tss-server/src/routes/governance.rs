use crate::app_state::AppState;
use crate::auth::{write_audit_log, AuthUser, InstanceRole};
use crate::entities::audit_logs;
use crate::error::ApiResult;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serde::{Deserialize, Serialize};
use tss_tsproto::{AddBanParams, BanRule, ComplaintEntry, DbClient, TsLogEntry};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/instances/{id}/servers/{sid}/bans",
            get(list_bans).post(add_ban),
        )
        .route(
            "/instances/{id}/servers/{sid}/bans/{banid}",
            delete(delete_ban),
        )
        .route(
            "/instances/{id}/servers/{sid}/db-clients",
            get(list_db_clients),
        )
        .route(
            "/instances/{id}/servers/{sid}/db-clients/{cldbid}",
            delete(delete_db_client),
        )
        .route(
            "/instances/{id}/servers/{sid}/complaints",
            get(list_complaints),
        )
        .route(
            "/instances/{id}/servers/{sid}/complaints/{tcldbid}/{fcldbid}",
            delete(delete_complaint),
        )
        .route("/instances/{id}/servers/{sid}/ts-logs", get(list_ts_logs))
        .route("/audit-logs", get(list_audit_logs))
}

#[derive(Deserialize)]
struct DbClientsQuery {
    #[serde(default)]
    offset: u32,
    #[serde(default = "default_limit")]
    limit: u32,
    q: Option<String>,
}

#[derive(Deserialize)]
struct TsLogsQuery {
    #[serde(default = "default_lines")]
    lines: u32,
}

#[derive(Deserialize)]
struct AuditQuery {
    #[serde(default = "default_audit_limit")]
    limit: u64,
    instance_id: Option<String>,
}

#[derive(Serialize)]
struct AuditLogView {
    id: String,
    actor_username: String,
    instance_id: Option<String>,
    virtual_server_id: Option<i32>,
    action: String,
    target: String,
    detail_json: String,
    ip_addr: String,
    created_at: String,
}

fn default_limit() -> u32 {
    50
}
fn default_lines() -> u32 {
    60
}
fn default_audit_limit() -> u64 {
    100
}

async fn list_bans(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
) -> ApiResult<Json<Vec<BanRule>>> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Auditor)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    Ok(Json(drv.list_bans(sid).await?))
}

async fn add_ban(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
    Json(req): Json<AddBanParams>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Moderator)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    let banid = drv.add_ban(sid, &req).await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "ban.create",
        &format!("banid:{banid}"),
        serde_json::to_value(&req).unwrap_or_default(),
        &auth.ip_addr,
    )
    .await;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "banid": banid }))))
}

async fn delete_ban(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, banid)): Path<(String, u32, u32)>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Moderator)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    drv.delete_ban(sid, banid).await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "ban.delete",
        &format!("banid:{banid}"),
        serde_json::json!({}),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn list_db_clients(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
    Query(q): Query<DbClientsQuery>,
) -> ApiResult<Json<Vec<DbClient>>> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Auditor)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    Ok(Json(
        drv.list_db_clients(sid, q.offset, q.limit.clamp(1, 200), q.q.as_deref())
            .await?,
    ))
}

async fn delete_db_client(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, cldbid)): Path<(String, u32, u32)>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    drv.delete_db_client(sid, cldbid).await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "db_client.delete",
        &format!("cldbid:{cldbid}"),
        serde_json::json!({}),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn list_complaints(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
) -> ApiResult<Json<Vec<ComplaintEntry>>> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Auditor)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    Ok(Json(drv.list_complaints(sid).await?))
}

async fn delete_complaint(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, tcldbid, fcldbid)): Path<(String, u32, u32, u32)>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Moderator)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    drv.delete_complaint(sid, tcldbid, fcldbid).await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "complaint.delete",
        &format!("target:{tcldbid}"),
        serde_json::json!({ "from_cldbid": fcldbid }),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn list_ts_logs(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
    Query(q): Query<TsLogsQuery>,
) -> ApiResult<Json<Vec<TsLogEntry>>> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Auditor)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    Ok(Json(drv.view_server_logs(sid, q.lines.clamp(1, 200)).await?))
}

async fn list_audit_logs(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<AuditQuery>,
) -> ApiResult<Json<Vec<AuditLogView>>> {
    let accessible = auth.accessible_instance_ids(&state.db).await?;
    let mut query = audit_logs::Entity::find()
        .order_by_desc(audit_logs::Column::CreatedAt)
        .limit(q.limit.clamp(1, 300));

    if let Some(inst) = q.instance_id.as_deref().filter(|s| !s.is_empty()) {
        auth.require_instance_role(&state.db, inst, InstanceRole::Auditor)
            .await?;
        query = query.filter(audit_logs::Column::InstanceId.eq(inst));
    } else if let Some(ids) = accessible {
        if ids.is_empty() {
            query = query.filter(audit_logs::Column::ActorUserId.eq(&auth.user_id));
        } else {
            query = query.filter(audit_logs::Column::InstanceId.is_in(ids));
        }
    }

    let rows = query.all(&state.db).await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| AuditLogView {
                id: r.id,
                actor_username: r.actor_username,
                instance_id: r.instance_id,
                virtual_server_id: r.virtual_server_id,
                action: r.action,
                target: r.target,
                detail_json: r.detail_json,
                ip_addr: r.ip_addr,
                created_at: r.created_at.to_rfc3339(),
            })
            .collect(),
    ))
}
