//! Behavioral tests for the `Store` contract. They only use the trait, so the
//! same tests run against every backend.
//!
//! By default each test gets a new SQLite file. To run them against a server, set
//! `POLARISCAL_TEST_DATABASE` to a `DatabaseConfig` in JSON for an account that may
//! create databases (see `.github/workflows/ci.yml`); each test then gets its own
//! new database on that server.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use chrono::{Days, Local, NaiveDate};
use polaris_core::audit::{Actor, AuditFilter, WriteContext};
use polaris_core::auth::{NewUser, PasswordPolicy, UserUpdate, verify_password};
use polaris_core::certificate::*;
use polaris_core::model::*;
use polaris_core::{Error, Store};
use polaris_store::DatabaseConfig;

struct Harness {
    store: Arc<dyn Store>,
    config: DatabaseConfig,
    _dir: tempfile::TempDir,
}

/// The server account from `POLARISCAL_TEST_DATABASE`, if tests should use a server.
fn server_admin() -> Option<DatabaseConfig> {
    let json = std::env::var("POLARISCAL_TEST_DATABASE").ok()?;
    if json.trim().is_empty() {
        return None;
    }
    Some(
        serde_json::from_str(&json)
            .expect("POLARISCAL_TEST_DATABASE must be a DatabaseConfig in JSON"),
    )
}

fn expected_backend() -> &'static str {
    match server_admin() {
        None | Some(DatabaseConfig::Sqlite { .. }) => "sqlite",
        Some(DatabaseConfig::Postgres(_)) => "postgres",
        Some(DatabaseConfig::Mysql(_)) => "mysql",
        Some(DatabaseConfig::SqlServer(_)) => "mssql",
    }
}

fn sqlite_in(dir: &tempfile::TempDir, name: &str) -> DatabaseConfig {
    DatabaseConfig::Sqlite {
        path: dir.path().join("nested").join(name).display().to_string(),
    }
}

/// Where a new, empty database for one test lives (nothing is created for SQLite).
async fn empty_database() -> (DatabaseConfig, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let config = match server_admin() {
        None | Some(DatabaseConfig::Sqlite { .. }) => sqlite_in(&dir, "polariscal.db"),
        Some(admin) => {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
                % 1_000_000_000;
            let name = format!(
                "polaris_t{}_{stamp}_{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            polaris_store::testing::create_database(&admin, &name)
                .await
                .expect("create test database")
        }
    };
    (config, dir)
}

/// A new database with the first admin set up.
async fn harness() -> Harness {
    let (config, dir) = empty_database().await;
    let store = polaris_store::open(&config).await.expect("open store");
    setup_admin(store.as_ref()).await;
    Harness {
        store,
        config,
        _dir: dir,
    }
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
        counts_as_calibration: None,
        is_repair: None,
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
            end_of_month: false,
        }),
        in_service_date: None,
        first_cal_date: None,
        last_cal_date: None,
        notes: None,
        cal_aggregate: false,
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

const CALIBRATION: i64 = 1;

fn cal(asset_id: i64, provider_id: i64, date: NaiveDate) -> CalibrationInput {
    CalibrationInput {
        asset_id,
        cal_date: date,
        // The seeded "Calibration" service type is always id 1 in a new database.
        service_type_id: CALIBRATION,
        calibrated_by: CalibratedBy::Provider,
        calibrated_by_id: Some(provider_id),
        extension_days: None,
        technician: Some("  J. Smith ".into()),
        cost_cents: Some(12_500),
        certificate_number: Some("CERT-001".into()),
        condition_received: ConditionReceived::OutOfTolerance,
        condition_returned: ConditionReturned::Pass,
        notes: None,
        new_status_id: None,
    }
}

#[tokio::test]
async fn fresh_database_is_migrated_and_seeded() {
    let h = harness().await;
    let info = h.store.info().await.unwrap();
    assert_eq!(info.backend, expected_backend());
    assert_eq!(info.schema_version, 20);
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
            "Calibration Not Required",
            "Lost / Missing",
            "Out for Calibration",
            "Out for Repair",
            "Out of Service",
            "Retired",
            "Storage"
        ]
    );
    let role = |name: &str| {
        statuses
            .iter()
            .find(|s| s.name == name)
            .unwrap()
            .status_role
    };
    assert_eq!(role("Active"), None);
    assert_eq!(
        role("Out for Calibration"),
        Some(StatusRole::OutForCalibration)
    );
    assert_eq!(role("Out for Repair"), Some(StatusRole::OutForRepair));
    assert_eq!(role("Retired"), Some(StatusRole::Retired));
    assert_eq!(role("Storage"), Some(StatusRole::Storage));
    let storage = statuses.iter().find(|s| s.name == "Storage").unwrap();
    assert_eq!(
        storage.tracks_due,
        Some(false),
        "stored items aren't tracked"
    );
    assert_eq!(statuses[0].tracks_due, Some(true));
    assert_eq!(statuses[2].tracks_due, Some(false));
    assert_eq!(statuses[0].is_internal, None);

    // Lists come back alphabetically, ignoring case, whatever order they were added in.
    for name in ["zeta lab", "Alpha Lab", "beta lab"] {
        h.store
            .create_lookup(&ctx(), LookupKind::Location, lookup(name))
            .await
            .unwrap();
    }
    let names: Vec<_> = h
        .store
        .list_lookups(LookupKind::Location, true)
        .await
        .unwrap()
        .into_iter()
        .map(|l| l.name)
        .collect();
    assert_eq!(names, ["Alpha Lab", "beta lab", "zeta lab"]);
}

#[tokio::test]
async fn reopening_an_existing_database_keeps_data() {
    let (config, _dir) = empty_database().await;
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
    let h = harness().await;
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
async fn unused_list_entries_can_be_deleted_but_used_ones_cannot() {
    let h = harness().await;
    let s = h.store.as_ref();

    // Unused: deleted, gone from the list, and audited with what it held.
    let mut spare = lookup("Spare room");
    spare.description = Some("Never used".into());
    let spare = s
        .create_lookup(&ctx(), LookupKind::Location, spare)
        .await
        .unwrap();
    s.delete_lookup(&ctx(), LookupKind::Location, spare.id)
        .await
        .unwrap();
    assert!(
        s.list_lookups(LookupKind::Location, true)
            .await
            .unwrap()
            .is_empty()
    );
    let entry = &s.list_audit(&AuditFilter::default()).await.unwrap()[0];
    assert_eq!(entry.action, AuditAction::Delete);
    assert_eq!(entry.entity, AuditEntity::Lookup);
    assert!(
        entry.changes.iter().any(|c| c.field == "Name"
            && c.old.as_deref() == Some("Spare room")
            && c.new.is_none())
    );
    assert!(matches!(
        s.delete_lookup(&ctx(), LookupKind::Location, spare.id)
            .await,
        Err(Error::NotFound { .. })
    ));

    // Used by an item, even an archived one: refused.
    let maker = s
        .create_lookup(&ctx(), LookupKind::Manufacturer, lookup("Fluke"))
        .await
        .unwrap();
    let mut input = asset_input(s, "DMM-1", AssetKind::Instrument).await;
    input.manufacturer_id = Some(maker.id);
    let asset = s.create_asset(&ctx(), input).await.unwrap();
    s.set_asset_archived(&ctx(), asset.id, asset.row_version, true)
        .await
        .unwrap();
    let err = s
        .delete_lookup(&ctx(), LookupKind::Manufacturer, maker.id)
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Conflict(ref m) if m.contains("used by 1 item")),
        "{err}"
    );

    // Used by a calibration record: refused.
    let vendor = provider(s).await;
    let other = s
        .create_asset(&ctx(), asset_input(s, "DMM-2", AssetKind::Instrument).await)
        .await
        .unwrap();
    s.create_calibration(&ctx(), cal(other.id, vendor, d(2026, 1, 5)))
        .await
        .unwrap();
    let err = s
        .delete_lookup(&ctx(), LookupKind::Provider, vendor)
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Conflict(ref m) if m.contains("service record")),
        "{err}"
    );

    // Unused statuses can go, except the built-in ones. (Active is in use above.)
    for status in s.list_lookups(LookupKind::Status, true).await.unwrap() {
        let result = s.delete_lookup(&ctx(), LookupKind::Status, status.id).await;
        if status.status_role.is_some() || status.name == "Active" {
            assert!(matches!(result, Err(Error::Conflict(_))), "{}", status.name);
        } else {
            result.unwrap();
        }
    }
    assert_eq!(
        s.list_lookups(LookupKind::Status, true)
            .await
            .unwrap()
            .len(),
        5
    );
}

#[tokio::test]
async fn asset_create_joins_lookups_and_derives_next_date() {
    let h = harness().await;
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
    let h = harness().await;
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
    let h = harness().await;
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
    let h = harness().await;
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
    assert_eq!(first.calibrated_by, Some(CalibratedBy::Provider));
    assert_eq!(
        first.calibrated_by_name.as_deref(),
        Some("Acme Calibration Labs")
    );

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
            include_archived: false,
            ..Default::default()
        })
        .await
        .unwrap()
        .len(),
        1
    );
    assert_eq!(
        s.list_calibrations(&CalibrationFilter {
            asset_id: Some(asset.id),
            include_archived: true,
            ..Default::default()
        })
        .await
        .unwrap()
        .len(),
        2
    );
    // a date range keeps only the calibrations inside it (both ends included)
    let in_range = s
        .list_calibrations(&CalibrationFilter {
            include_archived: true,
            from_date: Some(d(2025, 6, 10)),
            to_date: Some(d(2025, 12, 31)),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        in_range.iter().map(|c| c.cal_date).collect::<Vec<_>>(),
        [d(2025, 6, 10)]
    );

    // editing the interval recomputes next date
    let mut edit = asset_input(s, "GAUGE-1", AssetKind::Instrument).await;
    edit.first_cal_date = a.first_cal_date;
    edit.cal_interval = Some(CalInterval {
        value: 6,
        unit: IntervalUnit::Months,
        end_of_month: false,
    });
    let a = s
        .update_asset(&ctx(), a.id, a.row_version, edit)
        .await
        .unwrap();
    assert_eq!(a.next_cal_date, Some(d(2025, 12, 10)));
    assert_eq!(a.due_status, DueStatus::Overdue);
}

#[tokio::test]
async fn end_of_month_intervals_are_due_on_the_last_day() {
    let h = harness().await;
    let s = h.store.as_ref();
    let vendor = provider(s).await;
    let mut input = asset_input(s, "EOM-1", AssetKind::Instrument).await;
    input.cal_interval = Some(CalInterval {
        value: 12,
        unit: IntervalUnit::Months,
        end_of_month: true,
    });
    let asset = s.create_asset(&ctx(), input.clone()).await.unwrap();
    assert!(asset.cal_interval.unwrap().end_of_month);

    let record = s
        .create_calibration(&ctx(), cal(asset.id, vendor, d(2026, 11, 20)))
        .await
        .unwrap();
    assert_eq!(record.next_due_date, Some(d(2027, 11, 30)));
    let asset = s.get_asset(asset.id).await.unwrap();
    assert_eq!(asset.next_cal_date, Some(d(2027, 11, 30)));

    // Turning it off recomputes the due date from the calibration date itself.
    input.cal_interval = Some(CalInterval {
        value: 12,
        unit: IntervalUnit::Months,
        end_of_month: false,
    });
    let asset = s
        .update_asset(&ctx(), asset.id, asset.row_version, input)
        .await
        .unwrap();
    assert_eq!(asset.next_cal_date, Some(d(2027, 11, 20)));
    let change = &s.list_audit(&AuditFilter::default()).await.unwrap()[0].changes;
    assert!(change.iter().any(|c| c.field == "Calibration interval"
        && c.old.as_deref() == Some("12 months, end of month")
        && c.new.as_deref() == Some("12 months")));
}

#[tokio::test]
async fn a_manufacturer_can_be_who_calibrated() {
    let h = harness().await;
    let s = h.store.as_ref();
    let maker = s
        .create_lookup(&ctx(), LookupKind::Manufacturer, lookup("Keysight"))
        .await
        .unwrap();
    let asset = s
        .create_asset(
            &ctx(),
            asset_input(s, "SCOPE-1", AssetKind::Instrument).await,
        )
        .await
        .unwrap();
    let mut input = cal(asset.id, maker.id, d(2026, 2, 1));
    input.calibrated_by = CalibratedBy::Manufacturer;
    let record = s.create_calibration(&ctx(), input.clone()).await.unwrap();
    assert_eq!(record.calibrated_by, Some(CalibratedBy::Manufacturer));
    assert_eq!(record.calibrated_by_id, Some(maker.id));
    assert_eq!(record.calibrated_by_name.as_deref(), Some("Keysight"));
    let created = &s.list_audit(&AuditFilter::default()).await.unwrap()[0];
    assert!(
        created
            .changes
            .iter()
            .any(|c| c.field == "Performed by"
                && c.new.as_deref() == Some("Keysight (manufacturer)"))
    );

    // A manufacturer that calibrated something can't be deleted.
    let err = s
        .delete_lookup(&ctx(), LookupKind::Manufacturer, maker.id)
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Conflict(ref m) if m.contains("1 service record")),
        "{err}"
    );

    // Switching to a calibration provider clears the manufacturer.
    let vendor = provider(s).await;
    input.calibrated_by = CalibratedBy::Provider;
    input.calibrated_by_id = Some(vendor);
    let edited = s
        .update_calibration(&ctx(), record.id, record.row_version, input.clone())
        .await
        .unwrap();
    assert_eq!(edited.calibrated_by, Some(CalibratedBy::Provider));
    assert_eq!(
        edited.calibrated_by_name.as_deref(),
        Some("Acme Calibration Labs")
    );
    // Nothing names the manufacturer any more, so now it can be deleted.
    s.delete_lookup(&ctx(), LookupKind::Manufacturer, maker.id)
        .await
        .unwrap();

    // An id that doesn't exist is reported, not stored.
    input.calibrated_by = CalibratedBy::Manufacturer;
    input.calibrated_by_id = Some(9_999);
    let err = s
        .update_calibration(&ctx(), edited.id, edited.row_version, input)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Validation(_)), "{err}");
}

#[tokio::test]
async fn service_types_are_seeded_and_only_calibrations_set_due_dates() {
    let h = harness().await;
    let s = h.store.as_ref();
    let types = s.list_lookups(LookupKind::ServiceType, true).await.unwrap();
    let summary: Vec<_> = types
        .iter()
        .map(|t| (t.id, t.name.as_str(), t.counts_as_calibration, t.is_repair))
        .collect();
    assert_eq!(
        summary,
        [
            (CALIBRATION, "Calibration", Some(true), Some(false)),
            (3, "Calibration Extension", Some(false), Some(false)),
            (2, "Repair", Some(false), Some(true))
        ]
    );
    let repair = types.iter().find(|t| t.name == "Repair").unwrap().id;

    let vendor = provider(s).await;
    let asset = s
        .create_asset(&ctx(), asset_input(s, "SVC-1", AssetKind::Instrument).await)
        .await
        .unwrap();
    s.create_calibration(&ctx(), cal(asset.id, vendor, d(2026, 1, 10)))
        .await
        .unwrap();

    // A later repair sets no due date and leaves the schedule alone.
    let mut fix = cal(asset.id, vendor, d(2026, 3, 1));
    fix.service_type_id = repair;
    let record = s.create_calibration(&ctx(), fix).await.unwrap();
    assert_eq!(record.service_type_name, "Repair");
    assert!(!record.counts_as_calibration);
    assert_eq!(record.next_due_date, None);
    let a = s.get_asset(asset.id).await.unwrap();
    assert_eq!(a.last_cal_date, Some(d(2026, 1, 10)));
    assert_eq!(a.next_cal_date, Some(d(2027, 1, 10)));

    // If an administrator decides repairs count after all, dates are re-derived.
    let mut input = lookup("Repair");
    input.counts_as_calibration = Some(true);
    s.update_lookup(&ctx(), LookupKind::ServiceType, repair, input)
        .await
        .unwrap();
    let a = s.get_asset(asset.id).await.unwrap();
    assert_eq!(a.last_cal_date, Some(d(2026, 3, 1)));

    // In use, so it can't be deleted; an unused type can, but never the last one.
    assert!(matches!(
        s.delete_lookup(&ctx(), LookupKind::ServiceType, repair)
            .await,
        Err(Error::Conflict(_))
    ));
    let spare = s
        .create_lookup(&ctx(), LookupKind::ServiceType, lookup("Verification"))
        .await
        .unwrap();
    assert_eq!(spare.counts_as_calibration, Some(true));
    s.delete_lookup(&ctx(), LookupKind::ServiceType, spare.id)
        .await
        .unwrap();

    // An unknown service type is reported.
    let mut bad = cal(asset.id, vendor, d(2026, 4, 1));
    bad.service_type_id = 9_999;
    let err = s.create_calibration(&ctx(), bad).await.unwrap_err();
    assert!(
        matches!(err, Error::Validation(ref e) if e[0].field == "service_type_id"),
        "{err}"
    );
}

/// The seeded Calibration Extension service type.
async fn extension_type(store: &dyn Store) -> i64 {
    store
        .list_lookups(LookupKind::ServiceType, true)
        .await
        .unwrap()
        .into_iter()
        .find(|t| t.is_extension == Some(true))
        .expect("Calibration Extension is seeded")
        .id
}

#[tokio::test]
async fn calibration_extensions_authorize_use_past_the_due_date() {
    let h = harness().await;
    let s = h.store.as_ref();
    let vendor = provider(s).await;
    let ext = extension_type(s).await;
    let asset = s
        .create_asset(&ctx(), asset_input(s, "EXT-1", AssetKind::Instrument).await)
        .await
        .unwrap();
    // Calibrated long ago, so it's overdue now.
    let old = Local::now().date_naive() - Days::new(400);
    s.create_calibration(&ctx(), cal(asset.id, vendor, old))
        .await
        .unwrap();
    let before = s.get_asset(asset.id).await.unwrap();
    assert_eq!(before.due_status, DueStatus::Overdue);

    // Extend for 30 days from today: performer, certificate, and cost are cleared.
    let today = Local::now().date_naive();
    let mut input = cal(asset.id, vendor, today);
    input.service_type_id = ext;
    input.extension_days = Some(30);
    let record = s.create_calibration(&ctx(), input.clone()).await.unwrap();
    assert!(record.is_extension);
    assert_eq!(record.extension_days, Some(30));
    assert_eq!(record.calibrated_by, None);
    assert_eq!(record.calibrated_by_name, None);
    assert_eq!(record.certificate_number, None);
    assert_eq!(record.cost_cents, None);
    assert_eq!(record.next_due_date, Some(today + Days::new(30)));

    // The next date moves; the last calibration doesn't; it's "extended" but still
    // counted with the overdue items.
    let a = s.get_asset(asset.id).await.unwrap();
    assert_eq!(a.last_cal_date, before.last_cal_date);
    assert_eq!(a.next_cal_date, Some(today + Days::new(30)));
    assert!(a.calibration_extended);
    assert_eq!(a.due_status, DueStatus::Extended);
    let summary = polaris_core::schedule::DueSummary::from_assets(std::slice::from_ref(&a), today);
    assert_eq!(summary.overdue, 1);
    assert_eq!(summary.attention.len(), 1);

    // A new calibration ends the extension.
    s.create_calibration(&ctx(), cal(asset.id, vendor, today))
        .await
        .unwrap();
    let a = s.get_asset(asset.id).await.unwrap();
    assert!(!a.calibration_extended);
    assert_eq!(a.last_cal_date, Some(today));
    assert_eq!(a.due_status, DueStatus::Current);

    // An extension needs its length.
    input.extension_days = None;
    let err = s.create_calibration(&ctx(), input).await.unwrap_err();
    assert!(
        matches!(err, Error::Validation(ref e) if e[0].field == "extension_days"),
        "{err}"
    );

    // Every other kind of service still needs someone who did it.
    let mut nobody = cal(asset.id, vendor, today);
    nobody.calibrated_by_id = None;
    let err = s.create_calibration(&ctx(), nobody).await.unwrap_err();
    assert!(
        matches!(err, Error::Validation(ref e) if e[0].field == "calibrated_by_id"),
        "{err}"
    );

    // The built-in type can't be deleted or made to count as a calibration.
    assert!(matches!(
        s.delete_lookup(&ctx(), LookupKind::ServiceType, ext).await,
        Err(Error::Conflict(_))
    ));
    let mut rename = lookup("Extension");
    rename.counts_as_calibration = Some(true);
    let renamed = s
        .update_lookup(&ctx(), LookupKind::ServiceType, ext, rename)
        .await
        .unwrap();
    assert_eq!(renamed.counts_as_calibration, Some(false));
    assert_eq!(renamed.is_extension, Some(true));
}

/// The built-in status with this role.
async fn status_with_role(store: &dyn Store, role: StatusRole) -> LookupItem {
    store
        .list_lookups(LookupKind::Status, true)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.status_role == Some(role))
        .expect("built-in status")
}

#[tokio::test]
async fn failed_calibrations_keep_the_schedule_and_can_change_status() {
    let h = harness().await;
    let s = h.store.as_ref();
    let vendor = provider(s).await;
    let repair = status_with_role(s, StatusRole::OutForRepair).await;
    let retired = status_with_role(s, StatusRole::Retired).await;
    assert_eq!(repair.tracks_due, Some(true));
    assert_eq!(retired.tracks_due, Some(false));

    let asset = s
        .create_asset(
            &ctx(),
            asset_input(s, "FAIL-1", AssetKind::Instrument).await,
        )
        .await
        .unwrap();
    let today = Local::now().date_naive();
    let earlier = today - Days::new(400);
    s.create_calibration(&ctx(), cal(asset.id, vendor, earlier))
        .await
        .unwrap();
    let before = s.get_asset(asset.id).await.unwrap();
    assert_eq!(before.due_status, DueStatus::Overdue);

    // Failed today, sent for repair: the dates stay; the status changes in the same save.
    let mut failed = cal(asset.id, vendor, today);
    failed.condition_returned = ConditionReturned::Fail;
    failed.new_status_id = Some(repair.id);
    let record = s.create_calibration(&ctx(), failed.clone()).await.unwrap();
    assert_eq!(record.next_due_date, None);
    let a = s.get_asset(asset.id).await.unwrap();
    assert_eq!(a.last_cal_date, before.last_cal_date);
    assert_eq!(a.next_cal_date, before.next_cal_date);
    assert_eq!(a.status_id, repair.id);
    assert_eq!(a.status_role, Some(StatusRole::OutForRepair));
    // Away for repair, but still overdue.
    assert_eq!(a.due_status, DueStatus::Overdue);
    let audit = s
        .list_audit(&AuditFilter {
            asset_id: Some(asset.id),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        audit.iter().any(|e| e.entity == AuditEntity::Asset
            && e.action == AuditAction::Update
            && e.changes.iter().any(|c| c.field == "Status")),
        "status change audited"
    );

    // Passing after the repair renews the schedule as usual.
    s.create_calibration(&ctx(), cal(asset.id, vendor, today))
        .await
        .unwrap();
    let a = s.get_asset(asset.id).await.unwrap();
    assert_eq!(a.last_cal_date, Some(today));
    assert_eq!(a.due_status, DueStatus::Current);

    // Retiring takes the item out of due tracking.
    let mut retire = failed.clone();
    retire.new_status_id = Some(retired.id);
    s.create_calibration(&ctx(), retire).await.unwrap();
    let a = s.get_asset(asset.id).await.unwrap();
    assert_eq!(a.status_role, Some(StatusRole::Retired));
    assert_eq!(a.due_status, DueStatus::NotTracked);
    assert_eq!(a.last_cal_date, Some(today));

    // A status that doesn't exist is refused, and nothing is saved.
    let mut bad = failed;
    bad.new_status_id = Some(9_999);
    let err = s.create_calibration(&ctx(), bad).await.unwrap_err();
    assert!(
        matches!(err, Error::Validation(ref e) if e[0].field == "new_status_id"),
        "{err}"
    );

    // Built-in statuses keep their due tracking whatever an edit says.
    let mut edit = lookup("Away for Repair");
    edit.tracks_due = Some(false);
    let renamed = s
        .update_lookup(&ctx(), LookupKind::Status, repair.id, edit)
        .await
        .unwrap();
    assert_eq!(renamed.name, "Away for Repair");
    assert_eq!(renamed.tracks_due, Some(true));
}

#[tokio::test]
async fn repairs_remember_the_status_before_a_failure() {
    let h = harness().await;
    let s = h.store.as_ref();
    let vendor = provider(s).await;
    let repair_status = status_with_role(s, StatusRole::OutForRepair).await;
    let retired = status_with_role(s, StatusRole::Retired).await;
    let types = s.list_lookups(LookupKind::ServiceType, true).await.unwrap();
    let repair_type = types.iter().find(|t| t.is_repair == Some(true)).unwrap().id;

    // Start from a status of the user's own, to show it's the one remembered.
    let bench = s
        .create_lookup(&ctx(), LookupKind::Status, lookup("On the Bench"))
        .await
        .unwrap();
    let mut input = asset_input(s, "REP-1", AssetKind::Instrument).await;
    input.status_id = bench.id;
    let asset = s.create_asset(&ctx(), input).await.unwrap();
    assert_eq!(asset.previous_status_id, None);

    let today = Local::now().date_naive();
    let mut failed = cal(asset.id, vendor, today);
    failed.condition_returned = ConditionReturned::Fail;
    failed.new_status_id = Some(repair_status.id);
    s.create_calibration(&ctx(), failed).await.unwrap();
    let a = s.get_asset(asset.id).await.unwrap();
    assert_eq!(a.status_id, repair_status.id);
    assert_eq!(a.previous_status_id, Some(bench.id));

    // The repair passes; the status goes back to what it was.
    let mut repaired = cal(asset.id, vendor, today);
    repaired.service_type_id = repair_type;
    repaired.new_status_id = a.previous_status_id;
    let record = s
        .create_calibration(&ctx(), repaired.clone())
        .await
        .unwrap();
    assert!(record.is_repair);
    let a = s.get_asset(asset.id).await.unwrap();
    assert_eq!(a.status_id, bench.id);
    assert_eq!(a.previous_status_id, Some(repair_status.id));

    // A failed repair retires it.
    repaired.condition_returned = ConditionReturned::Fail;
    repaired.new_status_id = Some(retired.id);
    s.create_calibration(&ctx(), repaired).await.unwrap();
    let a = s.get_asset(asset.id).await.unwrap();
    assert_eq!(a.status_role, Some(StatusRole::Retired));
    assert_eq!(a.previous_status_id, Some(bench.id));

    // Deleting a status that's only remembered (not in use) forgets it.
    let mut edit = asset_input(s, "REP-1", AssetKind::Instrument).await;
    edit.status_id = retired.id;
    let a = s
        .update_asset(&ctx(), a.id, a.row_version, edit)
        .await
        .unwrap();
    assert_eq!(a.previous_status_id, Some(bench.id));
    s.delete_lookup(&ctx(), LookupKind::Status, bench.id)
        .await
        .unwrap();
    assert_eq!(s.get_asset(a.id).await.unwrap().previous_status_id, None);

    // New service types can be repairs; the extension type never is.
    let mut kind = lookup("Repair w/Calibration");
    kind.is_repair = Some(true);
    let created = s
        .create_lookup(&ctx(), LookupKind::ServiceType, kind)
        .await
        .unwrap();
    assert_eq!(created.is_repair, Some(true));
    assert_eq!(created.counts_as_calibration, Some(true));
    let ext = extension_type(s).await;
    let mut edit = lookup("Calibration Extension");
    edit.is_repair = Some(true);
    let ext = s
        .update_lookup(&ctx(), LookupKind::ServiceType, ext, edit)
        .await
        .unwrap();
    assert_eq!(ext.is_repair, Some(false));
}

#[tokio::test]
async fn last_cost_follows_provider_service_and_model() {
    let h = harness().await;
    let s = h.store.as_ref();
    let vendor = provider(s).await;
    let other_vendor = s
        .create_lookup(&ctx(), LookupKind::Provider, lookup("Second Lab"))
        .await
        .unwrap()
        .id;
    let maker = s
        .create_lookup(&ctx(), LookupKind::Manufacturer, lookup("Agilent"))
        .await
        .unwrap()
        .id;
    let item = |tag: &'static str, model: &'static str| async move {
        let mut input = asset_input(s, tag, AssetKind::Instrument).await;
        input.manufacturer_id = Some(maker);
        input.model_number = Some(model.into());
        s.create_asset(&ctx(), input).await.unwrap().id
    };
    let first = item("DMM-A", "34401A").await;
    let second = item("DMM-B", "34401a").await;
    let different = item("DMM-C", "34461A").await;

    assert_eq!(
        s.last_cost(vendor, CALIBRATION, second).await.unwrap(),
        None
    );

    let mut record = cal(first, vendor, d(2026, 1, 10));
    record.cost_cents = Some(12_500);
    s.create_calibration(&ctx(), record).await.unwrap();
    let mut record = cal(first, other_vendor, d(2026, 2, 10));
    record.cost_cents = Some(9_900);
    s.create_calibration(&ctx(), record).await.unwrap();

    // Same maker and model (any case), same provider and service type.
    let last = s
        .last_cost(vendor, CALIBRATION, second)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (last.cost_cents, last.asset_tag.as_str()),
        (12_500, "DMM-A")
    );
    assert_eq!(
        s.last_cost(vendor, CALIBRATION, different).await.unwrap(),
        None
    );
    assert_eq!(
        s.last_cost(other_vendor, CALIBRATION, second)
            .await
            .unwrap()
            .map(|c| c.cost_cents),
        Some(9_900)
    );

    // A newer price wins from then on.
    let mut record = cal(second, vendor, d(2026, 3, 1));
    record.cost_cents = Some(14_000);
    s.create_calibration(&ctx(), record).await.unwrap();
    let last = s
        .last_cost(vendor, CALIBRATION, first)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((last.cost_cents, last.cal_date), (14_000, d(2026, 3, 1)));
}

#[tokio::test]
async fn stored_items_pause_tracking_and_resume_on_their_schedule() {
    let h = harness().await;
    let s = h.store.as_ref();
    let storage = status_with_role(s, StatusRole::Storage).await;
    let mut input = asset_input(s, "STORE-1", AssetKind::Instrument).await;
    let active = input.status_id;
    // Calibrated long enough ago that it fell due while it was stored.
    input.last_cal_date = Some(d(2024, 1, 15));
    let asset = s.create_asset(&ctx(), input.clone()).await.unwrap();
    assert_eq!(asset.due_status, DueStatus::Overdue);
    let due = asset.next_cal_date;

    input.status_id = storage.id;
    let stored = s
        .update_asset(&ctx(), asset.id, asset.row_version, input.clone())
        .await
        .unwrap();
    assert_eq!(stored.status_role, Some(StatusRole::Storage));
    assert_eq!(stored.due_status, DueStatus::NotTracked);
    assert_eq!(stored.next_cal_date, due, "the schedule is kept, not moved");
    let today = Local::now().date_naive();
    let summary =
        polaris_core::schedule::DueSummary::from_assets(std::slice::from_ref(&stored), today);
    assert!(
        summary.attention.is_empty(),
        "stored items don't need attention"
    );

    input.status_id = active;
    let back = s
        .update_asset(&ctx(), stored.id, stored.row_version, input)
        .await
        .unwrap();
    assert_eq!(back.next_cal_date, due);
    assert_eq!(
        back.due_status,
        DueStatus::Overdue,
        "due straight away once back"
    );
}

#[tokio::test]
async fn aggregate_systems_follow_their_soonest_instrument() {
    let h = harness().await;
    let s = h.store.as_ref();
    let vendor = provider(s).await;
    let storage = status_with_role(s, StatusRole::Storage).await;

    let mut input = asset_input(s, "SYS-A", AssetKind::System).await;
    input.cal_aggregate = true;
    input.last_cal_date = Some(d(2025, 1, 1));
    let system = s.create_asset(&ctx(), input).await.unwrap();
    assert!(system.cal_aggregate);
    assert_eq!(
        system.cal_interval, None,
        "an aggregate System has no cycle of its own"
    );
    assert_eq!(system.next_cal_date, None, "no Instruments yet");

    let mut ids = Vec::new();
    for (tag, last) in [
        ("A-1", d(2026, 1, 31)),
        ("A-2", d(2026, 2, 15)),
        ("A-3", d(2026, 5, 15)),
    ] {
        let mut i = asset_input(s, tag, AssetKind::Instrument).await;
        i.last_cal_date = Some(last);
        ids.push(s.create_asset(&ctx(), i).await.unwrap().id);
    }
    // An Instrument can't be made aggregate.
    let mut i = asset_input(s, "A-X", AssetKind::Instrument).await;
    i.cal_aggregate = true;
    assert!(!s.create_asset(&ctx(), i).await.unwrap().cal_aggregate);

    s.add_to_system(&ctx(), system.id, &ids).await.unwrap();
    let system = s.get_asset(system.id).await.unwrap();
    assert_eq!(system.child_count, 3);
    assert_eq!(system.next_cal_date, Some(d(2027, 1, 31)));
    let moved = s.get_asset(ids[0]).await.unwrap();
    assert_eq!(moved.parent_tag.as_deref(), Some("SYS-A"));
    let history = s
        .list_audit(&AuditFilter {
            asset_id: Some(ids[0]),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        history
            .iter()
            .any(|e| e.changes.iter().any(|c| c.field == "Part of System"))
    );

    // Calibrating the soonest one moves the System's date to the next one due.
    s.create_calibration(&ctx(), cal(ids[0], vendor, d(2026, 11, 1)))
        .await
        .unwrap();
    assert_eq!(
        s.get_asset(system.id).await.unwrap().next_cal_date,
        Some(d(2027, 2, 15))
    );

    // Instruments that aren't tracked (stored) don't count.
    let a2 = s.get_asset(ids[1]).await.unwrap();
    let mut edit = asset_input(s, "A-2", AssetKind::Instrument).await;
    edit.parent_id = Some(system.id);
    edit.last_cal_date = Some(d(2026, 2, 15));
    edit.status_id = storage.id;
    s.update_asset(&ctx(), a2.id, a2.row_version, edit)
        .await
        .unwrap();
    assert_eq!(
        s.get_asset(system.id).await.unwrap().next_cal_date,
        Some(d(2027, 5, 15))
    );

    // The Dashboard counts the Instruments, not the aggregate System as well.
    let all = s.list_assets(&AssetFilter::default()).await.unwrap();
    let summary = polaris_core::schedule::DueSummary::from_assets(&all, d(2026, 11, 23));
    assert!(summary.items.iter().all(|i| i.asset.asset_tag != "SYS-A"));

    // Only Instruments can be added.
    let other = s
        .create_asset(&ctx(), asset_input(s, "SYS-B", AssetKind::System).await)
        .await
        .unwrap();
    let err = s
        .add_to_system(&ctx(), system.id, &[other.id])
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Validation(_)), "{err:?}");
}

#[tokio::test]
async fn instruments_take_their_systems_location_and_department() {
    let h = harness().await;
    let s = h.store.as_ref();
    let place = |name: &'static str| async move {
        s.create_lookup(&ctx(), LookupKind::Location, lookup(name))
            .await
            .unwrap()
            .id
    };
    let (lab, bench, store_room) = (
        place("Lab").await,
        place("Bench").await,
        place("Store room").await,
    );
    let qa = s
        .create_lookup(&ctx(), LookupKind::Department, lookup("QA"))
        .await
        .unwrap()
        .id;

    let mut sys = asset_input(s, "RIG-L", AssetKind::System).await;
    sys.location_id = Some(lab);
    sys.department_id = Some(qa);
    let system = s.create_asset(&ctx(), sys.clone()).await.unwrap();

    // Created inside the System: its location is the System's, whatever was entered.
    let mut i = asset_input(s, "L-1", AssetKind::Instrument).await;
    i.parent_id = Some(system.id);
    i.location_id = Some(bench);
    let inside = s.create_asset(&ctx(), i.clone()).await.unwrap();
    assert_eq!(
        (inside.location_id, inside.department_id),
        (Some(lab), Some(qa))
    );
    // Editing it can't move it away from the System's location either.
    let edited = s
        .update_asset(&ctx(), inside.id, inside.row_version, i)
        .await
        .unwrap();
    assert_eq!(edited.location_id, Some(lab));

    // Added to the System later: the same.
    let mut j = asset_input(s, "L-2", AssetKind::Instrument).await;
    j.location_id = Some(bench);
    let outside = s.create_asset(&ctx(), j).await.unwrap();
    assert_eq!(outside.location_id, Some(bench));
    s.add_to_system(&ctx(), system.id, &[outside.id])
        .await
        .unwrap();
    assert_eq!(
        s.get_asset(outside.id).await.unwrap().location_id,
        Some(lab)
    );

    // The System moves: its Instruments move with it, and the change is in their history.
    sys.location_id = Some(store_room);
    sys.department_id = None;
    let system = s
        .update_asset(&ctx(), system.id, system.row_version, sys)
        .await
        .unwrap();
    for id in [inside.id, outside.id] {
        let a = s.get_asset(id).await.unwrap();
        assert_eq!(
            (a.location_id, a.department_id),
            (Some(store_room), None),
            "{}",
            a.asset_tag
        );
    }
    let history = s
        .list_audit(&AuditFilter {
            asset_id: Some(outside.id),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        history[0]
            .changes
            .iter()
            .any(|c| c.field == "Location" && c.new.as_deref() == Some("Store room")),
        "{:?}",
        history[0].changes
    );
    assert_eq!(system.location_id, Some(store_room));
}

#[tokio::test]
async fn calibration_validation() {
    let h = harness().await;
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
    let h = harness().await;
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
    let h = harness().await;
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
    let h = harness().await;
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
    let h = harness().await;
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
    let h = harness().await;
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
    let h = harness().await;
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
    let h = harness().await;
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

#[tokio::test]
async fn password_rules_apply_to_new_passwords_and_expiry_is_recorded() {
    let h = harness().await;
    let s = h.store.as_ref();
    // Existing accounts keep their passwords; the clock for expiry is running.
    let admin = s.get_user(1).await.unwrap();
    assert!(admin.password_changed_at.is_some());

    let mut settings = s.settings().await.unwrap();
    assert_eq!(settings.password_policy, PasswordPolicy::default());
    settings.password_policy = PasswordPolicy {
        min_length: 12,
        require_uppercase: true,
        require_number: true,
        require_symbol: true,
        expiry_days: Some(90),
    };
    let saved = s.update_settings(&ctx(), settings.clone()).await.unwrap();
    assert_eq!(saved.password_policy, settings.password_policy);
    assert_eq!(
        s.settings().await.unwrap().password_policy,
        settings.password_policy
    );
    let audit = s
        .list_audit(&AuditFilter {
            entity: Some(AuditEntity::Settings),
            ..Default::default()
        })
        .await
        .unwrap();
    let fields: Vec<_> = audit[0].changes.iter().map(|c| c.field.as_str()).collect();
    assert!(fields.contains(&"Passwords expire after"), "{fields:?}");

    // New accounts and new passwords follow the rules.
    let weak = s.create_user(&ctx(), new_user("weak", Role::Viewer)).await;
    assert!(matches!(weak, Err(Error::Validation(ref e)) if e[0].field == "password"));
    let mut strong = new_user("strong", Role::Viewer);
    strong.password = "Strong-pass 2026".into();
    let user = s.create_user(&ctx(), strong).await.unwrap();
    let first_set = user.password_changed_at.unwrap();
    assert!(
        s.set_password(&ctx(), user.id, "no capitals 2026!", false)
            .await
            .is_err()
    );
    let reset = s
        .set_password(&ctx(), user.id, "Another-one 2027", false)
        .await
        .unwrap();
    assert!(reset.password_changed_at.unwrap() >= first_set);

    // An expired password: the user must change it, and it's in the history.
    let expired = s.expire_password(&reset).await.unwrap();
    assert!(expired.must_change_password);
    let again = s.expire_password(&expired).await.unwrap();
    assert_eq!(again.row_version, expired.row_version, "only once");
    let history = s
        .list_audit(&AuditFilter {
            entity: Some(AuditEntity::User),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(history[0].action, AuditAction::PasswordExpired);
    assert_eq!(history[0].user_id, Some(user.id));

    // Out-of-range settings are refused.
    settings.password_policy.min_length = 4;
    assert!(s.update_settings(&ctx(), settings).await.is_err());
}

#[tokio::test]
async fn password_reset_requests_wait_for_a_new_password() {
    let h = harness().await;
    let s = h.store.as_ref();
    let tech = s
        .create_user(&ctx(), new_user("jkim", Role::Technician))
        .await
        .unwrap();
    let gone = s
        .create_user(&ctx(), new_user("gone", Role::Viewer))
        .await
        .unwrap();
    s.update_user(
        &ctx(),
        gone.id,
        gone.row_version,
        UserUpdate {
            display_name: gone.display_name.clone(),
            email: None,
            role: Role::Viewer,
            is_active: false,
        },
    )
    .await
    .unwrap();

    // Any capitalization; asking twice is one request. Unknown and inactive accounts
    // succeed too, but nothing is recorded.
    s.request_password_reset(" JKim ").await.unwrap();
    let requested = s.get_user(tech.id).await.unwrap().reset_requested_at;
    assert!(requested.is_some());
    s.request_password_reset("jkim").await.unwrap();
    assert_eq!(
        s.get_user(tech.id).await.unwrap().reset_requested_at,
        requested
    );
    s.request_password_reset("nobody").await.unwrap();
    s.request_password_reset("gone").await.unwrap();
    assert!(
        s.get_user(gone.id)
            .await
            .unwrap()
            .reset_requested_at
            .is_none()
    );

    let requests: Vec<_> = s
        .list_audit(&AuditFilter {
            entity: Some(AuditEntity::User),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.action == AuditAction::PasswordResetRequest)
        .map(|e| (e.username, e.user_id, e.entity_id))
        .collect();
    assert_eq!(requests, [("jkim".to_string(), None, Some(tech.id))]);

    // An administrator's reset clears it.
    let reset = s
        .set_password(&ctx(), tech.id, "a fresh temporary one", true)
        .await
        .unwrap();
    assert!(reset.reset_requested_at.is_none());
    assert!(reset.must_change_password);
}

#[tokio::test]
async fn reasons_are_optional_and_recorded_when_given() {
    let h = harness().await;
    let s = h.store.as_ref();
    let vendor = provider(s).await;
    let input = asset_input(s, "R-1", AssetKind::Instrument).await;
    let a = s
        .create_asset(&ctx_without_reason(), input.clone())
        .await
        .unwrap();
    let rec = s
        .create_calibration(&ctx_without_reason(), cal(a.id, vendor, d(2026, 1, 5)))
        .await
        .unwrap();

    // Without a reason, edits and archives still go through.
    let nr = ctx_without_reason();
    let a = s
        .update_asset(&nr, a.id, a.row_version, input)
        .await
        .unwrap();
    let rec = s
        .update_calibration(
            &nr,
            rec.id,
            rec.row_version,
            cal(a.id, vendor, d(2026, 1, 6)),
        )
        .await
        .unwrap();
    s.set_calibration_archived(&nr, rec.id, rec.row_version, true)
        .await
        .unwrap();
    let latest = &s.list_audit(&AuditFilter::default()).await.unwrap()[0];
    assert_eq!(latest.action, AuditAction::Archive);
    assert_eq!(latest.reason, None);

    // With one, it's kept with the change.
    let a = s.get_asset(a.id).await.unwrap();
    s.set_asset_archived(&ctx(), a.id, a.row_version, true)
        .await
        .unwrap();
    let latest = &s.list_audit(&AuditFilter::default()).await.unwrap()[0];
    assert_eq!(latest.reason.as_deref(), Some("test change"));
}

#[tokio::test]
async fn audit_trail_records_who_what_why_and_values() {
    let h = harness().await;
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
    let h = harness().await;
    let s = h.store.as_ref();
    assert_eq!(s.settings().await.unwrap().idle_lock_minutes, 15);
    let bad = s
        .update_settings(
            &ctx(),
            AppSettings {
                idle_lock_minutes: 0,
                attachments_root: None,
                update_checks: true,
                currency_symbol: "$".into(),
                date_format: "system".into(),
                password_policy: Default::default(),
                photos_root: None,
                service_manuals_root: None,
                documents_root: None,
            },
        )
        .await;
    assert!(matches!(bad, Err(Error::Validation(_))));
    s.update_settings(
        &ctx(),
        AppSettings {
            idle_lock_minutes: 30,
            attachments_root: None,
            update_checks: true,
            currency_symbol: "$".into(),
            date_format: "system".into(),
            password_policy: Default::default(),
            photos_root: None,
            service_manuals_root: None,
            documents_root: None,
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

/// The audit table rejects edits and deletes at the database level, not just in the app.
#[tokio::test]
async fn audit_log_is_append_only_in_the_database() {
    let h = harness().await;
    for sql in [
        "UPDATE audit_log SET username = 'someone else'",
        "DELETE FROM audit_log",
    ] {
        let err = polaris_store::testing::execute_raw(&h.config, sql)
            .await
            .expect_err(sql);
        assert!(err.to_string().contains("append-only"), "{sql}: {err}");
    }
    assert!(
        !h.store
            .list_audit(&AuditFilter::default())
            .await
            .unwrap()
            .is_empty()
    );
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
        storage_folder: AttachmentFolder::Certificates,
        relative_path: path.into(),
        size_bytes: 1234,
        sha256: "a".repeat(64),
        content_type: Some("application/pdf".into()),
        description: Some("  ".into()),
    }
}

#[tokio::test]
async fn attachments_link_to_assets_and_calibrations() {
    let h = harness().await;
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
async fn archiving_attachments_is_audited() {
    let h = harness().await;
    let s = h.store.as_ref();
    let a = s
        .create_asset(&ctx(), asset_input(s, "ATT-3", AssetKind::Instrument).await)
        .await
        .unwrap();
    let f = s
        .create_attachment(&ctx(), file(a.id, None, "ATT-3/photo.jpg"))
        .await
        .unwrap();

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
async fn service_manuals_are_shared_by_manufacturer_and_model() {
    let h = harness().await;
    let s = h.store.as_ref();
    let fluke = s
        .create_lookup(&ctx(), LookupKind::Manufacturer, lookup("Fluke"))
        .await
        .unwrap()
        .id;
    let keysight = s
        .create_lookup(&ctx(), LookupKind::Manufacturer, lookup("Keysight"))
        .await
        .unwrap()
        .id;
    let item = |tag: &'static str, maker: Option<i64>, model: Option<&'static str>| async move {
        let mut i = asset_input(s, tag, AssetKind::Instrument).await;
        i.manufacturer_id = maker;
        i.model_number = model.map(Into::into);
        s.create_asset(&ctx(), i).await.unwrap()
    };
    let a = item("DMM-A", Some(fluke), Some("87V")).await;
    let b = item("DMM-B", Some(fluke), Some("87v")).await; // model compared ignoring case
    let other_model = item("DMM-C", Some(fluke), Some("179")).await;
    let other_maker = item("DMM-D", Some(keysight), Some("87V")).await;
    let no_model = item("DMM-E", Some(fluke), None).await;

    let mut manual = file(a.id, None, "manuals/DMM-A/87V manual.pdf");
    manual.category = AttachmentCategory::ServiceManual;
    manual.storage_folder = AttachmentFolder::ServiceManuals;
    let manual = s.create_attachment(&ctx(), manual).await.unwrap();
    assert_eq!(manual.storage_folder, AttachmentFolder::ServiceManuals);
    // Another of A's files that isn't a manual stays with A.
    let photo = s
        .create_attachment(&ctx(), {
            let mut f = file(a.id, None, "DMM-A/front.jpg");
            f.category = AttachmentCategory::Photo;
            f.sha256 = "b".repeat(64);
            f
        })
        .await
        .unwrap();

    let files_of = |id: i64, shared: bool| async move {
        let mut ids: Vec<i64> = s
            .list_attachments(&AttachmentFilter {
                asset_id: Some(id),
                include_shared_manuals: shared,
                ..Default::default()
            })
            .await
            .unwrap()
            .iter()
            .map(|f| f.id)
            .collect();
        ids.sort();
        ids
    };
    assert_eq!(files_of(a.id, true).await, [manual.id, photo.id]);
    assert_eq!(
        files_of(b.id, true).await,
        [manual.id],
        "same maker and model"
    );
    assert!(files_of(b.id, false).await.is_empty(), "only when asked");
    for unrelated in [other_model.id, other_maker.id, no_model.id] {
        assert!(files_of(unrelated, true).await.is_empty());
    }

    // The same path may be used in two folders.
    let mut same_path = file(b.id, None, "manuals/DMM-A/87V manual.pdf");
    same_path.category = AttachmentCategory::Document;
    same_path.storage_folder = AttachmentFolder::Documents;
    assert!(s.create_attachment(&ctx(), same_path).await.is_ok());
}

#[test]
fn files_go_to_their_types_folder_or_the_certificates_folder() {
    let mut settings = AppSettings {
        attachments_root: Some(r"\srv\cal\certs".into()),
        ..Default::default()
    };
    assert_eq!(
        settings.save_to(AttachmentCategory::Photo.folder()),
        (AttachmentFolder::Certificates, Some(r"\srv\cal\certs"))
    );
    settings.photos_root = Some(r"\srv\cal\photos".into());
    assert_eq!(
        settings.save_to(AttachmentCategory::Photo.folder()),
        (AttachmentFolder::Photos, Some(r"\srv\cal\photos"))
    );
    assert_eq!(
        AttachmentCategory::Other.folder(),
        AttachmentFolder::Documents
    );
    assert_eq!(
        AttachmentCategory::ServiceManual.folder(),
        AttachmentFolder::ServiceManuals
    );
}

#[tokio::test]
async fn instruments_check_out_transfer_and_check_in() {
    let h = harness().await;
    let s = h.store.as_ref();
    let owner = |name: &'static str| async move {
        s.create_lookup(&ctx(), LookupKind::Owner, lookup(name))
            .await
            .unwrap()
            .id
    };
    let (dana, jordan) = (owner("Dana Reyes").await, owner("Jordan Kim").await);
    let a = s
        .create_asset(&ctx(), asset_input(s, "CO-1", AssetKind::Instrument).await)
        .await
        .unwrap();
    let b = s
        .create_asset(&ctx(), asset_input(s, "CO-2", AssetKind::Instrument).await)
        .await
        .unwrap();
    assert_eq!((a.owner_id, a.owner_since), (None, None));

    // Check out both at once.
    let out = s
        .change_owner(
            &ctx(),
            &[a.id, b.id],
            OwnerChange::CheckOut { owner_id: dana },
        )
        .await
        .unwrap();
    assert_eq!(out.len(), 2);
    assert!(
        out.iter()
            .all(|x| x.owner_id == Some(dana) && x.owner_since.is_some())
    );

    // Already out: checking out again is refused, and nothing changes (all or none).
    let c = s
        .create_asset(&ctx(), asset_input(s, "CO-3", AssetKind::Instrument).await)
        .await
        .unwrap();
    let refused = s
        .change_owner(
            &ctx(),
            &[c.id, a.id],
            OwnerChange::CheckOut { owner_id: jordan },
        )
        .await;
    assert!(matches!(refused, Err(Error::Conflict(ref m)) if m.contains("Transfer it instead")));
    assert_eq!(s.get_asset(c.id).await.unwrap().owner_id, None);

    // Transfer one to Jordan; transferring to the same person again is refused.
    let moved = s
        .change_owner(&ctx(), &[a.id], OwnerChange::Transfer { owner_id: jordan })
        .await
        .unwrap();
    assert_eq!(moved[0].owner_name.as_deref(), Some("Jordan Kim"));
    assert!(
        s.change_owner(&ctx(), &[a.id], OwnerChange::Transfer { owner_id: jordan })
            .await
            .is_err()
    );
    // Can't transfer or check in what isn't out.
    assert!(
        s.change_owner(&ctx(), &[c.id], OwnerChange::Transfer { owner_id: dana })
            .await
            .is_err()
    );
    assert!(
        s.change_owner(&ctx(), &[c.id], OwnerChange::CheckIn)
            .await
            .is_err()
    );

    // Check in.
    let back = s
        .change_owner(&ctx(), &[a.id, b.id], OwnerChange::CheckIn)
        .await
        .unwrap();
    assert!(
        back.iter()
            .all(|x| x.owner_id.is_none() && x.owner_since.is_none())
    );

    // Systems, archived items, and unknown owners are refused.
    let sys = s
        .create_asset(&ctx(), asset_input(s, "CO-SYS", AssetKind::System).await)
        .await
        .unwrap();
    assert!(
        s.change_owner(&ctx(), &[sys.id], OwnerChange::CheckOut { owner_id: dana })
            .await
            .is_err()
    );
    assert!(
        s.change_owner(&ctx(), &[c.id], OwnerChange::CheckOut { owner_id: 99_999 })
            .await
            .is_err()
    );

    // Each step is in the item's history, with the reason.
    let reasoned = WriteContext::new(ctx().actor, Some("Field job at plant 2".into()));
    s.change_owner(&reasoned, &[c.id], OwnerChange::CheckOut { owner_id: dana })
        .await
        .unwrap();
    let history = s
        .list_audit(&AuditFilter {
            asset_id: Some(a.id),
            ..Default::default()
        })
        .await
        .unwrap();
    let actions: Vec<_> = history.iter().map(|e| e.action).take(3).collect();
    assert_eq!(
        actions,
        [
            AuditAction::CheckIn,
            AuditAction::Transfer,
            AuditAction::CheckOut
        ]
    );
    assert!(history[1].changes.iter().any(|ch| ch.field == "Owner"
        && ch.old.as_deref() == Some("Dana Reyes")
        && ch.new.as_deref() == Some("Jordan Kim")));
    let c_history = s
        .list_audit(&AuditFilter {
            asset_id: Some(c.id),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(c_history[0].reason.as_deref(), Some("Field job at plant 2"));

    // Choosing an owner on the edit form stamps the date too.
    let mut input = asset_input(s, "CO-4", AssetKind::Instrument).await;
    input.owner_id = Some(jordan);
    assert!(
        s.create_asset(&ctx(), input)
            .await
            .unwrap()
            .owner_since
            .is_some()
    );
}

#[tokio::test]
async fn viewers_request_check_outs_that_technicians_approve_or_decline() {
    let h = harness().await;
    let s = h.store.as_ref();
    let viewer = s
        .create_user(&ctx(), new_user("vlee", Role::Viewer))
        .await
        .unwrap();
    let as_viewer = WriteContext::new(
        Actor {
            user_id: viewer.id,
            username: viewer.username.clone(),
        },
        None,
    );
    let owner = |name: &'static str| async move {
        s.create_lookup(&ctx(), LookupKind::Owner, lookup(name))
            .await
            .unwrap()
            .id
    };
    let (val, jordan) = (owner("Val Lee").await, owner("Jordan Kim").await);
    let a = s
        .create_asset(&ctx(), asset_input(s, "RQ-1", AssetKind::Instrument).await)
        .await
        .unwrap();
    let b = s
        .create_asset(&ctx(), asset_input(s, "RQ-2", AssetKind::Instrument).await)
        .await
        .unwrap();

    // A viewer asks for A to be checked out to them.
    let asked = s
        .request_owner_change(
            &as_viewer,
            &[a.id],
            OwnerChange::CheckOut { owner_id: val },
            Some(" Site visit ".into()),
        )
        .await
        .unwrap();
    let req = &asked[0];
    assert_eq!(req.status, OwnerRequestStatus::Pending);
    assert_eq!(
        (req.kind, req.owner_name.as_str(), req.requested_by.as_str()),
        (OwnerRequestKind::CheckOut, "Val Lee", "vlee")
    );
    assert_eq!(req.note.as_deref(), Some("Site visit"));
    assert_eq!(
        s.get_asset(a.id).await.unwrap().owner_id,
        None,
        "not until approved"
    );

    // One waiting request per item; check-in can't be requested.
    assert!(matches!(
        s.request_owner_change(
            &as_viewer,
            &[a.id],
            OwnerChange::CheckOut { owner_id: jordan },
            None
        )
        .await,
        Err(Error::Conflict(_))
    ));
    assert!(
        s.request_owner_change(&as_viewer, &[b.id], OwnerChange::CheckIn, None)
            .await
            .is_err()
    );
    let waiting = s
        .list_owner_requests(&OwnerRequestFilter::default())
        .await
        .unwrap();
    assert_eq!(waiting.iter().map(|r| r.id).collect::<Vec<_>>(), [req.id]);

    // Approved: the item is checked out, recorded with who asked.
    let approved = s
        .decide_owner_request(&ctx(), req.id, true, Some("OK".into()))
        .await
        .unwrap();
    assert_eq!(approved.status, OwnerRequestStatus::Approved);
    assert_eq!(approved.decided_by.as_deref(), Some("admin"));
    assert_eq!(approved.current_owner_name.as_deref(), Some("Val Lee"));
    assert_eq!(s.get_asset(a.id).await.unwrap().owner_id, Some(val));
    let history = s
        .list_audit(&AuditFilter {
            asset_id: Some(a.id),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(history[0].action, AuditAction::CheckOut);
    assert_eq!(
        history[0].reason.as_deref(),
        Some("Requested by vlee: Site visit. Approved: OK")
    );
    assert_eq!(history[1].action, AuditAction::Request);
    assert_eq!(history[1].username, "vlee");
    // Deciding twice is refused.
    assert!(
        s.decide_owner_request(&ctx(), req.id, false, None)
            .await
            .is_err()
    );

    // A transfer request, declined: nothing changes.
    let t = s
        .request_owner_change(
            &as_viewer,
            &[a.id],
            OwnerChange::Transfer { owner_id: jordan },
            None,
        )
        .await
        .unwrap();
    let declined = s
        .decide_owner_request(&ctx(), t[0].id, false, Some("Needed here".into()))
        .await
        .unwrap();
    assert_eq!(declined.status, OwnerRequestStatus::Declined);
    assert_eq!(declined.decision_note.as_deref(), Some("Needed here"));
    assert_eq!(s.get_asset(a.id).await.unwrap().owner_id, Some(val));

    // Cancelled by the person who asked.
    let c = s
        .request_owner_change(
            &as_viewer,
            &[b.id],
            OwnerChange::CheckOut { owner_id: val },
            None,
        )
        .await
        .unwrap();
    let cancelled = s.cancel_owner_request(&as_viewer, c[0].id).await.unwrap();
    assert_eq!(cancelled.status, OwnerRequestStatus::Cancelled);

    // Recently decided ones are listed when asked for, after the waiting ones.
    let since = chrono::Utc::now() - chrono::Duration::days(30);
    let recent = s
        .list_owner_requests(&OwnerRequestFilter {
            asset_id: None,
            decided_since: Some(since),
        })
        .await
        .unwrap();
    assert_eq!(recent.len(), 3);
    assert!(
        s.list_owner_requests(&OwnerRequestFilter::default())
            .await
            .unwrap()
            .is_empty()
    );

    // A request that's no longer possible can't be approved, and stays waiting.
    let stale = s
        .request_owner_change(
            &as_viewer,
            &[b.id],
            OwnerChange::CheckOut { owner_id: val },
            None,
        )
        .await
        .unwrap();
    s.change_owner(&ctx(), &[b.id], OwnerChange::CheckOut { owner_id: jordan })
        .await
        .unwrap();
    assert!(matches!(
        s.decide_owner_request(&ctx(), stale[0].id, true, None).await,
        Err(Error::Conflict(ref m)) if m.contains("already checked out")
    ));
    assert_eq!(
        s.get_owner_request(stale[0].id).await.unwrap().status,
        OwnerRequestStatus::Pending
    );
}

#[tokio::test]
async fn attachments_need_an_active_item() {
    let h = harness().await;
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
    let h = harness().await;
    let s = h.store.as_ref();
    assert_eq!(s.settings().await.unwrap().attachments_root, None);
    let saved = s
        .update_settings(
            &ctx(),
            AppSettings {
                idle_lock_minutes: 15,
                attachments_root: Some(r" \\files\cal ".into()),
                update_checks: true,
                currency_symbol: "$".into(),
                date_format: "system".into(),
                password_policy: Default::default(),
                photos_root: None,
                service_manuals_root: None,
                documents_root: None,
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
                update_checks: true,
                currency_symbol: "$".into(),
                date_format: "system".into(),
                password_policy: Default::default(),
                photos_root: None,
                service_manuals_root: None,
                documents_root: None,
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
            update_checks: true,
            currency_symbol: "$".into(),
            date_format: "system".into(),
            password_policy: Default::default(),
            photos_root: None,
            service_manuals_root: None,
            documents_root: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(s.settings().await.unwrap().attachments_root, None);
}

#[tokio::test]
async fn update_checks_default_on_and_can_be_turned_off() {
    let h = harness().await;
    let s = h.store.as_ref();
    assert!(s.settings().await.unwrap().update_checks);
    let mut settings = s.settings().await.unwrap();
    settings.update_checks = false;
    s.update_settings(&ctx(), settings).await.unwrap();
    assert!(!s.settings().await.unwrap().update_checks);
    let entry = &s.list_audit(&AuditFilter::default()).await.unwrap()[0];
    assert_eq!(entry.entity, AuditEntity::Settings);
    assert_eq!(entry.changes[0].field, "Check for updates");
    assert_eq!(entry.changes[0].new.as_deref(), Some("No"));
}

#[tokio::test]
async fn date_format_defaults_to_the_windows_setting_and_is_checked() {
    let h = harness().await;
    let s = h.store.as_ref();
    let mut settings = s.settings().await.unwrap();
    assert_eq!(settings.date_format, "system");
    settings.date_format = "dmy_dot".into();
    s.update_settings(&ctx(), settings.clone()).await.unwrap();
    assert_eq!(s.settings().await.unwrap().date_format, "dmy_dot");

    settings.date_format = "yyyy.mm".into();
    let err = s.update_settings(&ctx(), settings).await.unwrap_err();
    assert!(matches!(err, Error::Validation(ref e) if e[0].field == "date_format"));
}

#[tokio::test]
async fn currency_symbol_defaults_to_dollar_and_is_audited() {
    let h = harness().await;
    let s = h.store.as_ref();
    let mut settings = s.settings().await.unwrap();
    assert_eq!(settings.currency_symbol, "$");
    settings.currency_symbol = " € ".into();
    assert_eq!(
        s.update_settings(&ctx(), settings.clone())
            .await
            .unwrap()
            .currency_symbol,
        "€"
    );
    assert_eq!(s.settings().await.unwrap().currency_symbol, "€");
    let entry = &s.list_audit(&AuditFilter::default()).await.unwrap()[0];
    assert!(entry.changes.iter().any(|c| c.field == "Currency symbol"
        && c.old.as_deref() == Some("$")
        && c.new.as_deref() == Some("€")));

    settings.currency_symbol = "   ".into();
    let err = s.update_settings(&ctx(), settings).await.unwrap_err();
    assert!(matches!(err, Error::Validation(ref e) if e[0].field == "currency_symbol"));
}

// ---------------------------------------------------------------------------
// Copying a whole database between backends
// ---------------------------------------------------------------------------

/// Fills a database with a bit of everything: lists, a System with an Instrument,
/// calibrations, a file, another user, settings, and their audit entries.
async fn populate(store: &dyn Store) {
    let provider = provider(store).await;
    let mut location = lookup("Lab 2");
    location.description = Some("Second floor".into());
    let location = store
        .create_lookup(&ctx(), LookupKind::Location, location)
        .await
        .unwrap();
    let system = store
        .create_asset(&ctx(), asset_input(store, "SYS-1", AssetKind::System).await)
        .await
        .unwrap();
    let mut child = asset_input(store, "INS-1", AssetKind::Instrument).await;
    child.parent_id = Some(system.id);
    child.location_id = Some(location.id);
    child.serial_number = Some("SN-ÄÖ-1".into());
    let child = store.create_asset(&ctx(), child).await.unwrap();
    let record = store
        .create_calibration(&ctx(), cal(child.id, provider, d(2026, 3, 1)))
        .await
        .unwrap();
    store
        .create_calibration(&ctx(), cal(child.id, provider, d(2025, 3, 1)))
        .await
        .unwrap();
    store
        .create_attachment(
            &ctx(),
            file(child.id, Some(record.id), "INS-1/2026-03-01/cert.pdf"),
        )
        .await
        .unwrap();
    store
        .create_user(&ctx(), new_user("tech", Role::Technician))
        .await
        .unwrap();
    store
        .update_settings(
            &ctx(),
            AppSettings {
                idle_lock_minutes: 30,
                attachments_root: Some(r"\\server\share\PolarisCal".into()),
                update_checks: true,
                currency_symbol: "$".into(),
                date_format: "system".into(),
                password_policy: Default::default(),
                photos_root: None,
                service_manuals_root: None,
                documents_root: None,
            },
        )
        .await
        .unwrap();
}

/// Everything a user can see, for comparing two databases.
async fn visible_state(
    store: &dyn Store,
) -> (
    Vec<Asset>,
    Vec<CalibrationRecord>,
    Vec<Attachment>,
    Vec<Vec<LookupItem>>,
    Vec<polaris_core::auth::User>,
    AppSettings,
) {
    let assets = store
        .list_assets(&AssetFilter {
            include_archived: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let cals = store
        .list_calibrations(&CalibrationFilter {
            include_archived: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let files = store
        .list_attachments(&AttachmentFilter {
            include_archived: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let mut lists = Vec::new();
    for kind in [
        LookupKind::Location,
        LookupKind::Department,
        LookupKind::Owner,
        LookupKind::Manufacturer,
        LookupKind::Provider,
        LookupKind::Status,
        LookupKind::ServiceType,
    ] {
        lists.push(store.list_lookups(kind, true).await.unwrap());
    }
    let users = store.list_users().await.unwrap();
    let settings = store.settings().await.unwrap();
    (assets, cals, files, lists, users, settings)
}

/// Copies `source` into the empty `target_config` and checks the result.
async fn copy_and_verify(source: &dyn Store, target_config: &DatabaseConfig) -> Arc<dyn Store> {
    let target = polaris_store::open(target_config).await.unwrap();
    let source_audit = source.list_audit(&AuditFilter::default()).await.unwrap();
    let report =
        polaris_store::copy_database(source, target.as_ref(), &ctx(), "the source", "the target")
            .await
            .unwrap();

    for t in &report.tables {
        assert_eq!(t.source_rows, t.copied_rows, "{}", t.table);
    }
    let assets = report.tables.iter().find(|t| t.table == "assets").unwrap();
    assert_eq!(
        (assets.label.as_str(), assets.copied_rows),
        ("Instruments and Systems", 2)
    );
    assert_eq!(
        visible_state(source).await,
        visible_state(target.as_ref()).await
    );

    // The audit trail came across intact, plus one entry for the copy on each side.
    let target_audit = target.list_audit(&AuditFilter::default()).await.unwrap();
    assert_eq!(target_audit.len(), source_audit.len() + 1);
    assert_eq!(target_audit[0].action, AuditAction::Import);
    assert_eq!(target_audit[0].entity, AuditEntity::Database);
    assert!(
        target_audit[0]
            .entity_label
            .as_deref()
            .unwrap()
            .contains("from the source")
    );
    assert_eq!(&target_audit[1..], &source_audit[..]);
    let source_after = source.list_audit(&AuditFilter::default()).await.unwrap();
    assert_eq!(source_after[0].action, AuditAction::Export);

    // Passwords still work, and new rows get fresh ids after the copied ones.
    let creds = target.credentials("TECH").await.unwrap().unwrap();
    assert!(verify_password(ADMIN_PASSWORD, &creds.password_hash));
    let max_id = visible_state(target.as_ref())
        .await
        .0
        .iter()
        .map(|a| a.id)
        .max()
        .unwrap();
    let new_asset = target
        .create_asset(
            &ctx(),
            asset_input(target.as_ref(), "NEW-1", AssetKind::Instrument).await,
        )
        .await
        .unwrap();
    assert!(new_asset.id > max_id);
    target
        .create_lookup(&ctx(), LookupKind::Location, lookup("Lab 3"))
        .await
        .unwrap();
    target
        .create_user(&ctx(), new_user("newer", Role::Viewer))
        .await
        .unwrap();

    // A database that already has data is never merged into.
    let again = polaris_store::copy_database(source, target.as_ref(), &ctx(), "a", "b").await;
    assert!(matches!(again, Err(Error::Conflict(_))), "{again:?}");
    target
}

#[tokio::test]
async fn copying_a_database_keeps_every_record_and_verifies_counts() {
    let h = harness().await;
    populate(h.store.as_ref()).await;

    // This backend to a new SQLite file (server -> SQLite when testing a server).
    let dir = tempfile::tempdir().unwrap();
    let file_copy = copy_and_verify(h.store.as_ref(), &sqlite_in(&dir, "copy.db")).await;

    // And back again into a new database of the backend under test (SQLite -> server).
    let (round_trip, _dir2) = empty_database().await;
    let fresh = polaris_store::open(&round_trip).await.unwrap();
    drop(fresh);
    let source = polaris_store::open(&sqlite_in(&dir, "copy-source.db"))
        .await
        .unwrap();
    setup_admin(source.as_ref()).await;
    populate(source.as_ref()).await;
    copy_and_verify(source.as_ref(), &round_trip).await;
    drop(file_copy);
}

#[tokio::test]
async fn copying_into_a_database_in_use_is_refused() {
    let h = harness().await;
    populate(h.store.as_ref()).await;
    let other = harness().await;
    let err =
        polaris_store::copy_database(h.store.as_ref(), other.store.as_ref(), &ctx(), "a", "b")
            .await
            .unwrap_err();
    assert!(err.to_string().contains("already contains data"), "{err}");
    // Nothing was changed in either database.
    assert_eq!(other.store.user_count().await.unwrap(), 1);
    assert!(
        h.store
            .list_audit(&AuditFilter::default())
            .await
            .unwrap()
            .iter()
            .all(|e| e.action != AuditAction::Export)
    );
}

fn certificate_details(kind: CertificateKind) -> CertificateDetails {
    CertificateDetails {
        kind,
        asset_id: None,
        customer_name: "Acme Labs".into(),
        customer_address: "1 Main St".into(),
        asset_tag: String::new(),
        manufacturer: "Fluke".into(),
        description: "Digital multimeter".into(),
        model_number: "87V".into(),
        serial_number: "SN-1".into(),
        procedure: "CP-100".into(),
        temperature: "21.5".into(),
        humidity: "45".into(),
        cal_date: Local::now().date_naive(),
        next_cal_date: None,
        condition_received: Some(ConditionReceived::Pass),
        condition_returned: Some(ConditionReturned::Pass),
        equipment: vec![CertificateEquipment {
            asset_tag: "STD-1".into(),
            description: "Calibrator".into(),
            ..Default::default()
        }],
        calibrated_by: "Tech".into(),
        calibrated_by_id: None,
        approved_by: String::new(),
        approved_by_id: None,
        calibrated_by_signature: None,
        approved_by_signature: None,
        test_data_file: String::new(),
    }
}

#[tokio::test]
async fn certificates_are_numbered_logged_and_keep_their_template() {
    let h = harness().await;
    let s = h.store.as_ref();
    let year = Local::now().format("%Y").to_string();

    let template = s.certificate_template().await.unwrap();
    assert_eq!(template, CertificateTemplate::default());

    let mut edit = template.clone();
    edit.organization_name = "Polaris Metrology".into();
    edit.number_pattern = "PM-{YYYY}-{SEQ}".into();
    edit.number_digits = 3;
    edit.next_number = 41;
    let saved = s.update_certificate_template(&ctx(), edit).await.unwrap();
    assert_eq!(saved.next_number, 41);

    let first = s
        .create_certificate(&ctx(), certificate_details(CertificateKind::External))
        .await
        .unwrap();
    assert_eq!(first.summary.certificate_number, format!("PM-{year}-041"));
    assert_eq!(first.summary.subject, "Acme Labs");
    assert_eq!(first.template.organization_name, "Polaris Metrology");
    assert_eq!(first.details.equipment.len(), 1);
    assert_eq!(s.certificate_template().await.unwrap().next_number, 42);

    // Internal certificates belong to an item.
    let asset = s
        .create_asset(&ctx(), asset_input(s, "DMM-9", AssetKind::Instrument).await)
        .await
        .unwrap();
    let mut internal = certificate_details(CertificateKind::Internal);
    internal.asset_id = Some(asset.id);
    internal.asset_tag = "DMM-9".into();
    let second = s.create_certificate(&ctx(), internal).await.unwrap();
    assert_eq!(second.summary.certificate_number, format!("PM-{year}-042"));
    assert_eq!(
        second.details.customer_name, "",
        "internal certificates have no customer"
    );

    // A later template change doesn't alter what was issued.
    let mut edit = s.certificate_template().await.unwrap();
    edit.organization_name = "Renamed".into();
    s.update_certificate_template(&ctx(), edit).await.unwrap();
    let again = s.get_certificate(first.summary.id).await.unwrap();
    assert_eq!(again.template.organization_name, "Polaris Metrology");
    assert_eq!(again.details, first.details);

    let all = s
        .list_certificates(&CertificateFilter::default())
        .await
        .unwrap();
    let numbers: Vec<_> = all.iter().map(|c| c.certificate_number.as_str()).collect();
    assert_eq!(
        numbers,
        [
            second.summary.certificate_number.as_str(),
            first.summary.certificate_number.as_str()
        ]
    );
    let found = s
        .list_certificates(&CertificateFilter {
            search: Some("acme".into()),
            asset_id: None,
        })
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    let for_item = s
        .list_certificates(&CertificateFilter {
            search: None,
            asset_id: Some(asset.id),
        })
        .await
        .unwrap();
    assert_eq!(for_item.len(), 1);

    // Setting the counter back to a used number is caught when issuing.
    let mut edit = s.certificate_template().await.unwrap();
    edit.next_number = 41;
    s.update_certificate_template(&ctx(), edit).await.unwrap();
    let err = s
        .create_certificate(&ctx(), certificate_details(CertificateKind::External))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Duplicate(_)), "{err:?}");
    assert_eq!(
        s.certificate_template().await.unwrap().next_number,
        41,
        "a failed issue keeps the counter"
    );

    // Details are checked before anything is written.
    let mut bad = certificate_details(CertificateKind::External);
    bad.customer_name = " ".into();
    let err = s.create_certificate(&ctx(), bad).await.unwrap_err();
    assert!(matches!(err, Error::Validation(ref e) if e[0].field == "customer_name"));

    // Signers who are users get their saved signature; input can't supply one.
    let me = s.list_users().await.unwrap()[0].clone();
    assert_eq!(s.user_signature(me.id).await.unwrap(), None);
    let png = "data:image/png;base64,iVBORw0KGgo=".to_string();
    s.set_user_signature(&ctx(), me.id, Some(png.clone()))
        .await
        .unwrap();
    assert_eq!(s.user_signature(me.id).await.unwrap(), Some(png.clone()));
    let signers = s.certificate_signers().await.unwrap();
    assert!(signers.iter().any(|u| u.id == me.id && u.has_signature));
    let mut signed = certificate_details(CertificateKind::External);
    signed.calibrated_by_id = Some(me.id);
    signed.calibrated_by_signature = Some("data:image/png;base64,forged".into());
    signed.approved_by_signature = Some("data:image/png;base64,forged".into());
    let mut edit = s.certificate_template().await.unwrap();
    edit.next_number = 50;
    s.update_certificate_template(&ctx(), edit).await.unwrap();
    let c = s.create_certificate(&ctx(), signed).await.unwrap();
    assert_eq!(c.details.calibrated_by_signature, Some(png.clone()));
    assert_eq!(
        c.details.approved_by, "Tech",
        "a blank approver is the calibrator"
    );
    assert_eq!(c.details.approved_by_id, Some(me.id));
    assert_eq!(c.details.approved_by_signature, Some(png));
    let err = s
        .set_user_signature(&ctx(), me.id, Some("not an image".into()))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Validation(ref e) if e[0].field == "signature"));
    s.set_user_signature(&ctx(), me.id, None).await.unwrap();
    assert_eq!(s.user_signature(me.id).await.unwrap(), None);

    let audit = s.list_audit(&AuditFilter::default()).await.unwrap();
    assert!(audit.iter().any(|e| e.entity == AuditEntity::Certificate
        && e.entity_label.as_deref() == Some(first.summary.certificate_number.as_str())));
}
