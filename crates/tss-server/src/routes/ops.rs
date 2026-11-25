use crate::app_state::AppState;
use crate::auth::{write_audit_log, AuthUser, InstanceRole};
use crate::entities::{
    automation_rules, backup_policies, snapshots, user_instance_roles, users, webhook_endpoints,
};
use crate::error::{ApiResult, AppError};
use crate::scheduler::{
    capture_snapshot_record, deliver_single_webhook, execute_automation_rule, validate_cron,
    SnapshotSummary,
};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::{header, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use tss_tsproto::TsSnapshotData;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        // Snapshots
        .route(
            "/instances/{id}/servers/{sid}/snapshots",
            get(list_snapshots).post(create_snapshot),
        )
        .route(
            "/instances/{id}/servers/{sid}/snapshots/import",
            post(import_snapshot),
        )
        .route(
            "/instances/{id}/servers/{sid}/snapshots/{snap_id}",
            delete(delete_snapshot),
        )
        .route(
            "/instances/{id}/servers/{sid}/snapshots/{snap_id}/diff",
            get(diff_snapshot),
        )
        .route(
            "/instances/{id}/servers/{sid}/snapshots/{snap_id}/deploy",
            post(deploy_snapshot),
        )
        .route(
            "/instances/{id}/servers/{sid}/snapshots/{snap_id}/export",
            get(export_snapshot),
        )
        // Backup policies
        .route(
            "/instances/{id}/servers/{sid}/backup-policies",
            get(list_backup_policies).post(create_backup_policy),
        )
        .route(
            "/instances/{id}/servers/{sid}/backup-policies/{pid}",
            put(update_backup_policy).delete(delete_backup_policy),
        )
        // Automation rules
        .route(
            "/instances/{id}/servers/{sid}/automation-rules",
            get(list_automation_rules).post(create_automation_rule),
        )
        .route(
            "/instances/{id}/servers/{sid}/automation-rules/{rid}",
            put(update_automation_rule).delete(delete_automation_rule),
        )
        .route(
            "/instances/{id}/servers/{sid}/automation-rules/{rid}/run-now",
            post(run_automation_rule_now),
        )
        // Webhooks
        .route("/webhooks", get(list_webhooks).post(create_webhook))
        .route(
            "/webhooks/{wid}",
            put(update_webhook).delete(delete_webhook),
        )
        .route("/webhooks/{wid}/test", post(test_webhook))
        // Users & RBAC
        .route("/users", get(list_users).post(create_user))
        .route("/users/{uid}", put(update_user).delete(delete_user))
        .route("/users/{uid}/roles", get(get_user_roles).put(set_user_roles))
        // WebSocket
        .route("/ws", get(ws_handler))
}

// --- Snapshots ---

#[derive(Serialize)]
struct SnapshotView {
    id: String,
    instance_id: String,
    virtual_server_id: i32,
    name: String,
    trigger_kind: String,
    summary: SnapshotSummary,
    size_bytes: i64,
    created_at: String,
}

#[derive(Deserialize)]
struct CreateSnapshotReq {
    name: Option<String>,
}

#[derive(Deserialize)]
struct ImportSnapshotReq {
    name: String,
    version: String,
    data: String,
    #[serde(default)]
    server_name: Option<String>,
}

#[derive(Deserialize)]
struct DiffQuery {
    #[serde(default = "default_against")]
    against: String,
}

fn default_against() -> String {
    "live".to_string()
}

#[derive(Serialize)]
struct SnapshotDiffResp {
    snapshot_name: String,
    against_label: String,
    snapshot_summary: SnapshotSummary,
    target_summary: SnapshotSummary,
    channels_added_in_target: Vec<String>,
    channels_removed_from_target: Vec<String>,
    groups_added_in_target: Vec<String>,
    groups_removed_from_target: Vec<String>,
}

fn to_snapshot_view(m: snapshots::Model) -> SnapshotView {
    let summary: SnapshotSummary =
        serde_json::from_str(&m.summary_json).unwrap_or(SnapshotSummary {
            server_name: "Unknown".into(),
            max_clients: 0,
            channel_count: 0,
            group_count: 0,
            channel_names: vec![],
            group_names: vec![],
        });
    SnapshotView {
        id: m.id,
        instance_id: m.instance_id,
        virtual_server_id: m.virtual_server_id,
        name: m.name,
        trigger_kind: m.trigger_kind,
        summary,
        size_bytes: m.size_bytes,
        created_at: m.created_at.to_rfc3339(),
    }
}

async fn list_snapshots(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
) -> ApiResult<Json<Vec<SnapshotView>>> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Auditor)
        .await?;
    let rows = snapshots::Entity::find()
        .filter(snapshots::Column::InstanceId.eq(&id))
        .filter(snapshots::Column::VirtualServerId.eq(sid as i32))
        .order_by_desc(snapshots::Column::CreatedAt)
        .all(&state.db)
        .await?;
    Ok(Json(rows.into_iter().map(to_snapshot_view).collect()))
}

async fn create_snapshot(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
    Json(req): Json<CreateSnapshotReq>,
) -> ApiResult<(StatusCode, Json<SnapshotView>)> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    let name = req
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("Manual {}", Utc::now().format("%Y-%m-%d %H:%M UTC")));
    let model =
        capture_snapshot_record(&state, &id, sid, &name, "manual", Some(&auth.user_id)).await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "snapshot.create",
        &model.name,
        serde_json::json!({ "snapshot_id": model.id }),
        &auth.ip_addr,
    )
    .await;

    Ok((StatusCode::CREATED, Json(to_snapshot_view(model))))
}

async fn import_snapshot(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
    Json(req): Json<ImportSnapshotReq>,
) -> ApiResult<(StatusCode, Json<SnapshotView>)> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    if req.name.trim().is_empty() || req.data.trim().is_empty() {
        return Err(AppError::Validation(
            "Snapshot name and data are required".into(),
        ));
    }
    let payload = TsSnapshotData {
        version: req.version,
        data: req.data,
    };
    let raw_payload = serde_json::to_string(&payload)
        .map_err(|e| AppError::Internal(format!("Serialize import failed: {e}")))?;
    let summary = SnapshotSummary {
        server_name: req.server_name.unwrap_or_else(|| "Imported Snapshot".into()),
        max_clients: 0,
        channel_count: 0,
        group_count: 0,
        channel_names: vec![],
        group_names: vec![],
    };
    let model = snapshots::ActiveModel {
        id: Set(Uuid::now_v7().to_string()),
        instance_id: Set(id.clone()),
        virtual_server_id: Set(sid as i32),
        name: Set(req.name.trim().to_string()),
        trigger_kind: Set("imported".to_string()),
        encrypted_payload: Set(state.crypto.encrypt_secret(&raw_payload)?),
        summary_json: Set(serde_json::to_string(&summary).unwrap_or_default()),
        size_bytes: Set(raw_payload.len() as i64),
        created_by: Set(Some(auth.user_id.clone())),
        created_at: Set(Utc::now().fixed_offset()),
    }
    .insert(&state.db)
    .await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "snapshot.import",
        &model.name,
        serde_json::json!({ "snapshot_id": model.id }),
        &auth.ip_addr,
    )
    .await;

    Ok((StatusCode::CREATED, Json(to_snapshot_view(model))))
}

async fn diff_snapshot(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, snap_id)): Path<(String, u32, String)>,
    Query(q): Query<DiffQuery>,
) -> ApiResult<Json<SnapshotDiffResp>> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Auditor)
        .await?;
    let snap = snapshots::Entity::find_by_id(&snap_id)
        .one(&state.db)
        .await?
        .filter(|s| s.instance_id == id)
        .ok_or_else(|| AppError::NotFound("Snapshot not found".into()))?;
    let snap_view = to_snapshot_view(snap);

    let (against_label, target_summary) = if q.against == "live" {
        let drv = state.hub.driver_for(&id).await?;
        let info = drv.get_server_info(sid).await?;
        let channels = drv.list_channels(sid).await?;
        let groups = drv.list_server_groups(sid).await?;
        (
            "Live Virtual Server".to_string(),
            SnapshotSummary {
                server_name: info.name,
                max_clients: info.max_clients,
                channel_count: channels.len(),
                group_count: groups.len(),
                channel_names: channels.into_iter().map(|c| c.channel_name).collect(),
                group_names: groups.into_iter().map(|g| g.name).collect(),
            },
        )
    } else {
        let other = snapshots::Entity::find_by_id(&q.against)
            .one(&state.db)
            .await?
            .filter(|s| s.instance_id == id)
            .ok_or_else(|| AppError::NotFound("Comparison snapshot not found".into()))?;
        let ov = to_snapshot_view(other);
        (ov.name, ov.summary)
    };

    let snap_ch: BTreeSet<&str> = snap_view
        .summary
        .channel_names
        .iter()
        .map(String::as_str)
        .collect();
    let tgt_ch: BTreeSet<&str> = target_summary
        .channel_names
        .iter()
        .map(String::as_str)
        .collect();
    let snap_gr: BTreeSet<&str> = snap_view
        .summary
        .group_names
        .iter()
        .map(String::as_str)
        .collect();
    let tgt_gr: BTreeSet<&str> = target_summary
        .group_names
        .iter()
        .map(String::as_str)
        .collect();

    Ok(Json(SnapshotDiffResp {
        snapshot_name: snap_view.name.clone(),
        against_label,
        channels_added_in_target: tgt_ch.difference(&snap_ch).map(|s| (*s).to_string()).collect(),
        channels_removed_from_target: snap_ch.difference(&tgt_ch).map(|s| (*s).to_string()).collect(),
        groups_added_in_target: tgt_gr.difference(&snap_gr).map(|s| (*s).to_string()).collect(),
        groups_removed_from_target: snap_gr.difference(&tgt_gr).map(|s| (*s).to_string()).collect(),
        snapshot_summary: snap_view.summary,
        target_summary,
    }))
}

async fn deploy_snapshot(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, snap_id)): Path<(String, u32, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    let snap = snapshots::Entity::find_by_id(&snap_id)
        .one(&state.db)
        .await?
        .filter(|s| s.instance_id == id)
        .ok_or_else(|| AppError::NotFound("Snapshot not found".into()))?;

    // Automatic pre-rollback safety snapshot
    let safety_name = format!("Pre-Rollback ({})", Utc::now().format("%Y-%m-%d %H:%M UTC"));
    let safety = capture_snapshot_record(
        &state,
        &id,
        sid,
        &safety_name,
        "pre_rollback",
        Some(&auth.user_id),
    )
    .await?;

    let raw_json = state.crypto.decrypt_secret(&snap.encrypted_payload)?;
    let ts_data: TsSnapshotData = serde_json::from_str(&raw_json)
        .map_err(|e| AppError::Internal(format!("Corrupted snapshot payload: {e}")))?;

    let drv = state.hub.driver_for(&id).await?;
    drv.deploy_snapshot(sid, &ts_data).await?;
    let _ = state.hub.refresh_virtual_server(&id, sid, false).await;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "snapshot.deploy",
        &snap.name,
        serde_json::json!({ "snapshot_id": snap.id, "safety_snapshot_id": safety.id }),
        &auth.ip_addr,
    )
    .await;

    Ok(Json(serde_json::json!({
        "deployed": true,
        "safety_snapshot_id": safety.id
    })))
}

async fn export_snapshot(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, _sid, snap_id)): Path<(String, u32, String)>,
) -> ApiResult<impl IntoResponse> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    let snap = snapshots::Entity::find_by_id(&snap_id)
        .one(&state.db)
        .await?
        .filter(|s| s.instance_id == id)
        .ok_or_else(|| AppError::NotFound("Snapshot not found".into()))?;
    let raw_json = state.crypto.decrypt_secret(&snap.encrypted_payload)?;
    let ts_data: TsSnapshotData = serde_json::from_str(&raw_json)
        .map_err(|e| AppError::Internal(format!("Corrupted snapshot payload: {e}")))?;

    let export_doc = serde_json::json!({
        "name": snap.name,
        "version": ts_data.version,
        "data": ts_data.data,
        "exported_at": Utc::now().to_rfc3339()
    });
    let mut resp = Json(export_doc).into_response();
    let disp = format!("attachment; filename=\"snapshot-{}.tssnap.json\"", snap.id);
    if let Ok(hv) = HeaderValue::from_str(&disp) {
        resp.headers_mut().insert(header::CONTENT_DISPOSITION, hv);
    }
    Ok(resp)
}

async fn delete_snapshot(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, snap_id)): Path<(String, u32, String)>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    let snap = snapshots::Entity::find_by_id(&snap_id)
        .one(&state.db)
        .await?
        .filter(|s| s.instance_id == id)
        .ok_or_else(|| AppError::NotFound("Snapshot not found".into()))?;
    snapshots::Entity::delete_by_id(&snap_id)
        .exec(&state.db)
        .await?;
    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "snapshot.delete",
        &snap.name,
        serde_json::json!({ "snapshot_id": snap_id }),
        &auth.ip_addr,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

// --- Backup Policies ---

#[derive(Serialize)]
struct BackupPolicyView {
    id: String,
    instance_id: String,
    virtual_server_id: i32,
    cron_expr: String,
    retention_count: i32,
    enabled: bool,
    last_run_at: Option<String>,
    last_status: Option<String>,
    created_at: String,
}

#[derive(Deserialize)]
struct BackupPolicyReq {
    cron_expr: String,
    #[serde(default = "default_retention")]
    retention_count: i32,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_retention() -> i32 {
    7
}
fn default_true() -> bool {
    true
}

fn to_policy_view(m: backup_policies::Model) -> BackupPolicyView {
    BackupPolicyView {
        id: m.id,
        instance_id: m.instance_id,
        virtual_server_id: m.virtual_server_id,
        cron_expr: m.cron_expr,
        retention_count: m.retention_count,
        enabled: m.enabled,
        last_run_at: m.last_run_at.map(|t| t.to_rfc3339()),
        last_status: m.last_status,
        created_at: m.created_at.to_rfc3339(),
    }
}

async fn list_backup_policies(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
) -> ApiResult<Json<Vec<BackupPolicyView>>> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Auditor)
        .await?;
    let rows = backup_policies::Entity::find()
        .filter(backup_policies::Column::InstanceId.eq(&id))
        .filter(backup_policies::Column::VirtualServerId.eq(sid as i32))
        .all(&state.db)
        .await?;
    Ok(Json(rows.into_iter().map(to_policy_view).collect()))
}

async fn create_backup_policy(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
    Json(req): Json<BackupPolicyReq>,
) -> ApiResult<(StatusCode, Json<BackupPolicyView>)> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    validate_cron(&req.cron_expr)?;
    let model = backup_policies::ActiveModel {
        id: Set(Uuid::now_v7().to_string()),
        instance_id: Set(id.clone()),
        virtual_server_id: Set(sid as i32),
        cron_expr: Set(req.cron_expr.trim().to_string()),
        retention_count: Set(req.retention_count.clamp(1, 100)),
        enabled: Set(req.enabled),
        last_run_at: Set(None),
        last_status: Set(None),
        created_at: Set(Utc::now().fixed_offset()),
    }
    .insert(&state.db)
    .await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "backup_policy.create",
        &model.cron_expr,
        serde_json::json!({ "retention_count": model.retention_count }),
        &auth.ip_addr,
    )
    .await;

    Ok((StatusCode::CREATED, Json(to_policy_view(model))))
}

async fn update_backup_policy(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, _sid, pid)): Path<(String, u32, String)>,
    Json(req): Json<BackupPolicyReq>,
) -> ApiResult<Json<BackupPolicyView>> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    validate_cron(&req.cron_expr)?;
    let model = backup_policies::Entity::find_by_id(&pid)
        .one(&state.db)
        .await?
        .filter(|p| p.instance_id == id)
        .ok_or_else(|| AppError::NotFound("Backup policy not found".into()))?;
    let mut active: backup_policies::ActiveModel = model.into();
    active.cron_expr = Set(req.cron_expr.trim().to_string());
    active.retention_count = Set(req.retention_count.clamp(1, 100));
    active.enabled = Set(req.enabled);
    let updated = active.update(&state.db).await?;
    Ok(Json(to_policy_view(updated)))
}

async fn delete_backup_policy(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, _sid, pid)): Path<(String, u32, String)>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    backup_policies::Entity::delete_by_id(&pid)
        .exec(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// --- Automation Rules ---

#[derive(Serialize)]
struct AutomationRuleView {
    id: String,
    instance_id: String,
    virtual_server_id: i32,
    name: String,
    rule_kind: String,
    config: serde_json::Value,
    enabled: bool,
    last_triggered_at: Option<String>,
    created_at: String,
}

#[derive(Deserialize)]
struct AutomationRuleReq {
    name: String,
    rule_kind: String,
    config: serde_json::Value,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn to_rule_view(m: automation_rules::Model) -> AutomationRuleView {
    let config = serde_json::from_str(&m.config_json).unwrap_or(serde_json::json!({}));
    AutomationRuleView {
        id: m.id,
        instance_id: m.instance_id,
        virtual_server_id: m.virtual_server_id,
        name: m.name,
        rule_kind: m.rule_kind,
        config,
        enabled: m.enabled,
        last_triggered_at: m.last_triggered_at.map(|t| t.to_rfc3339()),
        created_at: m.created_at.to_rfc3339(),
    }
}

fn validate_automation_req(req: &AutomationRuleReq) -> ApiResult<()> {
    if req.name.trim().is_empty() {
        return Err(AppError::Validation("Rule name is required".into()));
    }
    match req.rule_kind.as_str() {
        "afk_mover" | "alert_offline" | "alert_packet_loss" => Ok(()),
        "timed_broadcast" => {
            let expr = req
                .config
                .get("cron_expr")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("cron_expr is required".into()))?;
            validate_cron(expr)?;
            Ok(())
        }
        other => Err(AppError::Validation(format!("Invalid rule_kind '{other}'"))),
    }
}

async fn list_automation_rules(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
) -> ApiResult<Json<Vec<AutomationRuleView>>> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Auditor)
        .await?;
    let rows = automation_rules::Entity::find()
        .filter(automation_rules::Column::InstanceId.eq(&id))
        .filter(automation_rules::Column::VirtualServerId.eq(sid as i32))
        .order_by_desc(automation_rules::Column::CreatedAt)
        .all(&state.db)
        .await?;
    Ok(Json(rows.into_iter().map(to_rule_view).collect()))
}

async fn create_automation_rule(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
    Json(req): Json<AutomationRuleReq>,
) -> ApiResult<(StatusCode, Json<AutomationRuleView>)> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    validate_automation_req(&req)?;
    let model = automation_rules::ActiveModel {
        id: Set(Uuid::now_v7().to_string()),
        instance_id: Set(id.clone()),
        virtual_server_id: Set(sid as i32),
        name: Set(req.name.trim().to_string()),
        rule_kind: Set(req.rule_kind),
        config_json: Set(req.config.to_string()),
        enabled: Set(req.enabled),
        last_triggered_at: Set(None),
        created_at: Set(Utc::now().fixed_offset()),
    }
    .insert(&state.db)
    .await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "automation.create",
        &model.name,
        serde_json::json!({ "rule_kind": model.rule_kind }),
        &auth.ip_addr,
    )
    .await;

    Ok((StatusCode::CREATED, Json(to_rule_view(model))))
}

async fn update_automation_rule(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, _sid, rid)): Path<(String, u32, String)>,
    Json(req): Json<AutomationRuleReq>,
) -> ApiResult<Json<AutomationRuleView>> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    validate_automation_req(&req)?;
    let model = automation_rules::Entity::find_by_id(&rid)
        .one(&state.db)
        .await?
        .filter(|r| r.instance_id == id)
        .ok_or_else(|| AppError::NotFound("Automation rule not found".into()))?;
    let mut active: automation_rules::ActiveModel = model.into();
    active.name = Set(req.name.trim().to_string());
    active.rule_kind = Set(req.rule_kind);
    active.config_json = Set(req.config.to_string());
    active.enabled = Set(req.enabled);
    let updated = active.update(&state.db).await?;
    Ok(Json(to_rule_view(updated)))
}

async fn delete_automation_rule(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, _sid, rid)): Path<(String, u32, String)>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    automation_rules::Entity::delete_by_id(&rid)
        .exec(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn run_automation_rule_now(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, _sid, rid)): Path<(String, u32, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    let rule = automation_rules::Entity::find_by_id(&rid)
        .one(&state.db)
        .await?
        .filter(|r| r.instance_id == id)
        .ok_or_else(|| AppError::NotFound("Automation rule not found".into()))?;
    let result = execute_automation_rule(&state, &rule, true).await?;
    Ok(Json(result))
}

// --- Webhooks ---

#[derive(Serialize)]
struct WebhookView {
    id: String,
    name: String,
    url: String,
    has_signing_secret: bool,
    events_csv: String,
    enabled: bool,
    last_delivery_status: Option<String>,
    created_at: String,
}

#[derive(Deserialize)]
struct WebhookReq {
    name: String,
    url: String,
    signing_secret: Option<String>,
    #[serde(default = "default_events")]
    events_csv: String,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_events() -> String {
    "*".to_string()
}

fn to_webhook_view(m: webhook_endpoints::Model) -> WebhookView {
    WebhookView {
        id: m.id,
        name: m.name,
        url: m.url,
        has_signing_secret: m.encrypted_signing_secret.is_some(),
        events_csv: m.events_csv,
        enabled: m.enabled,
        last_delivery_status: m.last_delivery_status,
        created_at: m.created_at.to_rfc3339(),
    }
}

async fn list_webhooks(
    State(state): State<AppState>,
    auth: AuthUser,
) -> ApiResult<Json<Vec<WebhookView>>> {
    auth.require_super_admin()?;
    let rows = webhook_endpoints::Entity::find()
        .order_by_desc(webhook_endpoints::Column::CreatedAt)
        .all(&state.db)
        .await?;
    Ok(Json(rows.into_iter().map(to_webhook_view).collect()))
}

async fn create_webhook(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<WebhookReq>,
) -> ApiResult<(StatusCode, Json<WebhookView>)> {
    auth.require_super_admin()?;
    if req.name.trim().is_empty() || req.url.trim().is_empty() {
        return Err(AppError::Validation("Webhook name and URL are required".into()));
    }
    let enc = match req.signing_secret.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => Some(state.crypto.encrypt_secret(s)?),
        None => None,
    };
    let model = webhook_endpoints::ActiveModel {
        id: Set(Uuid::now_v7().to_string()),
        name: Set(req.name.trim().to_string()),
        url: Set(req.url.trim().to_string()),
        encrypted_signing_secret: Set(enc),
        events_csv: Set(req.events_csv.trim().to_string()),
        enabled: Set(req.enabled),
        last_delivery_status: Set(None),
        created_at: Set(Utc::now().fixed_offset()),
    }
    .insert(&state.db)
    .await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        None,
        None,
        "webhook.create",
        &model.name,
        serde_json::json!({ "url": model.url }),
        &auth.ip_addr,
    )
    .await;

    Ok((StatusCode::CREATED, Json(to_webhook_view(model))))
}

async fn update_webhook(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(wid): Path<String>,
    Json(req): Json<WebhookReq>,
) -> ApiResult<Json<WebhookView>> {
    auth.require_super_admin()?;
    let model = webhook_endpoints::Entity::find_by_id(&wid)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Webhook not found".into()))?;
    let mut active: webhook_endpoints::ActiveModel = model.into();
    active.name = Set(req.name.trim().to_string());
    active.url = Set(req.url.trim().to_string());
    if let Some(s) = req.signing_secret.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        active.encrypted_signing_secret = Set(Some(state.crypto.encrypt_secret(s)?));
    }
    active.events_csv = Set(req.events_csv.trim().to_string());
    active.enabled = Set(req.enabled);
    let updated = active.update(&state.db).await?;
    Ok(Json(to_webhook_view(updated)))
}

async fn delete_webhook(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(wid): Path<String>,
) -> ApiResult<StatusCode> {
    auth.require_super_admin()?;
    webhook_endpoints::Entity::delete_by_id(&wid)
        .exec(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn test_webhook(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(wid): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_super_admin()?;
    let ep = webhook_endpoints::Entity::find_by_id(&wid)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Webhook not found".into()))?;
    let ts = Utc::now().to_rfc3339();
    let payload = serde_json::json!({
        "event": "webhook.test",
        "timestamp": ts,
        "severity": "info",
        "details": { "triggered_by": auth.username }
    })
    .to_string();
    let status = deliver_single_webhook(&state, &ep, "webhook.test", &ts, &payload).await;
    let mut active: webhook_endpoints::ActiveModel = ep.into();
    active.last_delivery_status = Set(Some(status.clone()));
    active.update(&state.db).await?;
    Ok(Json(serde_json::json!({ "delivery_status": status })))
}

// --- Users & RBAC ---

#[derive(Serialize)]
struct UserRowView {
    id: String,
    username: String,
    display_name: String,
    global_role: String,
    totp_enabled: bool,
    is_active: bool,
    last_login_at: Option<String>,
    created_at: String,
    instance_roles: Vec<RoleAssignItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RoleAssignItem {
    instance_id: String,
    role: String,
}

#[derive(Deserialize)]
struct CreateUserReq {
    username: String,
    display_name: String,
    password: String,
    #[serde(default = "default_member_role")]
    global_role: String,
    #[serde(default)]
    instance_roles: Vec<RoleAssignItem>,
}

fn default_member_role() -> String {
    "member".to_string()
}

#[derive(Deserialize)]
struct UpdateUserReq {
    display_name: Option<String>,
    global_role: Option<String>,
    is_active: Option<bool>,
    password: Option<String>,
}

#[derive(Deserialize)]
struct SetUserRolesReq {
    roles: Vec<RoleAssignItem>,
}

async fn load_user_view(state: &AppState, u: users::Model) -> ApiResult<UserRowView> {
    let roles = user_instance_roles::Entity::find()
        .filter(user_instance_roles::Column::UserId.eq(&u.id))
        .all(&state.db)
        .await?;
    Ok(UserRowView {
        id: u.id,
        username: u.username,
        display_name: u.display_name,
        global_role: u.global_role,
        totp_enabled: u.totp_enabled,
        is_active: u.is_active,
        last_login_at: u.last_login_at.map(|t| t.to_rfc3339()),
        created_at: u.created_at.to_rfc3339(),
        instance_roles: roles
            .into_iter()
            .map(|r| RoleAssignItem {
                instance_id: r.instance_id,
                role: r.role,
            })
            .collect(),
    })
}

async fn list_users(
    State(state): State<AppState>,
    auth: AuthUser,
) -> ApiResult<Json<Vec<UserRowView>>> {
    auth.require_super_admin()?;
    let rows = users::Entity::find()
        .order_by_asc(users::Column::CreatedAt)
        .all(&state.db)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for u in rows {
        out.push(load_user_view(&state, u).await?);
    }
    Ok(Json(out))
}

async fn create_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateUserReq>,
) -> ApiResult<(StatusCode, Json<UserRowView>)> {
    auth.require_super_admin()?;
    let username = req.username.trim();
    if username.len() < 3 {
        return Err(AppError::Validation("Username must be at least 3 chars".into()));
    }
    if req.password.len() < 8 {
        return Err(AppError::Validation("Password must be at least 8 chars".into()));
    }
    if req.global_role != "super_admin" && req.global_role != "member" {
        return Err(AppError::Validation("global_role must be super_admin or member".into()));
    }
    let now = Utc::now().fixed_offset();
    let hash = state.crypto.hash_password(&req.password)?;
    let user = users::ActiveModel {
        id: Set(Uuid::now_v7().to_string()),
        username: Set(username.to_string()),
        display_name: Set(req.display_name.trim().to_string()),
        password_hash: Set(hash),
        totp_secret_enc: Set(None),
        totp_enabled: Set(false),
        global_role: Set(req.global_role),
        is_active: Set(true),
        last_login_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&state.db)
    .await?;

    for item in req.instance_roles {
        if InstanceRole::parse(&item.role).is_some() {
            let _ = user_instance_roles::ActiveModel {
                id: Set(Uuid::now_v7().to_string()),
                user_id: Set(user.id.clone()),
                instance_id: Set(item.instance_id),
                role: Set(item.role),
                created_at: Set(now),
            }
            .insert(&state.db)
            .await;
        }
    }

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        None,
        None,
        "user.create",
        &user.username,
        serde_json::json!({ "global_role": user.global_role }),
        &auth.ip_addr,
    )
    .await;

    Ok((StatusCode::CREATED, Json(load_user_view(&state, user).await?)))
}

async fn update_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(uid): Path<String>,
    Json(req): Json<UpdateUserReq>,
) -> ApiResult<Json<UserRowView>> {
    auth.require_super_admin()?;
    let user = users::Entity::find_by_id(&uid)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;
    let mut active: users::ActiveModel = user.into();
    if let Some(d) = req.display_name {
        active.display_name = Set(d.trim().to_string());
    }
    if let Some(gr) = req.global_role {
        if gr != "super_admin" && gr != "member" {
            return Err(AppError::Validation("Invalid global_role".into()));
        }
        active.global_role = Set(gr);
    }
    if let Some(act) = req.is_active {
        if uid == auth.user_id && !act {
            return Err(AppError::Validation("Cannot disable your own account".into()));
        }
        active.is_active = Set(act);
    }
    if let Some(pw) = req.password.filter(|s| !s.is_empty()) {
        if pw.len() < 8 {
            return Err(AppError::Validation("Password must be at least 8 chars".into()));
        }
        active.password_hash = Set(state.crypto.hash_password(&pw)?);
    }
    active.updated_at = Set(Utc::now().fixed_offset());
    let updated = active.update(&state.db).await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        None,
        None,
        "user.update",
        &updated.username,
        serde_json::json!({ "is_active": updated.is_active, "global_role": updated.global_role }),
        &auth.ip_addr,
    )
    .await;

    Ok(Json(load_user_view(&state, updated).await?))
}

async fn delete_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(uid): Path<String>,
) -> ApiResult<StatusCode> {
    auth.require_super_admin()?;
    if uid == auth.user_id {
        return Err(AppError::Validation("Cannot delete your own account".into()));
    }
    users::Entity::delete_by_id(&uid).exec(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_user_roles(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(uid): Path<String>,
) -> ApiResult<Json<Vec<RoleAssignItem>>> {
    auth.require_super_admin()?;
    let rows = user_instance_roles::Entity::find()
        .filter(user_instance_roles::Column::UserId.eq(&uid))
        .all(&state.db)
        .await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| RoleAssignItem {
                instance_id: r.instance_id,
                role: r.role,
            })
            .collect(),
    ))
}

async fn set_user_roles(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(uid): Path<String>,
    Json(req): Json<SetUserRolesReq>,
) -> ApiResult<Json<Vec<RoleAssignItem>>> {
    auth.require_super_admin()?;
    user_instance_roles::Entity::delete_many()
        .filter(user_instance_roles::Column::UserId.eq(&uid))
        .exec(&state.db)
        .await?;

    let now = Utc::now().fixed_offset();
    for item in &req.roles {
        let Some(role) = InstanceRole::parse(&item.role) else {
            return Err(AppError::Validation(format!("Invalid role '{}'", item.role)));
        };
        if role == InstanceRole::SuperAdmin {
            return Err(AppError::Validation(
                "Use global_role for super_admin".into(),
            ));
        }
        user_instance_roles::ActiveModel {
            id: Set(Uuid::now_v7().to_string()),
            user_id: Set(uid.clone()),
            instance_id: Set(item.instance_id.clone()),
            role: Set(role.as_str().to_string()),
            created_at: Set(now),
        }
        .insert(&state.db)
        .await?;
    }

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        None,
        None,
        "user.roles_update",
        &uid,
        serde_json::to_value(&req.roles).unwrap_or_default(),
        &auth.ip_addr,
    )
    .await;

    Ok(Json(req.roles))
}

// --- WebSocket ---

#[derive(Deserialize)]
struct WsQuery {
    token: String,
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(q): Query<WsQuery>,
) -> ApiResult<impl IntoResponse> {
    let claims = state.crypto.verify_jwt(&q.token, "access")?;
    let user = users::Entity::find_by_id(&claims.sub)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("User not found".into()))?;
    if !user.is_active {
        return Err(AppError::Unauthorized("Account disabled".into()));
    }
    let accessible = if user.global_role == "super_admin" {
        None
    } else {
        let rows = user_instance_roles::Entity::find()
            .filter(user_instance_roles::Column::UserId.eq(&user.id))
            .all(&state.db)
            .await?;
        Some(rows.into_iter().map(|r| r.instance_id).collect::<Vec<_>>())
    };
    Ok(ws.on_upgrade(move |socket| handle_ws_socket(socket, state, accessible)))
}

async fn handle_ws_socket(
    mut socket: WebSocket,
    state: AppState,
    accessible: Option<Vec<String>>,
) {
    let mut rx = state.hub.subscribe_ws();
    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(env) => {
                        if let Some(allowed) = &accessible {
                            if !allowed.contains(&env.instance_id) {
                                continue;
                            }
                        }
                        if let Ok(json) = serde_json::to_string(&env) {
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(txt))) => {
                        if txt.trim() == "ping" {
                            let pong = serde_json::json!({
                                "event": "pong",
                                "timestamp": Utc::now().to_rfc3339()
                            }).to_string();
                            if socket.send(Message::Text(pong.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}
