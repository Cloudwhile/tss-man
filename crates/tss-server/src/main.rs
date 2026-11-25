use std::sync::Arc;
use tracing::info;
use tss_server::config::ServerConfig;
use tss_server::crypto::CryptoContext;
use tss_server::{build_router, create_app_state, init_database, scheduler};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tss_server=info,tss_tsproto=info,tower_http=info".into()),
        )
        .init();

    let cfg = ServerConfig::from_env();
    info!("Starting TSS Manager on http://{}", cfg.listen_addr);

    let db = init_database(&cfg.database_url)
        .await
        .map_err(|e| format!("DB init error: {:?}", e.status_and_meta()))?;
    let crypto = Arc::new(
        CryptoContext::load_or_create(&cfg.data_dir, cfg.master_key_env.as_deref())
            .map_err(|e| format!("Crypto init error: {:?}", e.status_and_meta()))?,
    );

    let state = create_app_state(db, crypto)
        .await
        .map_err(|e| format!("App state error: {:?}", e.status_and_meta()))?;

    scheduler::start_scheduler(state.clone());
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(&cfg.listen_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
