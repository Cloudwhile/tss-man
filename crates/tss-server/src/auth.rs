use crate::app_state::AppState;
use crate::entities::{audit_logs, user_instance_roles, users};
use crate::error::{ApiResult, AppError};
use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts},
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceRole {
    Auditor = 1,
    Moderator = 2,
    ServerAdmin = 3,
    SuperAdmin = 4,
}

impl InstanceRole {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auditor" => Some(Self::Auditor),
            "moderator" => Some(Self::Moderator),
            "server_admin" => Some(Self::ServerAdmin),
            "super_admin" => Some(Self::SuperAdmin),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auditor => "auditor",
            Self::Moderator => "moderator",
            Self::ServerAdmin => "server_admin",
            Self::SuperAdmin => "super_admin",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
    pub username: String,
    pub global_role: String,
    pub ip_addr: String,
}

impl AuthUser {
    pub fn is_super_admin(&self) -> bool {
        self.global_role == "super_admin"
    }

    pub fn require_super_admin(&self) -> ApiResult<()> {
        if self.is_super_admin() {
            Ok(())
        } else {
            Err(AppError::Forbidden(
                "Super administrator privilege required".to_string(),
            ))
        }
    }

    pub async fn role_for_instance(
        &self,
        db: &DatabaseConnection,
        instance_id: &str,
    ) -> ApiResult<Option<InstanceRole>> {
        if self.is_super_admin() {
            return Ok(Some(InstanceRole::SuperAdmin));
        }
        let found = user_instance_roles::Entity::find()
            .filter(user_instance_roles::Column::UserId.eq(&self.user_id))
            .filter(user_instance_roles::Column::InstanceId.eq(instance_id))
            .one(db)
            .await?;
        Ok(found.and_then(|m| InstanceRole::parse(&m.role)))
    }

    pub async fn require_instance_role(
        &self,
        db: &DatabaseConnection,
        instance_id: &str,
        min_role: InstanceRole,
    ) -> ApiResult<InstanceRole> {
        let Some(role) = self.role_for_instance(db, instance_id).await? else {
            return Err(AppError::Forbidden(
                "You do not have access to this TeamSpeak instance".to_string(),
            ));
        };
        if role < min_role {
            return Err(AppError::Forbidden(format!(
                "Requires at least '{}' role on this instance",
                min_role.as_str()
            )));
        }
        Ok(role)
    }

    pub async fn accessible_instance_ids(
        &self,
        db: &DatabaseConnection,
    ) -> ApiResult<Option<Vec<String>>> {
        if self.is_super_admin() {
            return Ok(None); // None = all instances
        }
        let rows = user_instance_roles::Entity::find()
            .filter(user_instance_roles::Column::UserId.eq(&self.user_id))
            .all(db)
            .await?;
        Ok(Some(rows.into_iter().map(|r| r.instance_id).collect()))
    }
}

pub fn extract_ip_from_headers(headers: &axum::http::HeaderMap) -> String {
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = xff.split(',').next() {
            let trimmed = first.trim();
            if !trimmed.is_empty() {
                return trimmed.chars().take(64).collect();
            }
        }
    }
    if let Some(real) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let trimmed = real.trim();
        if !trimmed.is_empty() {
            return trimmed.chars().take(64).collect();
        }
    }
    "local".to_string()
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth_hdr = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| AppError::Unauthorized("Missing Authorization header".to_string()))?;

        let token = auth_hdr
            .strip_prefix("Bearer ")
            .or_else(|| auth_hdr.strip_prefix("bearer "))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::Unauthorized("Invalid Bearer header".to_string()))?;

        let claims = state.crypto.verify_jwt(token, "access")?;

        let user = users::Entity::find_by_id(&claims.sub)
            .one(&state.db)
            .await?
            .ok_or_else(|| AppError::Unauthorized("User account no longer exists".to_string()))?;

        if !user.is_active {
            return Err(AppError::Unauthorized("Account is disabled".to_string()));
        }

        let ip_addr = extract_ip_from_headers(&parts.headers);
        Ok(AuthUser {
            user_id: user.id,
            username: user.username,
            global_role: user.global_role,
            ip_addr,
        })
    }
}

pub async fn write_audit_log(
    db: &DatabaseConnection,
    actor_user_id: Option<&str>,
    actor_username: &str,
    instance_id: Option<&str>,
    virtual_server_id: Option<i32>,
    action: &str,
    target: &str,
    detail: serde_json::Value,
    ip_addr: &str,
) {
    let model = audit_logs::ActiveModel {
        id: Set(Uuid::now_v7().to_string()),
        actor_user_id: Set(actor_user_id.map(str::to_string)),
        actor_username: Set(actor_username.to_string()),
        instance_id: Set(instance_id.map(str::to_string)),
        virtual_server_id: Set(virtual_server_id),
        action: Set(action.to_string()),
        target: Set(target.chars().take(128).collect()),
        detail_json: Set(detail.to_string()),
        ip_addr: Set(ip_addr.chars().take(64).collect()),
        created_at: Set(Utc::now().fixed_offset()),
    };
    if let Err(e) = model.insert(db).await {
        tracing::warn!("Failed to write audit log: {e}");
    }
}
