//! Database backends for PolarisCal.
//!
//! One implementation of [`polaris_core::Store`], [`store::SqlStore`], serves every
//! supported database: a local SQLite file, PostgreSQL, MySQL/MariaDB, and SQL
//! Server. Queries are written once in a portable dialect (see [`sql`]) and adjusted
//! per server. [`open`] connects using a [`DatabaseConfig`] and upgrades the schema.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use polaris_core::{Error, Result, Store};
use serde::{Deserialize, Serialize};
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions, MySqlSslMode};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use ts_rs::TS;

mod copy;
mod sql;
pub mod store;
mod transfer;

pub use copy::copy_database;

const POOL_SIZE: u32 = 5;
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(15);

/// Where the data lives. Stored in the app's config file or a shared settings file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "backend", rename_all = "snake_case")]
#[ts(export)]
pub enum DatabaseConfig {
    /// A SQLite database file on this computer or a network share.
    Sqlite { path: String },
    /// A PostgreSQL server.
    Postgres(ServerConfig),
    /// A MySQL or MariaDB server.
    Mysql(ServerConfig),
    /// A Microsoft SQL Server (or Azure SQL) server.
    SqlServer(ServerConfig),
}

/// Connection details for a database server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ServerConfig {
    pub host: String,
    /// Leave empty for the server's standard port.
    #[serde(default)]
    pub port: Option<u16>,
    /// SQL Server named instance (e.g. `SQLEXPRESS`); found through SQL Server Browser.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    pub database: String,
    pub auth: ServerAuth,
    #[serde(default)]
    pub encryption: Encryption,
}

/// How PolarisCal signs in to a database server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "method", rename_all = "snake_case")]
#[ts(export)]
pub enum ServerAuth {
    /// A database account. Without a password, PostgreSQL looks in the user's
    /// `pgpass.conf`; the other servers sign in with an empty password.
    Password {
        username: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        password: Option<String>,
    },
    /// The Windows account of whoever is running PolarisCal (SQL Server only).
    Windows,
}

/// Whether the connection is encrypted, and whether the server's certificate must be trusted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Encryption {
    /// Never encrypt.
    Off,
    /// Encrypt when the server supports it, without checking its certificate.
    #[default]
    Prefer,
    /// Always encrypt, without checking the certificate.
    Require,
    /// Always encrypt and require a certificate this computer trusts.
    Verify,
}

impl DatabaseConfig {
    /// A short description that is safe to show and log (no credentials).
    pub fn describe(&self) -> String {
        match self {
            DatabaseConfig::Sqlite { path } => format!("SQLite file {path}"),
            DatabaseConfig::Postgres(s) => format!("PostgreSQL {}", s.location()),
            DatabaseConfig::Mysql(s) => format!("MySQL/MariaDB {}", s.location()),
            DatabaseConfig::SqlServer(s) => format!("SQL Server {}", s.location()),
        }
    }

    /// The same name [`StoreInfo::backend`](polaris_core::model::StoreInfo) uses.
    pub fn backend_name(&self) -> &'static str {
        match self {
            DatabaseConfig::Sqlite { .. } => "sqlite",
            DatabaseConfig::Postgres(_) => "postgres",
            DatabaseConfig::Mysql(_) => "mysql",
            DatabaseConfig::SqlServer(_) => "mssql",
        }
    }

    pub fn server(&self) -> Option<&ServerConfig> {
        match self {
            DatabaseConfig::Sqlite { .. } => None,
            DatabaseConfig::Postgres(s)
            | DatabaseConfig::Mysql(s)
            | DatabaseConfig::SqlServer(s) => Some(s),
        }
    }

    pub fn server_mut(&mut self) -> Option<&mut ServerConfig> {
        match self {
            DatabaseConfig::Sqlite { .. } => None,
            DatabaseConfig::Postgres(s)
            | DatabaseConfig::Mysql(s)
            | DatabaseConfig::SqlServer(s) => Some(s),
        }
    }

    /// Trims fields and checks the combination makes sense, reporting problems per field.
    pub fn normalize(&mut self) -> Result<()> {
        let mut errors = Vec::new();
        let is_sql_server = matches!(self, DatabaseConfig::SqlServer(_));
        match self {
            DatabaseConfig::Sqlite { path } => {
                *path = path.trim().to_string();
                if path.is_empty() {
                    errors.push(("path", "Choose where the database file is."));
                }
            }
            DatabaseConfig::Postgres(s)
            | DatabaseConfig::Mysql(s)
            | DatabaseConfig::SqlServer(s) => {
                s.host = s.host.trim().to_string();
                s.database = s.database.trim().to_string();
                s.instance = s
                    .instance
                    .take()
                    .map(|i| i.trim().to_string())
                    .filter(|i| !i.is_empty());
                if !is_sql_server {
                    s.instance = None;
                }
                if s.host.is_empty() {
                    errors.push(("host", "Enter the server name or address."));
                }
                if s.database.is_empty() {
                    errors.push(("database", "Enter the database name."));
                }
                if s.port == Some(0) {
                    errors.push((
                        "port",
                        "Enter a port between 1 and 65535, or leave it empty.",
                    ));
                }
                match &mut s.auth {
                    ServerAuth::Password { username, password } => {
                        *username = username.trim().to_string();
                        if username.is_empty() {
                            errors.push(("username", "Enter the database account name."));
                        }
                        if password.as_deref() == Some("") {
                            *password = None;
                        }
                    }
                    ServerAuth::Windows if !is_sql_server => errors.push((
                        "auth",
                        "Windows sign-in is only available for SQL Server. Use a database account.",
                    )),
                    ServerAuth::Windows if !cfg!(windows) => errors.push((
                        "auth",
                        "Windows sign-in only works when PolarisCal runs on Windows.",
                    )),
                    ServerAuth::Windows => {}
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(Error::Validation(
                errors
                    .into_iter()
                    .map(|(field, message)| polaris_core::FieldError {
                        field: field.into(),
                        message: message.into(),
                    })
                    .collect(),
            ))
        }
    }

    /// The same configuration with any password removed (for showing in the UI).
    pub fn without_password(&self) -> Self {
        let mut copy = self.clone();
        if let Some(ServerConfig {
            auth: ServerAuth::Password { password, .. },
            ..
        }) = copy.server_mut()
        {
            *password = None;
        }
        copy
    }

    pub fn has_password(&self) -> bool {
        matches!(
            self.server(),
            Some(ServerConfig {
                auth: ServerAuth::Password {
                    password: Some(_),
                    ..
                },
                ..
            })
        )
    }
}

impl ServerConfig {
    /// `host[\instance][:port]/database`.
    pub fn location(&self) -> String {
        let mut out = self.host.clone();
        if let Some(instance) = &self.instance {
            out.push('\\');
            out.push_str(instance);
        }
        if let Some(port) = self.port {
            out.push_str(&format!(":{port}"));
        }
        out.push('/');
        out.push_str(&self.database);
        out
    }
}

/// Connects to the configured database and brings its schema up to date.
pub async fn open(config: &DatabaseConfig) -> Result<Arc<dyn Store>> {
    Ok(Arc::new(store::SqlStore::open(config).await?))
}

/// What [`check_connection`] found, without changing anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ConnectionCheck {
    /// The server's own version string (or SQLite's library version).
    pub server_version: String,
    /// The PolarisCal schema version, or `None` if PolarisCal hasn't been set up there yet.
    pub schema_version: Option<i64>,
    /// How many PolarisCal user accounts exist, when the schema is present.
    pub user_count: Option<i64>,
    /// For SQLite: whether the file exists yet.
    pub exists: bool,
}

/// Connects without creating or upgrading anything and reports what is there.
pub async fn check_connection(config: &DatabaseConfig) -> Result<ConnectionCheck> {
    let mut config = config.clone();
    config.normalize()?;
    if let DatabaseConfig::Sqlite { path } = &config
        && !std::path::Path::new(path).exists()
    {
        let parent_ok = std::path::Path::new(path)
            .parent()
            .is_none_or(|p| p.as_os_str().is_empty() || p.exists());
        if !parent_ok {
            return Err(Error::Config(format!(
                "The folder for {path} doesn't exist or can't be reached."
            )));
        }
        return Ok(ConnectionCheck {
            server_version: "SQLite".into(),
            schema_version: None,
            user_count: None,
            exists: false,
        });
    }
    store::SqlStore::check(&config).await
}

// ---------------------------------------------------------------------------
// Driver options
// ---------------------------------------------------------------------------

/// Percent-encodes a URL component.
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn password_auth(s: &ServerConfig) -> Result<(&str, Option<&str>)> {
    match &s.auth {
        ServerAuth::Password { username, password } => Ok((
            username.as_str(),
            password.as_deref().filter(|p| !p.is_empty()),
        )),
        ServerAuth::Windows => Err(Error::Config(
            "Windows sign-in is only available for SQL Server.".into(),
        )),
    }
}

pub(crate) fn pg_options(s: &ServerConfig) -> Result<PgConnectOptions> {
    let (username, password) = password_auth(s)?;
    let host = if s.host.contains(':') {
        format!("[{}]", s.host)
    } else {
        url_encode(&s.host)
    };
    // Parsing a URL (rather than the builder) makes sqlx look up a password in the
    // user's pgpass.conf for this exact host, port, database, and user.
    let url = format!(
        "postgres://{}@{}:{}/{}",
        url_encode(username),
        host,
        s.port.unwrap_or(5432),
        url_encode(&s.database)
    );
    let mut options = PgConnectOptions::from_str(&url)
        .map_err(|e| Error::Config(format!("invalid PostgreSQL settings: {e}")))?;
    if let Some(password) = password {
        options = options.password(password);
    }
    Ok(options
        .ssl_mode(match s.encryption {
            Encryption::Off => PgSslMode::Disable,
            Encryption::Prefer => PgSslMode::Prefer,
            Encryption::Require => PgSslMode::Require,
            Encryption::Verify => PgSslMode::VerifyFull,
        })
        .application_name("PolarisCal"))
}

pub(crate) fn mysql_options(s: &ServerConfig) -> Result<MySqlConnectOptions> {
    let (username, password) = password_auth(s)?;
    let mut options = MySqlConnectOptions::new()
        .host(&s.host)
        .port(s.port.unwrap_or(3306))
        .username(username)
        .database(&s.database)
        .charset("utf8mb4")
        .ssl_mode(match s.encryption {
            Encryption::Off => MySqlSslMode::Disabled,
            Encryption::Prefer => MySqlSslMode::Preferred,
            Encryption::Require => MySqlSslMode::Required,
            Encryption::Verify => MySqlSslMode::VerifyIdentity,
        });
    if let Some(password) = password {
        options = options.password(password);
    }
    Ok(options)
}

pub(crate) fn mssql_options(s: &ServerConfig) -> Result<(tiberius::Config, bool)> {
    use tiberius::{AuthMethod, EncryptionLevel};
    let mut config = tiberius::Config::new();
    config.host(&s.host);
    if let Some(port) = s.port {
        config.port(port);
    }
    if let Some(instance) = &s.instance {
        config.instance_name(instance);
    }
    config.database(&s.database);
    config.application_name("PolarisCal");
    match &s.auth {
        ServerAuth::Password { username, password } => config.authentication(
            AuthMethod::sql_server(username, password.clone().unwrap_or_default()),
        ),
        #[cfg(windows)]
        ServerAuth::Windows => config.authentication(AuthMethod::Integrated),
        #[cfg(not(windows))]
        ServerAuth::Windows => {
            return Err(Error::Config(
                "Windows sign-in to SQL Server only works on Windows.".into(),
            ));
        }
    }
    match s.encryption {
        Encryption::Off => config.encryption(EncryptionLevel::NotSupported),
        Encryption::Prefer => {
            config.encryption(EncryptionLevel::On);
            config.trust_cert();
        }
        Encryption::Require => {
            config.encryption(EncryptionLevel::Required);
            config.trust_cert();
        }
        Encryption::Verify => config.encryption(EncryptionLevel::Required),
    }
    // A named instance without a fixed port is found through SQL Server Browser.
    let named = s.instance.is_some() && s.port.is_none();
    Ok((config, named))
}

pub(crate) fn pg_pool_options() -> PgPoolOptions {
    PgPoolOptions::new()
        .max_connections(POOL_SIZE)
        .acquire_timeout(ACQUIRE_TIMEOUT)
}

pub(crate) fn mysql_pool_options() -> MySqlPoolOptions {
    MySqlPoolOptions::new()
        .max_connections(POOL_SIZE)
        .acquire_timeout(ACQUIRE_TIMEOUT)
}

pub(crate) const MSSQL_POOL_SIZE: usize = POOL_SIZE as usize;

// ---------------------------------------------------------------------------
// Test support
// ---------------------------------------------------------------------------

/// Helpers for the backend test suite. Not part of the app's API.
#[doc(hidden)]
pub mod testing {
    use super::*;

    /// Creates an empty database named `name` on the server `admin` points at, and
    /// returns a configuration for it. `admin` must be allowed to create databases.
    pub async fn create_database(admin: &DatabaseConfig, name: &str) -> Result<DatabaseConfig> {
        assert!(
            name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "test database names are plain identifiers"
        );
        let sql = match admin {
            DatabaseConfig::Sqlite { .. } => panic!("SQLite databases are files; no server to ask"),
            DatabaseConfig::Postgres(_) => format!("CREATE DATABASE {name}"),
            DatabaseConfig::Mysql(_) => {
                format!("CREATE DATABASE {name} CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci")
            }
            DatabaseConfig::SqlServer(_) => format!("CREATE DATABASE {name}"),
        };
        execute_raw(admin, &sql).await?;
        let mut config = admin.clone();
        config.server_mut().expect("server config").database = name.to_string();
        Ok(config)
    }

    /// Runs one statement directly against the database, bypassing the store.
    pub async fn execute_raw(config: &DatabaseConfig, sql: &str) -> Result<()> {
        store::SqlStore::execute_raw(config, sql).await
    }
}
