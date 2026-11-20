//! Behavioral tests for the `Store` contract. They only use the trait, so the
//! same tests can run against every backend; today that is SQLite.

use std::sync::Arc;

use chrono::{Days, Local, NaiveDate};
use polaris_core::audit::{Actor, AuditFilter, WriteContext};
use polaris_core::auth::{NewUser, UserUpdate, verify_password};
use polaris_core::model::*;
use polaris_core::{Error, Store};
use polaris_store::DatabaseConfig;

struct Harness {
    store: Arc<dyn Store>,
    _dir: tempfile::TempDir,
}

async fn sqlite() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("polariscal.db");
    let store = polaris_store::open(&DatabaseConfig::Sqlite {
        path: path.display().to_string(),
    })
    .await
    .expect("open sqlite store");
    setup_admin(store.as_ref()).await;
    Harness { store, _dir: dir }
}

const ADMIN_PASSWORD: &str = "correct horse battery";

fn new_user(username: &str, role: Role) -> NewUser {
    NewUser {
        username: username.into(),
        display_name: format!("{username} name"),
        email: None,
        role,
        password: ADMIN_PASSWORD.into(),
        must_change_password: false,
    }
}

/// Creates the first admin (id 1), whom `ctx()` acts as.
async fn setup_admin(store: &dyn Store) {
    let admin = store
        .create_first_admin(new_user("admin", Role::Admin))
        .await
        .expect("create first admin");
    assert_eq!(admin.id, 1);
}

/// Acting as the first admin, with a reason (needed for edits and archives).
fn ctx() -> WriteContext {
    WriteContext::new(
        Actor {
            user_id: 1,
            username: "admin".into(),
        },
        Some("test change".into()),
    )
}

fn ctx_without_reason() -> WriteContext {
    WriteContext::new(
        Actor {
            user_id: 1,
            username: "admin".into(),
        },
        None,
    )
}

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn lookup(name: &str) -> LookupInput {
    LookupInput {
        name: name.into(),
        description: None,
        is_active: true,
        sort_order: 0,
        is_internal: None,
        tracks_due: None,
    }
}

async fn status_id(store: &dyn Store, name: &str) -> i64 {
    store
        .list_lookups(LookupKind::Status, true)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("status {name} not seeded"))
        .id
}

async fn asset_input(store: &dyn Store, tag: &str, kind: AssetKind) -> AssetInput {
    AssetInput {
        asset_tag: tag.into(),
        kind,
        parent_id: None,
        description: format!("{tag} description"),
        manufacturer_id: None,
        model_number: None,
        serial_number: None,
        location_id: None,
        department_id: None,
        owner_id: None,
        status_id: status_id(store, "Active").await,
        warranty_end_date: None,
        cal_interval: Some(CalInterval {
            value: 12,
            unit: IntervalUnit::Months,
        }),
        in_service_date: None,
        first_cal_date: None,
        last_cal_date: None,
        notes: None,
    }
}

async fn provider(store: &dyn Store) -> i64 {
    let mut input = lookup("Acme Calibration Labs");
    input.is_internal = Some(false);
    store
        .create_lookup(&ctx(), LookupKind::Provider, input)
        .await
        .unwrap()
        .id
}

fn cal(asset_id: i64, provider_id: i64, date: NaiveDate) -> CalibrationInput {
    CalibrationInput {
        asset_id,
        cal_date: date,
        provider_id,
        technician: Some("  J. Smith ".into()),
        cost_cents: Some(12_500),
        certificate_number: Some("CERT-001".into()),
        condition_received: ConditionReceived::OutOfTolerance,
        condition_returned: ConditionReturned::Pass,
        notes: None,
    }
}

#[tokio::test]
async fn fresh_database_is_migrated_and_seeded() {
    let h = sqlite().await;
    let info = h.store.info().await.unwrap();
    assert_eq!(info.backend, "sqlite");
    assert_eq!(info.schema_version, 4);
    assert_eq!(info.asset_count, 0);

    let statuses = h
        .store
        .list_lookups(LookupKind::Status, true)
        .await
        .unwrap();
    let names: Vec<_> = statuses.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Active",
            "Out for Calibration",
            "Out of Service",
            "Calibration Not Required",
            "Retired",
            "Lost / Missing"
        ]
    );
    assert_eq!(statuses[0].tracks_due, Some(true));
    assert_eq!(statuses[2].tracks_due, Some(false));
    assert_eq!(statuses[0].is_internal, None);
}

#[tokio::test]
async fn reopening_an_existing_database_keeps_data() {
    let dir = tempfile::tempdir().unwrap();
    let config = DatabaseConfig::Sqlite {
        path: dir.path().join("p.db").display().to_string(),
    };
    {
        let store = polaris_store::open(&config).await.unwrap();
        setup_admin(store.as_ref()).await;
        let input = asset_input(store.as_ref(), "KEEP-1", AssetKind::Instrument).await;
        store.create_asset(&ctx(), input).await.unwrap();
    }
    let store = polaris_store::open(&config).await.unwrap();
    assert_eq!(store.info().await.unwrap().asset_count, 1);
}

#[tokio::test]
async fn lookups_create_update_and_reject_duplicates() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    let loc = s
        .create_lookup(&ctx(), LookupKind::Location, lookup(" Lab A "))
        .await
        .unwrap();
    assert_eq!(loc.name, "Lab A");
    assert_eq!(loc.kind, LookupKind::Location);

    let dup = s
        .create_lookup(&ctx(), LookupKind::Location, lookup("LAB A"))
        .await;
    assert!(
        matches!(dup, Err(Error::Duplicate(_))),
        "names are unique ignoring case: {dup:?}"
    );

    // the same name is fine in a different list
    s.create_lookup(&ctx(), LookupKind::Department, lookup("Lab A"))
        .await
        .unwrap();

    let mut edit = lookup("Lab A (Building 2)");
    edit.is_active = false;
    let updated = s
        .update_lookup(&ctx(), LookupKind::Location, loc.id, edit)
        .await
        .unwrap();
    assert!(!updated.is_active);
    assert!(
        s.list_lookups(LookupKind::Location, false)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        s.list_lookups(LookupKind::Location, true)
            .await
            .unwrap()
            .len(),
        1
    );

    let vendor = s
        .create_lookup(&ctx(), LookupKind::Provider, lookup("Default Vendor"))
        .await
        .unwrap();
    assert_eq!(
        vendor.is_internal,
        Some(false),
        "providers default to external vendors"
    );
    let status = s
        .create_lookup(&ctx(), LookupKind::Status, lookup("Quarantined"))
        .await
        .unwrap();
    assert_eq!(
        status.tracks_due,
        Some(true),
        "new statuses track due dates by default"
    );

    let missing = s
        .update_lookup(&ctx(), LookupKind::Location, 9999, lookup("x"))
        .await;
    assert!(matches!(missing, Err(Error::NotFound { .. })));
}

#[tokio::test]
async fn asset_create_joins_lookups_and_derives_next_date() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    let mfr = s
        .create_lookup(&ctx(), LookupKind::Manufacturer, lookup("Fluke"))
        .await
        .unwrap();
    let mut input = asset_input(s, "DMM-001", AssetKind::Instrument).await;
    input.manufacturer_id = Some(mfr.id);
    input.last_cal_date = Some(d(2026, 3, 1));
    input.serial_number = Some(" 12345 ".into());

    let a = s.create_asset(&ctx(), input).await.unwrap();
    assert_eq!(a.asset_tag, "DMM-001");
    assert_eq!(a.manufacturer_name.as_deref(), Some("Fluke"));
    assert_eq!(a.status_name, "Active");
    assert_eq!(a.serial_number.as_deref(), Some("12345"));
    assert_eq!(a.next_cal_date, Some(d(2027, 3, 1)));
    assert_eq!(a.row_version, 1);

    let dup = s
        .create_asset(
            &ctx(),
            asset_input(s, "dmm-001", AssetKind::Instrument).await,
        )
        .await;
    assert!(matches!(dup, Err(Error::Duplicate(_))), "{dup:?}");
}

#[tokio::test]
async fn hierarchy_rules_are_enforced() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    let system = s
        .create_asset(&ctx(), asset_input(s, "RIG-1", AssetKind::System).await)
        .await
        .unwrap();
    let other = s
        .create_asset(&ctx(), asset_input(s, "DMM-9", AssetKind::Instrument).await)
        .await
        .unwrap();

    // instrument inside a system: ok
    let mut child = asset_input(s, "PROBE-1", AssetKind::Instrument).await;
    child.parent_id = Some(system.id);
    let child = s.create_asset(&ctx(), child).await.unwrap();
    assert_eq!(child.parent_tag.as_deref(), Some("RIG-1"));
    assert_eq!(s.get_asset(system.id).await.unwrap().child_count, 1);

    // parent must be a system
    let mut bad = asset_input(s, "PROBE-2", AssetKind::Instrument).await;
    bad.parent_id = Some(other.id);
    assert!(matches!(
        s.create_asset(&ctx(), bad).await,
        Err(Error::Validation(_))
    ));

    // a system with children can't become an instrument
    let mut demote = asset_input(s, "RIG-1", AssetKind::Instrument).await;
    demote.description = system.description.clone();
    let r = s
        .update_asset(&ctx(), system.id, system.row_version, demote)
        .await;
    assert!(
        matches!(r, Err(Error::Validation(ref e)) if e[0].field == "kind"),
        "{r:?}"
    );

    // nor be archived while it has active children
    let r = s
        .set_asset_archived(&ctx(), system.id, system.row_version, true)
        .await;
    assert!(matches!(r, Err(Error::Validation(_))), "{r:?}");

    // filter by parent
    let kids = s
        .list_assets(&AssetFilter {
            parent_id: Some(system.id),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(kids.len(), 1);
}

#[tokio::test]
async fn optimistic_concurrency_detects_stale_edits() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    let a = s
        .create_asset(&ctx(), asset_input(s, "T-1", AssetKind::Instrument).await)
        .await
        .unwrap();

    let mut edit = asset_input(s, "T-1", AssetKind::Instrument).await;
    edit.description = "first edit".into();
    let a2 = s
        .update_asset(&ctx(), a.id, a.row_version, edit.clone())
        .await
        .unwrap();
    assert_eq!(a2.row_version, 2);

    edit.description = "second edit from a stale copy".into();
    let stale = s
        .update_asset(&ctx(), a.id, a.row_version, edit.clone())
        .await;
    assert!(matches!(stale, Err(Error::Conflict(_))), "{stale:?}");

    let gone = s.update_asset(&ctx(), 424242, 1, edit).await;
    assert!(matches!(gone, Err(Error::NotFound { .. })), "{gone:?}");
}

#[tokio::test]
async fn calibration_records_drive_asset_dates() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    let vendor = provider(s).await;
    let mut input = asset_input(s, "GAUGE-1", AssetKind::Instrument).await;
    input.first_cal_date = None;
    let asset = s.create_asset(&ctx(), input).await.unwrap();
    assert_eq!(asset.next_cal_date, None);
    assert_eq!(asset.due_status, DueStatus::NoSchedule);

    let first = s
        .create_calibration(&ctx(), cal(asset.id, vendor, d(2025, 6, 10)))
        .await
        .unwrap();
    assert_eq!(first.technician.as_deref(), Some("J. Smith"));
    assert_eq!(first.next_due_date, Some(d(2026, 6, 10)));
    assert_eq!(first.provider_name, "Acme Calibration Labs");

    let latest = s
        .create_calibration(&ctx(), cal(asset.id, vendor, d(2026, 6, 1)))
        .await
        .unwrap();
    let a = s.get_asset(asset.id).await.unwrap();
    assert_eq!(
        a.first_cal_date,
        Some(d(2025, 6, 10)),
        "first date filled from earliest record"
    );
    assert_eq!(a.last_cal_date, Some(d(2026, 6, 1)));
    assert_eq!(a.next_cal_date, Some(d(2027, 6, 1)));

    // archiving the latest record rolls the dates back to the previous one
    s.set_calibration_archived(&ctx(), latest.id, latest.row_version, true)
        .await
        .unwrap();
    let a = s.get_asset(asset.id).await.unwrap();
    assert_eq!(a.last_cal_date, Some(d(2025, 6, 10)));
    assert_eq!(a.next_cal_date, Some(d(2026, 6, 10)));
    assert_eq!(
        s.list_calibrations(&CalibrationFilter {
            asset_id: Some(asset.id),
            include_archived: false
        })
        .await
        .unwrap()
        .len(),
        1
    );
    assert_eq!(
        s.list_calibrations(&CalibrationFilter {
            asset_id: Some(asset.id),
            include_archived: true
        })
        .await
        .unwrap()
        .len(),
        2
    );

    // editing the interval recomputes next date
    let mut edit = asset_input(s, "GAUGE-1", AssetKind::Instrument).await;
    edit.first_cal_date = a.first_cal_date;
    edit.cal_interval = Some(CalInterval {
        value: 6,
        unit: IntervalUnit::Months,
    });
    let a = s
        .update_asset(&ctx(), a.id, a.row_version, edit)
        .await
        .unwrap();
    assert_eq!(a.next_cal_date, Some(d(2025, 12, 10)));
    assert_eq!(a.due_status, DueStatus::Overdue);
}

#[tokio::test]
async fn calibration_validation() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    let vendor = provider(s).await;
    let asset = s
        .create_asset(&ctx(), asset_input(s, "V-1", AssetKind::Instrument).await)
        .await
        .unwrap();

    let tomorrow = Local::now()
        .date_naive()
        .checked_add_days(Days::new(1))
        .unwrap();
    let future = s
        .create_calibration(&ctx(), cal(asset.id, vendor, tomorrow))
        .await;
    assert!(matches!(future, Err(Error::Validation(_))), "{future:?}");

    let no_asset = s
        .create_calibration(&ctx(), cal(9999, vendor, d(2026, 1, 1)))
        .await;
    assert!(matches!(no_asset, Err(Error::Validation(ref e)) if e[0].field == "asset_id"));

    let no_provider = s
        .create_calibration(&ctx(), cal(asset.id, 9999, d(2026, 1, 1)))
        .await;
    assert!(
        matches!(no_provider, Err(Error::Validation(_))),
        "{no_provider:?}"
    );

    let rec = s
        .create_calibration(&ctx(), cal(asset.id, vendor, d(2026, 1, 1)))
        .await
        .unwrap();
    let other = s
        .create_asset(&ctx(), asset_input(s, "V-2", AssetKind::Instrument).await)
        .await
        .unwrap();
    let mv = s
        .update_calibration(
            &ctx(),
            rec.id,
            rec.row_version,
            cal(other.id, vendor, d(2026, 1, 1)),
        )
        .await;
    assert!(
        matches!(mv, Err(Error::Validation(_))),
        "records cannot move between assets"
    );
}

#[tokio::test]
async fn search_and_archive_filtering() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    let mut a = asset_input(s, "SCOPE-1", AssetKind::Instrument).await;
    a.serial_number = Some("XY_99".into());
    let scope = s.create_asset(&ctx(), a).await.unwrap();
    s.create_asset(&ctx(), asset_input(s, "DMM-2", AssetKind::Instrument).await)
        .await
        .unwrap();

    let by = |q: &str| AssetFilter {
        search: Some(q.into()),
        ..Default::default()
    };
    assert_eq!(
        s.list_assets(&by("scope")).await.unwrap().len(),
        1,
        "case-insensitive"
    );
    assert_eq!(
        s.list_assets(&by("xy_9")).await.unwrap().len(),
        1,
        "matches serial"
    );
    assert_eq!(s.list_assets(&by("y_")).await.unwrap().len(), 1);
    assert_eq!(
        s.list_assets(&by("%")).await.unwrap().len(),
        0,
        "wildcards are literal"
    );

    s.set_asset_archived(&ctx(), scope.id, scope.row_version, true)
        .await
        .unwrap();
    assert_eq!(
        s.list_assets(&AssetFilter::default()).await.unwrap().len(),
        1
    );
    let all = AssetFilter {
        include_archived: true,
        ..Default::default()
    };
    assert_eq!(s.list_assets(&all).await.unwrap().len(), 2);
}

#[tokio::test]
async fn entered_last_date_returns_when_all_records_are_archived() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    let vendor = provider(s).await;
    let mut input = asset_input(s, "LEGACY-1", AssetKind::Instrument).await;
    input.last_cal_date = Some(d(2026, 3, 15));
    let a = s.create_asset(&ctx(), input).await.unwrap();
    assert_eq!(a.next_cal_date, Some(d(2027, 3, 15)));

    let rec = s
        .create_calibration(&ctx(), cal(a.id, vendor, d(2026, 11, 18)))
        .await
        .unwrap();
    let a = s.get_asset(a.id).await.unwrap();
    assert_eq!(
        a.first_cal_date,
        Some(d(2026, 3, 15)),
        "earliest known calibration"
    );
    assert_eq!(a.last_cal_date, Some(d(2026, 11, 18)));
    assert_eq!(a.last_cal_date_entered, Some(d(2026, 3, 15)));
    assert_eq!(a.next_cal_date, Some(d(2027, 11, 18)));

    s.set_calibration_archived(&ctx(), rec.id, rec.row_version, true)
        .await
        .unwrap();
    let a = s.get_asset(a.id).await.unwrap();
    assert_eq!(
        a.last_cal_date,
        Some(d(2026, 3, 15)),
        "falls back to the entered date"
    );
    assert_eq!(a.next_cal_date, Some(d(2027, 3, 15)));
}

// ---------------------------------------------------------------------------
// Users, sessions, reasons, audit trail, settings
// ---------------------------------------------------------------------------

#[tokio::test]
async fn first_admin_can_only_be_created_once() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    assert_eq!(s.user_count().await.unwrap(), 1);
    let again = s.create_first_admin(new_user("second", Role::Admin)).await;
    assert!(matches!(again, Err(Error::Conflict(_))), "{again:?}");

    let creds = s
        .credentials("ADMIN")
        .await
        .unwrap()
        .expect("case-insensitive lookup");
    assert_eq!(creds.user.role, Role::Admin);
    assert!(!creds.user.must_change_password);
    assert!(verify_password(ADMIN_PASSWORD, &creds.password_hash));
    assert!(s.credentials("nobody").await.unwrap().is_none());
}

#[tokio::test]
async fn password_rules_apply_to_new_users() {
    let h = sqlite().await;
    let mut weak = new_user("tech", Role::Technician);
    weak.password = "short".into();
    let r = h.store.create_user(&ctx(), weak).await;
    assert!(
        matches!(r, Err(Error::Validation(ref e)) if e[0].field == "password"),
        "{r:?}"
    );

    let dup = h
        .store
        .create_user(&ctx(), new_user("Admin", Role::Viewer))
        .await;
    assert!(matches!(dup, Err(Error::Duplicate(_))), "{dup:?}");
}

#[tokio::test]
async fn the_last_active_admin_is_protected() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    let admin = s.get_user(1).await.unwrap();
    let demote = UserUpdate {
        display_name: admin.display_name.clone(),
        email: None,
        role: Role::Viewer,
        is_active: true,
    };
    let r = s
        .update_user(&ctx(), 1, admin.row_version, demote.clone())
        .await;
    assert!(matches!(r, Err(Error::Validation(_))), "{r:?}");

    // You can't deactivate yourself either.
    let deactivate = UserUpdate {
        role: Role::Admin,
        is_active: false,
        ..demote.clone()
    };
    let r = s
        .update_user(&ctx(), 1, admin.row_version, deactivate)
        .await;
    assert!(
        matches!(r, Err(Error::Validation(ref e)) if e[0].field == "is_active"),
        "{r:?}"
    );

    // With a second admin, the first can step down.
    let second = s
        .create_user(&ctx(), new_user("boss2", Role::Admin))
        .await
        .unwrap();
    assert_eq!(second.role, Role::Admin);
    let updated = s
        .update_user(&ctx(), 1, admin.row_version, demote)
        .await
        .unwrap();
    assert_eq!(updated.role, Role::Viewer);
    assert_eq!(updated.row_version, admin.row_version + 1);
}

#[tokio::test]
async fn password_change_versus_admin_reset() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    let tech = s
        .create_user(&ctx(), new_user("tech", Role::Technician))
        .await
        .unwrap();

    // Admin reset: forces a change at next sign-in.
    let reset = s
        .set_password(&ctx(), tech.id, "temporary password 1", true)
        .await
        .unwrap();
    assert!(reset.must_change_password);

    // The user changes it themself: the flag clears.
    let as_tech = WriteContext::new(
        Actor {
            user_id: tech.id,
            username: "tech".into(),
        },
        None,
    );
    let changed = s
        .set_password(&as_tech, tech.id, "my own new password", true)
        .await
        .unwrap();
    assert!(!changed.must_change_password);
    let creds = s.credentials("tech").await.unwrap().unwrap();
    assert!(verify_password("my own new password", &creds.password_hash));

    let weak = s.set_password(&as_tech, tech.id, "tech", false).await;
    assert!(matches!(weak, Err(Error::Validation(_))));

    let entries = s
        .list_audit(&AuditFilter {
            entity: Some(AuditEntity::User),
            ..Default::default()
        })
        .await
        .unwrap();
    let actions: Vec<_> = entries.iter().map(|e| e.action).collect();
    assert_eq!(
        &actions[..3],
        [
            AuditAction::PasswordChange,
            AuditAction::PasswordReset,
            AuditAction::Create
        ]
    );
}

#[tokio::test]
async fn sign_ins_are_audited() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    let admin = s.get_user(1).await.unwrap();
    s.record_sign_in("admin", Some(&admin)).await.unwrap();
    s.record_sign_in("mallory", None).await.unwrap();
    s.record_sign_out(&ctx().actor).await.unwrap();

    assert!(s.get_user(1).await.unwrap().last_sign_in_at.is_some());
    let entries = s
        .list_audit(&AuditFilter {
            entity: Some(AuditEntity::Session),
            ..Default::default()
        })
        .await
        .unwrap();
    let summary: Vec<_> = entries
        .iter()
        .map(|e| (e.action, e.username.as_str(), e.user_id))
        .collect();
    assert_eq!(
        summary,
        [
            (AuditAction::SignOut, "admin", Some(1)),
            (AuditAction::SignInFailed, "mallory", None),
            (AuditAction::SignIn, "admin", Some(1)),
        ]
    );
}

fn is_missing_reason<T>(r: polaris_core::Result<T>) -> bool {
    matches!(r, Err(Error::Validation(e)) if e[0].field == "reason")
}

#[tokio::test]
async fn edits_and_archives_require_a_reason() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    let vendor = provider(s).await;
    // Creating needs no reason.
    let input = asset_input(s, "R-1", AssetKind::Instrument).await;
    let a = s
        .create_asset(&ctx_without_reason(), input.clone())
        .await
        .unwrap();
    let rec = s
        .create_calibration(&ctx_without_reason(), cal(a.id, vendor, d(2026, 1, 5)))
        .await
        .unwrap();

    let nr = ctx_without_reason();
    assert!(is_missing_reason(
        s.update_asset(&nr, a.id, a.row_version, input).await
    ));
    assert!(is_missing_reason(
        s.set_asset_archived(&nr, a.id, a.row_version, true).await
    ));
    assert!(is_missing_reason(
        s.update_calibration(
            &nr,
            rec.id,
            rec.row_version,
            cal(a.id, vendor, d(2026, 1, 6))
        )
        .await
    ));
    assert!(is_missing_reason(
        s.set_calibration_archived(&nr, rec.id, rec.row_version, true)
            .await
    ));
    // Nothing changed.
    assert_eq!(s.get_asset(a.id).await.unwrap().row_version, a.row_version);
}

#[tokio::test]
async fn audit_trail_records_who_what_why_and_values() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    let vendor = provider(s).await;
    let mut input = asset_input(s, "AUD-1", AssetKind::Instrument).await;
    input.serial_number = Some("SN-1".into());
    let a = s.create_asset(&ctx(), input.clone()).await.unwrap();

    input.serial_number = Some("SN-2".into());
    let reason_ctx = WriteContext::new(ctx().actor, Some("Serial was mistyped".into()));
    s.update_asset(&reason_ctx, a.id, a.row_version, input)
        .await
        .unwrap();
    s.create_calibration(&ctx(), cal(a.id, vendor, d(2026, 2, 1)))
        .await
        .unwrap();

    let history = s
        .list_audit(&AuditFilter {
            asset_id: Some(a.id),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        history.len(),
        3,
        "asset create, asset update, calibration create"
    );
    let (cal_entry, update, create) = (&history[0], &history[1], &history[2]);

    assert_eq!(create.action, AuditAction::Create);
    assert_eq!(create.entity_label.as_deref(), Some("AUD-1"));
    assert!(
        create
            .changes
            .iter()
            .any(|c| c.field == "Serial number" && c.new.as_deref() == Some("SN-1"))
    );

    assert_eq!(update.action, AuditAction::Update);
    assert_eq!(update.username, "admin");
    assert_eq!(update.reason.as_deref(), Some("Serial was mistyped"));
    assert_eq!(
        update.changes.len(),
        1,
        "only the serial changed: {:?}",
        update.changes
    );
    assert_eq!(update.changes[0].old.as_deref(), Some("SN-1"));
    assert_eq!(update.changes[0].new.as_deref(), Some("SN-2"));

    assert_eq!(cal_entry.entity, AuditEntity::Calibration);
    assert_eq!(cal_entry.asset_id, Some(a.id));

    // Paging: entries older than the newest one.
    let older = s
        .list_audit(&AuditFilter {
            asset_id: Some(a.id),
            before_id: Some(cal_entry.id),
            limit: Some(1),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(older.len(), 1);
    assert_eq!(older[0].id, update.id);
}

#[tokio::test]
async fn settings_are_validated_and_audited() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    assert_eq!(s.settings().await.unwrap().idle_lock_minutes, 15);
    let bad = s
        .update_settings(
            &ctx(),
            AppSettings {
                idle_lock_minutes: 0,
                attachments_root: None,
            },
        )
        .await;
    assert!(matches!(bad, Err(Error::Validation(_))));
    s.update_settings(
        &ctx(),
        AppSettings {
            idle_lock_minutes: 30,
            attachments_root: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(s.settings().await.unwrap().idle_lock_minutes, 30);
    let entries = s
        .list_audit(&AuditFilter {
            entity: Some(AuditEntity::Settings),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].changes[0].new.as_deref(), Some("30"));
}

/// SQLite-specific: the audit table rejects edits and deletes at the database level.
#[tokio::test]
async fn sqlite_audit_log_is_append_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.db");
    let store = polaris_store::open(&DatabaseConfig::Sqlite {
        path: path.display().to_string(),
    })
    .await
    .unwrap();
    setup_admin(store.as_ref()).await;
    drop(store);

    let pool = sqlx::SqlitePool::connect(&format!("sqlite:{}", path.display()))
        .await
        .unwrap();
    let update = sqlx::query("UPDATE audit_log SET username = 'someone else'")
        .execute(&pool)
        .await;
    assert!(update.unwrap_err().to_string().contains("append-only"));
    let delete = sqlx::query("DELETE FROM audit_log").execute(&pool).await;
    assert!(delete.unwrap_err().to_string().contains("append-only"));
}

// ---------------------------------------------------------------------------
// Attachments
// ---------------------------------------------------------------------------

fn file(asset_id: i64, calibration_id: Option<i64>, path: &str) -> NewAttachment {
    NewAttachment {
        asset_id,
        calibration_id,
        category: AttachmentCategory::Certificate,
        file_name: format!(" {} ", path.rsplit('/').next().unwrap()),
        relative_path: path.into(),
        size_bytes: 1234,
        sha256: "a".repeat(64),
        content_type: Some("application/pdf".into()),
        description: Some("  ".into()),
    }
}

#[tokio::test]
async fn attachments_link_to_assets_and_calibrations() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    let vendor = provider(s).await;
    let a = s
        .create_asset(&ctx(), asset_input(s, "ATT-1", AssetKind::Instrument).await)
        .await
        .unwrap();
    let other = s
        .create_asset(&ctx(), asset_input(s, "ATT-2", AssetKind::Instrument).await)
        .await
        .unwrap();
    let rec = s
        .create_calibration(&ctx(), cal(a.id, vendor, d(2026, 3, 1)))
        .await
        .unwrap();
    assert_eq!(rec.attachment_count, 0);

    let manual = s
        .create_attachment(&ctx(), file(a.id, None, "ATT-1/manual.pdf"))
        .await
        .unwrap();
    assert_eq!(manual.file_name, "manual.pdf", "trimmed");
    assert_eq!(manual.description, None, "blank description dropped");
    assert_eq!(manual.uploaded_by, "admin");
    assert_eq!(manual.asset_tag, "ATT-1");

    let cert = s
        .create_attachment(&ctx(), file(a.id, Some(rec.id), "ATT-1/2026/cert.pdf"))
        .await
        .unwrap();
    assert_eq!(cert.calibration_date, Some(d(2026, 3, 1)));
    assert_eq!(s.get_calibration(rec.id).await.unwrap().attachment_count, 1);

    let by_asset = s
        .list_attachments(&AttachmentFilter {
            asset_id: Some(a.id),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(by_asset.len(), 2);
    let by_cal = s
        .list_attachments(&AttachmentFilter {
            calibration_id: Some(rec.id),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(by_cal.len(), 1);
    assert_eq!(by_cal[0].id, cert.id);

    // A calibration record from another item is refused.
    let wrong = s
        .create_attachment(&ctx(), file(other.id, Some(rec.id), "ATT-2/x.pdf"))
        .await;
    assert!(
        matches!(wrong, Err(Error::Validation(ref e)) if e[0].field == "calibration_id"),
        "{wrong:?}"
    );

    // The same stored path can't be recorded twice.
    let mut same_path = file(a.id, None, "att-1/MANUAL.pdf");
    same_path.sha256 = "b".repeat(64);
    let dup = s.create_attachment(&ctx(), same_path).await;
    assert!(matches!(dup, Err(Error::Duplicate(_))), "{dup:?}");

    // Nor can identical content be attached twice to the same item (or the same record)...
    let same_content = s
        .create_attachment(&ctx(), file(a.id, None, "ATT-1/manual (2).pdf"))
        .await;
    assert!(
        matches!(same_content, Err(Error::Duplicate(ref m)) if m.contains("manual.pdf")),
        "{same_content:?}"
    );
    // ...but the same content on a different item is fine.
    s.create_attachment(&ctx(), file(other.id, None, "ATT-2/manual.pdf"))
        .await
        .unwrap();
}

#[tokio::test]
async fn archiving_attachments_needs_a_reason_and_is_audited() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    let a = s
        .create_asset(&ctx(), asset_input(s, "ATT-3", AssetKind::Instrument).await)
        .await
        .unwrap();
    let f = s
        .create_attachment(&ctx(), file(a.id, None, "ATT-3/photo.jpg"))
        .await
        .unwrap();

    let no_reason = s
        .set_attachment_archived(&ctx_without_reason(), f.id, f.row_version, true)
        .await;
    assert!(is_missing_reason(no_reason));

    let archived = s
        .set_attachment_archived(&ctx(), f.id, f.row_version, true)
        .await
        .unwrap();
    assert!(archived.is_archived);
    let active = s
        .list_attachments(&AttachmentFilter {
            asset_id: Some(a.id),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(active.is_empty());

    let stale = s
        .set_attachment_archived(&ctx(), f.id, f.row_version, false)
        .await;
    assert!(matches!(stale, Err(Error::Conflict(_))));

    let history = s
        .list_audit(&AuditFilter {
            asset_id: Some(a.id),
            ..Default::default()
        })
        .await
        .unwrap();
    let attachment_events: Vec<_> = history
        .iter()
        .filter(|e| e.entity == AuditEntity::Attachment)
        .map(|e| e.action)
        .collect();
    assert_eq!(
        attachment_events,
        [AuditAction::Archive, AuditAction::Create]
    );
}

#[tokio::test]
async fn attachments_need_an_active_item() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    let a = s
        .create_asset(&ctx(), asset_input(s, "ATT-4", AssetKind::Instrument).await)
        .await
        .unwrap();
    s.set_asset_archived(&ctx(), a.id, a.row_version, true)
        .await
        .unwrap();
    let r = s
        .create_attachment(&ctx(), file(a.id, None, "ATT-4/a.pdf"))
        .await;
    assert!(
        matches!(r, Err(Error::Validation(ref e)) if e[0].field == "asset_id"),
        "{r:?}"
    );
    let missing = s
        .create_attachment(&ctx(), file(9999, None, "nope/a.pdf"))
        .await;
    assert!(matches!(missing, Err(Error::Validation(_))));
}

#[tokio::test]
async fn attachments_folder_setting_round_trips() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    assert_eq!(s.settings().await.unwrap().attachments_root, None);
    let saved = s
        .update_settings(
            &ctx(),
            AppSettings {
                idle_lock_minutes: 15,
                attachments_root: Some(r" \\files\cal ".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(saved.attachments_root.as_deref(), Some(r"\\files\cal"));
    assert_eq!(
        s.settings().await.unwrap().attachments_root.as_deref(),
        Some(r"\\files\cal")
    );

    let bad = s
        .update_settings(
            &ctx(),
            AppSettings {
                idle_lock_minutes: 15,
                attachments_root: Some("relative".into()),
            },
        )
        .await;
    assert!(matches!(bad, Err(Error::Validation(ref e)) if e[0].field == "attachments_root"));

    // Clearing it is allowed.
    s.update_settings(
        &ctx(),
        AppSettings {
            idle_lock_minutes: 15,
            attachments_root: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(s.settings().await.unwrap().attachments_root, None);
}
