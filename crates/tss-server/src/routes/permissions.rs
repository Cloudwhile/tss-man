use crate::app_state::AppState;
use crate::auth::{write_audit_log, AuthUser, InstanceRole};
use crate::error::{ApiResult, AppError};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, put},
    Json, Router,
};
use serde::Deserialize;
use tss_tsproto::{
    ChannelGroup, CreatePrivilegeKeyParams, GroupMember, PermissionEntry, PrivilegeKey,
    ServerGroup, SetPermissionParams,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/instances/{id}/servers/{sid}/server-groups",
            get(list_server_groups).post(create_server_group),
        )
        .route(
            "/instances/{id}/servers/{sid}/server-groups/{sgid}",
            put(rename_server_group).delete(delete_server_group),
        )
        .route(
            "/instances/{id}/servers/{sid}/server-groups/{sgid}/members",
            get(list_group_members).post(add_group_member),
        )
        .route(
            "/instances/{id}/servers/{sid}/server-groups/{sgid}/members/{cldbid}",
            delete(remove_group_member),
        )
        .route(
            "/instances/{id}/servers/{sid}/server-groups/{sgid}/permissions",
            get(list_group_permissions).put(set_group_permission),
        )
        .route(
            "/instances/{id}/servers/{sid}/server-groups/{sgid}/permissions/{permname}",
            delete(remove_group_permission),
        )
        .route(
            "/instances/{id}/servers/{sid}/channel-groups",
            get(list_channel_groups),
        )
        .route(
            "/instances/{id}/servers/{sid}/tokens",
            get(list_tokens).post(create_tokens),
        )
        .route(
            "/instances/{id}/servers/{sid}/tokens/{token}",
            delete(delete_token),
        )
}

#[derive(Deserialize)]
struct CreateGroupReq {
    name: String,
    group_type: Option<u8>,
}

#[derive(Deserialize)]
struct RenameGroupReq {
    name: String,
}

#[derive(Deserialize)]
struct ForceQuery {
    #[serde(default)]
    force: bool,
}

#[derive(Deserialize)]
struct AddMemberReq {
    cldbid: u32,
}

#[derive(Deserialize)]
struct CreateTokensReq {
    token_type: u8,
    group_id: u32,
    channel_id: Option<u32>,
    description: Option<String>,
    #[serde(default = "default_count")]
    count: u32,
}

fn default_count() -> u32 {
    1
}

async fn list_server_groups(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
) -> ApiResult<Json<Vec<ServerGroup>>> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Auditor)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    Ok(Json(drv.list_server_groups(sid).await?))
}

async fn create_server_group(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
    Json(req): Json<CreateGroupReq>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    if req.name.trim().is_empty() {
        return Err(AppError::Validation("Group name is required".into()));
    }
    let drv = state.hub.driver_for(&id).await?;
    let sgid = drv
        .create_server_group(sid, req.name.trim(), req.group_type)
        .await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "server_group.create",
        req.name.trim(),
        serde_json::json!({ "sgid": sgid }),
        &auth.ip_addr,
    )
    .await;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "sgid": sgid }))))
}

async fn rename_server_group(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, sgid)): Path<(String, u32, u32)>,
    Json(req): Json<RenameGroupReq>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    drv.rename_server_group(sid, sgid, req.name.trim()).await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "server_group.rename",
        &format!("sgid:{sgid}"),
        serde_json::json!({ "name": req.name.trim() }),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn delete_server_group(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, sgid)): Path<(String, u32, u32)>,
    Query(q): Query<ForceQuery>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    drv.delete_server_group(sid, sgid, q.force).await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "server_group.delete",
        &format!("sgid:{sgid}"),
        serde_json::json!({ "force": q.force }),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn list_group_members(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, sgid)): Path<(String, u32, u32)>,
) -> ApiResult<Json<Vec<GroupMember>>> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Auditor)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    Ok(Json(drv.list_server_group_clients(sid, sgid).await?))
}

async fn add_group_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, sgid)): Path<(String, u32, u32)>,
    Json(req): Json<AddMemberReq>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    drv.add_client_to_server_group(sid, sgid, req.cldbid)
        .await?;
    let _ = state.hub.refresh_virtual_server(&id, sid, false).await;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "server_group.member_add",
        &format!("sgid:{sgid}"),
        serde_json::json!({ "cldbid": req.cldbid }),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn remove_group_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, sgid, cldbid)): Path<(String, u32, u32, u32)>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    drv.remove_client_from_server_group(sid, sgid, cldbid)
        .await?;
    let _ = state.hub.refresh_virtual_server(&id, sid, false).await;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "server_group.member_remove",
        &format!("sgid:{sgid}"),
        serde_json::json!({ "cldbid": cldbid }),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn list_group_permissions(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, sgid)): Path<(String, u32, u32)>,
) -> ApiResult<Json<Vec<PermissionEntry>>> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Auditor)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    Ok(Json(drv.list_server_group_permissions(sid, sgid).await?))
}

async fn set_group_permission(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, sgid)): Path<(String, u32, u32)>,
    Json(req): Json<SetPermissionParams>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    if req.permname.trim().is_empty() {
        return Err(AppError::Validation("Permission name is required".into()));
    }
    let drv = state.hub.driver_for(&id).await?;
    drv.set_server_group_permission(sid, sgid, &req).await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "server_group.permission_set",
        &format!("sgid:{sgid}"),
        serde_json::to_value(&req).unwrap_or_default(),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn remove_group_permission(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, sgid, permname)): Path<(String, u32, u32, String)>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    drv.remove_server_group_permission(sid, sgid, &permname)
        .await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "server_group.permission_remove",
        &format!("sgid:{sgid}"),
        serde_json::json!({ "permname": permname }),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn list_channel_groups(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
) -> ApiResult<Json<Vec<ChannelGroup>>> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Auditor)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    Ok(Json(drv.list_channel_groups(sid).await?))
}

async fn list_tokens(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
) -> ApiResult<Json<Vec<PrivilegeKey>>> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Auditor)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    Ok(Json(drv.list_privilege_keys(sid).await?))
}

async fn create_tokens(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
    Json(req): Json<CreateTokensReq>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    let count = req.count.clamp(1, 20);
    let drv = state.hub.driver_for(&id).await?;
    let params = CreatePrivilegeKeyParams {
        token_type: req.token_type,
        group_id: req.group_id,
        channel_id: req.channel_id,
        description: req.description.clone(),
    };
    let mut tokens = Vec::with_capacity(count as usize);
    for _ in 0..count {
        tokens.push(drv.create_privilege_key(sid, &params).await?);
    }

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "token.create",
        &format!("group:{}", req.group_id),
        serde_json::json!({ "count": tokens.len(), "token_type": req.token_type }),
        &auth.ip_addr,
    )
    .await;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "tokens": tokens }))))
}

async fn delete_token(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, token)): Path<(String, u32, String)>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    drv.delete_privilege_key(sid, &token).await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "token.delete",
        &token,
        serde_json::json!({}),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}
