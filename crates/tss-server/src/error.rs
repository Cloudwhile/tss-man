use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use tss_tsproto::TsProtoError;

#[derive(Debug, Clone)]
pub enum AppError {
    Validation(String),
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    Conflict(String),
    RateLimited(String),
    UpstreamTs {
        ts_error_id: Option<u32>,
        detail: String,
    },
    Internal(String),
}

#[derive(Debug, Serialize)]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    pub problem_type: &'static str,
    pub title: &'static str,
    pub status: u16,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    pub code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts_error_id: Option<u32>,
}

impl AppError {
    pub fn status_and_meta(&self) -> (StatusCode, &'static str, &'static str, &'static str) {
        match self {
            AppError::Validation(_) => (
                StatusCode::BAD_REQUEST,
                "urn:tss-manager:problem:validation-error",
                "Invalid Request Parameters",
                "VALIDATION_ERROR",
            ),
            AppError::Unauthorized(_) => (
                StatusCode::UNAUTHORIZED,
                "urn:tss-manager:problem:unauthorized",
                "Authentication Required",
                "UNAUTHORIZED",
            ),
            AppError::Forbidden(_) => (
                StatusCode::FORBIDDEN,
                "urn:tss-manager:problem:forbidden",
                "Permission Denied",
                "FORBIDDEN",
            ),
            AppError::NotFound(_) => (
                StatusCode::NOT_FOUND,
                "urn:tss-manager:problem:not-found",
                "Resource Not Found",
                "NOT_FOUND",
            ),
            AppError::Conflict(_) => (
                StatusCode::CONFLICT,
                "urn:tss-manager:problem:conflict",
                "Resource State Conflict",
                "CONFLICT",
            ),
            AppError::RateLimited(_) => (
                StatusCode::TOO_MANY_REQUESTS,
                "urn:tss-manager:problem:rate-limited",
                "Too Many Requests",
                "RATE_LIMITED",
            ),
            AppError::UpstreamTs { .. } => (
                StatusCode::BAD_GATEWAY,
                "urn:tss-manager:problem:upstream-ts-error",
                "TeamSpeak Protocol Command Failed",
                "UPSTREAM_TS_ERROR",
            ),
            AppError::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "urn:tss-manager:problem:internal-error",
                "Internal Server Error",
                "INTERNAL_ERROR",
            ),
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Validation(m) => write!(f, "Validation error: {m}"),
            AppError::Unauthorized(m) => write!(f, "Unauthorized: {m}"),
            AppError::Forbidden(m) => write!(f, "Forbidden: {m}"),
            AppError::NotFound(m) => write!(f, "Not found: {m}"),
            AppError::Conflict(m) => write!(f, "Conflict: {m}"),
            AppError::RateLimited(m) => write!(f, "Rate limited: {m}"),
            AppError::UpstreamTs {
                ts_error_id: Some(id),
                detail,
            } => write!(f, "TeamSpeak upstream error (id={id}): {detail}"),
            AppError::UpstreamTs {
                ts_error_id: None,
                detail,
            } => write!(f, "TeamSpeak upstream error: {detail}"),
            AppError::Internal(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for AppError {}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, problem_type, title, code) = self.status_and_meta();
        let (detail, ts_error_id) = match &self {
            AppError::Validation(m)
            | AppError::Unauthorized(m)
            | AppError::Forbidden(m)
            | AppError::NotFound(m)
            | AppError::Conflict(m)
            | AppError::RateLimited(m) => (m.clone(), None),
            AppError::UpstreamTs { ts_error_id, detail } => (detail.clone(), *ts_error_id),
            AppError::Internal(m) => {
                tracing::error!("Internal error: {m}");
                ("An internal server error occurred".to_string(), None)
            }
        };

        let body = ProblemDetails {
            problem_type,
            title,
            status: status.as_u16(),
            detail,
            instance: None,
            code,
            ts_error_id,
        };

        let mut resp = (status, Json(body)).into_response();
        resp.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json; charset=utf-8"),
        );
        if status == StatusCode::UNAUTHORIZED {
            resp.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static("Bearer realm=\"tss-manager\""),
            );
        }
        resp
    }
}

impl From<sea_orm::DbErr> for AppError {
    fn from(e: sea_orm::DbErr) -> Self {
        let msg = e.to_string();
        if msg.contains("UNIQUE") || msg.contains("duplicate key") {
            AppError::Conflict("Record already exists".to_string())
        } else {
            AppError::Internal(msg)
        }
    }
}

impl From<TsProtoError> for AppError {
    fn from(e: TsProtoError) -> Self {
        match e {
            TsProtoError::QueryError { id, msg, extra_msg } => {
                let detail = match extra_msg {
                    Some(ex) if !ex.is_empty() => format!("{msg}: {ex} (error id={id})"),
                    _ => format!("{msg} (error id={id})"),
                };
                AppError::UpstreamTs {
                    ts_error_id: Some(id),
                    detail,
                }
            }
            other => AppError::UpstreamTs {
                ts_error_id: None,
                detail: other.to_string(),
            },
        }
    }
}

pub type ApiResult<T> = std::result::Result<T, AppError>;
