use async_trait::async_trait;

use crate::model::{
    Asset, AssetFilter, AssetInput, CalibrationFilter, CalibrationInput, CalibrationRecord,
    LookupInput, LookupItem, LookupKind, StoreInfo,
};
use crate::{Id, Result};

/// Persistence interface implemented once per database backend.
///
/// Contract every implementation must honor:
/// * Inputs are normalized/validated with the `normalize` methods in [`crate::model`]
///   before writing; backends additionally enforce rules that need the database
///   (parent must be a System, a System with children can't become an Instrument, ...).
/// * Writes that touch several rows run in one transaction.
/// * After any change to an asset's calibration records or interval, the asset's
///   first/last/next dates are recomputed with [`crate::schedule::derive_dates`].
/// * `expected_version` is checked against `row_version`; a mismatch returns
///   [`crate::Error::Conflict`] and changes nothing.
/// * Nothing is ever hard-deleted; records are archived instead.
#[async_trait]
pub trait Store: Send + Sync {
    async fn info(&self) -> Result<StoreInfo>;

    async fn list_lookups(
        &self,
        kind: LookupKind,
        include_inactive: bool,
    ) -> Result<Vec<LookupItem>>;
    async fn create_lookup(&self, kind: LookupKind, input: LookupInput) -> Result<LookupItem>;
    async fn update_lookup(
        &self,
        kind: LookupKind,
        id: Id,
        input: LookupInput,
    ) -> Result<LookupItem>;

    async fn list_assets(&self, filter: &AssetFilter) -> Result<Vec<Asset>>;
    async fn get_asset(&self, id: Id) -> Result<Asset>;
    async fn create_asset(&self, input: AssetInput) -> Result<Asset>;
    async fn update_asset(&self, id: Id, expected_version: i64, input: AssetInput)
    -> Result<Asset>;
    async fn set_asset_archived(
        &self,
        id: Id,
        expected_version: i64,
        archived: bool,
    ) -> Result<Asset>;

    async fn list_calibrations(&self, filter: &CalibrationFilter)
    -> Result<Vec<CalibrationRecord>>;
    async fn get_calibration(&self, id: Id) -> Result<CalibrationRecord>;
    async fn create_calibration(&self, input: CalibrationInput) -> Result<CalibrationRecord>;
    async fn update_calibration(
        &self,
        id: Id,
        expected_version: i64,
        input: CalibrationInput,
    ) -> Result<CalibrationRecord>;
    async fn set_calibration_archived(
        &self,
        id: Id,
        expected_version: i64,
        archived: bool,
    ) -> Result<CalibrationRecord>;
}
