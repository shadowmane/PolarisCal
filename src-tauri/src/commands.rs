//! Tauri commands: the only API the UI can call. Each one is a thin wrapper
//! over [`polaris_core::Store`] that converts errors into [`CommandError`].

use std::sync::Arc;

use chrono::Local;
use polaris_core::model::{
    Asset, AssetFilter, AssetInput, CalibrationFilter, CalibrationInput, CalibrationRecord,
    LookupInput, LookupItem, LookupKind, StoreInfo,
};
use polaris_core::schedule::DueSummary;
use polaris_core::{Error, FieldError, Id, Store};
use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use crate::state::AppState;

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
    state.store.clone().map_err(|message| CommandError {
        kind: ErrorKind::NotConnected,
        message,
        fields: vec![],
    })
}

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

#[tauri::command(rename_all = "snake_case")]
pub async fn list_lookups(
    state: State<'_, AppState>,
    kind: LookupKind,
    include_inactive: bool,
) -> CmdResult<Vec<LookupItem>> {
    Ok(store(&state)?.list_lookups(kind, include_inactive).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn create_lookup(
    state: State<'_, AppState>,
    kind: LookupKind,
    input: LookupInput,
) -> CmdResult<LookupItem> {
    Ok(store(&state)?.create_lookup(kind, input).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_lookup(
    state: State<'_, AppState>,
    kind: LookupKind,
    id: Id,
    input: LookupInput,
) -> CmdResult<LookupItem> {
    Ok(store(&state)?.update_lookup(kind, id, input).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_assets(state: State<'_, AppState>, filter: AssetFilter) -> CmdResult<Vec<Asset>> {
    Ok(store(&state)?.list_assets(&filter).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_asset(state: State<'_, AppState>, id: Id) -> CmdResult<Asset> {
    Ok(store(&state)?.get_asset(id).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn create_asset(state: State<'_, AppState>, input: AssetInput) -> CmdResult<Asset> {
    Ok(store(&state)?.create_asset(input).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_asset(
    state: State<'_, AppState>,
    id: Id,
    expected_version: i64,
    input: AssetInput,
) -> CmdResult<Asset> {
    Ok(store(&state)?
        .update_asset(id, expected_version, input)
        .await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn set_asset_archived(
    state: State<'_, AppState>,
    id: Id,
    expected_version: i64,
    archived: bool,
) -> CmdResult<Asset> {
    Ok(store(&state)?
        .set_asset_archived(id, expected_version, archived)
        .await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_calibrations(
    state: State<'_, AppState>,
    filter: CalibrationFilter,
) -> CmdResult<Vec<CalibrationRecord>> {
    Ok(store(&state)?.list_calibrations(&filter).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_calibration(state: State<'_, AppState>, id: Id) -> CmdResult<CalibrationRecord> {
    Ok(store(&state)?.get_calibration(id).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn create_calibration(
    state: State<'_, AppState>,
    input: CalibrationInput,
) -> CmdResult<CalibrationRecord> {
    Ok(store(&state)?.create_calibration(input).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_calibration(
    state: State<'_, AppState>,
    id: Id,
    expected_version: i64,
    input: CalibrationInput,
) -> CmdResult<CalibrationRecord> {
    Ok(store(&state)?
        .update_calibration(id, expected_version, input)
        .await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn set_calibration_archived(
    state: State<'_, AppState>,
    id: Id,
    expected_version: i64,
    archived: bool,
) -> CmdResult<CalibrationRecord> {
    Ok(store(&state)?
        .set_calibration_archived(id, expected_version, archived)
        .await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn due_summary(state: State<'_, AppState>) -> CmdResult<DueSummary> {
    let assets = store(&state)?.list_assets(&AssetFilter::default()).await?;
    Ok(DueSummary::from_assets(&assets, Local::now().date_naive()))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
