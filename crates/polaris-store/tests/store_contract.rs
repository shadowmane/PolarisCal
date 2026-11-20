//! Behavioral tests for the `Store` contract. They only use the trait, so the
//! same tests can run against every backend; today that is SQLite.

use std::sync::Arc;

use chrono::{Days, Local, NaiveDate};
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
    Harness { store, _dir: dir }
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
        .create_lookup(LookupKind::Provider, input)
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
    assert_eq!(info.schema_version, 1);
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
        let input = asset_input(store.as_ref(), "KEEP-1", AssetKind::Instrument).await;
        store.create_asset(input).await.unwrap();
    }
    let store = polaris_store::open(&config).await.unwrap();
    assert_eq!(store.info().await.unwrap().asset_count, 1);
}

#[tokio::test]
async fn lookups_create_update_and_reject_duplicates() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    let loc = s
        .create_lookup(LookupKind::Location, lookup(" Lab A "))
        .await
        .unwrap();
    assert_eq!(loc.name, "Lab A");
    assert_eq!(loc.kind, LookupKind::Location);

    let dup = s.create_lookup(LookupKind::Location, lookup("LAB A")).await;
    assert!(
        matches!(dup, Err(Error::Duplicate(_))),
        "names are unique ignoring case: {dup:?}"
    );

    // the same name is fine in a different list
    s.create_lookup(LookupKind::Department, lookup("Lab A"))
        .await
        .unwrap();

    let mut edit = lookup("Lab A (Building 2)");
    edit.is_active = false;
    let updated = s
        .update_lookup(LookupKind::Location, loc.id, edit)
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
        .create_lookup(LookupKind::Provider, lookup("Default Vendor"))
        .await
        .unwrap();
    assert_eq!(
        vendor.is_internal,
        Some(false),
        "providers default to external vendors"
    );
    let status = s
        .create_lookup(LookupKind::Status, lookup("Quarantined"))
        .await
        .unwrap();
    assert_eq!(
        status.tracks_due,
        Some(true),
        "new statuses track due dates by default"
    );

    let missing = s
        .update_lookup(LookupKind::Location, 9999, lookup("x"))
        .await;
    assert!(matches!(missing, Err(Error::NotFound { .. })));
}

#[tokio::test]
async fn asset_create_joins_lookups_and_derives_next_date() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    let mfr = s
        .create_lookup(LookupKind::Manufacturer, lookup("Fluke"))
        .await
        .unwrap();
    let mut input = asset_input(s, "DMM-001", AssetKind::Instrument).await;
    input.manufacturer_id = Some(mfr.id);
    input.last_cal_date = Some(d(2026, 3, 1));
    input.serial_number = Some(" 12345 ".into());

    let a = s.create_asset(input).await.unwrap();
    assert_eq!(a.asset_tag, "DMM-001");
    assert_eq!(a.manufacturer_name.as_deref(), Some("Fluke"));
    assert_eq!(a.status_name, "Active");
    assert_eq!(a.serial_number.as_deref(), Some("12345"));
    assert_eq!(a.next_cal_date, Some(d(2027, 3, 1)));
    assert_eq!(a.row_version, 1);

    let dup = s
        .create_asset(asset_input(s, "dmm-001", AssetKind::Instrument).await)
        .await;
    assert!(matches!(dup, Err(Error::Duplicate(_))), "{dup:?}");
}

#[tokio::test]
async fn hierarchy_rules_are_enforced() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    let system = s
        .create_asset(asset_input(s, "RIG-1", AssetKind::System).await)
        .await
        .unwrap();
    let other = s
        .create_asset(asset_input(s, "DMM-9", AssetKind::Instrument).await)
        .await
        .unwrap();

    // instrument inside a system: ok
    let mut child = asset_input(s, "PROBE-1", AssetKind::Instrument).await;
    child.parent_id = Some(system.id);
    let child = s.create_asset(child).await.unwrap();
    assert_eq!(child.parent_tag.as_deref(), Some("RIG-1"));
    assert_eq!(s.get_asset(system.id).await.unwrap().child_count, 1);

    // parent must be a system
    let mut bad = asset_input(s, "PROBE-2", AssetKind::Instrument).await;
    bad.parent_id = Some(other.id);
    assert!(matches!(
        s.create_asset(bad).await,
        Err(Error::Validation(_))
    ));

    // a system with children can't become an instrument
    let mut demote = asset_input(s, "RIG-1", AssetKind::Instrument).await;
    demote.description = system.description.clone();
    let r = s.update_asset(system.id, system.row_version, demote).await;
    assert!(
        matches!(r, Err(Error::Validation(ref e)) if e[0].field == "kind"),
        "{r:?}"
    );

    // nor be archived while it has active children
    let r = s
        .set_asset_archived(system.id, system.row_version, true)
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
        .create_asset(asset_input(s, "T-1", AssetKind::Instrument).await)
        .await
        .unwrap();

    let mut edit = asset_input(s, "T-1", AssetKind::Instrument).await;
    edit.description = "first edit".into();
    let a2 = s
        .update_asset(a.id, a.row_version, edit.clone())
        .await
        .unwrap();
    assert_eq!(a2.row_version, 2);

    edit.description = "second edit from a stale copy".into();
    let stale = s.update_asset(a.id, a.row_version, edit.clone()).await;
    assert!(matches!(stale, Err(Error::Conflict(_))), "{stale:?}");

    let gone = s.update_asset(424242, 1, edit).await;
    assert!(matches!(gone, Err(Error::NotFound { .. })), "{gone:?}");
}

#[tokio::test]
async fn calibration_records_drive_asset_dates() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    let vendor = provider(s).await;
    let mut input = asset_input(s, "GAUGE-1", AssetKind::Instrument).await;
    input.first_cal_date = None;
    let asset = s.create_asset(input).await.unwrap();
    assert_eq!(asset.next_cal_date, None);
    assert_eq!(asset.due_status, DueStatus::NoSchedule);

    let first = s
        .create_calibration(cal(asset.id, vendor, d(2025, 6, 10)))
        .await
        .unwrap();
    assert_eq!(first.technician.as_deref(), Some("J. Smith"));
    assert_eq!(first.next_due_date, Some(d(2026, 6, 10)));
    assert_eq!(first.provider_name, "Acme Calibration Labs");

    let latest = s
        .create_calibration(cal(asset.id, vendor, d(2026, 6, 1)))
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
    s.set_calibration_archived(latest.id, latest.row_version, true)
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
    let a = s.update_asset(a.id, a.row_version, edit).await.unwrap();
    assert_eq!(a.next_cal_date, Some(d(2025, 12, 10)));
    assert_eq!(a.due_status, DueStatus::Overdue);
}

#[tokio::test]
async fn calibration_validation() {
    let h = sqlite().await;
    let s = h.store.as_ref();
    let vendor = provider(s).await;
    let asset = s
        .create_asset(asset_input(s, "V-1", AssetKind::Instrument).await)
        .await
        .unwrap();

    let tomorrow = Local::now()
        .date_naive()
        .checked_add_days(Days::new(1))
        .unwrap();
    let future = s.create_calibration(cal(asset.id, vendor, tomorrow)).await;
    assert!(matches!(future, Err(Error::Validation(_))), "{future:?}");

    let no_asset = s.create_calibration(cal(9999, vendor, d(2026, 1, 1))).await;
    assert!(matches!(no_asset, Err(Error::Validation(ref e)) if e[0].field == "asset_id"));

    let no_provider = s
        .create_calibration(cal(asset.id, 9999, d(2026, 1, 1)))
        .await;
    assert!(
        matches!(no_provider, Err(Error::Validation(_))),
        "{no_provider:?}"
    );

    let rec = s
        .create_calibration(cal(asset.id, vendor, d(2026, 1, 1)))
        .await
        .unwrap();
    let other = s
        .create_asset(asset_input(s, "V-2", AssetKind::Instrument).await)
        .await
        .unwrap();
    let mv = s
        .update_calibration(
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
    let scope = s.create_asset(a).await.unwrap();
    s.create_asset(asset_input(s, "DMM-2", AssetKind::Instrument).await)
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

    s.set_asset_archived(scope.id, scope.row_version, true)
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
