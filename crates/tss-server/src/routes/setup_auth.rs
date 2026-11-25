use crate::app_state::AppState;
use crate::auth::{extract_ip_from_headers, write_audit_log, AuthUser};
use crate::crypto::sha256_hex;
use crate::entities::{refresh_tokens, user_instance_roles, users};
use crate::error::{ApiResult, AppError};
use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/setup/status", get(setup_status))
        .route("/setup/init", post(setup_init))
        .route("/auth/login", post(login))
        .route("/auth/login/totp", post(login_totp))
        .route("/auth/refresh", post(refresh_token))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/totp/setup", post(totp_setup))
        .route("/auth/totp/enable", post(totp_enable))
        .route("/auth/totp/disable", post(totp_disable))
        .route("/auth/password", put(change_password))
}

const REFRESH_COOKIE_NAME: &str = "tss_refresh";

#[derive(Serialize)]
struct SetupStatusResp {
    initialized: bool,
}

#[derive(Deserialize)]
struct SetupInitReq {
    username: String,
    display_name: Option<String>,
    password: String,
}

#[derive(Deserialize)]
struct LoginReq {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct LoginTotpReq {
    mfa_ticket: String,
    code: String,
}

#[derive(Serialize)]
struct AuthUserView {
    id: String,
    username: String,
    display_name: String,
    global_role: String,
    totp_enabled: bool,
    instance_roles: Vec<InstanceRoleBinding>,
}

#[derive(Serialize)]
struct InstanceRoleBinding {
    instance_id: String,
    role: String,
}

#[derive(Serialize)]
struct LoginResp {
    mfa_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    mfa_ticket: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    access_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<AuthUserView>,
}

#[derive(Deserialize)]
struct TotpEnableReq {
    secret_base32: String,
    code: String,
}

#[derive(Deserialize)]
struct TotpDisableReq {
    password: String,
}

#[derive(Deserialize)]
struct ChangePasswordReq {
    current_password: String,
    new_password: String,
}

async fn build_user_view(state: &AppState, user: &users::Model) -> ApiResult<AuthUserView> {
    let roles = user_instance_roles::Entity::find()
        .filter(user_instance_roles::Column::UserId.eq(&user.id))
        .all(&state.db)
        .await?;
    Ok(AuthUserView {
        id: user.id.clone(),
        username: user.username.clone(),
        display_name: user.display_name.clone(),
        global_role: user.global_role.clone(),
        totp_enabled: user.totp_enabled,
        instance_roles: roles
            .into_iter()
            .map(|r| InstanceRoleBinding {
                instance_id: r.instance_id,
                role: r.role,
            })
            .collect(),
    })
}

async fn issue_session(
    state: &AppState,
    user: &users::Model,
    headers: &HeaderMap,
) -> ApiResult<(String, HeaderValue, AuthUserView)> {
    let access_token = state
        .crypto
        .issue_access_jwt(&user.id, &user.username, &user.global_role)?;
    let (raw_refresh, hash) = state.crypto.generate_refresh_token();
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .chars()
        .take(255)
        .collect::<String>();
    let ip = extract_ip_from_headers(headers);
    let now = Utc::now().fixed_offset();

    refresh_tokens::ActiveModel {
        id: Set(Uuid::now_v7().to_string()),
        user_id: Set(user.id.clone()),
        token_hash: Set(hash),
        user_agent: Set(ua),
        ip_addr: Set(ip),
        expires_at: Set(now + Duration::days(7)),
        revoked_at: Set(None),
        created_at: Set(now),
    }
    .insert(&state.db)
    .await?;

    let mut active_user: users::ActiveModel = user.clone().into();
    active_user.last_login_at = Set(Some(now));
    active_user.updated_at = Set(now);
    let updated_user = active_user.update(&state.db).await?;

    let cookie_val = format!(
        "{REFRESH_COOKIE_NAME}={raw_refresh}; HttpOnly; SameSite=Lax; Path=/api/v1/auth; Max-Age=604800"
    );
    let cookie = HeaderValue::from_str(&cookie_val)
        .map_err(|e| AppError::Internal(format!("Invalid cookie header: {e}")))?;
    let view = build_user_view(state, &updated_user).await?;
    Ok((access_token, cookie, view))
}

fn extract_refresh_cookie(headers: &HeaderMap) -> Option<String> {
    let cookie_hdr = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie_hdr.split(';') {
        let trimmed = part.trim();
        if let Some(val) = trimmed.strip_prefix(&format!("{REFRESH_COOKIE_NAME}=")) {
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    None
}

async fn setup_status(State(state): State<AppState>) -> ApiResult<Json<SetupStatusResp>> {
    let count = users::Entity::find().count(&state.db).await?;
    Ok(Json(SetupStatusResp {
        initialized: count > 0,
    }))
}

async fn setup_init(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SetupInitReq>,
) -> ApiResult<impl IntoResponse> {
    let existing = users::Entity::find().count(&state.db).await?;
    if existing > 0 {
        return Err(AppError::Conflict(
            "System is already initialized; setup wizard is locked".to_string(),
        ));
    }
    let username = req.username.trim();
    if username.len() < 3 || username.len() > 64 {
        return Err(AppError::Validation(
            "Username must be 3 to 64 characters".to_string(),
        ));
    }
    if req.password.len() < 8 {
        return Err(AppError::Validation(
            "Password must be at least 8 characters".to_string(),
        ));
    }

    let now = Utc::now().fixed_offset();
    let password_hash = state.crypto.hash_password(&req.password)?;
    let display = req
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(username)
        .to_string();

    let user = users::ActiveModel {
        id: Set(Uuid::now_v7().to_string()),
        username: Set(username.to_string()),
        display_name: Set(display),
        password_hash: Set(password_hash),
        totp_secret_enc: Set(None),
        totp_enabled: Set(false),
        global_role: Set("super_admin".to_string()),
        is_active: Set(true),
        last_login_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&state.db)
    .await?;

    let ip = extract_ip_from_headers(&headers);
    write_audit_log(
        &state.db,
        Some(&user.id),
        &user.username,
        None,
        None,
        "setup.init",
        &user.username,
        serde_json::json!({}),
        &ip,
    )
    .await;

    let (access_token, cookie, view) = issue_session(&state, &user, &headers).await?;
    let mut resp = (
        StatusCode::CREATED,
        Json(LoginResp {
            mfa_required: false,
            mfa_ticket: None,
            access_token: Some(access_token),
            user: Some(view),
        }),
    )
        .into_response();
    resp.headers_mut().insert(header::SET_COOKIE, cookie);
    Ok(resp)
}

async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LoginReq>,
) -> ApiResult<impl IntoResponse> {
    let user = users::Entity::find()
        .filter(users::Column::Username.eq(req.username.trim()))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Invalid username or password".to_string()))?;

    if !user.is_active {
        return Err(AppError::Unauthorized("Account is disabled".to_string()));
    }

    if !state.crypto.verify_password(&req.password, &user.password_hash) {
        return Err(AppError::Unauthorized(
            "Invalid username or password".to_string(),
        ));
    }

    if user.totp_enabled {
        let mfa_ticket =
            state
                .crypto
                .issue_mfa_ticket(&user.id, &user.username, &user.global_role)?;
        return Ok(Json(LoginResp {
            mfa_required: true,
            mfa_ticket: Some(mfa_ticket),
            access_token: None,
            user: None,
        })
        .into_response());
    }

    let ip = extract_ip_from_headers(&headers);
    write_audit_log(
        &state.db,
        Some(&user.id),
        &user.username,
        None,
        None,
        "auth.login",
        &user.username,
        serde_json::json!({ "mfa": false }),
        &ip,
    )
    .await;

    let (access_token, cookie, view) = issue_session(&state, &user, &headers).await?;
    let mut resp = Json(LoginResp {
        mfa_required: false,
        mfa_ticket: None,
        access_token: Some(access_token),
        user: Some(view),
    })
    .into_response();
    resp.headers_mut().insert(header::SET_COOKIE, cookie);
    Ok(resp)
}

async fn login_totp(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LoginTotpReq>,
) -> ApiResult<impl IntoResponse> {
    let claims = state.crypto.verify_jwt(&req.mfa_ticket, "mfa_pending")?;
    let user = users::Entity::find_by_id(&claims.sub)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("User not found".to_string()))?;

    let enc_secret = user
        .totp_secret_enc
        .as_deref()
        .ok_or_else(|| AppError::Unauthorized("TOTP is not configured".to_string()))?;
    let secret = state.crypto.decrypt_secret(enc_secret)?;
    if !state
        .crypto
        .verify_totp_code(&secret, &user.username, &req.code)?
    {
        return Err(AppError::Unauthorized("Invalid 2FA verification code".to_string()));
    }

    let ip = extract_ip_from_headers(&headers);
    write_audit_log(
        &state.db,
        Some(&user.id),
        &user.username,
        None,
        None,
        "auth.login",
        &user.username,
        serde_json::json!({ "mfa": true }),
        &ip,
    )
    .await;

    let (access_token, cookie, view) = issue_session(&state, &user, &headers).await?;
    let mut resp = Json(LoginResp {
        mfa_required: false,
        mfa_ticket: None,
        access_token: Some(access_token),
        user: Some(view),
    })
    .into_response();
    resp.headers_mut().insert(header::SET_COOKIE, cookie);
    Ok(resp)
}

async fn refresh_token(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<impl IntoResponse> {
    let raw = extract_refresh_cookie(&headers)
        .ok_or_else(|| AppError::Unauthorized("Missing refresh token cookie".to_string()))?;
    let hash = sha256_hex(raw.as_bytes());
    let record = refresh_tokens::Entity::find()
        .filter(refresh_tokens::Column::TokenHash.eq(&hash))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Invalid refresh token".to_string()))?;

    let now = Utc::now().fixed_offset();
    if record.revoked_at.is_some() || record.expires_at <= now {
        return Err(AppError::Unauthorized(
            "Refresh token expired or revoked".to_string(),
        ));
    }

    let user = users::Entity::find_by_id(&record.user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("User not found".to_string()))?;
    if !user.is_active {
        return Err(AppError::Unauthorized("Account is disabled".to_string()));
    }

    // Revoke old refresh token (rotation)
    let mut active_rt: refresh_tokens::ActiveModel = record.into();
    active_rt.revoked_at = Set(Some(now));
    active_rt.update(&state.db).await?;

    let (access_token, cookie, view) = issue_session(&state, &user, &headers).await?;
    let mut resp = Json(LoginResp {
        mfa_required: false,
        mfa_ticket: None,
        access_token: Some(access_token),
        user: Some(view),
    })
    .into_response();
    resp.headers_mut().insert(header::SET_COOKIE, cookie);
    Ok(resp)
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<impl IntoResponse> {
    if let Some(raw) = extract_refresh_cookie(&headers) {
        let hash = sha256_hex(raw.as_bytes());
        if let Some(record) = refresh_tokens::Entity::find()
            .filter(refresh_tokens::Column::TokenHash.eq(&hash))
            .one(&state.db)
            .await?
        {
            let mut active: refresh_tokens::ActiveModel = record.into();
            active.revoked_at = Set(Some(Utc::now().fixed_offset()));
            let _ = active.update(&state.db).await;
        }
    }
    let mut resp = StatusCode::NO_CONTENT.into_response();
    resp.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static(
            "tss_refresh=; HttpOnly; SameSite=Lax; Path=/api/v1/auth; Max-Age=0",
        ),
    );
    Ok(resp)
}

async fn me(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<AuthUserView>> {
    let user = users::Entity::find_by_id(&auth.user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;
    Ok(Json(build_user_view(&state, &user).await?))
}

async fn totp_setup(
    State(state): State<AppState>,
    auth: AuthUser,
) -> ApiResult<Json<crate::crypto::TotpSetupInfo>> {
    let info = state.crypto.generate_totp_setup(&auth.username)?;
    Ok(Json(info))
}

async fn totp_enable(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<TotpEnableReq>,
) -> ApiResult<StatusCode> {
    if !state
        .crypto
        .verify_totp_code(&req.secret_base32, &auth.username, &req.code)?
    {
        return Err(AppError::Validation(
            "Invalid verification code for TOTP secret".to_string(),
        ));
    }
    let user = users::Entity::find_by_id(&auth.user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;
    let enc = state.crypto.encrypt_secret(req.secret_base32.trim())?;
    let mut active: users::ActiveModel = user.into();
    active.totp_secret_enc = Set(Some(enc));
    active.totp_enabled = Set(true);
    active.updated_at = Set(Utc::now().fixed_offset());
    active.update(&state.db).await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        None,
        None,
        "auth.totp_enabled",
        &auth.username,
        serde_json::json!({}),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn totp_disable(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<TotpDisableReq>,
) -> ApiResult<StatusCode> {
    let user = users::Entity::find_by_id(&auth.user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;
    if !state.crypto.verify_password(&req.password, &user.password_hash) {
        return Err(AppError::Unauthorized("Current password is incorrect".to_string()));
    }
    let mut active: users::ActiveModel = user.into();
    active.totp_secret_enc = Set(None);
    active.totp_enabled = Set(false);
    active.updated_at = Set(Utc::now().fixed_offset());
    active.update(&state.db).await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        None,
        None,
        "auth.totp_disabled",
        &auth.username,
        serde_json::json!({}),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn change_password(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<ChangePasswordReq>,
) -> ApiResult<StatusCode> {
    if req.new_password.len() < 8 {
        return Err(AppError::Validation(
            "New password must be at least 8 characters".to_string(),
        ));
    }
    let user = users::Entity::find_by_id(&auth.user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;
    if !state
        .crypto
        .verify_password(&req.current_password, &user.password_hash)
    {
        return Err(AppError::Unauthorized(
            "Current password is incorrect".to_string(),
        ));
    }
    let new_hash = state.crypto.hash_password(&req.new_password)?;
    let mut active: users::ActiveModel = user.into();
    active.password_hash = Set(new_hash);
    active.updated_at = Set(Utc::now().fixed_offset());
    active.update(&state.db).await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        &auth.username,
        None,
        None,
        "auth.password_changed",
        &auth.username,
        serde_json::json!({}),
        &auth.ip_addr,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}
