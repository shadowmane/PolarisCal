//! Commands for choosing the database and copying data between databases.
//!
//! Connection settings belong to this computer (or to a shared settings file), not
//! to a database. When PolarisCal is connected and set up, only an Administrator
//! may change them. When it can't connect, or the database has no accounts yet,
//! anyone at the computer may, since they could edit the config file anyway.

use std::path::Path;

use polaris_core::transfer::CopyReport;
use polaris_store::{ConnectionCheck, DatabaseConfig, ServerAuth};
use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use crate::commands::{CmdResult, CommandError, ErrorKind, Need, ctx, require};
use crate::config::{self, AppConfig, SharedSettings};
use crate::state::AppState;

/// The connection settings in effect on this computer.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ConnectionSettings {
    /// This computer's config file.
    pub config_path: String,
    /// The shared settings file this computer follows, if any.
    pub shared_settings_file: Option<String>,
    /// The connection in use, without any password. `None` if the shared file can't be read.
    pub database: Option<DatabaseConfig>,
    /// A password is saved for `database` (it is never sent to the screen).
    pub has_password: bool,
    pub description: String,
    /// Why the settings couldn't be read, if they couldn't.
    pub problem: Option<String>,
}

/// What the person chose in the connection dialog.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "source", rename_all = "snake_case")]
#[ts(export)]
pub enum ConnectionChoice {
    /// Keep the connection in this computer's own settings.
    Local { database: DatabaseConfig },
    /// Follow an existing shared settings file.
    SharedFile { path: String },
    /// Write `database` to a shared settings file (creating or replacing it) and follow it.
    WriteSharedFile {
        path: String,
        database: DatabaseConfig,
    },
}

/// A shared settings file's contents, for previewing before following it.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SharedFilePreview {
    pub database: DatabaseConfig,
    pub has_password: bool,
    pub description: String,
}

fn config_error(message: impl Into<String>) -> CommandError {
    CommandError::new(ErrorKind::Config, message)
}

/// Admins only, unless there's no usable, set-up database to protect.
async fn require_connection_rights(state: &AppState) -> CmdResult<()> {
    let Ok(store) = state.store() else {
        return Ok(());
    };
    match store.user_count().await {
        Ok(0) | Err(_) => Ok(()),
        Ok(_) => require(state, Need::Admin).await.map(|_| ()),
    }
}

fn load_config(state: &AppState) -> CmdResult<AppConfig> {
    let dir = state.config_path.parent().unwrap_or(Path::new("."));
    config::load_or_create(dir, dir)
        .map(|(c, _)| c)
        .map_err(config_error)
}

/// A blank password means "keep the saved one" when the server and account match.
fn keep_saved_password(new: &mut DatabaseConfig, saved: Option<&DatabaseConfig>) {
    let Some(saved) = saved else { return };
    if std::mem::discriminant(new) != std::mem::discriminant(saved) {
        return;
    }
    let (Some(n), Some(s)) = (new.server_mut(), saved.server()) else {
        return;
    };
    if let (
        ServerAuth::Password {
            username: new_user,
            password: new_password @ None,
        },
        ServerAuth::Password {
            username: saved_user,
            password: Some(saved_password),
        },
    ) = (&mut n.auth, &s.auth)
        && new_user.eq_ignore_ascii_case(saved_user)
        && n.host.eq_ignore_ascii_case(&s.host)
    {
        *new_password = Some(saved_password.clone());
    }
}

/// The full connection (password included) a choice leads to.
fn resolve(choice: &ConnectionChoice, saved: Option<&DatabaseConfig>) -> CmdResult<DatabaseConfig> {
    let mut database = match choice {
        ConnectionChoice::Local { database }
        | ConnectionChoice::WriteSharedFile { database, .. } => {
            let mut database = database.clone();
            keep_saved_password(&mut database, saved);
            database
        }
        ConnectionChoice::SharedFile { path } => {
            config::read_shared(Path::new(path.trim()))
                .map_err(config_error)?
                .database
        }
    };
    database.normalize()?;
    Ok(database)
}

fn shared_path(path: &str) -> CmdResult<String> {
    let path = path.trim();
    if path.is_empty() {
        return Err(CommandError::from(polaris_core::Error::validation(
            "shared_settings_file",
            "Choose the shared settings file.",
        )));
    }
    if !Path::new(path).is_absolute() {
        return Err(CommandError::from(polaris_core::Error::validation(
            "shared_settings_file",
            "Use a full path such as \\\\server\\share\\PolarisCal\\polariscal-connection.json.",
        )));
    }
    Ok(path.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_connection_settings(state: State<'_, AppState>) -> CmdResult<ConnectionSettings> {
    require_connection_rights(&state).await?;
    let config = load_config(&state)?;
    let (database, problem) = match config.effective_database() {
        Ok(db) => (Some(db), None),
        Err(e) => (None, Some(e)),
    };
    Ok(ConnectionSettings {
        config_path: state.config_path.display().to_string(),
        shared_settings_file: config.shared_settings_file.clone(),
        has_password: database.as_ref().is_some_and(DatabaseConfig::has_password),
        description: state.connection().description,
        database: database.map(|d| d.without_password()),
        problem,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn read_shared_settings(
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<SharedFilePreview> {
    require_connection_rights(&state).await?;
    let shared = config::read_shared(Path::new(&shared_path(&path)?)).map_err(config_error)?;
    Ok(SharedFilePreview {
        has_password: shared.database.has_password(),
        description: shared.database.describe(),
        database: shared.database.without_password(),
    })
}

/// Connects without changing anything and reports what's there.
#[tauri::command(rename_all = "snake_case")]
pub async fn test_connection(
    state: State<'_, AppState>,
    choice: ConnectionChoice,
) -> CmdResult<ConnectionCheck> {
    require_connection_rights(&state).await?;
    let saved = load_config(&state)?.effective_database().ok();
    let database = resolve(&choice, saved.as_ref())?;
    Ok(polaris_store::check_connection(&database).await?)
}

/// Connects to the chosen database (setting it up if it's new), then saves the
/// choice and switches to it. Nothing is saved if the connection fails.
#[tauri::command(rename_all = "snake_case")]
pub async fn save_connection(
    state: State<'_, AppState>,
    choice: ConnectionChoice,
) -> CmdResult<ConnectionSettings> {
    require_connection_rights(&state).await?;
    let mut config = load_config(&state)?;
    let saved = config.effective_database().ok();
    let database = resolve(&choice, saved.as_ref())?;
    let store = polaris_store::open(&database).await?;

    match &choice {
        ConnectionChoice::Local { .. } => {
            config.shared_settings_file = None;
            config.database = database.clone();
        }
        ConnectionChoice::SharedFile { path } => {
            config.shared_settings_file = Some(shared_path(path)?);
        }
        ConnectionChoice::WriteSharedFile { path, .. } => {
            let path = shared_path(path)?;
            config::write_shared(Path::new(&path), &SharedSettings::new(database.clone()))
                .map_err(config_error)?;
            config.shared_settings_file = Some(path);
        }
    }
    config::save(&state.config_path, &config).map_err(config_error)?;
    state.replace_connection(database.describe(), store).await;

    Ok(ConnectionSettings {
        config_path: state.config_path.display().to_string(),
        shared_settings_file: config.shared_settings_file.clone(),
        has_password: database.has_password(),
        description: database.describe(),
        database: Some(database.without_password()),
        problem: None,
    })
}

/// Copies every record from the database in use into another, empty database,
/// verifying row counts. Doesn't switch to it.
#[tauri::command(rename_all = "snake_case")]
pub async fn copy_database(
    state: State<'_, AppState>,
    target: DatabaseConfig,
) -> CmdResult<CopyReport> {
    let (source, me) = require(&state, Need::Admin).await?;
    let saved = load_config(&state)?.effective_database().ok();
    let target = resolve(
        &ConnectionChoice::Local { database: target },
        saved.as_ref(),
    )?;
    let source_name = state.connection().description;
    let target_name = target.describe();
    if saved.as_ref().is_some_and(|s| s.describe() == target_name) {
        return Err(CommandError::from(polaris_core::Error::validation(
            "",
            "That's the database PolarisCal is using now. Choose a different one to copy to.",
        )));
    }
    let target_store = polaris_store::open(&target).await?;
    let report = polaris_store::copy_database(
        source.as_ref(),
        target_store.as_ref(),
        &ctx(&me, None),
        &source_name,
        &target_name,
    )
    .await?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use polaris_store::{Encryption, ServerConfig};

    fn pg(user: &str, password: Option<&str>) -> DatabaseConfig {
        DatabaseConfig::Postgres(ServerConfig {
            host: "db".into(),
            port: None,
            instance: None,
            database: "polariscal".into(),
            auth: ServerAuth::Password {
                username: user.into(),
                password: password.map(Into::into),
            },
            encryption: Encryption::Prefer,
        })
    }

    #[test]
    fn blank_password_keeps_the_saved_one_for_the_same_account() {
        let saved = pg("polaris", Some("secret"));
        let mut same = pg("polaris", None);
        keep_saved_password(&mut same, Some(&saved));
        assert_eq!(same, saved);

        let mut other_user = pg("someone", None);
        keep_saved_password(&mut other_user, Some(&saved));
        assert!(!other_user.has_password());

        let mut typed = pg("polaris", Some("new"));
        keep_saved_password(&mut typed, Some(&saved));
        assert_eq!(typed, pg("polaris", Some("new")));
    }
}
