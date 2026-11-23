//! The audit trail: who changed what, when, why, and the before/after values.
//!
//! Backends write one [`AuditEntry`] in the same transaction as each change, using
//! the snapshot functions here so every backend records identical field names and
//! value formats.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::auth::User;
use crate::model::{
    Asset, Attachment, AuditAction, AuditEntity, CalInterval, CalibratedBy, CalibrationRecord,
    IntervalUnit, LookupItem,
};
use crate::{Error, Id, Result};

/// The signed-in user making a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    pub user_id: Id,
    pub username: String,
}

/// Who is writing and, for edits that need one, why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteContext {
    pub actor: Actor,
    pub reason: Option<String>,
}

impl WriteContext {
    pub fn new(actor: Actor, reason: Option<String>) -> Self {
        let reason = reason
            .map(|r| r.trim().to_string())
            .filter(|r| !r.is_empty());
        Self { actor, reason }
    }

    /// Edits and archives of existing items and calibration records must say why.
    pub fn require_reason(&self) -> Result<&str> {
        match self.reason.as_deref() {
            Some(r) if r.chars().count() <= 500 => Ok(r),
            Some(_) => Err(Error::validation(
                "reason",
                "Keep the reason under 500 characters.",
            )),
            None => Err(Error::validation(
                "reason",
                "Enter a reason for this change.",
            )),
        }
    }
}

/// One field's value before and after a change. `None` means empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FieldChange {
    pub field: String,
    pub old: Option<String>,
    pub new: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct AuditEntry {
    pub id: Id,
    pub occurred_at: DateTime<Utc>,
    /// Empty for failed sign-ins with an unknown username.
    pub user_id: Option<Id>,
    /// The username at the time, kept even if the account is later renamed.
    pub username: String,
    pub action: AuditAction,
    pub entity: AuditEntity,
    pub entity_id: Option<Id>,
    /// The asset this entry concerns (an asset itself, or a calibration record's asset).
    pub asset_id: Option<Id>,
    /// A readable name for the subject at the time, e.g. an asset tag.
    pub entity_label: Option<String>,
    pub reason: Option<String>,
    pub changes: Vec<FieldChange>,
}

/// An entry about to be written.
#[derive(Debug, Clone, PartialEq)]
pub struct NewAuditEntry {
    pub user_id: Option<Id>,
    pub username: String,
    pub action: AuditAction,
    pub entity: AuditEntity,
    pub entity_id: Option<Id>,
    pub asset_id: Option<Id>,
    pub entity_label: Option<String>,
    pub reason: Option<String>,
    pub changes: Vec<FieldChange>,
}

impl NewAuditEntry {
    /// An entry attributed to the context's user, carrying its reason.
    pub fn by(ctx: &WriteContext, action: AuditAction, entity: AuditEntity) -> Self {
        Self {
            user_id: Some(ctx.actor.user_id),
            username: ctx.actor.username.clone(),
            action,
            entity,
            entity_id: None,
            asset_id: None,
            entity_label: None,
            reason: ctx.reason.clone(),
            changes: Vec::new(),
        }
    }

    pub fn subject(mut self, id: Id, label: impl Into<String>) -> Self {
        self.entity_id = Some(id);
        self.entity_label = Some(label.into());
        self
    }

    pub fn asset(mut self, asset_id: Id) -> Self {
        self.asset_id = Some(asset_id);
        self
    }

    pub fn changes(mut self, changes: Vec<FieldChange>) -> Self {
        self.changes = changes;
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AuditFilter {
    /// Entries about this asset, including its calibration records.
    pub asset_id: Option<Id>,
    pub entity: Option<AuditEntity>,
    pub user_id: Option<Id>,
    /// Return entries older than this id (for paging); newest first.
    pub before_id: Option<Id>,
    /// Defaults to 200, capped at 1000.
    pub limit: Option<i64>,
}

impl AuditFilter {
    pub fn effective_limit(&self) -> i64 {
        self.limit.unwrap_or(200).clamp(1, 1000)
    }
}

// ---------------------------------------------------------------------------
// Snapshots and diffs
// ---------------------------------------------------------------------------

/// Field label → display value, in a fixed order.
pub type Snapshot = Vec<(&'static str, Option<String>)>;

fn opt<T: ToString>(v: &Option<T>) -> Option<String> {
    v.as_ref().map(ToString::to_string)
}

fn yes_no(b: bool) -> Option<String> {
    Some(if b { "Yes" } else { "No" }.to_string())
}

fn interval(i: &Option<CalInterval>) -> Option<String> {
    i.map(|i| {
        let unit = match (i.unit, i.value) {
            (IntervalUnit::Months, 1) => "month",
            (IntervalUnit::Months, _) => "months",
            (IntervalUnit::Days, 1) => "day",
            (IntervalUnit::Days, _) => "days",
        };
        let eom = if i.end_of_month { ", end of month" } else { "" };
        format!("{} {unit}{eom}", i.value)
    })
}

fn cents(c: Option<i64>) -> Option<String> {
    c.map(|c| format!("{}.{:02}", c / 100, c % 100))
}

pub fn asset_snapshot(a: &Asset) -> Snapshot {
    vec![
        ("Asset tag", Some(a.asset_tag.clone())),
        ("Type", Some(a.kind.to_string())),
        ("Part of System", a.parent_tag.clone()),
        ("Description", Some(a.description.clone())),
        ("Manufacturer", a.manufacturer_name.clone()),
        ("Model number", a.model_number.clone()),
        ("Serial number", a.serial_number.clone()),
        ("Location", a.location_name.clone()),
        ("Department", a.department_name.clone()),
        ("Owner", a.owner_name.clone()),
        ("Status", Some(a.status_name.clone())),
        ("Warranty ends", opt(&a.warranty_end_date)),
        (
            "Calibration interval",
            if a.cal_aggregate {
                Some("From its Instruments".to_string())
            } else {
                interval(&a.cal_interval)
            },
        ),
        ("Put into service", opt(&a.in_service_date)),
        ("First calibration", opt(&a.first_cal_date)),
        ("Last calibration", opt(&a.last_cal_date)),
        ("Last calibration (entered)", opt(&a.last_cal_date_entered)),
        ("Next calibration", opt(&a.next_cal_date)),
        ("Notes", a.notes.clone()),
        ("Archived", yes_no(a.is_archived)),
    ]
}

/// "Acme Labs", or "Fluke (manufacturer)".
fn calibrated_by(r: &CalibrationRecord) -> Option<String> {
    let name = r.calibrated_by_name.clone()?;
    Some(match r.calibrated_by? {
        CalibratedBy::Provider => name,
        CalibratedBy::Manufacturer => format!("{name} (manufacturer)"),
    })
}

pub fn calibration_snapshot(r: &CalibrationRecord) -> Snapshot {
    vec![
        ("Date of service", Some(r.cal_date.to_string())),
        ("Service type", Some(r.service_type_name.clone())),
        ("Performed by", calibrated_by(r)),
        ("Extension (days)", r.extension_days.map(|d| d.to_string())),
        ("Technician", r.technician.clone()),
        ("Cost", cents(r.cost_cents)),
        ("Certificate number", r.certificate_number.clone()),
        ("Condition received", Some(r.condition_received.to_string())),
        ("Condition returned", Some(r.condition_returned.to_string())),
        ("Due date set", opt(&r.next_due_date)),
        ("Notes", r.notes.clone()),
        ("Archived", yes_no(r.is_archived)),
    ]
}

pub fn lookup_snapshot(l: &LookupItem) -> Snapshot {
    let mut s: Snapshot = vec![
        ("Name", Some(l.name.clone())),
        ("Description", l.description.clone()),
        ("Sort order", Some(l.sort_order.to_string())),
        ("Active", yes_no(l.is_active)),
    ];
    if let Some(v) = l.is_internal {
        s.push(("Internal", yes_no(v)));
    }
    if let Some(v) = l.tracks_due {
        s.push(("Track due dates", yes_no(v)));
    }
    if let Some(v) = l.counts_as_calibration {
        s.push(("Counts as a calibration", yes_no(v)));
    }
    if let Some(v) = l.is_repair {
        s.push(("Repair", yes_no(v)));
    }
    s
}

pub fn attachment_snapshot(a: &Attachment) -> Snapshot {
    vec![
        ("File name", Some(a.file_name.clone())),
        ("Category", Some(a.category.to_string())),
        ("Calibration record", opt(&a.calibration_date)),
        ("Stored as", Some(a.relative_path.clone())),
        ("Size (bytes)", Some(a.size_bytes.to_string())),
        ("SHA-256", Some(a.sha256.clone())),
        ("Description", a.description.clone()),
        ("Archived", yes_no(a.is_archived)),
    ]
}

pub fn user_snapshot(u: &User) -> Snapshot {
    vec![
        ("Username", Some(u.username.clone())),
        ("Name", Some(u.display_name.clone())),
        ("Email", u.email.clone()),
        ("Role", Some(u.role.to_string())),
        ("Active", yes_no(u.is_active)),
        ("Must change password", yes_no(u.must_change_password)),
        (
            "Password reset requested",
            yes_no(u.reset_requested_at.is_some()),
        ),
    ]
}

/// The fields that differ between two snapshots of the same kind.
pub fn diff(before: &Snapshot, after: &Snapshot) -> Vec<FieldChange> {
    before
        .iter()
        .zip(after)
        .filter(|((_, old), (_, new))| old != new)
        .map(|((field, old), (_, new))| FieldChange {
            field: (*field).to_string(),
            old: old.clone(),
            new: new.clone(),
        })
        .collect()
}

/// For a newly created record: every field that has a value.
pub fn created(after: &Snapshot) -> Vec<FieldChange> {
    after
        .iter()
        .filter(|(field, v)| v.is_some() && *field != "Archived")
        .map(|(field, v)| FieldChange {
            field: (*field).to_string(),
            old: None,
            new: v.clone(),
        })
        .collect()
}

/// For a deleted record: every field it had, so the audit trail keeps what was removed.
pub fn deleted(before: &Snapshot) -> Vec<FieldChange> {
    before
        .iter()
        .filter(|(_, v)| v.is_some())
        .map(|(field, v)| FieldChange {
            field: (*field).to_string(),
            old: v.clone(),
            new: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor() -> Actor {
        Actor {
            user_id: 1,
            username: "admin".into(),
        }
    }

    #[test]
    fn reason_is_trimmed_and_required() {
        let ctx = WriteContext::new(actor(), Some("   ".into()));
        assert_eq!(ctx.reason, None);
        assert!(
            matches!(ctx.require_reason(), Err(Error::Validation(e)) if e[0].field == "reason")
        );
        let ctx = WriteContext::new(actor(), Some(" Wrong serial ".into()));
        assert_eq!(ctx.require_reason().unwrap(), "Wrong serial");
    }

    #[test]
    fn diff_reports_only_changed_fields() {
        let before: Snapshot = vec![
            ("A", Some("1".into())),
            ("B", None),
            ("C", Some("x".into())),
        ];
        let after: Snapshot = vec![
            ("A", Some("1".into())),
            ("B", Some("2".into())),
            ("C", None),
        ];
        let d = diff(&before, &after);
        assert_eq!(d.len(), 2);
        assert_eq!(
            d[0],
            FieldChange {
                field: "B".into(),
                old: None,
                new: Some("2".into())
            }
        );
        assert_eq!(d[1].new, None);
    }

    #[test]
    fn created_lists_filled_fields() {
        let after: Snapshot = vec![
            ("A", Some("1".into())),
            ("B", None),
            ("Archived", Some("No".into())),
        ];
        let c = created(&after);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].field, "A");
    }

    #[test]
    fn money_and_interval_formatting() {
        assert_eq!(cents(Some(12_550)).as_deref(), Some("125.50"));
        assert_eq!(cents(Some(5)).as_deref(), Some("0.05"));
        let one = Some(CalInterval {
            value: 1,
            unit: IntervalUnit::Months,
            end_of_month: false,
        });
        assert_eq!(interval(&one).as_deref(), Some("1 month"));
    }

    #[test]
    fn filter_limit_is_clamped() {
        assert_eq!(AuditFilter::default().effective_limit(), 200);
        assert_eq!(
            AuditFilter {
                limit: Some(5000),
                ..Default::default()
            }
            .effective_limit(),
            1000
        );
    }
}
