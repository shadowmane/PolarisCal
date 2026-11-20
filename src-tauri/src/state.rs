use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use polaris_core::Store;
use polaris_core::auth::User;
use polaris_core::model::AppSettings;

use crate::config;

/// The signed-in user on this computer. Lives only in memory: closing the app signs out.
#[derive(Debug, Clone)]
pub struct Session {
    pub user: User,
    pub last_activity: Instant,
    pub locked: bool,
}

/// Shared state handed to every command.
pub struct AppState {
    pub config_path: PathBuf,
    /// Credential-free description of the configured database.
    pub database_description: String,
    /// The open database, or why it could not be opened. The app still starts
    /// on failure so the UI can show the problem instead of crashing.
    pub store: Result<Arc<dyn Store>, String>,
    pub session: Mutex<Option<Session>>,
    /// Cached from the database settings so idle checks don't need a query.
    pub idle_lock_minutes: AtomicU32,
}

impl AppState {
    pub async fn initialize(config_dir: &Path, data_dir: &Path) -> Self {
        let (config, config_path) = match config::load_or_create(config_dir, data_dir) {
            Ok(loaded) => loaded,
            Err(message) => {
                return Self::new(
                    config_dir.join(config::CONFIG_FILE),
                    "unknown (config file problem)".into(),
                    Err(message),
                );
            }
        };
        let store = polaris_store::open(&config.database)
            .await
            .map_err(|e| e.to_string());
        let state = Self::new(config_path, config.database.describe(), store);
        if let Ok(store) = &state.store
            && let Ok(settings) = store.settings().await
        {
            state.set_idle_lock_minutes(settings.idle_lock_minutes);
        }
        state
    }

    pub fn new(
        config_path: PathBuf,
        database_description: String,
        store: Result<Arc<dyn Store>, String>,
    ) -> Self {
        Self {
            config_path,
            database_description,
            store,
            session: Mutex::new(None),
            idle_lock_minutes: AtomicU32::new(AppSettings::DEFAULT_IDLE_LOCK_MINUTES),
        }
    }

    pub fn idle_timeout(&self) -> Duration {
        Duration::from_secs(u64::from(self.idle_lock_minutes.load(Ordering::Relaxed)) * 60)
    }

    pub fn set_idle_lock_minutes(&self, minutes: u32) {
        self.idle_lock_minutes.store(minutes, Ordering::Relaxed);
    }

    /// Runs `f` with the session slot locked. Never hold this across an `.await`.
    pub fn with_session<R>(&self, f: impl FnOnce(&mut Option<Session>) -> R) -> R {
        let mut guard = self
            .session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut guard)
    }
}
