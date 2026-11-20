//! Database backends for PolarisCal. Each backend implements [`polaris_core::Store`];
//! [`open`] picks one from a [`DatabaseConfig`] at runtime.
//!
//! Backends are behind cargo features so a build only links the drivers it needs.
//! SQLite ships first; PostgreSQL, MySQL/MariaDB, and SQL Server follow.

use std::sync::Arc;

use polaris_core::{Result, Store};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[cfg(feature = "sqlite")]
pub mod sqlite;

/// Where the data lives. Stored in the app's config file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "backend", rename_all = "snake_case")]
#[ts(export)]
pub enum DatabaseConfig {
    /// A local SQLite database file.
    Sqlite { path: String },
}

impl DatabaseConfig {
    /// A short description that is safe to show and log (no credentials).
    pub fn describe(&self) -> String {
        match self {
            DatabaseConfig::Sqlite { path } => format!("SQLite file {path}"),
        }
    }
}

/// Connects to the configured database and brings its schema up to date.
pub async fn open(config: &DatabaseConfig) -> Result<Arc<dyn Store>> {
    match config {
        #[cfg(feature = "sqlite")]
        DatabaseConfig::Sqlite { path } => Ok(Arc::new(
            sqlite::SqliteStore::open(std::path::Path::new(path)).await?,
        )),
        #[allow(unreachable_patterns)]
        other => Err(polaris_core::Error::Config(format!(
            "{} is not supported by this build of PolarisCal",
            other.describe()
        ))),
    }
}
