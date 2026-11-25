pub mod app_state;
pub mod auth;
pub mod config;
pub mod crypto;
pub mod entities;
pub mod error;
pub mod routes;
pub mod scheduler;
pub mod state_hub;
pub mod static_assets;

use crate::app_state::AppState;
use crate::crypto::CryptoContext;
use crate::error::{ApiResult, AppError};
use crate::state_hub::StateHub;
use axum::Router;
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend};
use sea_orm_migration::MigratorTrait;
use std::path::Path;
use std::sync::Arc;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tss_migration::Migrator;

pub async fn init_database(database_url: &str) -> ApiResult<DatabaseConnection> {
    if let Some(rest) = database_url.strip_prefix("sqlite://") {
        let path_part = rest.split('?').next().unwrap_or("");
        if !path_part.is_empty() && path_part != ":memory:" {
            if let Some(parent) = Path::new(path_part).parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        AppError::Internal(format!("Failed to create SQLite dir: {e}"))
                    })?;
                }
            }
        }
    }

    let mut opt = ConnectOptions::new(database_url.to_string());
    opt.max_connections(16)
        .min_connections(1)
        .sqlx_logging(false);

    let db = Database::connect(opt)
        .await
        .map_err(|e| AppError::Internal(format!("Database connect failed: {e}")))?;

    if db.get_database_backend() == DbBackend::Sqlite {
        db.execute_unprepared(
            "PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON; PRAGMA synchronous = NORMAL; PRAGMA busy_timeout = 5000;",
        )
        .await
        .map_err(|e| AppError::Internal(format!("SQLite pragma failed: {e}")))?;
    }

    Migrator::up(&db, None)
        .await
        .map_err(|e| AppError::Internal(format!("Database migration failed: {e}")))?;

    Ok(db)
}

pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .nest("/api/v1", routes::api_router())
        .fallback(static_assets::serve_spa)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

pub async fn create_app_state(
    db: DatabaseConnection,
    crypto: Arc<CryptoContext>,
) -> ApiResult<AppState> {
    let hub = StateHub::new(db.clone(), crypto.clone());
    hub.bootstrap_all().await?;
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(format!("HTTP client init failed: {e}")))?;
    Ok(AppState {
        db,
        crypto,
        hub,
        http_client,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{header, Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use serde_json::{json, Value};
    use tower::ServiceExt;

    async fn send_json(
        app: &Router,
        method: &str,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, String, Value) {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(t) = token {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        let req = if let Some(b) = body {
            builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string()))
                .unwrap()
        } else {
            builder.body(Body::empty()).unwrap()
        };
        let resp = app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let ct = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let parsed = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, ct, parsed)
    }

    #[tokio::test]
    async fn e2e_setup_instance_snapshots_and_rbac() {
        let mock = tss_tsproto::MockTsServer::start()
            .await
            .expect("start test mock server");

        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let db_url = format!(
            "sqlite://{}?mode=rwc",
            db_path.to_string_lossy().replace('\\', "/")
        );
        let db = init_database(&db_url).await.unwrap();
        let crypto = Arc::new(CryptoContext::from_bytes([42u8; 32]));
        let state = create_app_state(db, crypto).await.unwrap();
        let app = build_router(state);

        // 1. Setup status is false & unauthenticated request returns 401
        let (st, _, body) = send_json(&app, "GET", "/api/v1/setup/status", None, None).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body["initialized"], json!(false));

        let (st, ct, err_body) = send_json(&app, "GET", "/api/v1/instances", None, None).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
        assert!(ct.contains("application/problem+json"));
        assert_eq!(err_body["code"], json!("UNAUTHORIZED"));

        // 2. Setup init creates initial super_admin
        let (st, _, init_resp) = send_json(
            &app,
            "POST",
            "/api/v1/setup/init",
            None,
            Some(json!({
                "username": "rootadmin",
                "display_name": "Root Admin",
                "password": "StrongPassword123!"
            })),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED);
        let admin_token = init_resp["access_token"].as_str().unwrap().to_string();

        // Setup cannot be run twice
        let (st, ct, _) = send_json(
            &app,
            "POST",
            "/api/v1/setup/init",
            None,
            Some(json!({
                "username": "other",
                "password": "StrongPassword123!"
            })),
        )
        .await;
        assert_eq!(st, StatusCode::CONFLICT);
        assert!(ct.contains("application/problem+json"));

        // 3. Connect test MockTsServer via standard POST /api/v1/instances
        let (st, _, created_inst) = send_json(
            &app,
            "POST",
            "/api/v1/instances",
            Some(&admin_token),
            Some(json!({
                "name": "Test TS Node",
                "protocol": "raw_tcp",
                "host": mock.host(),
                "port": mock.port(),
                "username": mock.username(),
                "secret": mock.password(),
                "default_virtual_server_id": 1,
                "tls_verify": false,
                "enabled": true
            })),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED);
        let inst_id = created_inst["id"].as_str().unwrap().to_string();

        let (st, _, insts) =
            send_json(&app, "GET", "/api/v1/instances", Some(&admin_token), None).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(insts.as_array().unwrap().len(), 1);

        // Fetch live tree
        let (st, _, tree) = send_json(
            &app,
            "GET",
            &format!("/api/v1/instances/{inst_id}/servers/1/tree"),
            Some(&admin_token),
            None,
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(tree["channels"].as_array().unwrap().len(), 6);
        assert_eq!(tree["clients"].as_array().unwrap().len(), 8);

        // 4. Snapshot create -> create channel -> diff -> rollback deploy
        let (st, _, snap) = send_json(
            &app,
            "POST",
            &format!("/api/v1/instances/{inst_id}/servers/1/snapshots"),
            Some(&admin_token),
            Some(json!({ "name": "Baseline V1" })),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED);
        let snap_id = snap["id"].as_str().unwrap().to_string();

        let (st, _, _) = send_json(
            &app,
            "POST",
            &format!("/api/v1/instances/{inst_id}/servers/1/channels"),
            Some(&admin_token),
            Some(json!({ "name": "Temp Diff Channel", "is_permanent": true })),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED);

        let (st, _, diff) = send_json(
            &app,
            "GET",
            &format!("/api/v1/instances/{inst_id}/servers/1/snapshots/{snap_id}/diff?against=live"),
            Some(&admin_token),
            None,
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert!(diff["channels_added_in_target"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "Temp Diff Channel"));

        let (st, _, deploy) = send_json(
            &app,
            "POST",
            &format!("/api/v1/instances/{inst_id}/servers/1/snapshots/{snap_id}/deploy"),
            Some(&admin_token),
            None,
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(deploy["deployed"], json!(true));

        let (st, _, tree_after) = send_json(
            &app,
            "GET",
            &format!("/api/v1/instances/{inst_id}/servers/1/tree"),
            Some(&admin_token),
            None,
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(tree_after["channels"].as_array().unwrap().len(), 6);

        // 5. RBAC verification: create moderator user assigned to sandbox
        let (st, _, _) = send_json(
            &app,
            "POST",
            "/api/v1/users",
            Some(&admin_token),
            Some(json!({
                "username": "mod_alice",
                "display_name": "Alice Mod",
                "password": "ModPassword123!",
                "global_role": "member",
                "instance_roles": [{ "instance_id": inst_id, "role": "moderator" }]
            })),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED);

        let (st, _, mod_login) = send_json(
            &app,
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({
                "username": "mod_alice",
                "password": "ModPassword123!"
            })),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let mod_token = mod_login["access_token"].as_str().unwrap().to_string();

        // Moderator can poke client
        let (st, _, _) = send_json(
            &app,
            "POST",
            &format!("/api/v1/instances/{inst_id}/servers/1/clients/1/poke"),
            Some(&mod_token),
            Some(json!({ "msg": "Ready check" })),
        )
        .await;
        assert_eq!(st, StatusCode::NO_CONTENT);

        // Moderator CANNOT create snapshot (requires server_admin)
        let (st, ct, err_mod) = send_json(
            &app,
            "POST",
            &format!("/api/v1/instances/{inst_id}/servers/1/snapshots"),
            Some(&mod_token),
            Some(json!({ "name": "Forbidden Snap" })),
        )
        .await;
        assert_eq!(st, StatusCode::FORBIDDEN);
        assert!(ct.contains("application/problem+json"));
        assert_eq!(err_mod["code"], json!("FORBIDDEN"));

        // Moderator CANNOT access /users (requires super_admin)
        let (st, _, _) = send_json(&app, "GET", "/api/v1/users", Some(&mod_token), None).await;
        assert_eq!(st, StatusCode::FORBIDDEN);
    }
}
