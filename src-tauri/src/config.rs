//! The per-machine config file (`config.json` in the app config folder).

use std::fs;
use std::path::{Path, PathBuf};

use polaris_store::DatabaseConfig;
use serde::{Deserialize, Serialize};

pub const CONFIG_FILE: &str = "config.json";
pub const DEFAULT_DB_FILE: &str = "polariscal.db";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    pub database: DatabaseConfig,
}

impl AppConfig {
    pub fn default_for(data_dir: &Path) -> Self {
        Self {
            database: DatabaseConfig::Sqlite {
                path: data_dir.join(DEFAULT_DB_FILE).display().to_string(),
            },
        }
    }
}

/// Reads the config file, creating it with defaults on first run.
pub fn load_or_create(config_dir: &Path, data_dir: &Path) -> Result<(AppConfig, PathBuf), String> {
    let path = config_dir.join(CONFIG_FILE);
    if path.exists() {
        let text = fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let config = serde_json::from_str(&text)
            .map_err(|e| format!("{} is not valid: {e}", path.display()))?;
        return Ok((config, path));
    }

    let config = AppConfig::default_for(data_dir);
    fs::create_dir_all(config_dir)
        .map_err(|e| format!("cannot create {}: {e}", config_dir.display()))?;
    let text = serde_json::to_string_pretty(&config).expect("config serializes");
    fs::write(&path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok((config, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_run_writes_default_then_reads_it_back() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("config");
        let data_dir = dir.path().join("data");

        let (created, path) = load_or_create(&config_dir, &data_dir).unwrap();
        assert!(path.exists());
        let DatabaseConfig::Sqlite { path: db } = &created.database;
        assert!(db.ends_with(DEFAULT_DB_FILE));

        let (loaded, _) = load_or_create(&config_dir, &data_dir).unwrap();
        assert_eq!(loaded, created);
        let json = fs::read_to_string(path).unwrap();
        assert!(json.contains(r#""backend": "sqlite""#), "{json}");
    }

    #[test]
    fn invalid_file_is_reported_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(CONFIG_FILE), "{ not json").unwrap();
        let err = load_or_create(dir.path(), dir.path()).unwrap_err();
        assert!(err.contains("not valid"), "{err}");
        assert_eq!(
            fs::read_to_string(dir.path().join(CONFIG_FILE)).unwrap(),
            "{ not json"
        );
    }
}
