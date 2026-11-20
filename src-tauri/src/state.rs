use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};
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

/// The database in use. Replaced as a whole when an admin changes the connection.
#[derive(Clone)]
pub struct Connection {
    /// Credential-free description of the configured database.
    pub description: String,
    /// The open database, or why it could not be opened. The app still starts
    /// on failure so the UI can show the problem instead of crashing.
    pub store: Result<Arc<dyn Store>, String>,
}

/// Shared state handed to every command.
pub struct AppState {
    pub config_path: PathBuf,
    connection: RwLock<Connection>,
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
        let (description, store) = match config.effective_database() {
            Ok(database) => (
                database.describe(),
                polaris_store::open(&database)
                    .await
                    .map_err(|e| e.to_string()),
            ),
            Err(message) => (
                format!(
                    "shared settings file {}",
                    config.shared_settings_file.as_deref().unwrap_or_default()
                ),
                Err(message),
            ),
        };
        let state = Self::new(config_path, description, store);
        state.refresh_idle_lock().await;
        state
    }

    pub fn new(
        config_path: PathBuf,
        description: String,
        store: Result<Arc<dyn Store>, String>,
    ) -> Self {
        Self {
            config_path,
            connection: RwLock::new(Connection { description, store }),
            session: Mutex::new(None),
            idle_lock_minutes: AtomicU32::new(AppSettings::DEFAULT_IDLE_LOCK_MINUTES),
        }
    }

    pub fn connection(&self) -> Connection {
        self.connection
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    pub fn store(&self) -> Result<Arc<dyn Store>, String> {
        self.connection().store
    }

    /// Switches to another database. Whoever was signed in is signed out, since
    /// accounts belong to a database.
    pub async fn replace_connection(&self, description: String, store: Arc<dyn Store>) {
        *self.connection.write().unwrap_or_else(|p| p.into_inner()) = Connection {
            description,
            store: Ok(store),
        };
        self.with_session(|slot| *slot = None);
        self.refresh_idle_lock().await;
    }

    async fn refresh_idle_lock(&self) {
        let minutes = match self.store() {
            Ok(store) => store.settings().await.ok().map(|s| s.idle_lock_minutes),
            Err(_) => None,
        };
        self.set_idle_lock_minutes(minutes.unwrap_or(AppSettings::DEFAULT_IDLE_LOCK_MINUTES));
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
