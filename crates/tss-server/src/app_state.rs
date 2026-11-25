use crate::crypto::CryptoContext;
use crate::state_hub::StateHub;
use sea_orm::DatabaseConnection;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub crypto: Arc<CryptoContext>,
    pub hub: Arc<StateHub>,
    pub http_client: reqwest::Client,
}
