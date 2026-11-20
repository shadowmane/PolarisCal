//! Where to find the database.
//!
//! Each computer has a `config.json` in its app config folder. It either holds the
//! connection itself or points at a **shared settings file** (usually on a network
//! share) that IT maintains once for everyone:
//!
//! ```json
//! { "shared_settings_file": "\\\\server\\share\\PolarisCal\\polariscal-connection.json",
//!   "database": { "backend": "sqlite", "path": "C:\\...\\polariscal.db" } }
//! ```
//!
//! While `shared_settings_file` is set, the connection in that file is used and the
//! local `database` is ignored (it is kept so switching back is easy).

use std::fs;
use std::path::{Path, PathBuf};

use polaris_store::DatabaseConfig;
use serde::{Deserialize, Serialize};

pub const CONFIG_FILE: &str = "config.json";
pub const DEFAULT_DB_FILE: &str = "polariscal.db";

/// This computer's settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_settings_file: Option<String>,
    pub database: DatabaseConfig,
}

/// The contents of a shared settings file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedSettings {
    #[serde(rename = "_about", default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    pub database: DatabaseConfig,
}

const ABOUT: &str = "PolarisCal shared connection settings. Each computer points at this file \
                     (Settings > Database connection). A relative SQLite path is relative to this file.";

impl SharedSettings {
    pub fn new(database: DatabaseConfig) -> Self {
        Self {
            about: Some(ABOUT.into()),
            database,
        }
    }
}

impl AppConfig {
    pub fn default_for(data_dir: &Path) -> Self {
        Self {
            shared_settings_file: None,
            database: DatabaseConfig::Sqlite {
                path: data_dir.join(DEFAULT_DB_FILE).display().to_string(),
            },
        }
    }

    /// The connection to use: from the shared file when one is set.
    pub fn effective_database(&self) -> Result<DatabaseConfig, String> {
        match &self.shared_settings_file {
            Some(path) => Ok(read_shared(Path::new(path))?.database),
            None => Ok(self.database.clone()),
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
    save(&path, &config)?;
    Ok((config, path))
}

/// Writes JSON next to the target first, then renames it into place, so a
/// failure never leaves a half-written file.
fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).expect("settings serialize");
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("cannot replace {}: {e}", path.display())
    })
}

pub fn save(path: &Path, config: &AppConfig) -> Result<(), String> {
    write_json(path, config)
}

/// Reads a shared settings file. A relative SQLite path is taken relative to the file.
pub fn read_shared(path: &Path) -> Result<SharedSettings, String> {
    let text = fs::read_to_string(path).map_err(|e| {
        format!(
            "The shared settings file {} can't be read: {e}",
            path.display()
        )
    })?;
    let mut shared: SharedSettings = serde_json::from_str(&text).map_err(|e| {
        format!(
            "The shared settings file {} isn't valid PolarisCal settings: {e}",
            path.display()
        )
    })?;
    if let DatabaseConfig::Sqlite { path: db } = &mut shared.database
        && Path::new(db).is_relative()
        && let Some(folder) = path.parent()
    {
        *db = folder.join(&*db).display().to_string();
    }
    Ok(shared)
}

pub fn write_shared(path: &Path, settings: &SharedSettings) -> Result<(), String> {
    if let Some(folder) = path.parent().filter(|p| !p.as_os_str().is_empty())
        && !folder.exists()
    {
        return Err(format!(
            "The folder {} doesn't exist or can't be reached.",
            folder.display()
        ));
    }
    write_json(path, settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use polaris_store::{Encryption, ServerAuth, ServerConfig};

    #[test]
    fn first_run_writes_default_then_reads_it_back() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("config");
        let data_dir = dir.path().join("data");

        let (created, path) = load_or_create(&config_dir, &data_dir).unwrap();
        assert!(path.exists());
        let DatabaseConfig::Sqlite { path: db } = &created.database else {
            panic!("default is SQLite")
        };
        assert!(db.ends_with(DEFAULT_DB_FILE));

        let (loaded, _) = load_or_create(&config_dir, &data_dir).unwrap();
        assert_eq!(loaded, created);
        let json = fs::read_to_string(path).unwrap();
        assert!(json.contains(r#""backend": "sqlite""#), "{json}");
        assert!(!json.contains("shared_settings_file"), "{json}");
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

    #[test]
    fn shared_file_overrides_the_local_connection() {
        let dir = tempfile::tempdir().unwrap();
        let shared_path = dir.path().join("polariscal-connection.json");
        let server = DatabaseConfig::Postgres(ServerConfig {
            host: "db.example.local".into(),
            port: None,
            instance: None,
            database: "polariscal".into(),
            auth: ServerAuth::Password {
                username: "polaris".into(),
                password: Some("secret".into()),
            },
            encryption: Encryption::Require,
        });
        write_shared(&shared_path, &SharedSettings::new(server.clone())).unwrap();

        let mut config = AppConfig::default_for(dir.path());
        assert_eq!(config.effective_database().unwrap(), config.database);
        config.shared_settings_file = Some(shared_path.display().to_string());
        assert_eq!(config.effective_database().unwrap(), server);

        let text = fs::read_to_string(&shared_path).unwrap();
        assert!(text.contains("_about"), "{text}");
    }

    #[test]
    fn relative_sqlite_paths_in_a_shared_file_are_beside_it() {
        let dir = tempfile::tempdir().unwrap();
        let shared_path = dir.path().join("shared.json");
        fs::write(
            &shared_path,
            r#"{ "database": { "backend": "sqlite", "path": "polariscal.db" } }"#,
        )
        .unwrap();
        let DatabaseConfig::Sqlite { path } = read_shared(&shared_path).unwrap().database else {
            panic!("sqlite")
        };
        assert_eq!(Path::new(&path), dir.path().join("polariscal.db"));
    }

    #[test]
    fn missing_shared_file_is_explained() {
        let dir = tempfile::tempdir().unwrap();
        let config = AppConfig {
            shared_settings_file: Some(dir.path().join("nope.json").display().to_string()),
            database: AppConfig::default_for(dir.path()).database,
        };
        let err = config.effective_database().unwrap_err();
        assert!(err.contains("can't be read"), "{err}");
    }
}
