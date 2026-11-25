use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub listen_addr: String,
    pub database_url: String,
    pub data_dir: PathBuf,
    pub master_key_env: Option<String>,
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

impl ServerConfig {
    pub fn from_env() -> Self {
        let data_dir = PathBuf::from(
            non_empty_env("TSS_DATA_DIR").unwrap_or_else(|| "./data".to_string()),
        );
        let default_db = format!(
            "sqlite://{}/tss-manager.db?mode=rwc",
            data_dir.to_string_lossy().replace('\\', "/")
        );
        let database_url = non_empty_env("DATABASE_URL").unwrap_or(default_db);
        let listen_addr =
            non_empty_env("TSS_LISTEN_ADDR").unwrap_or_else(|| "0.0.0.0:3000".to_string());
        let master_key_env = non_empty_env("TSS_MASTER_KEY");

        Self {
            listen_addr,
            database_url,
            data_dir,
            master_key_env,
        }
    }
}
