//! Domain types shared by the backend and (via generated TypeScript bindings) the UI.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::auth::PasswordPolicy;
use crate::{Error, FieldError, Id, Result};

/// Declares a fieldless enum that is stored in the database as a lowercase
/// snake_case string and serialized to the UI the same way.
macro_rules! text_enum {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
        #[serde(rename_all = "snake_case")]
        #[ts(export)]
        pub enum $name { $($(#[$vmeta])* $variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self { $($name::$variant => $text),+ }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = Error;
            fn from_str(s: &str) -> Result<Self> {
                match s {
                    $($text => Ok($name::$variant),)+
                    other => Err(Error::Database(format!(
                        concat!("unknown ", stringify!($name), " value '{}'"), other
                    ))),
                }
            }
        }
    };
}
pub(crate) use text_enum;

text_enum!(
    /// An Instrument is a single calibrated item; a System groups Instruments.
    AssetKind { Instrument => "instrument", System => "system" }
);

text_enum!(IntervalUnit { Days => "days", Months => "months" });

text_enum!(
    /// Condition of the item when it arrived for calibration ("as found").
    ConditionReceived {
        Pass => "pass",
        OutOfTolerance => "out_of_tolerance",
        Inoperative => "inoperative",
    }
);

text_enum!(
    /// Condition of the item after calibration ("as left").
    ConditionReturned { Pass => "pass", Fail => "fail", Limited => "limited" }
);

text_enum!(
    /// Marks a built-in status the app relies on, so it keeps working if renamed.
    StatusRole {
        OutForCalibration => "out_for_calibration",
        /// Where an item goes after failing calibration, unless it's retired.
        OutForRepair => "out_for_repair",
        Retired => "retired",
        /// Out of active use but not retired: not tracked while stored.
        Storage => "storage",
    }
);

impl StatusRole {
    /// Whether items in this status are away being serviced.
    pub fn is_out_for_service(self) -> bool {
        matches!(self, Self::OutForCalibration | Self::OutForRepair)
    }
}

text_enum!(
    /// Where an item stands against its calibration schedule.
    DueStatus {
        /// The item's status (e.g. Out of Service) excludes it from due tracking.
        NotTracked => "not_tracked",
        /// Tracked, but there is no next calibration date to measure against.
        NoSchedule => "no_schedule",
        Overdue => "overdue",
        /// Past its calibration due date but authorized to stay in use until a
        /// calibration extension ends. Counted with the overdue items.
        Extended => "extended",
        DueSoon => "due_soon",
        Current => "current",
    }
);

text_enum!(
    /// The admin-managed pick lists.
    LookupKind {
        Location => "location",
        Department => "department",
        Owner => "owner",
        Manufacturer => "manufacturer",
        /// Who performs calibrations: internal technicians or outside vendors.
        Provider => "provider",
        /// Asset statuses such as Active or Out of Service.
        Status => "status",
        /// What a service record is: Calibration, Repair, and so on.
        ServiceType => "service_type",
    }
);

text_enum!(
    /// What a signed-in user may do. See [`Role::can_edit_records`] and [`Role::is_admin`].
    Role {
        /// Everything, including users, pick lists, and settings.
        Admin => "admin",
        /// Create, edit, and archive Instruments, Systems, and Calibration Records.
        Technician => "technician",
        /// Read-only.
        Viewer => "viewer",
    }
);

impl Role {
    pub fn can_edit_records(self) -> bool {
        matches!(self, Role::Admin | Role::Technician)
    }

    pub fn is_admin(self) -> bool {
        self == Role::Admin
    }
}

text_enum!(
    /// The kind of event recorded in the audit trail.
    AuditAction {
        Create => "create",
        Update => "update",
        Archive => "archive",
        Restore => "restore",
        /// Permanently removed. Only allowed for list entries nothing uses yet.
        Delete => "delete",
        SignIn => "sign_in",
        SignInFailed => "sign_in_failed",
        SignOut => "sign_out",
        /// A user changed their own password.
        PasswordChange => "password_change",
        /// An administrator set a new password for someone else.
        PasswordReset => "password_reset",
        /// Someone asked for a password reset from the sign-in screen.
        PasswordResetRequest => "password_reset_request",
        /// A password reached the end of its life at sign-in and must be changed.
        PasswordExpired => "password_expired",
        /// All data was copied from this database to another one.
        Export => "export",
        /// This database was filled with data copied from another one.
        Import => "import",
    }
);

text_enum!(
    /// What an audit entry is about.
    AuditEntity {
        Asset => "asset",
        Calibration => "calibration",
        Lookup => "lookup",
        User => "user",
        Session => "session",
        Settings => "settings",
        Attachment => "attachment",
        Certificate => "certificate",
        /// The database as a whole (data copies).
        Database => "database",
    }
);

text_enum!(
    /// Who performed a calibration: a calibration provider (an outside lab or an
    /// in-house technician), or the item's manufacturer.
    CalibratedBy {
        Provider => "provider",
        Manufacturer => "manufacturer",
    }
);

text_enum!(
    /// What an attached file is.
    AttachmentCategory {
        Certificate => "certificate",
        Document => "document",
        Photo => "photo",
        Other => "other",
    }
);

/// How often an item must be calibrated, e.g. 12 months.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CalInterval {
    pub value: u32,
    pub unit: IntervalUnit,
    /// Due on the last day of the month the interval lands in (12 months from
    /// 2026-11-20 is 2027-11-30 rather than 2027-11-20).
    #[serde(default)]
    pub end_of_month: bool,
}

/// An Instrument or System, with the display names of its lookups joined in.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Asset {
    pub id: Id,
    pub asset_tag: String,
    pub kind: AssetKind,
    pub parent_id: Option<Id>,
    pub parent_tag: Option<String>,
    pub description: String,
    pub manufacturer_id: Option<Id>,
    pub manufacturer_name: Option<String>,
    pub model_number: Option<String>,
    pub serial_number: Option<String>,
    pub location_id: Option<Id>,
    pub location_name: Option<String>,
    pub department_id: Option<Id>,
    pub department_name: Option<String>,
    pub owner_id: Option<Id>,
    pub owner_name: Option<String>,
    pub status_id: Id,
    pub status_name: String,
    /// Set when the item's status is a built-in one (e.g. Out for Repair).
    pub status_role: Option<StatusRole>,
    /// The status the item had before a service record last changed it (e.g. before a
    /// failed calibration sent it to Out for Repair); offered back when a repair passes.
    pub previous_status_id: Option<Id>,
    pub warranty_end_date: Option<NaiveDate>,
    pub cal_interval: Option<CalInterval>,
    pub in_service_date: Option<NaiveDate>,
    pub first_cal_date: Option<NaiveDate>,
    /// Derived: the latest calibration record's date, or `last_cal_date_entered`
    /// while the item has no (non-archived) records.
    pub last_cal_date: Option<NaiveDate>,
    /// The last calibration date a user typed in, e.g. for items brought over from
    /// another system. Kept separately so it returns if every record is archived.
    pub last_cal_date_entered: Option<NaiveDate>,
    /// Derived: last calibration date plus the interval. Never set directly.
    pub next_cal_date: Option<NaiveDate>,
    /// A calibration extension sets `next_cal_date` (see [`crate::schedule::apply_extension`]).
    pub calibration_extended: bool,
    /// Systems only: the schedule comes from the System's Instruments. `next_cal_date` is
    /// then the soonest next calibration among its tracked, unarchived Instruments.
    pub cal_aggregate: bool,
    pub due_status: DueStatus,
    pub child_count: i64,
    pub notes: Option<String>,
    pub is_archived: bool,
    pub row_version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Fields a user can set when creating or editing an Instrument or System.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AssetInput {
    pub asset_tag: String,
    pub kind: AssetKind,
    pub parent_id: Option<Id>,
    pub description: String,
    pub manufacturer_id: Option<Id>,
    pub model_number: Option<String>,
    pub serial_number: Option<String>,
    pub location_id: Option<Id>,
    pub department_id: Option<Id>,
    pub owner_id: Option<Id>,
    pub status_id: Id,
    pub warranty_end_date: Option<NaiveDate>,
    pub cal_interval: Option<CalInterval>,
    pub in_service_date: Option<NaiveDate>,
    /// Filled in from the earliest calibration record if left empty.
    pub first_cal_date: Option<NaiveDate>,
    /// The last calibration before PolarisCal tracked the item. It is the item's
    /// last calibration date only while it has no calibration records; afterwards
    /// the latest record's date wins, but this value is kept.
    pub last_cal_date: Option<NaiveDate>,
    pub notes: Option<String>,
    /// Systems only: take the schedule from the System's Instruments (no interval or
    /// calibration dates of its own).
    #[serde(default)]
    pub cal_aggregate: bool,
}

impl AssetInput {
    /// Trims text, turns blank optional text into `None`, and checks the rules
    /// that don't need the database. Parent/child rules are checked by the store.
    pub fn normalize(&mut self) -> Result<()> {
        let mut errors = Vec::new();

        self.asset_tag = self.asset_tag.trim().to_string();
        self.description = self.description.trim().to_string();
        for field in [
            &mut self.model_number,
            &mut self.serial_number,
            &mut self.notes,
        ] {
            clean(field);
        }

        // Only Systems can aggregate, and an aggregate System has no schedule of its own.
        if self.kind != AssetKind::System {
            self.cal_aggregate = false;
        }
        if self.cal_aggregate {
            self.cal_interval = None;
            self.first_cal_date = None;
            self.last_cal_date = None;
        }

        // No warranty date given: assume the standard one year from going into service.
        if self.warranty_end_date.is_none() {
            self.warranty_end_date = self
                .in_service_date
                .and_then(|d| d.checked_add_months(chrono::Months::new(12)));
        }

        if self.asset_tag.is_empty() {
            errors.push(FieldError::new("asset_tag", "Asset tag is required."));
        } else if self.asset_tag.chars().count() > 64 {
            errors.push(FieldError::new(
                "asset_tag",
                "Asset tag must be 64 characters or fewer.",
            ));
        }
        if self.description.is_empty() {
            errors.push(FieldError::new("description", "Description is required."));
        }
        if matches!(self.cal_interval, Some(i) if i.value == 0) {
            errors.push(FieldError::new(
                "cal_interval",
                "Calibration interval must be greater than zero.",
            ));
        }
        if self.kind == AssetKind::System && self.parent_id.is_some() {
            errors.push(FieldError::new(
                "parent_id",
                "A System cannot belong to another System.",
            ));
        }
        if let (Some(first), Some(last)) = (self.first_cal_date, self.last_cal_date)
            && first > last
        {
            errors.push(FieldError::new(
                "first_cal_date",
                "First calibration date cannot be after the last calibration date.",
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(Error::Validation(errors))
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AssetFilter {
    /// Matches asset tag, description, model number, or serial number.
    pub search: Option<String>,
    pub kind: Option<AssetKind>,
    pub parent_id: Option<Id>,
    #[serde(default)]
    pub include_archived: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct CalibrationRecord {
    pub id: Id,
    pub asset_id: Id,
    pub asset_tag: String,
    pub cal_date: NaiveDate,
    /// What kind of service this was (Calibration, Repair, ...).
    pub service_type_id: Id,
    pub service_type_name: String,
    /// Whether this record's service type counts as a calibration (and so sets due dates).
    pub counts_as_calibration: bool,
    /// A calibration extension: authorizes use for `extension_days` past the due date.
    pub is_extension: bool,
    /// Whether this record's service type is a repair.
    pub is_repair: bool,
    /// Extensions only: how many days from `cal_date` the extension lasts.
    pub extension_days: Option<i32>,
    /// Whether a calibration provider or a manufacturer did the work (none for extensions).
    pub calibrated_by: Option<CalibratedBy>,
    /// The calibration provider's or manufacturer's id, per `calibrated_by`.
    pub calibrated_by_id: Option<Id>,
    /// The calibration provider's or manufacturer's name.
    pub calibrated_by_name: Option<String>,
    /// The individual who performed the work, if recorded.
    pub technician: Option<String>,
    /// Cost in cents to avoid floating-point rounding.
    pub cost_cents: Option<i64>,
    pub certificate_number: Option<String>,
    pub condition_received: ConditionReceived,
    pub condition_returned: ConditionReturned,
    /// Due date this calibration established (cal date + the item's interval at the time).
    pub next_due_date: Option<NaiveDate>,
    pub notes: Option<String>,
    /// Non-archived files attached to this record (certificates, data sheets).
    pub attachment_count: i64,
    pub is_archived: bool,
    pub row_version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CalibrationInput {
    pub asset_id: Id,
    pub cal_date: NaiveDate,
    /// What kind of service this was (a service type list entry).
    pub service_type_id: Id,
    /// Whether `calibrated_by_id` is a calibration provider or a manufacturer.
    pub calibrated_by: CalibratedBy,
    /// Required, except for calibration extensions (which have no performer).
    pub calibrated_by_id: Option<Id>,
    /// Calibration extensions only: how many days the extension lasts (at least 1).
    #[serde(default)]
    pub extension_days: Option<i32>,
    pub technician: Option<String>,
    pub cost_cents: Option<i64>,
    pub certificate_number: Option<String>,
    pub condition_received: ConditionReceived,
    pub condition_returned: ConditionReturned,
    pub notes: Option<String>,
    /// Also change the item's status, in the same save (e.g. to Out for Repair or
    /// Retired after a failed calibration).
    #[serde(default)]
    pub new_status_id: Option<Id>,
}

impl CalibrationInput {
    pub fn normalize(&mut self, today: NaiveDate) -> Result<()> {
        let mut errors = Vec::new();
        for field in [
            &mut self.technician,
            &mut self.certificate_number,
            &mut self.notes,
        ] {
            clean(field);
        }
        if self.cal_date > today {
            errors.push(FieldError::new(
                "cal_date",
                "Calibration date cannot be in the future.",
            ));
        }
        if matches!(self.cost_cents, Some(c) if c < 0) {
            errors.push(FieldError::new("cost_cents", "Cost cannot be negative."));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(Error::Validation(errors))
        }
    }
}

/// What a provider last charged for a service on the same make and model, offered as the
/// cost of a new service record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct LastCost {
    pub cost_cents: i64,
    pub cal_date: NaiveDate,
    /// The item that service record belongs to.
    pub asset_tag: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CalibrationFilter {
    pub asset_id: Option<Id>,
    #[serde(default)]
    pub include_archived: bool,
    /// Only calibrations on or after this date.
    #[serde(default)]
    pub from_date: Option<NaiveDate>,
    /// Only calibrations on or before this date.
    #[serde(default)]
    pub to_date: Option<NaiveDate>,
}

/// An entry in one of the admin-managed pick lists.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct LookupItem {
    pub id: Id,
    pub kind: LookupKind,
    pub name: String,
    pub description: Option<String>,
    pub is_active: bool,
    pub sort_order: i32,
    /// Providers only: an internal technician rather than an outside vendor.
    pub is_internal: Option<bool>,
    /// Statuses only: whether items in this status appear in due/overdue tracking.
    pub tracks_due: Option<bool>,
    /// Service types only: whether records of this type are calibrations, which set
    /// an item's last calibration and next due dates.
    pub counts_as_calibration: Option<bool>,
    /// Service types only: the built-in Calibration Extension type.
    pub is_extension: Option<bool>,
    /// Service types only: whether records of this type are repairs.
    pub is_repair: Option<bool>,
    /// Statuses only: set on the built-in statuses, which can't be deleted.
    pub status_role: Option<StatusRole>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LookupInput {
    pub name: String,
    pub description: Option<String>,
    pub is_active: bool,
    pub sort_order: i32,
    pub is_internal: Option<bool>,
    pub tracks_due: Option<bool>,
    #[serde(default)]
    pub counts_as_calibration: Option<bool>,
    #[serde(default)]
    pub is_repair: Option<bool>,
}

impl LookupInput {
    pub fn normalize(&mut self) -> Result<()> {
        self.name = self.name.trim().to_string();
        clean(&mut self.description);
        if self.name.is_empty() {
            return Err(Error::validation("name", "Name is required."));
        }
        if self.name.chars().count() > 100 {
            return Err(Error::validation(
                "name",
                "Name must be 100 characters or fewer.",
            ));
        }
        Ok(())
    }
}

/// A file attached to an Instrument/System or to one of its calibration records.
/// The file itself lives under the shared attachments folder; the database keeps
/// its path relative to that folder plus a SHA-256 fingerprint to detect changes.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Attachment {
    pub id: Id,
    pub asset_id: Id,
    pub asset_tag: String,
    /// Set when the file belongs to a calibration record (e.g. its certificate).
    pub calibration_id: Option<Id>,
    pub calibration_date: Option<NaiveDate>,
    pub category: AttachmentCategory,
    /// The original file name, as shown to users.
    pub file_name: String,
    /// Path under the attachments folder, with `/` separators.
    pub relative_path: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub content_type: Option<String>,
    pub description: Option<String>,
    pub uploaded_by: String,
    pub is_archived: bool,
    pub row_version: i64,
    pub created_at: DateTime<Utc>,
}

/// A file that has already been copied into the attachments folder and now
/// needs its database record.
#[derive(Debug, Clone, PartialEq)]
pub struct NewAttachment {
    pub asset_id: Id,
    pub calibration_id: Option<Id>,
    pub category: AttachmentCategory,
    pub file_name: String,
    pub relative_path: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub content_type: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AttachmentFilter {
    pub asset_id: Option<Id>,
    pub calibration_id: Option<Id>,
    #[serde(default)]
    pub include_archived: bool,
}

/// Organization-wide settings stored in the database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AppSettings {
    /// Minutes without activity before the app locks and asks for the password.
    pub idle_lock_minutes: u32,
    /// Folder (ideally a UNC share such as `\\server\share\PolarisCal`) where attached
    /// files are stored. Attaching files is unavailable until it is set.
    pub attachments_root: Option<String>,
    /// Whether PolarisCal checks online for new versions. Turn off where IT deploys updates.
    #[serde(default = "default_true")]
    pub update_checks: bool,
    /// Shown before amounts of money throughout the app, e.g. "$" or "€".
    #[serde(default = "default_currency")]
    pub currency_symbol: String,
    /// How dates are shown throughout the app: one of [`AppSettings::DATE_FORMATS`].
    /// "system" follows each PC's Windows region setting.
    #[serde(default = "default_date_format")]
    pub date_format: String,
    /// What new passwords must contain and how long passwords last.
    #[serde(default)]
    pub password_policy: PasswordPolicy,
}

fn default_currency() -> String {
    AppSettings::DEFAULT_CURRENCY.into()
}

fn default_date_format() -> String {
    AppSettings::DEFAULT_DATE_FORMAT.into()
}

fn default_true() -> bool {
    true
}

impl AppSettings {
    pub const DEFAULT_IDLE_LOCK_MINUTES: u32 = 15;
    pub const DEFAULT_CURRENCY: &str = "$";
    pub const DEFAULT_DATE_FORMAT: &str = "system";
    /// The date formats the app knows how to show (see `src/lib/dates.ts`).
    pub const DATE_FORMATS: &[&str] = &[
        "system",
        "mdy_numeric",
        "mdy_medium",
        "mdy_long",
        "iso",
        "dmy_slash",
        "dmy_dot",
        "dmy_dash",
        "dmy_medium",
        "dmy_long",
        "dmy_mil",
        "ymd_slash",
    ];

    /// Trims the folder path (blank means unset) and checks the values. Whether the
    /// folder is reachable and writable is checked by the app, not here.
    pub fn normalize(&mut self) -> Result<()> {
        clean(&mut self.attachments_root);
        self.currency_symbol = self.currency_symbol.trim().to_string();
        if self.currency_symbol.is_empty() || self.currency_symbol.chars().count() > 5 {
            return Err(Error::validation(
                "currency_symbol",
                "Choose a currency symbol of up to 5 characters.",
            ));
        }
        if !Self::DATE_FORMATS.contains(&self.date_format.as_str()) {
            return Err(Error::validation(
                "date_format",
                "Choose one of the listed date formats.",
            ));
        }
        self.password_policy.normalize()?;
        if !(1..=240).contains(&self.idle_lock_minutes) {
            return Err(Error::validation(
                "idle_lock_minutes",
                "Auto-lock must be between 1 and 240 minutes.",
            ));
        }
        if let Some(root) = &self.attachments_root
            && !is_absolute_windows_path(root)
        {
            return Err(Error::validation(
                "attachments_root",
                "Use a full path such as \\\\server\\share\\PolarisCal or D:\\PolarisCal.",
            ));
        }
        Ok(())
    }
}

/// `\\server\share...` or `C:\...` (either slash direction).
fn is_absolute_windows_path(p: &str) -> bool {
    let b = p.as_bytes();
    let unc = p.starts_with("\\\\") || p.starts_with("//");
    let drive = b.len() >= 3
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && (b[2] == b'\\' || b[2] == b'/');
    unc || drive
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            idle_lock_minutes: Self::DEFAULT_IDLE_LOCK_MINUTES,
            attachments_root: None,
            update_checks: true,
            currency_symbol: Self::DEFAULT_CURRENCY.into(),
            date_format: Self::DEFAULT_DATE_FORMAT.into(),
            password_policy: PasswordPolicy::default(),
        }
    }
}

/// Which database the app is connected to and what's in it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct StoreInfo {
    /// "sqlite", "postgres", "mysql", or "mssql".
    pub backend: String,
    /// File path or host/database name, never including a password.
    pub location: String,
    pub schema_version: i64,
    pub asset_count: i64,
    pub calibration_count: i64,
}

fn clean(field: &mut Option<String>) {
    if let Some(s) = field {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            *field = None;
        } else if trimmed.len() != s.len() {
            *field = Some(trimmed.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> AssetInput {
        AssetInput {
            asset_tag: "  DMM-001 ".into(),
            kind: AssetKind::Instrument,
            parent_id: None,
            description: "Digital multimeter".into(),
            manufacturer_id: None,
            model_number: Some("   ".into()),
            serial_number: Some(" SN123 ".into()),
            location_id: None,
            department_id: None,
            owner_id: None,
            status_id: 1,
            warranty_end_date: None,
            cal_interval: Some(CalInterval {
                value: 12,
                unit: IntervalUnit::Months,
                end_of_month: false,
            }),
            in_service_date: None,
            first_cal_date: None,
            last_cal_date: None,
            notes: None,
            cal_aggregate: false,
        }
    }

    #[test]
    fn normalize_trims_and_blanks() {
        let mut i = input();
        i.normalize().unwrap();
        assert_eq!(i.asset_tag, "DMM-001");
        assert_eq!(i.model_number, None);
        assert_eq!(i.serial_number.as_deref(), Some("SN123"));
    }

    #[test]
    fn normalize_collects_all_errors() {
        let mut i = input();
        i.asset_tag = " ".into();
        i.description = String::new();
        i.cal_interval = Some(CalInterval {
            value: 0,
            unit: IntervalUnit::Days,
            end_of_month: false,
        });
        let Err(Error::Validation(errors)) = i.normalize() else {
            panic!("expected validation error")
        };
        let fields: Vec<_> = errors.iter().map(|e| e.field.as_str()).collect();
        assert_eq!(fields, ["asset_tag", "description", "cal_interval"]);
    }

    #[test]
    fn settings_normalize_paths() {
        let mut s = AppSettings {
            idle_lock_minutes: 15,
            attachments_root: Some("   ".into()),
            update_checks: true,
            currency_symbol: "$".into(),
            date_format: "system".into(),
            ..Default::default()
        };
        s.normalize().unwrap();
        assert_eq!(s.attachments_root, None, "blank means unset");

        for ok in [r"\\server\share\PolarisCal", r"D:\Cal", "//server/share"] {
            let mut s = AppSettings {
                idle_lock_minutes: 15,
                attachments_root: Some(ok.into()),
                update_checks: true,
                currency_symbol: "$".into(),
                date_format: "system".into(),
                ..Default::default()
            };
            assert!(s.normalize().is_ok(), "{ok}");
        }
        for bad in ["relative\\folder", "Cal", "C:"] {
            let mut s = AppSettings {
                idle_lock_minutes: 15,
                attachments_root: Some(bad.into()),
                update_checks: true,
                currency_symbol: "$".into(),
                date_format: "system".into(),
                ..Default::default()
            };
            assert!(
                matches!(s.normalize(), Err(Error::Validation(ref e)) if e[0].field == "attachments_root"),
                "{bad}"
            );
        }
        let mut s = AppSettings {
            idle_lock_minutes: 0,
            attachments_root: None,
            update_checks: true,
            currency_symbol: "$".into(),
            date_format: "system".into(),
            ..Default::default()
        };
        assert!(s.normalize().is_err());
    }

    #[test]
    fn system_cannot_have_parent() {
        let mut i = input();
        i.kind = AssetKind::System;
        i.parent_id = Some(5);
        assert!(matches!(i.normalize(), Err(Error::Validation(_))));
    }

    #[test]
    fn text_enums_round_trip() {
        for kind in ConditionReceived::ALL {
            assert_eq!(kind.as_str().parse::<ConditionReceived>().unwrap(), *kind);
            // serde (UI) and the database must agree on the text form
            assert_eq!(
                serde_json::to_string(kind).unwrap(),
                format!("\"{}\"", kind.as_str())
            );
        }
        for kind in ConditionReturned::ALL {
            assert_eq!(
                serde_json::to_string(kind).unwrap(),
                format!("\"{}\"", kind.as_str())
            );
        }
        for kind in DueStatus::ALL {
            assert_eq!(
                serde_json::to_string(kind).unwrap(),
                format!("\"{}\"", kind.as_str())
            );
        }
        assert!("bogus".parse::<AssetKind>().is_err());
    }

    #[test]
    fn warranty_defaults_to_a_year_after_going_into_service() {
        let d = |y, m, day| NaiveDate::from_ymd_opt(y, m, day).unwrap();
        let mut i = input();
        i.in_service_date = Some(d(2026, 2, 28));
        i.normalize().unwrap();
        assert_eq!(i.warranty_end_date, Some(d(2027, 2, 28)));

        // A date that's entered is kept.
        let mut i = input();
        i.in_service_date = Some(d(2026, 2, 28));
        i.warranty_end_date = Some(d(2029, 1, 1));
        i.normalize().unwrap();
        assert_eq!(i.warranty_end_date, Some(d(2029, 1, 1)));

        // Nothing to go on: no warranty date.
        let mut i = input();
        i.normalize().unwrap();
        assert_eq!(i.warranty_end_date, None);
    }
}
