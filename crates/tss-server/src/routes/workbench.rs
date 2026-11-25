use crate::app_state::AppState;
use crate::auth::{write_audit_log, AuthUser, InstanceRole};
use crate::error::{ApiResult, AppError};
use crate::state_hub::LiveVirtualServerState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use tss_tsproto::{CreateChannelParams, EditChannelParams};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/instances/{id}/servers/{sid}/tree", get(get_tree))
        .route(
            "/instances/{id}/servers/{sid}/channels",
            post(create_channel),
        )
        .route(
            "/instances/{id}/servers/{sid}/channels/{cid}",
            put(edit_channel).delete(delete_channel),
        )
        .route(
            "/instances/{id}/servers/{sid}/channels/{cid}/move",
            post(move_channel),
        )
        .route(
            "/instances/{id}/servers/{sid}/clients/{clid}/kick",
            post(kick_client),
        )
        .route(
            "/instances/{id}/servers/{sid}/clients/{clid}/move",
            post(move_client),
        )
        .route(
            "/instances/{id}/servers/{sid}/clients/{clid}/poke",
            post(poke_client),
        )
        .route(
            "/instances/{id}/servers/{sid}/clients/{clid}/ban",
            post(ban_client),
        )
        .route(
            "/instances/{id}/servers/{sid}/clients/{clid}/message",
            post(message_client),
        )
        .route(
            "/instances/{id}/servers/{sid}/clients/{clid}/groups",
            post(modify_client_group),
        )
        .route(
            "/instances/{id}/servers/{sid}/broadcast",
            post(broadcast_message),
        )
}

#[derive(Deserialize)]
struct ForceQuery {
    #[serde(default)]
    force: bool,
}

#[derive(Deserialize)]
struct MoveChannelReq {
    cpid: u32,
    #[serde(default)]
    order: u32,
}

#[derive(Deserialize)]
struct KickClientReq {
    #[serde(default)]
    from_server: bool,
    reason: Option<String>,
}

#[derive(Deserialize)]
struct MoveClientReq {
    cid: u32,
    password: Option<String>,
}

#[derive(Deserialize)]
struct MsgReq {
    msg: String,
}

#[derive(Deserialize)]
struct BanClientReq {
    #[serde(default)]
    duration_secs: i64,
    reason: Option<String>,
}

#[derive(Deserialize)]
struct ModifyClientGroupReq {
    action: String, // "add" | "remove"
    sgid: u32,
    cldbid: u32,
}

async fn get_tree(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
) -> ApiResult<Json<LiveVirtualServerState>> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Auditor)
        .await?;
    if let Some(live) = state.hub.get_live_state(&id, sid).await {
        return Ok(Json(live));
    }
    let live = state.hub.refresh_virtual_server(&id, sid, false).await?;
    Ok(Json(live))
}

async fn create_channel(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
    Json(req): Json<CreateChannelParams>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Moderator)
        .await?;
    if req.name.trim().is_empty() {
        return Err(AppError::Validation("Channel name is required".into()));
    }
    let drv = state.hub.driver_for(&id).await?;
    let cid = drv.create_channel(sid, &req).await?;
    let _ = state.hub.refresh_virtual_server(&id, sid, false).await;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "channel.create",
        req.name.trim(),
        serde_json::json!({ "cid": cid, "parent_id": req.parent_id }),
        &auth.ip_addr,
    )
    .await;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "cid": cid }))))
}

async fn edit_channel(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, cid)): Path<(String, u32, u32)>,
    Json(req): Json<EditChannelParams>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Moderator)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    drv.edit_channel(sid, cid, &req).await?;
    let _ = state.hub.refresh_virtual_server(&id, sid, false).await;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "channel.edit",
        &format!("cid:{cid}"),
        serde_json::to_value(&req).unwrap_or_default(),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn move_channel(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, cid)): Path<(String, u32, u32)>,
    Json(req): Json<MoveChannelReq>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Moderator)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    drv.move_channel(sid, cid, req.cpid, req.order).await?;
    let _ = state.hub.refresh_virtual_server(&id, sid, false).await;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "channel.move",
        &format!("cid:{cid}"),
        serde_json::json!({ "cpid": req.cpid, "order": req.order }),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn delete_channel(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, cid)): Path<(String, u32, u32)>,
    Query(q): Query<ForceQuery>,
) -> ApiResult<StatusCode> {
    let role = auth
        .require_instance_role(&state.db, &id, InstanceRole::Moderator)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    if role < InstanceRole::ServerAdmin {
        if let Ok(channels) = drv.list_channels(sid).await {
            if let Some(ch) = channels.iter().find(|c| c.cid == cid) {
                if ch.is_permanent {
                    return Err(AppError::Forbidden(
                        "Only server_admin or super_admin may delete permanent channels".into(),
                    ));
                }
            }
        }
    }
    drv.delete_channel(sid, cid, q.force).await?;
    let _ = state.hub.refresh_virtual_server(&id, sid, false).await;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "channel.delete",
        &format!("cid:{cid}"),
        serde_json::json!({ "force": q.force }),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn kick_client(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, clid)): Path<(String, u32, u32)>,
    Json(req): Json<KickClientReq>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Moderator)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    drv.kick_client(sid, clid, req.from_server, req.reason.as_deref())
        .await?;
    let _ = state.hub.refresh_virtual_server(&id, sid, false).await;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "client.kick",
        &format!("clid:{clid}"),
        serde_json::json!({ "from_server": req.from_server, "reason": req.reason }),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn move_client(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, clid)): Path<(String, u32, u32)>,
    Json(req): Json<MoveClientReq>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Moderator)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    drv.move_client(sid, clid, req.cid, req.password.as_deref())
        .await?;
    let _ = state.hub.refresh_virtual_server(&id, sid, false).await;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "client.move",
        &format!("clid:{clid}"),
        serde_json::json!({ "cid": req.cid }),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn poke_client(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, clid)): Path<(String, u32, u32)>,
    Json(req): Json<MsgReq>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Moderator)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    drv.poke_client(sid, clid, &req.msg).await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "client.poke",
        &format!("clid:{clid}"),
        serde_json::json!({ "msg": req.msg }),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn ban_client(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, clid)): Path<(String, u32, u32)>,
    Json(req): Json<BanClientReq>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Moderator)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    drv.ban_client(sid, clid, req.duration_secs, req.reason.as_deref())
        .await?;
    let _ = state.hub.refresh_virtual_server(&id, sid, false).await;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "client.ban",
        &format!("clid:{clid}"),
        serde_json::json!({ "duration_secs": req.duration_secs, "reason": req.reason }),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn message_client(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, clid)): Path<(String, u32, u32)>,
    Json(req): Json<MsgReq>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Moderator)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    drv.send_text_message(sid, 1, clid, &req.msg).await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "client.message",
        &format!("clid:{clid}"),
        serde_json::json!({ "msg": req.msg }),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn modify_client_group(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid, _clid)): Path<(String, u32, u32)>,
    Json(req): Json<ModifyClientGroupReq>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::ServerAdmin)
        .await?;
    let drv = state.hub.driver_for(&id).await?;
    match req.action.as_str() {
        "add" => drv.add_client_to_server_group(sid, req.sgid, req.cldbid).await?,
        "remove" => {
            drv.remove_client_from_server_group(sid, req.sgid, req.cldbid)
                .await?
        }
        other => return Err(AppError::Validation(format!("Invalid group action '{other}'"))),
    }
    let _ = state.hub.refresh_virtual_server(&id, sid, false).await;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "client.server_group_modify",
        &format!("cldbid:{}", req.cldbid),
        serde_json::json!({ "action": req.action, "sgid": req.sgid }),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn broadcast_message(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, sid)): Path<(String, u32)>,
    Json(req): Json<MsgReq>,
) -> ApiResult<StatusCode> {
    auth.require_instance_role(&state.db, &id, InstanceRole::Moderator)
        .await?;
    if req.msg.trim().is_empty() {
        return Err(AppError::Validation("Broadcast message cannot be empty".into()));
    }
    let drv = state.hub.driver_for(&id).await?;
    drv.send_text_message(sid, 3, sid, req.msg.trim()).await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        Some(&id),
        Some(sid as i32),
        "server.broadcast",
        &format!("sid:{sid}"),
        serde_json::json!({ "msg": req.msg.trim() }),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}
