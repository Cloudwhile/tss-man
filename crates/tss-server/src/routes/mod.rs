pub mod governance;
pub mod instances;
pub mod ops;
pub mod permissions;
pub mod setup_auth;
pub mod workbench;

use crate::app_state::AppState;
use axum::{routing::get, Json, Router};

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .merge(setup_auth::router())
        .merge(instances::router())
        .merge(workbench::router())
        .merge(permissions::router())
        .merge(governance::router())
        .merge(ops::router())
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}
