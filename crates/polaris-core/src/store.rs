use async_trait::async_trait;

use crate::audit::{Actor, AuditEntry, AuditFilter, WriteContext};
use crate::auth::{NewUser, User, UserCredentials, UserUpdate};
use crate::certificate::{
    Certificate, CertificateDetails, CertificateFilter, CertificateSigner, CertificateSummary,
    CertificateTemplate,
};
use crate::model::AuditAction;
use crate::model::{
    AppSettings, Asset, AssetFilter, AssetInput, Attachment, AttachmentFilter, CalibrationFilter,
    CalibrationInput, CalibrationRecord, LastCost, LookupInput, LookupItem, LookupKind,
    NewAttachment, StoreInfo,
};
use crate::transfer::{DataSet, TableCount};
use crate::{Id, Result};

/// Persistence interface implemented once per database backend.
///
/// Contract every implementation must honor (checked by `polaris-store/tests/store_contract.rs`):
/// * Inputs are normalized/validated with the `normalize` methods in [`crate::model`] and
///   [`crate::auth`] before writing; backends additionally enforce rules that need the
///   database (parent must be a System, a System with children can't become an
///   Instrument, the last active admin can't be removed, ...).
/// * Every write runs in one transaction **together with its audit entry**, built with
///   the snapshot functions in [`crate::audit`]. The audit table is append-only.
/// * A reason for editing or archiving/restoring an asset, service record, or file is
///   recommended and stored in the audit entry when given, but never required.
/// * After any change to an asset's calibration records or interval, the asset's
///   first/last/next dates are recomputed with [`crate::schedule::derive_dates`].
/// * `expected_version` is checked against `row_version`; a mismatch returns
///   [`crate::Error::Conflict`] and changes nothing.
/// * Nothing is hard-deleted except unused list entries ([`Store::delete_lookup`]);
///   records are archived and accounts deactivated.
///
/// Authorization (who may call what) is enforced by the caller, not the store.
#[async_trait]
pub trait Store: Send + Sync {
    async fn info(&self) -> Result<StoreInfo>;

    // --- Users and sessions ------------------------------------------------

    async fn user_count(&self) -> Result<i64>;
    /// Creates the first account as an Admin. Fails with a conflict if any user exists.
    async fn create_first_admin(&self, input: NewUser) -> Result<User>;
    /// Looks up an account by username (case-insensitive) for sign-in.
    async fn credentials(&self, username: &str) -> Result<Option<UserCredentials>>;
    /// Audits a sign-in attempt; on success also stamps the user's last sign-in time.
    async fn record_sign_in(&self, attempted_username: &str, user: Option<&User>) -> Result<()>;
    async fn record_sign_out(&self, actor: &Actor) -> Result<()>;
    async fn list_users(&self) -> Result<Vec<User>>;
    async fn get_user(&self, id: Id) -> Result<User>;
    async fn create_user(&self, ctx: &WriteContext, input: NewUser) -> Result<User>;
    async fn update_user(
        &self,
        ctx: &WriteContext,
        id: Id,
        expected_version: i64,
        input: UserUpdate,
    ) -> Result<User>;
    /// Sets a new password. When `ctx.actor` is the user themself it is a password change
    /// (and clears `must_change_password`); otherwise an admin reset.
    async fn set_password(
        &self,
        ctx: &WriteContext,
        user_id: Id,
        new_password: &str,
        must_change: bool,
    ) -> Result<User>;

    // --- Settings ------------------------------------------------------------

    async fn settings(&self) -> Result<AppSettings>;
    async fn update_settings(
        &self,
        ctx: &WriteContext,
        settings: AppSettings,
    ) -> Result<AppSettings>;

    // --- Audit trail -----------------------------------------------------------

    async fn list_audit(&self, filter: &AuditFilter) -> Result<Vec<AuditEntry>>;

    // --- Pick lists ------------------------------------------------------------

    /// A pick list in alphabetical order (case-insensitive).
    async fn list_lookups(
        &self,
        kind: LookupKind,
        include_inactive: bool,
    ) -> Result<Vec<LookupItem>>;
    async fn create_lookup(
        &self,
        ctx: &WriteContext,
        kind: LookupKind,
        input: LookupInput,
    ) -> Result<LookupItem>;
    async fn update_lookup(
        &self,
        ctx: &WriteContext,
        kind: LookupKind,
        id: Id,
        input: LookupInput,
    ) -> Result<LookupItem>;
    /// Permanently deletes a list entry that no item or calibration record (archived or
    /// not) uses, and audits it. Fails with a conflict if it's in use; the caller can
    /// make it inactive instead. The last remaining status can't be deleted.
    async fn delete_lookup(&self, ctx: &WriteContext, kind: LookupKind, id: Id) -> Result<()>;

    // --- Instruments and Systems ---------------------------------------------

    async fn list_assets(&self, filter: &AssetFilter) -> Result<Vec<Asset>>;
    async fn get_asset(&self, id: Id) -> Result<Asset>;
    async fn create_asset(&self, ctx: &WriteContext, input: AssetInput) -> Result<Asset>;
    async fn update_asset(
        &self,
        ctx: &WriteContext,
        id: Id,
        expected_version: i64,
        input: AssetInput,
    ) -> Result<Asset>;
    async fn set_asset_archived(
        &self,
        ctx: &WriteContext,
        id: Id,
        expected_version: i64,
        archived: bool,
    ) -> Result<Asset>;

    // --- Attachments -------------------------------------------------------------

    async fn list_attachments(&self, filter: &AttachmentFilter) -> Result<Vec<Attachment>>;
    async fn get_attachment(&self, id: Id) -> Result<Attachment>;
    /// Records a file the caller has already copied into the attachments folder.
    /// The asset must exist and not be archived; a calibration record, if given,
    /// must belong to that asset.
    async fn create_attachment(
        &self,
        ctx: &WriteContext,
        input: NewAttachment,
    ) -> Result<Attachment>;
    /// Archiving or restoring requires a reason. The file itself is never deleted.
    async fn set_attachment_archived(
        &self,
        ctx: &WriteContext,
        id: Id,
        expected_version: i64,
        archived: bool,
    ) -> Result<Attachment>;

    // --- Calibration records ---------------------------------------------------

    async fn list_calibrations(&self, filter: &CalibrationFilter)
    -> Result<Vec<CalibrationRecord>>;
    async fn get_calibration(&self, id: Id) -> Result<CalibrationRecord>;
    /// The cost on the most recent (unarchived) service record by `provider_id` of
    /// `service_type_id` for an item with the same manufacturer and model number as
    /// `asset_id` (model compared ignoring case). None if there is none, or the item has
    /// no manufacturer or model number.
    async fn last_cost(
        &self,
        provider_id: Id,
        service_type_id: Id,
        asset_id: Id,
    ) -> Result<Option<LastCost>>;
    async fn create_calibration(
        &self,
        ctx: &WriteContext,
        input: CalibrationInput,
    ) -> Result<CalibrationRecord>;
    async fn update_calibration(
        &self,
        ctx: &WriteContext,
        id: Id,
        expected_version: i64,
        input: CalibrationInput,
    ) -> Result<CalibrationRecord>;
    async fn set_calibration_archived(
        &self,
        ctx: &WriteContext,
        id: Id,
        expected_version: i64,
        archived: bool,
    ) -> Result<CalibrationRecord>;

    // --- Certificates --------------------------------------------------------------

    /// The certificate template, with `next_number` set to the counter for the next
    /// certificate (1 in a new year when the counter resets yearly).
    async fn certificate_template(&self) -> Result<CertificateTemplate>;
    /// Saves the template and the counter, audited as a settings change.
    async fn update_certificate_template(
        &self,
        ctx: &WriteContext,
        template: CertificateTemplate,
    ) -> Result<CertificateTemplate>;
    async fn list_certificates(
        &self,
        filter: &CertificateFilter,
    ) -> Result<Vec<CertificateSummary>>;
    async fn get_certificate(&self, id: Id) -> Result<Certificate>;
    /// Issues a certificate: assigns the next number (formatted with the issue date),
    /// stores the details with a copy of the template, and advances the counter, in one
    /// transaction. A number already used fails with [`crate::Error::Duplicate`].
    /// Active users, for picking who calibrated and approved a certificate.
    async fn certificate_signers(&self) -> Result<Vec<CertificateSigner>>;
    /// A user's saved signature image (a data URL), if any.
    async fn user_signature(&self, user_id: Id) -> Result<Option<String>>;
    /// Saves or (with None) removes a user's signature, audited as a change to the user.
    async fn set_user_signature(
        &self,
        ctx: &WriteContext,
        user_id: Id,
        signature: Option<String>,
    ) -> Result<()>;
    async fn create_certificate(
        &self,
        ctx: &WriteContext,
        details: CertificateDetails,
    ) -> Result<Certificate>;

    // --- Moving data between databases -------------------------------------------

    /// Row counts for every table, in [`DataSet`] order.
    async fn table_counts(&self) -> Result<Vec<TableCount>>;
    /// Reads every row of every table, ids included, in one consistent read.
    async fn export_data(&self) -> Result<DataSet>;
    /// Fills an empty database with `data`, keeping ids, in one transaction.
    /// Fails with a conflict if the database already holds users or records.
    /// Writes no audit entry; see [`Store::record_data_copy`].
    async fn import_data(&self, data: &DataSet) -> Result<()>;
    /// Audits a data copy (`Export` on the source, `Import` on the target).
    async fn record_data_copy(
        &self,
        ctx: &WriteContext,
        action: AuditAction,
        description: &str,
    ) -> Result<()>;
}
