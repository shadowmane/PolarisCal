use std::path::{Path, PathBuf};
use std::sync::Arc;

use polaris_core::Store;

use crate::config;

/// Shared state handed to every command.
pub struct AppState {
    pub config_path: PathBuf,
    /// Credential-free description of the configured database.
    pub database_description: String,
    /// The open database, or why it could not be opened. The app still starts
    /// on failure so the UI can show the problem instead of crashing.
    pub store: Result<Arc<dyn Store>, String>,
}

impl AppState {
    pub async fn initialize(config_dir: &Path, data_dir: &Path) -> Self {
        let (config, config_path) = match config::load_or_create(config_dir, data_dir) {
            Ok(loaded) => loaded,
            Err(message) => {
                return Self {
                    config_path: config_dir.join(config::CONFIG_FILE),
                    database_description: "unknown (config file problem)".into(),
                    store: Err(message),
                };
            }
        };
        let store = polaris_store::open(&config.database)
            .await
            .map_err(|e| e.to_string());
        Self {
            config_path,
            database_description: config.database.describe(),
            store,
        }
    }
}
