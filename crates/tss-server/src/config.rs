use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub listen_addr: String,
    pub database_url: String,
    pub data_dir: PathBuf,
    pub master_key_env: Option<String>,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        let data_dir = PathBuf::from(
            std::env::var("TSS_DATA_DIR").unwrap_or_else(|_| "./data".to_string()),
        );
        let default_db = format!(
            "sqlite://{}/tss-manager.db?mode=rwc",
            data_dir.to_string_lossy().replace('\\', "/")
        );
        let database_url = std::env::var("DATABASE_URL").unwrap_or(default_db);
        let listen_addr =
            std::env::var("TSS_LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
        let master_key_env = std::env::var("TSS_MASTER_KEY").ok();

        Self {
            listen_addr,
            database_url,
            data_dir,
            master_key_env,
        }
    }
}
