//! Tauri commands: the only API the UI can call.
//!
//! Every command that touches data first passes [`require`], which checks that someone
//! is signed in, that the session hasn't idled into the locked state, that the account
//! is still active, and that its role allows the action. The UI hides what a role can't
//! do, but this is where it is enforced.

use std::sync::{Arc, OnceLock};
use std::time::Instant;

use chrono::Local;
use polaris_core::audit::{Actor, AuditEntry, AuditFilter, WriteContext};
use polaris_core::auth::{NewUser, User, UserUpdate, hash_password, verify_password};
use polaris_core::model::{
    AppSettings, Asset, AssetFilter, AssetInput, CalibrationFilter, CalibrationInput,
    CalibrationRecord, LookupInput, LookupItem, LookupKind, StoreInfo,
};
use polaris_core::schedule::DueSummary;
use polaris_core::{Error, FieldError, Id, Store};
use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use crate::state::{AppState, Session};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ErrorKind {
    NotFound,
    Validation,
    Conflict,
    Duplicate,
    Database,
    Config,
    NotConnected,
    /// Nobody is signed in (or the account was deactivated).
    Unauthenticated,
    /// The session locked after being idle; the password is needed to continue.
    Locked,
    /// Signed in, but the role doesn't allow this.
    Forbidden,
}

/// The error shape every command rejects with.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CommandError {
    pub kind: ErrorKind,
    pub message: String,
    /// Per-field problems for form validation; empty for other kinds.
    pub fields: Vec<FieldError>,
}

impl CommandError {
    fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            fields: vec![],
        }
    }
}

impl From<Error> for CommandError {
    fn from(e: Error) -> Self {
        let message = e.to_string();
        let (kind, fields) = match e {
            Error::NotFound { .. } => (ErrorKind::NotFound, vec![]),
            Error::Validation(fields) => (ErrorKind::Validation, fields),
            Error::Conflict(_) => (ErrorKind::Conflict, vec![]),
            Error::Duplicate(_) => (ErrorKind::Duplicate, vec![]),
            Error::Database(_) => (ErrorKind::Database, vec![]),
            Error::Config(_) => (ErrorKind::Config, vec![]),
        };
        Self {
            kind,
            message,
            fields,
        }
    }
}

type CmdResult<T> = Result<T, CommandError>;

fn store(state: &AppState) -> CmdResult<Arc<dyn Store>> {
    state
        .store
        .clone()
        .map_err(|message| CommandError::new(ErrorKind::NotConnected, message))
}

// ---------------------------------------------------------------------------
// Authorization
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Need {
    /// Any signed-in user with a usable password.
    SignedIn,
    /// Admin or Technician: create/edit/archive items and calibration records.
    Editor,
    /// Admin only: users, pick lists, settings, the full audit log.
    Admin,
    /// Signed in, even if the password must be changed first.
    PasswordChange,
}

/// Checks the session and role, refreshes the account from the database, and
/// records activity. Returns the store and the current user.
async fn require(state: &AppState, need: Need) -> CmdResult<(Arc<dyn Store>, User)> {
    let store = store(state)?;
    let timeout = state.idle_timeout();
    let user_id = state.with_session(|slot| {
        let session = slot
            .as_mut()
            .ok_or_else(|| CommandError::new(ErrorKind::Unauthenticated, "Sign in to continue."))?;
        if !session.locked && session.last_activity.elapsed() > timeout {
            session.locked = true;
        }
        if session.locked {
            return Err(CommandError::new(
                ErrorKind::Locked,
                "PolarisCal locked after a period of inactivity.",
            ));
        }
        Ok(session.user.id)
    })?;

    // Another admin may have changed this account's role or deactivated it since sign-in.
    let user = match store.get_user(user_id).await {
        Ok(u) if u.is_active => u,
        Ok(_) | Err(Error::NotFound { .. }) => {
            state.with_session(|slot| *slot = None);
            return Err(CommandError::new(
                ErrorKind::Unauthenticated,
                "Your account is no longer active.",
            ));
        }
        Err(e) => return Err(e.into()),
    };
    state.with_session(|slot| {
        if let Some(session) = slot {
            session.user = user.clone();
            session.last_activity = Instant::now();
        }
    });

    if user.must_change_password && need != Need::PasswordChange {
        return Err(CommandError::new(
            ErrorKind::Forbidden,
            "Change your password to continue.",
        ));
    }
    let allowed = match need {
        Need::SignedIn | Need::PasswordChange => true,
        Need::Editor => user.role.can_edit_records(),
        Need::Admin => user.role.is_admin(),
    };
    if !allowed {
        return Err(CommandError::new(
            ErrorKind::Forbidden,
            "Your role doesn't allow this action.",
        ));
    }
    Ok((store, user))
}

fn actor(user: &User) -> Actor {
    Actor {
        user_id: user.id,
        username: user.username.clone(),
    }
}

fn ctx(user: &User, reason: Option<String>) -> WriteContext {
    WriteContext::new(actor(user), reason)
}

fn start_session(state: &AppState, user: &User) {
    state.with_session(|slot| {
        *slot = Some(Session {
            user: user.clone(),
            last_activity: Instant::now(),
            locked: false,
        })
    });
}

/// A real Argon2 hash to verify against when the username doesn't exist, so a
/// failed sign-in takes the same time whether or not the account exists.
fn dummy_hash() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| hash_password("polariscal-timing-equalizer").unwrap_or_default())
}

const BAD_CREDENTIALS: &str = "The username or password is incorrect, or the account is inactive.";

// ---------------------------------------------------------------------------
// App and session
// ---------------------------------------------------------------------------

/// What the app is connected to, for the status bar and Settings page.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AppInfo {
    pub version: String,
    pub config_path: String,
    pub database: String,
    /// Present when connected.
    pub store: Option<StoreInfo>,
    /// Present when the database could not be opened at startup.
    pub connection_error: Option<String>,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn app_info(state: State<'_, AppState>) -> CmdResult<AppInfo> {
    let (store_info, connection_error) = match &state.store {
        Ok(store) => match store.info().await {
            Ok(info) => (Some(info), None),
            Err(e) => (None, Some(e.to_string())),
        },
        Err(message) => (None, Some(message.clone())),
    };
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        config_path: state.config_path.display().to_string(),
        database: state.database_description.clone(),
        store: store_info,
        connection_error,
    })
}

/// Everything the UI needs to decide which screen to show.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AuthStatus {
    pub connected: bool,
    pub connection_error: Option<String>,
    /// No accounts exist yet: show the first-run setup screen.
    pub setup_required: bool,
    pub user: Option<User>,
    pub locked: bool,
    pub idle_lock_minutes: u32,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn auth_status(state: State<'_, AppState>) -> CmdResult<AuthStatus> {
    let idle_lock_minutes = state
        .idle_lock_minutes
        .load(std::sync::atomic::Ordering::Relaxed);
    let store = match &state.store {
        Ok(s) => s.clone(),
        Err(message) => {
            return Ok(AuthStatus {
                connected: false,
                connection_error: Some(message.clone()),
                setup_required: false,
                user: None,
                locked: false,
                idle_lock_minutes,
            });
        }
    };
    let setup_required = store.user_count().await? == 0;
    let timeout = state.idle_timeout();
    let (user, locked) = state.with_session(|slot| match slot {
        Some(session) => {
            if !session.locked && session.last_activity.elapsed() > timeout {
                session.locked = true;
            }
            (Some(session.user.clone()), session.locked)
        }
        None => (None, false),
    });
    Ok(AuthStatus {
        connected: true,
        connection_error: None,
        setup_required,
        user,
        locked,
        idle_lock_minutes,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn setup_admin(state: State<'_, AppState>, input: NewUser) -> CmdResult<User> {
    let store = store(&state)?;
    let username = input.username.clone();
    let user = store.create_first_admin(input).await?;
    store.record_sign_in(&username, Some(&user)).await?;
    start_session(&state, &user);
    Ok(user)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn sign_in(
    state: State<'_, AppState>,
    username: String,
    password: String,
) -> CmdResult<User> {
    let store = store(&state)?;
    let creds = store.credentials(&username).await?;
    let accepted = match &creds {
        Some(c) => verify_password(&password, &c.password_hash) && c.user.is_active,
        None => {
            verify_password(&password, dummy_hash());
            false
        }
    };
    if !accepted {
        store.record_sign_in(&username, None).await?;
        return Err(CommandError::new(
            ErrorKind::Unauthenticated,
            BAD_CREDENTIALS,
        ));
    }
    let user = creds.expect("accepted implies an account").user;
    store.record_sign_in(&username, Some(&user)).await?;
    start_session(&state, &user);
    Ok(user)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn sign_out(state: State<'_, AppState>) -> CmdResult<()> {
    let user = state.with_session(|slot| slot.take().map(|s| s.user));
    if let (Some(user), Ok(store)) = (user, store(&state)) {
        store.record_sign_out(&actor(&user)).await?;
    }
    Ok(())
}

/// Locks the session now (the UI calls this when its idle timer fires).
#[tauri::command(rename_all = "snake_case")]
pub async fn lock_session(state: State<'_, AppState>) -> CmdResult<()> {
    state.with_session(|slot| {
        if let Some(session) = slot {
            session.locked = true;
        }
    });
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn unlock_session(state: State<'_, AppState>, password: String) -> CmdResult<User> {
    let store = store(&state)?;
    let username = state
        .with_session(|slot| slot.as_ref().map(|s| s.user.username.clone()))
        .ok_or_else(|| CommandError::new(ErrorKind::Unauthenticated, "Sign in to continue."))?;
    let creds = store.credentials(&username).await?;
    match creds {
        Some(c) if c.user.is_active && verify_password(&password, &c.password_hash) => {
            store.record_sign_in(&username, Some(&c.user)).await?;
            start_session(&state, &c.user);
            Ok(c.user)
        }
        _ => {
            store.record_sign_in(&username, None).await?;
            Err(CommandError::new(
                ErrorKind::Locked,
                "That password is incorrect.",
            ))
        }
    }
}

/// Keeps the session alive while the user is active in the UI without saving anything.
#[tauri::command(rename_all = "snake_case")]
pub async fn touch_session(state: State<'_, AppState>) -> CmdResult<()> {
    require(&state, Need::PasswordChange).await.map(|_| ())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn change_password(
    state: State<'_, AppState>,
    current_password: String,
    new_password: String,
) -> CmdResult<User> {
    let (store, user) = require(&state, Need::PasswordChange).await?;
    let creds = store
        .credentials(&user.username)
        .await?
        .ok_or_else(|| CommandError::new(ErrorKind::Unauthenticated, "Sign in to continue."))?;
    if !verify_password(&current_password, &creds.password_hash) {
        return Err(
            Error::validation("current_password", "Your current password is incorrect.").into(),
        );
    }
    if current_password == new_password {
        return Err(
            Error::validation("password", "Choose a password you haven't just used.").into(),
        );
    }
    let updated = store
        .set_password(&ctx(&user, None), user.id, &new_password, false)
        .await?;
    start_session(&state, &updated);
    Ok(updated)
}

// ---------------------------------------------------------------------------
// Users and settings (admin)
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
pub async fn list_users(state: State<'_, AppState>) -> CmdResult<Vec<User>> {
    let (store, _) = require(&state, Need::Admin).await?;
    Ok(store.list_users().await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn create_user(state: State<'_, AppState>, input: NewUser) -> CmdResult<User> {
    let (store, me) = require(&state, Need::Admin).await?;
    Ok(store.create_user(&ctx(&me, None), input).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_user(
    state: State<'_, AppState>,
    id: Id,
    expected_version: i64,
    input: UserUpdate,
) -> CmdResult<User> {
    let (store, me) = require(&state, Need::Admin).await?;
    let updated = store
        .update_user(&ctx(&me, None), id, expected_version, input)
        .await?;
    if updated.id == me.id {
        start_session(&state, &updated);
    }
    Ok(updated)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn reset_password(
    state: State<'_, AppState>,
    id: Id,
    new_password: String,
    must_change: bool,
) -> CmdResult<User> {
    let (store, me) = require(&state, Need::Admin).await?;
    Ok(store
        .set_password(&ctx(&me, None), id, &new_password, must_change)
        .await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<AppSettings> {
    let (store, _) = require(&state, Need::SignedIn).await?;
    Ok(store.settings().await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> CmdResult<AppSettings> {
    let (store, me) = require(&state, Need::Admin).await?;
    let saved = store.update_settings(&ctx(&me, None), settings).await?;
    state.set_idle_lock_minutes(saved.idle_lock_minutes);
    Ok(saved)
}

/// One item's history is visible to everyone; the full log is for admins.
#[tauri::command(rename_all = "snake_case")]
pub async fn list_audit(
    state: State<'_, AppState>,
    filter: AuditFilter,
) -> CmdResult<Vec<AuditEntry>> {
    let need = if filter.asset_id.is_some() {
        Need::SignedIn
    } else {
        Need::Admin
    };
    let (store, _) = require(&state, need).await?;
    Ok(store.list_audit(&filter).await?)
}

// ---------------------------------------------------------------------------
// Pick lists
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
pub async fn list_lookups(
    state: State<'_, AppState>,
    kind: LookupKind,
    include_inactive: bool,
) -> CmdResult<Vec<LookupItem>> {
    let (store, _) = require(&state, Need::SignedIn).await?;
    Ok(store.list_lookups(kind, include_inactive).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn create_lookup(
    state: State<'_, AppState>,
    kind: LookupKind,
    input: LookupInput,
) -> CmdResult<LookupItem> {
    let (store, me) = require(&state, Need::Admin).await?;
    Ok(store.create_lookup(&ctx(&me, None), kind, input).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_lookup(
    state: State<'_, AppState>,
    kind: LookupKind,
    id: Id,
    input: LookupInput,
) -> CmdResult<LookupItem> {
    let (store, me) = require(&state, Need::Admin).await?;
    Ok(store
        .update_lookup(&ctx(&me, None), kind, id, input)
        .await?)
}

// ---------------------------------------------------------------------------
// Instruments and Systems
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
pub async fn list_assets(state: State<'_, AppState>, filter: AssetFilter) -> CmdResult<Vec<Asset>> {
    let (store, _) = require(&state, Need::SignedIn).await?;
    Ok(store.list_assets(&filter).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_asset(state: State<'_, AppState>, id: Id) -> CmdResult<Asset> {
    let (store, _) = require(&state, Need::SignedIn).await?;
    Ok(store.get_asset(id).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn create_asset(state: State<'_, AppState>, input: AssetInput) -> CmdResult<Asset> {
    let (store, me) = require(&state, Need::Editor).await?;
    Ok(store.create_asset(&ctx(&me, None), input).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_asset(
    state: State<'_, AppState>,
    id: Id,
    expected_version: i64,
    input: AssetInput,
    reason: Option<String>,
) -> CmdResult<Asset> {
    let (store, me) = require(&state, Need::Editor).await?;
    Ok(store
        .update_asset(&ctx(&me, reason), id, expected_version, input)
        .await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn set_asset_archived(
    state: State<'_, AppState>,
    id: Id,
    expected_version: i64,
    archived: bool,
    reason: Option<String>,
) -> CmdResult<Asset> {
    let (store, me) = require(&state, Need::Editor).await?;
    Ok(store
        .set_asset_archived(&ctx(&me, reason), id, expected_version, archived)
        .await?)
}

// ---------------------------------------------------------------------------
// Calibration records
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
pub async fn list_calibrations(
    state: State<'_, AppState>,
    filter: CalibrationFilter,
) -> CmdResult<Vec<CalibrationRecord>> {
    let (store, _) = require(&state, Need::SignedIn).await?;
    Ok(store.list_calibrations(&filter).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_calibration(state: State<'_, AppState>, id: Id) -> CmdResult<CalibrationRecord> {
    let (store, _) = require(&state, Need::SignedIn).await?;
    Ok(store.get_calibration(id).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn create_calibration(
    state: State<'_, AppState>,
    input: CalibrationInput,
) -> CmdResult<CalibrationRecord> {
    let (store, me) = require(&state, Need::Editor).await?;
    Ok(store.create_calibration(&ctx(&me, None), input).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_calibration(
    state: State<'_, AppState>,
    id: Id,
    expected_version: i64,
    input: CalibrationInput,
    reason: Option<String>,
) -> CmdResult<CalibrationRecord> {
    let (store, me) = require(&state, Need::Editor).await?;
    Ok(store
        .update_calibration(&ctx(&me, reason), id, expected_version, input)
        .await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn set_calibration_archived(
    state: State<'_, AppState>,
    id: Id,
    expected_version: i64,
    archived: bool,
    reason: Option<String>,
) -> CmdResult<CalibrationRecord> {
    let (store, me) = require(&state, Need::Editor).await?;
    Ok(store
        .set_calibration_archived(&ctx(&me, reason), id, expected_version, archived)
        .await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn due_summary(state: State<'_, AppState>) -> CmdResult<DueSummary> {
    let (store, _) = require(&state, Need::SignedIn).await?;
    let assets = store.list_assets(&AssetFilter::default()).await?;
    Ok(DueSummary::from_assets(&assets, Local::now().date_naive()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use polaris_core::model::Role;
    use polaris_store::DatabaseConfig;
    use std::time::Duration;

    #[test]
    fn validation_errors_keep_their_fields() {
        let e: CommandError = Error::validation("asset_tag", "Asset tag is required.").into();
        assert_eq!(e.kind, ErrorKind::Validation);
        assert_eq!(e.message, "Asset tag is required.");
        assert_eq!(e.fields[0].field, "asset_tag");
    }

    #[test]
    fn error_serializes_for_the_ui() {
        let e: CommandError = Error::stale("asset").into();
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["kind"], "conflict");
        assert!(
            json["message"]
                .as_str()
                .unwrap()
                .contains("changed by someone else")
        );
    }

    fn err_kind<T>(r: CmdResult<T>) -> ErrorKind {
        match r {
            Err(e) => e.kind,
            Ok(_) => panic!("expected an error"),
        }
    }

    async fn state_with_users() -> (AppState, tempfile::TempDir, User, User) {
        let dir = tempfile::tempdir().unwrap();
        let config = DatabaseConfig::Sqlite {
            path: dir.path().join("t.db").display().to_string(),
        };
        let store = polaris_store::open(&config).await.unwrap();
        let new = |name: &str, role| NewUser {
            username: name.into(),
            display_name: name.into(),
            email: None,
            role,
            password: "a long password".into(),
            must_change_password: false,
        };
        let admin = store
            .create_first_admin(new("admin", Role::Admin))
            .await
            .unwrap();
        let viewer = store
            .create_user(&ctx(&admin, None), new("viewer", Role::Viewer))
            .await
            .unwrap();
        let state = AppState::new(dir.path().join("config.json"), "test".into(), Ok(store));
        (state, dir, admin, viewer)
    }

    #[tokio::test]
    async fn require_enforces_sign_in_roles_and_idle_lock() {
        let (state, _dir, admin, viewer) = state_with_users().await;

        assert_eq!(
            err_kind(require(&state, Need::SignedIn).await),
            ErrorKind::Unauthenticated
        );

        start_session(&state, &viewer);
        assert!(require(&state, Need::SignedIn).await.is_ok());
        assert_eq!(
            err_kind(require(&state, Need::Editor).await),
            ErrorKind::Forbidden
        );
        assert_eq!(
            err_kind(require(&state, Need::Admin).await),
            ErrorKind::Forbidden
        );

        start_session(&state, &admin);
        assert!(require(&state, Need::Admin).await.is_ok());

        // Simulate the idle timeout passing.
        state.with_session(|s| {
            s.as_mut().unwrap().last_activity = Instant::now() - Duration::from_secs(16 * 60)
        });
        assert_eq!(
            err_kind(require(&state, Need::SignedIn).await),
            ErrorKind::Locked
        );
        assert!(state.with_session(|s| s.as_ref().unwrap().locked));
    }

    #[tokio::test]
    async fn deactivated_accounts_are_signed_out() {
        let (state, _dir, admin, viewer) = state_with_users().await;
        start_session(&state, &viewer);
        let store = state.store.clone().unwrap();
        store
            .update_user(
                &ctx(&admin, None),
                viewer.id,
                viewer.row_version,
                UserUpdate {
                    display_name: viewer.display_name.clone(),
                    email: None,
                    role: Role::Viewer,
                    is_active: false,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            err_kind(require(&state, Need::SignedIn).await),
            ErrorKind::Unauthenticated
        );
        assert!(state.with_session(|s| s.is_none()));
    }
}
