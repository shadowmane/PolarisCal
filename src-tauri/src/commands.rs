//! Tauri commands: the only API the UI can call.
//!
//! Every command that touches data first passes [`require`], which checks that someone
//! is signed in, that the session hasn't idled into the locked state, that the account
//! is still active, and that its role allows the action. The UI hides what a role can't
//! do, but this is where it is enforced.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use chrono::{Local, Utc};
use polaris_core::audit::{Actor, AuditEntry, AuditFilter, WriteContext};
use polaris_core::auth::{
    NewUser, PasswordPolicy, User, UserUpdate, hash_password, verify_password,
};
use polaris_core::model::{
    AppSettings, Asset, AssetFilter, AssetInput, Attachment, AttachmentCategory, AttachmentFilter,
    AttachmentFolder, CalibrationFilter, CalibrationInput, CalibrationRecord, LastCost,
    LookupInput, LookupItem, LookupKind, NewAttachment, StoreInfo,
};
use polaris_core::schedule::DueSummary;
use polaris_core::{Error, FieldError, Id, Store};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use ts_rs::TS;

use crate::files::{self, FileError};
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
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
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

pub(crate) type CmdResult<T> = Result<T, CommandError>;

pub(crate) fn store(state: &AppState) -> CmdResult<Arc<dyn Store>> {
    state
        .store()
        .map_err(|message| CommandError::new(ErrorKind::NotConnected, message))
}

// ---------------------------------------------------------------------------
// Authorization
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Need {
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
pub(crate) async fn require(state: &AppState, need: Need) -> CmdResult<(Arc<dyn Store>, User)> {
    check_session(state, need, true).await
}

/// Like [`require`], but doesn't count as activity, for checks the UI makes on its own in
/// the background (they mustn't keep an idle session from locking).
async fn require_passive(state: &AppState, need: Need) -> CmdResult<(Arc<dyn Store>, User)> {
    check_session(state, need, false).await
}

async fn check_session(
    state: &AppState,
    need: Need,
    activity: bool,
) -> CmdResult<(Arc<dyn Store>, User)> {
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
            if activity {
                session.last_activity = Instant::now();
            }
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

pub(crate) fn ctx(user: &User, reason: Option<String>) -> WriteContext {
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
pub(crate) fn dummy_hash() -> &'static str {
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
    let connection = state.connection();
    let (store_info, connection_error) = match &connection.store {
        Ok(store) => match store.info().await {
            Ok(info) => (Some(info), None),
            Err(e) => (None, Some(e.to_string())),
        },
        Err(message) => (None, Some(message.clone())),
    };
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        config_path: state.config_path.display().to_string(),
        database: connection.description.clone(),
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
    /// Whether to look for a new version at startup (an admin setting). Known before
    /// sign-in so the update notice can appear on the sign-in screen.
    pub update_checks: bool,
    /// The rules for new passwords, for the setup and password-change screens.
    pub password_policy: PasswordPolicy,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn auth_status(state: State<'_, AppState>) -> CmdResult<AuthStatus> {
    let idle_lock_minutes = state
        .idle_lock_minutes
        .load(std::sync::atomic::Ordering::Relaxed);
    let store = match state.store() {
        Ok(s) => s,
        Err(message) => {
            return Ok(AuthStatus {
                connected: false,
                connection_error: Some(message.clone()),
                setup_required: false,
                user: None,
                locked: false,
                idle_lock_minutes,
                update_checks: true,
                password_policy: PasswordPolicy::default(),
            });
        }
    };
    let setup_required = store.user_count().await? == 0;
    let settings = store.settings().await.unwrap_or_default();
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
        update_checks: settings.update_checks,
        password_policy: settings.password_policy,
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
    let mut user = creds.expect("accepted implies an account").user;
    store.record_sign_in(&username, Some(&user)).await?;
    // An expired password still signs in, but straight to choosing a new one.
    let policy = store.settings().await?.password_policy;
    if policy.is_expired(user.password_changed_at, Utc::now()) {
        user = store.expire_password(&user).await?;
    }
    start_session(&state, &user);
    Ok(user)
}

/// From the sign-in screen, after failed attempts: asks the administrators to reset this
/// account's password. Succeeds whether or not the account exists, so it can't be used to
/// find out which usernames are real.
#[tauri::command(rename_all = "snake_case")]
pub async fn request_password_reset(state: State<'_, AppState>, username: String) -> CmdResult<()> {
    if username.trim().is_empty() {
        return Err(Error::validation("username", "Enter your username.").into());
    }
    Ok(store(&state)?.request_password_reset(&username).await?)
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

/// How many active accounts are waiting for a password reset, for the badge on Users. The
/// sidebar checks this in the background, so it doesn't count as activity.
#[tauri::command(rename_all = "snake_case")]
pub async fn password_reset_requests(state: State<'_, AppState>) -> CmdResult<usize> {
    let (store, _) = require_passive(&state, Need::Admin).await?;
    Ok(store
        .list_users()
        .await?
        .iter()
        .filter(|u| u.is_active && u.reset_requested_at.is_some())
        .count())
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
    // Check the format first (clear message), then whether this computer can write there.
    let mut settings = settings;
    settings.normalize()?;
    // Only folders that changed, so an unreachable share doesn't block saving other settings.
    let before = store.settings().await?;
    for (field, folder) in [
        ("attachments_root", AttachmentFolder::Certificates),
        ("photos_root", AttachmentFolder::Photos),
        ("service_manuals_root", AttachmentFolder::ServiceManuals),
        ("documents_root", AttachmentFolder::Documents),
    ] {
        if let Some(root) = settings.own_root(folder)
            && before.own_root(folder) != Some(root)
        {
            check_folder(PathBuf::from(root))
                .await
                .map_err(|e| CommandError::from(Error::validation(field, e.message)))?;
        }
    }
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
    // Technicians may add entries while entering items; editing and deleting stay admin-only.
    let (store, me) = require(&state, Need::Editor).await?;
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

/// Deletes a list entry nothing uses yet (admins only).
#[tauri::command(rename_all = "snake_case")]
pub async fn delete_lookup(state: State<'_, AppState>, kind: LookupKind, id: Id) -> CmdResult<()> {
    let (store, me) = require(&state, Need::Admin).await?;
    Ok(store.delete_lookup(&ctx(&me, None), kind, id).await?)
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

/// Moves Instruments into a System, e.g. the ones chosen when creating it.
#[tauri::command(rename_all = "snake_case")]
pub async fn add_to_system(
    state: State<'_, AppState>,
    system_id: Id,
    instrument_ids: Vec<Id>,
) -> CmdResult<()> {
    let (store, me) = require(&state, Need::Editor).await?;
    Ok(store
        .add_to_system(&ctx(&me, None), system_id, &instrument_ids)
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

/// What the provider last charged for this service on the same make and model.
#[tauri::command(rename_all = "snake_case")]
pub async fn last_cost(
    state: State<'_, AppState>,
    provider_id: Id,
    service_type_id: Id,
    asset_id: Id,
) -> CmdResult<Option<LastCost>> {
    let (store, _) = require(&state, Need::SignedIn).await?;
    Ok(store
        .last_cost(provider_id, service_type_id, asset_id)
        .await?)
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

// ---------------------------------------------------------------------------
// Attachments
// ---------------------------------------------------------------------------

impl From<FileError> for CommandError {
    fn from(e: FileError) -> Self {
        let kind = match e {
            FileError::RootUnavailable(_) => ErrorKind::Config,
            FileError::Io { .. } => ErrorKind::Database,
            FileError::NotAFile(_) | FileError::TooLarge { .. } | FileError::BadPath(_) => {
                ErrorKind::Validation
            }
        };
        CommandError::new(kind, e.to_string())
    }
}

/// Runs blocking file work off the async runtime.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| CommandError::new(ErrorKind::Database, format!("background task failed: {e}")))
}

fn no_folder() -> CommandError {
    CommandError::new(
        ErrorKind::Config,
        "An administrator needs to choose the attachments folder in Settings first.",
    )
}

/// Where new files of this type are saved: their own folder, or the certificates folder
/// if that isn't set. Also says which folder that is, to record with the file.
async fn save_folder(
    store: &Arc<dyn Store>,
    category: AttachmentCategory,
) -> CmdResult<(AttachmentFolder, PathBuf)> {
    let settings = store.settings().await?;
    let (folder, root) = settings.save_to(category.folder());
    root.map(|r| (folder, PathBuf::from(r)))
        .ok_or_else(no_folder)
}

/// Where a saved file's folder is now. If that folder's setting has since been cleared,
/// the certificates folder is tried (where files go when it's unset).
async fn stored_root(store: &Arc<dyn Store>, folder: AttachmentFolder) -> CmdResult<PathBuf> {
    let settings = store.settings().await?;
    settings
        .own_root(folder)
        .or(settings.attachments_root.as_deref())
        .map(PathBuf::from)
        .ok_or_else(no_folder)
}

async fn check_folder(root: PathBuf) -> CmdResult<()> {
    blocking(move || files::check_writable(&root))
        .await?
        .map_err(Into::into)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_attachments(
    state: State<'_, AppState>,
    filter: AttachmentFilter,
) -> CmdResult<Vec<Attachment>> {
    let (store, _) = require(&state, Need::SignedIn).await?;
    Ok(store.list_attachments(&filter).await?)
}

/// Copies the chosen files into the attachments folder and records them.
#[tauri::command(rename_all = "snake_case")]
pub async fn add_attachments(
    state: State<'_, AppState>,
    asset_id: Id,
    calibration_id: Option<Id>,
    category: AttachmentCategory,
    paths: Vec<String>,
    description: Option<String>,
) -> CmdResult<Vec<Attachment>> {
    let (store, me) = require(&state, Need::Editor).await?;
    let (storage_folder, root) = save_folder(&store, category).await?;
    let asset = store.get_asset(asset_id).await?;
    if asset.is_archived {
        return Err(
            Error::validation("asset_id", "Restore this item before attaching files.").into(),
        );
    }
    let mut folder = vec![asset.asset_tag.clone()];
    if let Some(cal_id) = calibration_id {
        let record = store.get_calibration(cal_id).await?;
        if record.asset_id != asset_id {
            return Err(Error::validation(
                "calibration_id",
                "That calibration record doesn't belong to this item.",
            )
            .into());
        }
        folder.push(record.cal_date.to_string());
    }

    let mut saved = Vec::with_capacity(paths.len());
    for path in paths {
        let (root_c, folder_c) = (root.clone(), folder.clone());
        let stored = blocking(move || {
            let parts: Vec<&str> = folder_c.iter().map(String::as_str).collect();
            files::store_file(&root_c, &parts, &PathBuf::from(path))
        })
        .await??;
        let input = NewAttachment {
            asset_id,
            calibration_id,
            category,
            file_name: stored.file_name.clone(),
            storage_folder,
            relative_path: stored.relative_path.clone(),
            size_bytes: stored.size_bytes,
            sha256: stored.sha256.clone(),
            content_type: stored.content_type.clone(),
            description: description.clone(),
        };
        match store.create_attachment(&ctx(&me, None), input).await {
            Ok(a) => saved.push(a),
            Err(e) => {
                files::discard(&root, &stored.relative_path);
                return Err(e.into());
            }
        }
    }
    Ok(saved)
}

/// Finds an attachment's file and confirms it is the one that was attached.
async fn verified_path(
    store: &Arc<dyn Store>,
    id: Id,
    force: bool,
) -> CmdResult<(Attachment, PathBuf)> {
    let attachment = store.get_attachment(id).await?;
    let root = stored_root(store, attachment.storage_folder).await?;
    let path = files::resolve(&root, &attachment.relative_path)?;
    let check = path.clone();
    let hashed = blocking(move || files::hash_file(&check)).await?;
    match hashed {
        Ok((sha, _)) if sha == attachment.sha256 || force => Ok((attachment, path)),
        Ok(_) => Err(CommandError::new(
            ErrorKind::Conflict,
            format!(
                "{} has changed since it was attached, so it may not be the original.",
                attachment.file_name
            ),
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(CommandError::new(
            ErrorKind::NotFound,
            format!(
                "{} is missing from the attachments folder (expected at {}).",
                attachment.file_name,
                path.display()
            ),
        )),
        Err(e) => Err(CommandError::new(
            ErrorKind::Database,
            format!("Couldn't read {}: {e}", path.display()),
        )),
    }
}

/// Opens the file in its default program after checking its fingerprint.
/// `force` opens it even if it has changed since it was attached.
#[tauri::command(rename_all = "snake_case")]
pub async fn open_attachment(
    app: AppHandle,
    state: State<'_, AppState>,
    id: Id,
    force: bool,
) -> CmdResult<()> {
    let (store, _) = require(&state, Need::SignedIn).await?;
    let (_, path) = verified_path(&store, id, force).await?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| CommandError::new(ErrorKind::Config, format!("Couldn't open the file: {e}")))
}

/// Image types the app's window can show.
const SHOWABLE_IMAGES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "image/bmp",
    "image/avif",
];
/// Larger photos aren't shown on the page (they can still be opened).
const MAX_SHOWN_IMAGE_BYTES: i64 = 40 * 1024 * 1024;

/// An attached photo's image, for showing on the item's page. Only image types the window
/// can show, and only if the file is unchanged since it was attached.
#[tauri::command(rename_all = "snake_case")]
pub async fn attachment_image(
    state: State<'_, AppState>,
    id: Id,
) -> CmdResult<tauri::ipc::Response> {
    let (store, _) = require(&state, Need::SignedIn).await?;
    let attachment = store.get_attachment(id).await?;
    let showable = attachment
        .content_type
        .as_deref()
        .is_some_and(|t| SHOWABLE_IMAGES.contains(&t));
    if !showable {
        return Err(
            Error::validation("id", "This file isn't a picture PolarisCal can show.").into(),
        );
    }
    if attachment.size_bytes > MAX_SHOWN_IMAGE_BYTES {
        return Err(Error::validation(
            "id",
            "This picture is too large to show here; open it instead.",
        )
        .into());
    }
    let (_, path) = verified_path(&store, id, false).await?;
    let bytes = blocking(move || std::fs::read(&path)).await?.map_err(|e| {
        CommandError::new(
            ErrorKind::Database,
            format!("Couldn't read the picture: {e}"),
        )
    })?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Shows the file in File Explorer.
#[tauri::command(rename_all = "snake_case")]
pub async fn reveal_attachment(
    app: AppHandle,
    state: State<'_, AppState>,
    id: Id,
) -> CmdResult<()> {
    let (store, _) = require(&state, Need::SignedIn).await?;
    let (_, path) = verified_path(&store, id, true).await?;
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| CommandError::new(ErrorKind::Config, format!("Couldn't show the file: {e}")))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn set_attachment_archived(
    state: State<'_, AppState>,
    id: Id,
    expected_version: i64,
    archived: bool,
    reason: Option<String>,
) -> CmdResult<Attachment> {
    let (store, me) = require(&state, Need::Editor).await?;
    Ok(store
        .set_attachment_archived(&ctx(&me, reason), id, expected_version, archived)
        .await?)
}

/// Lets an admin test a folder before saving it as the attachments folder.
#[tauri::command(rename_all = "snake_case")]
pub async fn check_attachments_folder(state: State<'_, AppState>, path: String) -> CmdResult<()> {
    require(&state, Need::Admin).await?;
    check_folder(PathBuf::from(path.trim())).await
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
    async fn background_checks_dont_keep_the_session_awake() {
        let (state, _dir, admin, _viewer) = state_with_users().await;
        start_session(&state, &admin);
        let idle = Duration::from_secs(10 * 60);
        state.with_session(|s| s.as_mut().unwrap().last_activity = Instant::now() - idle);
        assert!(require_passive(&state, Need::Admin).await.is_ok());
        assert!(state.with_session(|s| s.as_ref().unwrap().last_activity.elapsed() >= idle));
        // A real command still counts as activity.
        assert!(require(&state, Need::Admin).await.is_ok());
        assert!(state.with_session(|s| s.as_ref().unwrap().last_activity.elapsed() < idle));
    }

    #[tokio::test]
    async fn deactivated_accounts_are_signed_out() {
        let (state, _dir, admin, viewer) = state_with_users().await;
        start_session(&state, &viewer);
        let store = state.store().unwrap();
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
