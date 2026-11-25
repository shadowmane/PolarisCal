//! The one [`Store`] implementation, shared by every database backend.

use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Local, NaiveDate, SubsecRound, Utc};
use polaris_core::audit::{
    Actor, AuditEntry, AuditFilter, FieldChange, NewAuditEntry, Snapshot, WriteContext,
    asset_snapshot, attachment_snapshot, calibration_snapshot, created, deleted, diff,
    lookup_snapshot, user_snapshot,
};
use polaris_core::auth::{
    NewUser, PasswordPolicy, User, UserCredentials, UserUpdate, hash_password,
};
use polaris_core::certificate::{
    Certificate, CertificateDetails, CertificateFilter, CertificateKind, CertificateSigner,
    CertificateSummary, CertificateTemplate, check_image,
};
use polaris_core::model::{
    AppSettings, Asset, AssetFilter, AssetInput, AssetKind, Attachment, AttachmentFilter,
    AuditAction, AuditEntity, CalInterval, CalibratedBy, CalibrationFilter, CalibrationInput,
    CalibrationRecord, ConditionReturned, LastCost, LookupInput, LookupItem, LookupKind,
    NewAttachment, OwnerChange, OwnerRequest, OwnerRequestFilter, OwnerRequestKind,
    OwnerRequestStatus, Reservation, ReservationCheck, ReservationFilter, ReservationStatus, Role,
    StatusRole, StoreInfo,
};
use polaris_core::schedule::{
    add_interval, apply_extension, derive_dates, due_status_with_extension,
};
use polaris_core::transfer::{DataSet, TableCount};
use polaris_core::{Error, Id, Result, Store};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{ConnectOptions, Connection};

use crate::sql::{Conn, Dialect, Pool, Row, Value, mssql, params, write_err};
use crate::{ConnectionCheck, DatabaseConfig};

static SQLITE_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations/sqlite");
static POSTGRES_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations/postgres");
static MYSQL_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations/mysql");

pub struct SqlStore {
    pool: Pool,
    backend: &'static str,
    location: String,
}

fn upgrade_failed(e: impl std::fmt::Display) -> Error {
    Error::Database(format!("database upgrade failed: {e}"))
}

impl SqlStore {
    /// Connects (creating a SQLite file if needed) and runs pending migrations.
    pub async fn open(config: &DatabaseConfig) -> Result<Self> {
        let mut config = config.clone();
        config.normalize()?;
        let store = Self::connect(&config, true).await?;
        store.migrate().await?;
        Ok(store)
    }

    /// Connects without touching the schema.
    async fn connect(config: &DatabaseConfig, create: bool) -> Result<Self> {
        let unreachable = |e: &dyn std::fmt::Display| {
            Error::Config(format!("Can't connect to {}: {e}", config.describe()))
        };
        let location = match config {
            DatabaseConfig::Sqlite { path } => path.clone(),
            other => other.server().expect("server config").location(),
        };
        let (pool, backend) = match config {
            DatabaseConfig::Sqlite { path } => (
                Pool::Sqlite(sqlite_pool(Path::new(path), create).await?),
                "sqlite",
            ),
            DatabaseConfig::Postgres(s) => {
                let options = crate::pg_options(s)?;
                // One direct attempt first: it fails fast with the real reason
                // (refused, unknown host, bad password) instead of a pool timeout.
                options
                    .connect()
                    .await
                    .map_err(|e| unreachable(&e))?
                    .close()
                    .await
                    .ok();
                (
                    Pool::Postgres(
                        crate::pg_pool_options()
                            .connect_with(options)
                            .await
                            .map_err(|e| unreachable(&e))?,
                    ),
                    "postgres",
                )
            }
            DatabaseConfig::Mysql(s) => {
                let options = crate::mysql_options(s)?;
                options
                    .connect()
                    .await
                    .map_err(|e| unreachable(&e))?
                    .close()
                    .await
                    .ok();
                (
                    Pool::MySql(
                        crate::mysql_pool_options()
                            .connect_with(options)
                            .await
                            .map_err(|e| unreachable(&e))?,
                    ),
                    "mysql",
                )
            }
            DatabaseConfig::SqlServer(s) => {
                let (tds, named) = crate::mssql_options(s)?;
                let pool = mssql::Pool::new(tds, named, crate::MSSQL_POOL_SIZE);
                // Connect once now so a bad address or password is reported at startup.
                drop(pool.get().await.map_err(|e| unreachable(&e.message))?);
                (Pool::Mssql(pool), "mssql")
            }
        };
        Ok(Self {
            pool,
            backend,
            location,
        })
    }

    async fn migrate(&self) -> Result<()> {
        match &self.pool {
            Pool::Sqlite(p) => SQLITE_MIGRATOR.run(p).await.map_err(upgrade_failed),
            Pool::Postgres(p) => POSTGRES_MIGRATOR.run(p).await.map_err(upgrade_failed),
            Pool::MySql(p) => MYSQL_MIGRATOR.run(p).await.map_err(upgrade_failed),
            Pool::Mssql(p) => mssql::migrate(p)
                .await
                .map_err(|e| upgrade_failed(e.message)),
        }
    }

    fn schema_version_sql(&self) -> String {
        match self.pool.dialect() {
            Dialect::Mssql => format!(
                "SELECT COALESCE(MAX(version), 0) AS n FROM {}",
                mssql::VERSION_TABLE
            ),
            _ => "SELECT COALESCE(MAX(version), 0) AS n FROM _sqlx_migrations WHERE success = TRUE"
                .into(),
        }
    }

    /// Reports what's in a database without creating or upgrading anything.
    pub(crate) async fn check(config: &DatabaseConfig) -> Result<ConnectionCheck> {
        let store = Self::connect(config, false).await?;
        let result = async {
            let mut conn = store.pool.acquire().await?;
            let version_sql = match store.pool.dialect() {
                Dialect::Sqlite => "SELECT 'SQLite ' || sqlite_version() AS v",
                Dialect::Postgres => "SELECT version() AS v",
                Dialect::MySql => "SELECT VERSION() AS v",
                Dialect::Mssql => "SELECT CAST(@@VERSION AS NVARCHAR(4000)) AS v",
            };
            let server_version: String = conn
                .fetch_optional(version_sql, params![])
                .await?
                .map(|r| r.get::<String>("v"))
                .transpose()?
                .unwrap_or_default();
            let server_version = server_version
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            // These fail on a database PolarisCal hasn't set up yet; that's an answer, not an error.
            let schema_version = conn
                .scalar(&store.schema_version_sql(), params![])
                .await
                .ok();
            let user_count = match schema_version {
                Some(_) => conn
                    .scalar("SELECT COUNT(*) AS n FROM users", params![])
                    .await
                    .ok(),
                None => None,
            };
            Ok::<_, Error>(ConnectionCheck {
                server_version,
                schema_version,
                user_count,
                exists: true,
            })
        }
        .await;
        store.pool.close().await;
        result
    }

    pub(crate) async fn execute_raw(config: &DatabaseConfig, sql: &str) -> Result<()> {
        let store = Self::connect(config, false).await?;
        let result = async {
            let mut conn = store.pool.acquire().await?;
            conn.batch(sql).await?;
            Ok(())
        }
        .await;
        store.pool.close().await;
        result
    }

    pub(crate) fn dialect(&self) -> Dialect {
        self.pool.dialect()
    }

    pub(crate) async fn begin(&self) -> Result<Conn> {
        self.pool.begin().await
    }

    pub(crate) async fn conn(&self) -> Result<Conn> {
        self.pool.acquire().await
    }
}

async fn sqlite_pool(path: &Path, create: bool) -> Result<sqlx::SqlitePool> {
    if create && let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| {
            Error::Config(format!("Can't create the folder {}: {e}", parent.display()))
        })?;
    }

    // WAL is faster and allows readers during writes, but it does not work
    // on network shares, so fall back to rollback journaling for UNC paths.
    let on_network_share = path.to_string_lossy().starts_with(r"\\");
    let (journal, sync) = if on_network_share {
        (SqliteJournalMode::Delete, SqliteSynchronous::Full)
    } else {
        (SqliteJournalMode::Wal, SqliteSynchronous::Normal)
    };

    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(create)
        .foreign_keys(true)
        .journal_mode(journal)
        .synchronous(sync)
        .busy_timeout(Duration::from_secs(10));

    SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await
        .map_err(|e| Error::Config(format!("Can't open {}: {e}", path.display())))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Reads an asset's interval from the `cal_interval_*` columns of `row`.
fn interval_from(row: &Row) -> Result<Option<CalInterval>> {
    let value: Option<i64> = row.get("cal_interval_value")?;
    let unit: Option<String> = row.get("cal_interval_unit")?;
    match (value, unit) {
        (Some(v), Some(u)) => Ok(Some(CalInterval {
            value: u32::try_from(v).map_err(|_| Error::Database(format!("bad interval {v}")))?,
            unit: u.parse()?,
            end_of_month: row.get("cal_interval_end_of_month")?,
        })),
        _ => Ok(None),
    }
}

/// The current time to the microsecond: the finest every backend stores, so a copy
/// between backends keeps timestamps exactly.
fn now() -> DateTime<Utc> {
    Utc::now().trunc_subsecs(6)
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// The escape character for LIKE patterns (paired with `ESCAPE '!'`). A backslash
/// would need different quoting on MySQL; `[` is a wildcard on SQL Server.
const LIKE_ESCAPE: char = '!';

/// Escapes LIKE wildcards in user search text.
fn like_pattern(search: &str) -> String {
    let mut out = String::with_capacity(search.len() + 2);
    out.push('%');
    for c in search.chars() {
        if matches!(c, '%' | '_' | '[' | LIKE_ESCAPE) {
            out.push(LIKE_ESCAPE);
        }
        out.push(c);
    }
    out.push('%');
    out
}

/// After an UPDATE matched no rows: is the row gone, or did its version move?
async fn missing_or_stale(
    conn: &mut Conn,
    table: &'static str,
    entity: &'static str,
    id: Id,
) -> Error {
    let sql = format!("SELECT 1 AS n FROM {table} WHERE id = ?1");
    match conn.fetch_optional(&sql, params![id]).await {
        Ok(Some(_)) => Error::stale(entity),
        Ok(None) => Error::NotFound { entity, id },
        Err(e) => e.into(),
    }
}

// ---------------------------------------------------------------------------
// Assets
// ---------------------------------------------------------------------------

macro_rules! asset_select {
    () => {
        "SELECT a.id, a.asset_tag, a.kind, a.parent_id, p.asset_tag AS parent_tag, a.description,
                a.manufacturer_id, m.name AS manufacturer_name, a.model_number, a.serial_number,
                a.location_id, l.name AS location_name,
                a.department_id, d.name AS department_name,
                a.owner_id, o.name AS owner_name, a.owner_since,
                a.status_id, s.name AS status_name, s.tracks_due AS status_tracks_due,
                s.builtin_role AS status_role, a.previous_status_id,
                a.warranty_end_date, a.cal_interval_value, a.cal_interval_unit, a.cal_interval_end_of_month,
                a.in_service_date, a.first_cal_date, a.last_cal_date, a.last_cal_date_entered, a.next_cal_date,
                a.cal_extended, a.cal_aggregate,
                (SELECT MIN(c.next_cal_date) FROM assets c
                   JOIN asset_statuses cs ON cs.id = c.status_id
                  WHERE c.parent_id = a.id AND c.is_archived = FALSE AND cs.tracks_due = TRUE) AS children_next_cal_date,
                (SELECT COUNT(*) FROM assets c WHERE c.parent_id = a.id AND c.is_archived = FALSE) AS child_count,
                a.notes, a.is_archived, a.row_version, a.created_at, a.updated_at
         FROM assets a
         LEFT JOIN assets p        ON p.id = a.parent_id
         LEFT JOIN manufacturers m ON m.id = a.manufacturer_id
         LEFT JOIN locations l     ON l.id = a.location_id
         LEFT JOIN departments d   ON d.id = a.department_id
         LEFT JOIN owners o        ON o.id = a.owner_id
         JOIN asset_statuses s     ON s.id = a.status_id"
    };
}

fn role_from(row: &Row) -> Result<Option<StatusRole>> {
    row.get::<Option<String>>("status_role")?
        .map(|r| r.parse())
        .transpose()
}

fn map_asset(row: &Row, today: NaiveDate) -> Result<Asset> {
    // An aggregate System is due when its soonest tracked Instrument is.
    let cal_aggregate: bool = row.get("cal_aggregate")?;
    let next_cal_date: Option<NaiveDate> = if cal_aggregate {
        row.get("children_next_cal_date")?
    } else {
        row.get("next_cal_date")?
    };
    let cal_extended: bool = !cal_aggregate && row.get::<bool>("cal_extended")?;
    let tracks_due: bool = row.get("status_tracks_due")?;
    Ok(Asset {
        id: row.get("id")?,
        asset_tag: row.get("asset_tag")?,
        kind: row.get::<String>("kind")?.parse()?,
        parent_id: row.get("parent_id")?,
        parent_tag: row.get("parent_tag")?,
        description: row.get("description")?,
        manufacturer_id: row.get("manufacturer_id")?,
        manufacturer_name: row.get("manufacturer_name")?,
        model_number: row.get("model_number")?,
        serial_number: row.get("serial_number")?,
        location_id: row.get("location_id")?,
        location_name: row.get("location_name")?,
        department_id: row.get("department_id")?,
        department_name: row.get("department_name")?,
        owner_id: row.get("owner_id")?,
        owner_name: row.get("owner_name")?,
        owner_since: row.get("owner_since")?,
        status_id: row.get("status_id")?,
        status_name: row.get("status_name")?,
        status_role: role_from(row)?,
        previous_status_id: row.get("previous_status_id")?,
        warranty_end_date: row.get("warranty_end_date")?,
        cal_interval: interval_from(row)?,
        in_service_date: row.get("in_service_date")?,
        first_cal_date: row.get("first_cal_date")?,
        last_cal_date: row.get("last_cal_date")?,
        last_cal_date_entered: row.get("last_cal_date_entered")?,
        next_cal_date,
        calibration_extended: cal_extended,
        cal_aggregate,
        due_status: due_status_with_extension(next_cal_date, tracks_due, cal_extended, today),
        child_count: row.get("child_count")?,
        notes: row.get("notes")?,
        is_archived: row.get("is_archived")?,
        row_version: row.get("row_version")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

/// An Instrument in a System is wherever the System is: it takes the System's location
/// and department, whatever was entered.
async fn inherit_from_system(conn: &mut Conn, input: &mut AssetInput) -> Result<()> {
    if let Some(parent_id) = input.parent_id
        && let Some(parent) = conn
            .fetch_optional(
                "SELECT location_id, department_id FROM assets WHERE id = ?1",
                params![parent_id],
            )
            .await?
    {
        input.location_id = parent.get("location_id")?;
        input.department_id = parent.get("department_id")?;
    }
    Ok(())
}

/// After a System's location or department changes, gives its Instruments the same,
/// each audited as an edit of the Instrument.
async fn cascade_to_instruments(conn: &mut Conn, ctx: &WriteContext, system: &Asset) -> Result<()> {
    let children: Vec<Id> = conn
        .fetch_all(
            "SELECT id FROM assets WHERE parent_id = ?1 ORDER BY id",
            params![system.id],
        )
        .await?
        .iter()
        .map(|r| r.get::<Id>("id"))
        .collect::<Result<_>>()?;
    for id in children {
        let before = fetch_asset(conn, id).await?;
        if before.location_id == system.location_id && before.department_id == system.department_id
        {
            continue;
        }
        conn.execute(
            "UPDATE assets SET location_id = ?2, department_id = ?3, updated_at = ?4,
                               row_version = row_version + 1
             WHERE id = ?1",
            params![id, system.location_id, system.department_id, now()],
        )
        .await?;
        let after = fetch_asset(conn, id).await?;
        write_audit(
            conn,
            NewAuditEntry::by(ctx, AuditAction::Update, AuditEntity::Asset)
                .subject(id, &after.asset_tag)
                .asset(id)
                .changes(diff(&asset_snapshot(&before), &asset_snapshot(&after))),
        )
        .await?;
    }
    Ok(())
}

/// Enforces the System/Instrument hierarchy rules that need the database.
async fn check_hierarchy(conn: &mut Conn, self_id: Option<Id>, input: &AssetInput) -> Result<()> {
    if let Some(parent_id) = input.parent_id {
        if Some(parent_id) == self_id {
            return Err(Error::validation(
                "parent_id",
                "An item cannot belong to itself.",
            ));
        }
        let parent = conn
            .fetch_optional(
                "SELECT kind, is_archived FROM assets WHERE id = ?1",
                params![parent_id],
            )
            .await?
            .ok_or_else(|| Error::validation("parent_id", "The selected System does not exist."))?;
        if parent.get::<String>("kind")?.parse::<AssetKind>()? != AssetKind::System {
            return Err(Error::validation(
                "parent_id",
                "Instruments can only belong to a System.",
            ));
        }
        if parent.get::<bool>("is_archived")? {
            return Err(Error::validation(
                "parent_id",
                "The selected System is archived.",
            ));
        }
    }
    if let (Some(id), AssetKind::Instrument) = (self_id, input.kind) {
        let children = conn
            .scalar(
                "SELECT COUNT(*) AS n FROM assets WHERE parent_id = ?1",
                params![id],
            )
            .await?;
        if children > 0 {
            return Err(Error::validation(
                "kind",
                format!(
                    "This System contains {children} item(s). Move them before changing it to an Instrument."
                ),
            ));
        }
    }
    Ok(())
}

/// Re-derives an asset's first/last/next calibration dates from its records.
async fn recompute_asset_dates(conn: &mut Conn, asset_id: Id) -> Result<()> {
    let row = conn
        .fetch_optional(
            "SELECT first_cal_date, last_cal_date_entered, cal_interval_value, cal_interval_unit,
                    cal_interval_end_of_month
             FROM assets WHERE id = ?1",
            params![asset_id],
        )
        .await?
        .ok_or(Error::NotFound {
            entity: "asset",
            id: asset_id,
        })?;

    let record_dates = conn
        .fetch_all(
            "SELECT r.cal_date FROM calibration_records r
             JOIN service_types t ON t.id = r.service_type_id
             WHERE r.asset_id = ?1 AND r.is_archived = FALSE AND t.counts_as_calibration = TRUE
               AND r.condition_returned <> 'fail'",
            params![asset_id],
        )
        .await?
        .iter()
        .map(|r| r.get::<NaiveDate>("cal_date"))
        .collect::<Result<Vec<_>>>()?;

    let derived = derive_dates(
        row.get("first_cal_date")?,
        row.get("last_cal_date_entered")?,
        &record_dates,
        interval_from(&row)?,
    );

    // The latest calibration extension, if one was granted since the last calibration.
    let extension_sql = format!(
        "SELECT r.cal_date, r.extension_days FROM calibration_records r
         JOIN service_types t ON t.id = r.service_type_id
         WHERE r.asset_id = ?1 AND r.is_archived = FALSE AND t.is_extension = TRUE
           AND r.extension_days IS NOT NULL
         ORDER BY r.cal_date DESC, r.id DESC
         {}",
        conn.dialect().limit("1")
    );
    let extension = match conn
        .fetch_optional(&extension_sql, params![asset_id])
        .await?
    {
        Some(r) => Some((
            r.get::<NaiveDate>("cal_date")?,
            u32::try_from(r.get::<i64>("extension_days")?).unwrap_or(0),
        )),
        None => None,
    };
    let (next_cal_date, extended) =
        apply_extension(derived.last_cal_date, derived.next_cal_date, extension);

    conn.execute(
        "UPDATE assets SET first_cal_date = ?2, last_cal_date = ?3, next_cal_date = ?4,
                           cal_extended = ?5
         WHERE id = ?1",
        params![
            asset_id,
            derived.first_cal_date,
            derived.last_cal_date,
            next_cal_date,
            extended
        ],
    )
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Calibration records
// ---------------------------------------------------------------------------

macro_rules! calibration_select {
    () => {
        "SELECT r.id, r.asset_id, a.asset_tag, r.cal_date, r.provider_id, r.manufacturer_id,
                r.service_type_id, t.name AS service_type_name, t.counts_as_calibration,
                t.is_extension, t.is_repair, r.extension_days,
                COALESCE(p.name, m.name) AS calibrated_by_name,
                r.technician, r.cost_cents, r.certificate_number,
                r.condition_received, r.condition_returned, r.next_due_date, r.notes,
                (SELECT COUNT(*) FROM attachments f
                  WHERE f.calibration_id = r.id AND f.is_archived = FALSE) AS attachment_count,
                r.is_archived, r.row_version, r.created_at, r.updated_at
         FROM calibration_records r
         JOIN assets a                ON a.id = r.asset_id
         JOIN service_types t         ON t.id = r.service_type_id
         LEFT JOIN calibration_providers p ON p.id = r.provider_id
         LEFT JOIN manufacturers m         ON m.id = r.manufacturer_id"
    };
}

fn map_calibration(row: &Row) -> Result<CalibrationRecord> {
    // At most one of the two is set (a CHECK constraint); extensions have neither.
    let provider: Option<Id> = row.get("provider_id")?;
    let manufacturer: Option<Id> = row.get("manufacturer_id")?;
    let (calibrated_by, calibrated_by_id) = match (provider, manufacturer) {
        (Some(id), _) => (Some(CalibratedBy::Provider), Some(id)),
        (None, Some(id)) => (Some(CalibratedBy::Manufacturer), Some(id)),
        (None, None) => (None, None),
    };
    Ok(CalibrationRecord {
        id: row.get("id")?,
        asset_id: row.get("asset_id")?,
        asset_tag: row.get("asset_tag")?,
        cal_date: row.get("cal_date")?,
        service_type_id: row.get("service_type_id")?,
        service_type_name: row.get("service_type_name")?,
        counts_as_calibration: row.get("counts_as_calibration")?,
        is_extension: row.get("is_extension")?,
        is_repair: row.get("is_repair")?,
        extension_days: row.get("extension_days")?,
        calibrated_by,
        calibrated_by_id,
        calibrated_by_name: row.get("calibrated_by_name")?,
        technician: row.get("technician")?,
        cost_cents: row.get("cost_cents")?,
        certificate_number: row.get("certificate_number")?,
        condition_received: row.get::<String>("condition_received")?.parse()?,
        condition_returned: row.get::<String>("condition_returned")?.parse()?,
        next_due_date: row.get("next_due_date")?,
        notes: row.get("notes")?,
        attachment_count: row.get("attachment_count")?,
        is_archived: row.get("is_archived")?,
        row_version: row.get("row_version")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

/// The due date a calibration on `cal_date` establishes for this asset.
/// Checks a service record against its item and service type, clears what doesn't
/// apply, and returns the due date it sets:
/// * a calibration-type service: the item's interval from the service date;
/// * a calibration extension: the service date plus the extension's days (and no
///   performer, certificate, or cost);
/// * anything else (a repair): none.
async fn prepare_service(
    conn: &mut Conn,
    input: &mut CalibrationInput,
) -> Result<Option<NaiveDate>> {
    let service_type = conn
        .fetch_optional(
            "SELECT counts_as_calibration, is_extension FROM service_types WHERE id = ?1",
            params![input.service_type_id],
        )
        .await?
        .ok_or_else(|| {
            Error::validation(
                "service_type_id",
                "The selected service type does not exist.",
            )
        })?;
    let counts: bool = service_type.get("counts_as_calibration")?;
    let is_extension: bool = service_type.get("is_extension")?;
    let row = conn
        .fetch_optional(
            "SELECT cal_interval_value, cal_interval_unit, cal_interval_end_of_month, is_archived
             FROM assets WHERE id = ?1",
            params![input.asset_id],
        )
        .await?
        .ok_or_else(|| Error::validation("asset_id", "The selected item does not exist."))?;
    if row.get::<bool>("is_archived")? {
        return Err(Error::validation(
            "asset_id",
            "The selected item is archived.",
        ));
    }

    if is_extension {
        let days = input.extension_days.filter(|d| *d >= 1).ok_or_else(|| {
            Error::validation(
                "extension_days",
                "Enter how many days the extension lasts (at least 1).",
            )
        })?;
        input.calibrated_by_id = None;
        input.certificate_number = None;
        input.cost_cents = None;
        return Ok(input
            .cal_date
            .checked_add_days(chrono::Days::new(days.unsigned_abs().into())));
    }

    input.extension_days = None;
    if input.calibrated_by_id.is_none() {
        return Err(Error::validation(
            "calibrated_by_id",
            "Choose who did the work.",
        ));
    }
    // A failed calibration doesn't renew anything: the item isn't fit for use.
    if !counts || input.condition_returned == ConditionReturned::Fail {
        return Ok(None);
    }
    let interval = interval_from(&row)?;
    Ok(interval.and_then(|i| add_interval(input.cal_date, i)))
}

// ---------------------------------------------------------------------------
// Lookups
// ---------------------------------------------------------------------------

/// Table name and the kind-specific editable flag columns. All are fixed strings chosen
/// by `kind`, never user input, so formatting them into SQL is safe.
fn lookup_table(kind: LookupKind) -> (&'static str, &'static [&'static str]) {
    match kind {
        LookupKind::Location => ("locations", &[]),
        LookupKind::Department => ("departments", &[]),
        LookupKind::Owner => ("owners", &[]),
        LookupKind::Manufacturer => ("manufacturers", &[]),
        LookupKind::Provider => ("calibration_providers", &["is_internal"]),
        LookupKind::Status => ("asset_statuses", &["tracks_due"]),
        LookupKind::ServiceType => ("service_types", &["counts_as_calibration", "is_repair"]),
    }
}

fn lookup_select(kind: LookupKind) -> String {
    let (table, _) = lookup_table(kind);
    let internal = if kind == LookupKind::Provider {
        "is_internal"
    } else {
        "NULL"
    };
    let tracks = if kind == LookupKind::Status {
        "tracks_due"
    } else {
        "NULL"
    };
    let (counts, extension, repair) = if kind == LookupKind::ServiceType {
        ("counts_as_calibration", "is_extension", "is_repair")
    } else {
        ("NULL", "NULL", "NULL")
    };
    let role = if kind == LookupKind::Status {
        "builtin_role"
    } else {
        "NULL"
    };
    let (user, user_name) = if kind == LookupKind::Owner {
        (
            "user_id",
            "(SELECT lu.display_name FROM users lu WHERE lu.id = owners.user_id)",
        )
    } else {
        ("NULL", "NULL")
    };
    format!(
        "SELECT id, name, description, is_active, sort_order,
                {internal} AS is_internal, {tracks} AS tracks_due,
                {counts} AS counts_as_calibration, {extension} AS is_extension,
                {repair} AS is_repair, {role} AS status_role,
                {user} AS user_id, {user_name} AS user_name
         FROM {table}"
    )
}

fn map_lookup(row: &Row, kind: LookupKind) -> Result<LookupItem> {
    Ok(LookupItem {
        id: row.get("id")?,
        kind,
        name: row.get("name")?,
        description: row.get("description")?,
        is_active: row.get("is_active")?,
        sort_order: row.get("sort_order")?,
        is_internal: row.get("is_internal")?,
        tracks_due: row.get("tracks_due")?,
        counts_as_calibration: row.get("counts_as_calibration")?,
        is_extension: row.get("is_extension")?,
        is_repair: row.get("is_repair")?,
        status_role: role_from(row)?,
        user_id: row.get("user_id")?,
        user_name: row.get("user_name")?,
    })
}

/// An Owner entry linked to a user account is managed with the account (Users page).
fn refuse_linked_owner(item: &LookupItem) -> Result<()> {
    match &item.user_name {
        Some(user) if item.user_id.is_some() => Err(Error::Conflict(format!(
            "'{}' is linked to {user}'s account, so it's managed on the Users page. Unlink it there to change it here.",
            item.name
        ))),
        _ => Ok(()),
    }
}

/// The values for a kind's flag columns (in [`lookup_table`] order), applying defaults.
fn lookup_extra(kind: LookupKind, input: &LookupInput) -> Vec<bool> {
    match kind {
        LookupKind::Provider => vec![input.is_internal.unwrap_or(false)],
        LookupKind::Status => vec![input.tracks_due.unwrap_or(true)],
        LookupKind::ServiceType => vec![
            input.counts_as_calibration.unwrap_or(true),
            input.is_repair.unwrap_or(false),
        ],
        _ => Vec::new(),
    }
}

/// Where a list entry can be used: (table, column, what one row is called).
type Usage = (&'static str, &'static str, &'static str);

fn lookup_usage(kind: LookupKind) -> &'static [Usage] {
    match kind {
        LookupKind::Location => &[("assets", "location_id", "item")],
        LookupKind::Department => &[("assets", "department_id", "item")],
        LookupKind::Owner => &[("assets", "owner_id", "item")],
        LookupKind::Manufacturer => &[
            ("assets", "manufacturer_id", "item"),
            ("calibration_records", "manufacturer_id", "service record"),
        ],
        LookupKind::Status => &[("assets", "status_id", "item")],
        LookupKind::Provider => &[("calibration_records", "provider_id", "service record")],
        LookupKind::ServiceType => &[("calibration_records", "service_type_id", "service record")],
    }
}

/// Changes an item's status as part of saving a service record (e.g. to Out for Repair
/// after a failed calibration), audited as an edit of the item, and remembers the old
/// status as `previous_status_id`. `before` is the item as it was before the save;
/// nothing happens if the status is already `status_id`.
async fn set_status_with_service(
    conn: &mut Conn,
    ctx: &WriteContext,
    before: &Asset,
    status_id: Option<Id>,
) -> Result<()> {
    let Some(status_id) = status_id.filter(|s| *s != before.status_id) else {
        return Ok(());
    };
    if conn
        .fetch_optional(
            "SELECT 1 AS n FROM asset_statuses WHERE id = ?1",
            params![status_id],
        )
        .await?
        .is_none()
    {
        return Err(Error::validation(
            "new_status_id",
            "The selected status does not exist.",
        ));
    }
    conn.execute(
        "UPDATE assets SET status_id = ?2, previous_status_id = ?3, updated_at = ?4,
                           row_version = row_version + 1
         WHERE id = ?1",
        params![before.id, status_id, before.status_id, now()],
    )
    .await?;
    let after = fetch_asset(conn, before.id).await?;
    write_audit(
        conn,
        NewAuditEntry::by(ctx, AuditAction::Update, AuditEntity::Asset)
            .subject(before.id, &after.asset_tag)
            .asset(before.id)
            .changes(diff(&asset_snapshot(before), &asset_snapshot(&after))),
    )
    .await
}

/// The SQL columns for who calibrated: (provider_id, manufacturer_id).
fn calibrated_by_columns(input: &CalibrationInput) -> (Option<Id>, Option<Id>) {
    match input.calibrated_by {
        CalibratedBy::Provider => (input.calibrated_by_id, None),
        CalibratedBy::Manufacturer => (None, input.calibrated_by_id),
    }
}

// ---------------------------------------------------------------------------
// Fetch helpers usable inside a transaction or on a plain connection
// ---------------------------------------------------------------------------

async fn fetch_asset(conn: &mut Conn, id: Id) -> Result<Asset> {
    const SQL: &str = concat!(asset_select!(), " WHERE a.id = ?1");
    let row = conn
        .fetch_optional(SQL, params![id])
        .await?
        .ok_or(Error::NotFound {
            entity: "asset",
            id,
        })?;
    map_asset(&row, today())
}

async fn fetch_calibration(conn: &mut Conn, id: Id) -> Result<CalibrationRecord> {
    const SQL: &str = concat!(calibration_select!(), " WHERE r.id = ?1");
    let row = conn
        .fetch_optional(SQL, params![id])
        .await?
        .ok_or(Error::NotFound {
            entity: "calibration record",
            id,
        })?;
    map_calibration(&row)
}

async fn fetch_lookup(conn: &mut Conn, kind: LookupKind, id: Id) -> Result<LookupItem> {
    let sql = format!("{} WHERE id = ?1", lookup_select(kind));
    let row = conn
        .fetch_optional(&sql, params![id])
        .await?
        .ok_or(Error::NotFound {
            entity: "list item",
            id,
        })?;
    map_lookup(&row, kind)
}

const USER_COLUMNS: &str =
    "id, username, display_name, email, role, is_active, must_change_password,
                            last_sign_in_at, password_changed_at, reset_requested_at, row_version,
                            created_at, updated_at,
                            (SELECT lo.id FROM owners lo WHERE lo.user_id = users.id) AS owner_id,
                            (SELECT lo.name FROM owners lo WHERE lo.user_id = users.id) AS owner_name";

fn map_user(row: &Row) -> Result<User> {
    Ok(User {
        id: row.get("id")?,
        username: row.get("username")?,
        display_name: row.get("display_name")?,
        email: row.get("email")?,
        role: row.get::<String>("role")?.parse()?,
        is_active: row.get("is_active")?,
        must_change_password: row.get("must_change_password")?,
        last_sign_in_at: row.get("last_sign_in_at")?,
        password_changed_at: row.get("password_changed_at")?,
        reset_requested_at: row.get("reset_requested_at")?,
        owner_id: row.get("owner_id")?,
        owner_name: row.get("owner_name")?,
        row_version: row.get("row_version")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

async fn fetch_user(conn: &mut Conn, id: Id) -> Result<User> {
    let sql = format!("SELECT {USER_COLUMNS} FROM users WHERE id = ?1");
    let row = conn
        .fetch_optional(&sql, params![id])
        .await?
        .ok_or(Error::NotFound { entity: "user", id })?;
    map_user(&row)
}

async fn active_admin_count(conn: &mut Conn) -> Result<i64> {
    conn.scalar(
        "SELECT COUNT(*) AS n FROM users WHERE role = 'admin' AND is_active = TRUE",
        params![],
    )
    .await
}

async fn insert_user(conn: &mut Conn, input: &NewUser) -> Result<Id> {
    let hash = hash_password(&input.password)?;
    conn.insert(
        "INSERT INTO users (username, display_name, email, role, password_hash,
                            must_change_password, password_changed_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?7)",
        params![
            &input.username,
            &input.display_name,
            &input.email,
            input.role.as_str(),
            hash,
            input.must_change_password,
            now()
        ],
    )
    .await
    .map_err(|e| {
        write_err(e, || {
            format!("The username '{}' is already taken.", input.username)
        })
    })
}

// ---------------------------------------------------------------------------
// Audit trail
// ---------------------------------------------------------------------------

pub(crate) async fn write_audit(conn: &mut Conn, entry: NewAuditEntry) -> Result<()> {
    let changes = serde_json::to_string(&entry.changes)
        .map_err(|e| Error::Database(format!("could not encode audit changes: {e}")))?;
    conn.execute(
        "INSERT INTO audit_log (occurred_at, user_id, username, action, entity, entity_id,
                                asset_id, entity_label, reason, changes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            now(),
            entry.user_id,
            &entry.username,
            entry.action.as_str(),
            entry.entity.as_str(),
            entry.entity_id,
            entry.asset_id,
            &entry.entity_label,
            &entry.reason,
            changes
        ],
    )
    .await?;
    Ok(())
}

fn map_audit(row: &Row) -> Result<AuditEntry> {
    let changes: String = row.get("changes")?;
    Ok(AuditEntry {
        id: row.get("id")?,
        occurred_at: row.get("occurred_at")?,
        user_id: row.get("user_id")?,
        username: row.get("username")?,
        action: row.get::<String>("action")?.parse()?,
        entity: row.get::<String>("entity")?.parse()?,
        entity_id: row.get("entity_id")?,
        asset_id: row.get("asset_id")?,
        entity_label: row.get("entity_label")?,
        reason: row.get("reason")?,
        changes: serde_json::from_str::<Vec<FieldChange>>(&changes)
            .map_err(|e| Error::Database(format!("bad audit changes: {e}")))?,
    })
}

fn lookup_label(item: &LookupItem) -> String {
    format!("{}: {}", item.kind, item.name)
}

// ---------------------------------------------------------------------------
// Certificates
// ---------------------------------------------------------------------------

const TEMPLATE_KEY: &str = "certificate_template";
const NEXT_NUMBER_KEY: &str = "certificate_next_number";
const NUMBER_YEAR_KEY: &str = "certificate_number_year";

async fn read_setting(conn: &mut Conn, key: &str) -> Result<Option<String>> {
    conn.fetch_optional(
        "SELECT setting_value FROM app_settings WHERE setting_key = ?1",
        params![key],
    )
    .await?
    .map(|r| r.get::<String>("setting_value"))
    .transpose()
}

async fn write_setting(conn: &mut Conn, key: &str, value: &str) -> Result<()> {
    let exists = conn
        .fetch_optional(
            "SELECT 1 AS n FROM app_settings WHERE setting_key = ?1",
            params![key],
        )
        .await?
        .is_some();
    let sql = if exists {
        "UPDATE app_settings SET setting_value = ?2, updated_at = ?3 WHERE setting_key = ?1"
    } else {
        "INSERT INTO app_settings (setting_key, setting_value, updated_at) VALUES (?1, ?2, ?3)"
    };
    conn.execute(sql, params![key, value, now()]).await?;
    Ok(())
}

/// The template with `next_number` from the counter (1 again in a new year when the
/// counter resets yearly). A missing or unreadable template is the default one.
async fn read_certificate_template(conn: &mut Conn) -> Result<CertificateTemplate> {
    let mut template = read_setting(conn, TEMPLATE_KEY)
        .await?
        .and_then(|json| serde_json::from_str::<CertificateTemplate>(&json).ok())
        .unwrap_or_default();
    let next = read_setting(conn, NEXT_NUMBER_KEY)
        .await?
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(1);
    let year = read_setting(conn, NUMBER_YEAR_KEY)
        .await?
        .and_then(|v| v.parse::<i32>().ok());
    let new_year = year.is_some_and(|y| y != today().year());
    template.next_number = if template.number_reset_yearly && new_year {
        1
    } else {
        next
    };
    Ok(template)
}

async fn write_certificate_counter(conn: &mut Conn, next: u32) -> Result<()> {
    write_setting(conn, NEXT_NUMBER_KEY, &next.to_string()).await?;
    write_setting(conn, NUMBER_YEAR_KEY, &today().year().to_string()).await
}

/// Audit lines for a template change: one per changed setting; the logo is summarized.
fn template_changes(before: &CertificateTemplate, after: &CertificateTemplate) -> Vec<FieldChange> {
    let as_map = |t: &CertificateTemplate| match serde_json::to_value(t) {
        Ok(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    let (old, new) = (as_map(before), as_map(after));
    let show = |key: &str, v: Option<&serde_json::Value>| -> Option<String> {
        match v? {
            serde_json::Value::Null => None,
            serde_json::Value::String(s) if key == "logo" => {
                Some(format!("Image ({} KB)", s.len() / 1024))
            }
            serde_json::Value::String(s) => Some(s.clone()),
            other => Some(other.to_string()),
        }
    };
    new.iter()
        .filter(|(key, value)| old.get(*key) != Some(*value))
        .map(|(key, value)| {
            let label = key.replace('_', " ");
            let mut label_chars = label.chars();
            let label = label_chars
                .next()
                .map(|c| c.to_uppercase().collect::<String>() + label_chars.as_str())
                .unwrap_or_default();
            FieldChange {
                field: format!("Certificate template: {label}"),
                old: show(key, old.get(key)),
                new: show(key, Some(value)),
            }
        })
        .collect()
}

macro_rules! certificate_select {
    () => {
        "SELECT c.id, c.certificate_number, c.kind, c.asset_id, c.subject, c.description,
                c.cal_date, c.issued_by, c.created_at
         FROM certificates c"
    };
}

fn map_certificate_summary(row: &Row) -> Result<CertificateSummary> {
    Ok(CertificateSummary {
        id: row.get("id")?,
        certificate_number: row.get("certificate_number")?,
        kind: row.get::<String>("kind")?.parse()?,
        asset_id: row.get("asset_id")?,
        subject: row.get("subject")?,
        description: row.get("description")?,
        cal_date: row.get("cal_date")?,
        issued_by: row.get("issued_by")?,
        created_at: row.get("created_at")?,
    })
}

async fn fetch_certificate(conn: &mut Conn, id: Id) -> Result<Certificate> {
    const SQL: &str =
        "SELECT c.id, c.certificate_number, c.kind, c.asset_id, c.subject, c.description,
                              c.cal_date, c.issued_by, c.created_at, c.details, c.template
                       FROM certificates c WHERE c.id = ?1";
    let row = conn
        .fetch_optional(SQL, params![id])
        .await?
        .ok_or(Error::NotFound {
            entity: "certificate",
            id,
        })?;
    let decode = |what: &str, e: serde_json::Error| {
        Error::Database(format!("certificate {id} has unreadable {what}: {e}"))
    };
    Ok(Certificate {
        summary: map_certificate_summary(&row)?,
        details: serde_json::from_str(&row.get::<String>("details")?)
            .map_err(|e| decode("details", e))?,
        template: serde_json::from_str(&row.get::<String>("template")?)
            .map_err(|e| decode("template", e))?,
    })
}

async fn fetch_signature(conn: &mut Conn, user_id: Id) -> Result<Option<Option<String>>> {
    conn.fetch_optional(
        "SELECT signature FROM users WHERE id = ?1",
        params![user_id],
    )
    .await?
    .map(|r| r.get::<Option<String>>("signature"))
    .transpose()
}

/// Fills in a signer's saved signature (and forgets an id that isn't a user).
async fn resolve_signer(
    conn: &mut Conn,
    id: &mut Option<Id>,
    signature: &mut Option<String>,
) -> Result<()> {
    *signature = None;
    if let Some(user_id) = *id {
        match fetch_signature(conn, user_id).await? {
            Some(saved) => *signature = saved,
            None => *id = None,
        }
    }
    Ok(())
}

fn certificate_snapshot(c: &Certificate) -> Snapshot {
    let d = &c.details;
    vec![
        (
            "Certificate number",
            Some(c.summary.certificate_number.clone()),
        ),
        (
            "Type",
            Some(match d.kind {
                CertificateKind::Internal => "Internal".to_string(),
                CertificateKind::External => "External".to_string(),
            }),
        ),
        (
            if d.kind == CertificateKind::Internal {
                "Asset number"
            } else {
                "Customer"
            },
            Some(c.summary.subject.clone()).filter(|s| !s.is_empty()),
        ),
        (
            "Description",
            Some(d.description.clone()).filter(|s| !s.is_empty()),
        ),
        (
            "Serial number",
            Some(d.serial_number.clone()).filter(|s| !s.is_empty()),
        ),
        ("Calibration date", Some(d.cal_date.to_string())),
        ("Next calibration", d.next_cal_date.map(|n| n.to_string())),
    ]
}

// ---------------------------------------------------------------------------
// Check-out
// ---------------------------------------------------------------------------

fn unique_ids(ids: &[Id]) -> Result<Vec<Id>> {
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Err(Error::validation(
            "asset_ids",
            "Choose at least one instrument.",
        ));
    }
    Ok(ids)
}

/// The owner an item is going to must exist and be active. Returns their name.
async fn check_new_owner(conn: &mut Conn, owner_id: Id) -> Result<String> {
    let row = conn
        .fetch_optional(
            "SELECT name, is_active FROM owners WHERE id = ?1",
            params![owner_id],
        )
        .await?
        .ok_or_else(|| Error::validation("owner_id", "Choose who it's going to."))?;
    if !row.get::<bool>("is_active")? {
        return Err(Error::validation(
            "owner_id",
            "That owner is inactive. Choose another, or make them active in Lists.",
        ));
    }
    row.get("name")
}

/// Whether `change` can be made to this item as it is now.
fn check_owner_change(asset: &Asset, change: OwnerChange) -> Result<()> {
    let tag = &asset.asset_tag;
    if asset.kind == AssetKind::System {
        return Err(Error::validation(
            "asset_ids",
            format!("{tag} is a System. Check out its Instruments instead."),
        ));
    }
    if asset.is_archived {
        return Err(Error::validation(
            "asset_ids",
            format!("{tag} is archived. Restore it first."),
        ));
    }
    let holder = asset.owner_name.clone().unwrap_or_default();
    let problem = match (change, asset.owner_id) {
        (OwnerChange::CheckOut { .. }, Some(_)) => Some(format!(
            "{tag} is already checked out to {holder}. Transfer it instead."
        )),
        (OwnerChange::CheckIn, None) => Some(format!("{tag} isn't checked out.")),
        (OwnerChange::Transfer { .. }, None) => {
            Some(format!("{tag} isn't checked out. Check it out instead."))
        }
        (OwnerChange::Transfer { owner_id }, Some(current)) if current == owner_id => {
            Some(format!("{tag} is already checked out to {holder}."))
        }
        _ => None,
    };
    match problem {
        Some(message) => Err(Error::Conflict(message)),
        None => Ok(()),
    }
}

/// Makes the change to every item (or fails for all), each audited on the item.
async fn apply_owner_change(
    conn: &mut Conn,
    ctx: &WriteContext,
    asset_ids: &[Id],
    change: OwnerChange,
) -> Result<Vec<Asset>> {
    let ids = unique_ids(asset_ids)?;
    let (new_owner, action) = match change {
        OwnerChange::CheckOut { owner_id } => (Some(owner_id), AuditAction::CheckOut),
        OwnerChange::CheckIn => (None, AuditAction::CheckIn),
        OwnerChange::Transfer { owner_id } => (Some(owner_id), AuditAction::Transfer),
    };
    if let Some(owner_id) = new_owner {
        check_new_owner(conn, owner_id).await?;
    }
    let stamp = now();
    let mut changed = Vec::with_capacity(ids.len());
    for id in ids {
        let before = fetch_asset(conn, id).await?;
        check_owner_change(&before, change)?;
        check_reservation_hold(conn, &before, change).await?;
        conn.execute(
            "UPDATE assets SET owner_id = ?2, owner_since = ?3, updated_at = ?4,
                               row_version = row_version + 1
             WHERE id = ?1",
            params![id, new_owner, new_owner.map(|_| stamp), stamp],
        )
        .await?;
        let after = fetch_asset(conn, id).await?;
        write_audit(
            conn,
            NewAuditEntry::by(ctx, action, AuditEntity::Asset)
                .subject(id, &after.asset_tag)
                .asset(id)
                .changes(diff(&asset_snapshot(&before), &asset_snapshot(&after))),
        )
        .await?;
        changed.push(after);
    }
    Ok(changed)
}

fn request_label(kind: OwnerRequestKind, owner: &str) -> String {
    match kind {
        OwnerRequestKind::CheckOut => format!("Check out to {owner}"),
        OwnerRequestKind::Transfer => format!("Transfer to {owner}"),
    }
}

macro_rules! owner_request_select {
    () => {
        "SELECT r.id, r.asset_id, a.asset_tag, a.description AS asset_description, r.kind,
                r.owner_id, o.name AS owner_name, co.name AS current_owner_name,
                r.requested_by_id, r.requested_by, r.note, r.status, r.decided_by, r.decided_at,
                r.decision_note, r.created_at, r.approver_user_id, au.display_name AS approver_name
         FROM owner_requests r
         JOIN assets a       ON a.id = r.asset_id
         JOIN owners o       ON o.id = r.owner_id
         LEFT JOIN owners co ON co.id = a.owner_id
         LEFT JOIN users au  ON au.id = r.approver_user_id"
    };
}

fn map_owner_request(row: &Row) -> Result<OwnerRequest> {
    Ok(OwnerRequest {
        id: row.get("id")?,
        asset_id: row.get("asset_id")?,
        asset_tag: row.get("asset_tag")?,
        asset_description: row.get("asset_description")?,
        kind: row.get::<String>("kind")?.parse()?,
        owner_id: row.get("owner_id")?,
        owner_name: row.get("owner_name")?,
        current_owner_name: row.get("current_owner_name")?,
        requested_by_id: row.get("requested_by_id")?,
        requested_by: row.get("requested_by")?,
        note: row.get("note")?,
        status: row.get::<String>("status")?.parse()?,
        decided_by: row.get("decided_by")?,
        decided_at: row.get("decided_at")?,
        decision_note: row.get("decision_note")?,
        created_at: row.get("created_at")?,
        approver_user_id: row.get("approver_user_id")?,
        approver_name: row.get("approver_name")?,
    })
}

async fn fetch_owner_request(conn: &mut Conn, id: Id) -> Result<OwnerRequest> {
    const SQL: &str = concat!(owner_request_select!(), " WHERE r.id = ?1");
    let row = conn
        .fetch_optional(SQL, params![id])
        .await?
        .ok_or(Error::NotFound {
            entity: "request",
            id,
        })?;
    map_owner_request(&row)
}

fn require_pending(request: &OwnerRequest) -> Result<()> {
    if request.status == OwnerRequestStatus::Pending {
        Ok(())
    } else {
        Err(Error::Conflict(format!(
            "This request was already {}.",
            request.status
        )))
    }
}

async fn close_owner_request(
    conn: &mut Conn,
    ctx: &WriteContext,
    id: Id,
    status: OwnerRequestStatus,
    note: &Option<String>,
) -> Result<()> {
    let stamp = now();
    conn.execute(
        "UPDATE owner_requests SET status = ?2, decided_by_id = ?3, decided_by = ?4,
                                   decided_at = ?5, decision_note = ?6, updated_at = ?5
         WHERE id = ?1 AND status = 'pending'",
        params![
            id,
            status.as_str(),
            ctx.actor.user_id,
            &ctx.actor.username,
            stamp,
            note
        ],
    )
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Reservations
// ---------------------------------------------------------------------------

macro_rules! reservation_select {
    () => {
        "SELECT r.id, r.asset_id, a.asset_tag, a.description AS asset_description,
                a.owner_id AS asset_owner_id, ao.name AS asset_owner_name,
                r.user_id, u.display_name AS user_name,
                (SELECT uo.id FROM owners uo WHERE uo.user_id = r.user_id) AS user_owner_id,
                r.start_date, r.end_date, r.note, r.status, r.asks_reservation_id,
                b.user_id AS asks_user_id, bu.display_name AS asks_user_name,
                r.decided_by, r.decided_at, r.decision_note, r.created_at
         FROM reservations r
         JOIN assets a          ON a.id = r.asset_id
         JOIN users u           ON u.id = r.user_id
         LEFT JOIN owners ao    ON ao.id = a.owner_id
         LEFT JOIN reservations b ON b.id = r.asks_reservation_id
         LEFT JOIN users bu     ON bu.id = b.user_id"
    };
}

fn map_reservation(row: &Row) -> Result<Reservation> {
    Ok(Reservation {
        id: row.get("id")?,
        asset_id: row.get("asset_id")?,
        asset_tag: row.get("asset_tag")?,
        asset_description: row.get("asset_description")?,
        asset_owner_id: row.get("asset_owner_id")?,
        asset_owner_name: row.get("asset_owner_name")?,
        user_id: row.get("user_id")?,
        user_name: row.get("user_name")?,
        user_owner_id: row.get("user_owner_id")?,
        start_date: row.get("start_date")?,
        end_date: row.get("end_date")?,
        note: row.get("note")?,
        status: row.get::<String>("status")?.parse()?,
        asks_reservation_id: row.get("asks_reservation_id")?,
        asks_user_id: row.get("asks_user_id")?,
        asks_user_name: row.get("asks_user_name")?,
        decided_by: row.get("decided_by")?,
        decided_at: row.get("decided_at")?,
        decision_note: row.get("decision_note")?,
        created_at: row.get("created_at")?,
    })
}

async fn fetch_reservation(conn: &mut Conn, id: Id) -> Result<Reservation> {
    const SQL: &str = concat!(reservation_select!(), " WHERE r.id = ?1");
    let row = conn
        .fetch_optional(SQL, params![id])
        .await?
        .ok_or(Error::NotFound {
            entity: "reservation",
            id,
        })?;
    map_reservation(&row)
}

/// Booked and waiting reservations of an item overlapping `start`..`end`.
async fn overlapping_reservations(
    conn: &mut Conn,
    asset_id: Id,
    start: NaiveDate,
    end: NaiveDate,
    exclude: Option<Id>,
) -> Result<Vec<Reservation>> {
    const SQL: &str = concat!(
        reservation_select!(),
        " WHERE r.asset_id = ?1 AND r.status IN ('active', 'pending')
            AND r.start_date <= ?3 AND r.end_date >= ?2
            AND (?4 IS NULL OR r.id <> ?4)
          ORDER BY r.start_date, r.id"
    );
    conn.fetch_all(SQL, params![asset_id, start, end, exclude])
        .await?
        .iter()
        .map(map_reservation)
        .collect()
}

fn reservation_span(start: NaiveDate, end: NaiveDate) -> String {
    if start == end {
        start.format("%Y-%m-%d").to_string()
    } else {
        format!("{} to {}", start.format("%Y-%m-%d"), end.format("%Y-%m-%d"))
    }
}

/// What's in the way of reserving `asset` for `start`..`end`.
async fn reservation_check(
    conn: &mut Conn,
    asset: &Asset,
    start: NaiveDate,
    end: NaiveDate,
    changing: Option<&Reservation>,
) -> Result<ReservationCheck> {
    let exclude = changing.map(|r| r.id);
    if end < start {
        return Err(Error::validation(
            "end_date",
            "The reservation must end on or after the day it starts.",
        ));
    }
    let conflicts = overlapping_reservations(conn, asset.id, start, end, exclude).await?;
    let can_ask = match conflicts.as_slice() {
        [only] if only.status == ReservationStatus::Active && only.start_date < start => {
            Some(only.clone())
        }
        _ => None,
    };
    // The first start, on or after the one asked for, when the same number of days is free.
    let days = (end - start).num_days();
    let mut next_free_start = None;
    let mut candidate = start;
    for _ in 0..100 {
        let clash = overlapping_reservations(
            conn,
            asset.id,
            candidate,
            candidate + chrono::Duration::days(days),
            exclude,
        )
        .await?;
        match clash.iter().map(|r| r.end_date).max() {
            None => {
                next_free_start = Some(candidate);
                break;
            }
            Some(last) => candidate = last + chrono::Duration::days(1),
        }
    }
    // Checked out to the person whose reservation is being changed: that's the handover,
    // not something in the way.
    let with_holder =
        changing.is_some_and(|r| asset.owner_id.is_some() && asset.owner_id == r.user_owner_id);
    Ok(ReservationCheck {
        checked_out_to: if with_holder {
            None
        } else {
            asset.owner_id.and(asset.owner_name.clone())
        },
        conflicts,
        can_ask,
        next_free_start,
    })
}

/// An item with a booked reservation (current or upcoming) can only go to the person it's
/// reserved for (their linked Owner entry), until that reservation ends or is cancelled.
async fn check_reservation_hold(conn: &mut Conn, asset: &Asset, change: OwnerChange) -> Result<()> {
    let to = match change {
        OwnerChange::CheckOut { owner_id } | OwnerChange::Transfer { owner_id } => owner_id,
        OwnerChange::CheckIn => return Ok(()),
    };
    const SQL: &str = concat!(
        reservation_select!(),
        " WHERE r.asset_id = ?1 AND r.status = 'active' AND r.end_date >= ?2
          ORDER BY r.start_date, r.id"
    );
    let next = conn
        .fetch_all(SQL, params![asset.id, today()])
        .await?
        .first()
        .map(map_reservation)
        .transpose()?;
    match next {
        Some(r) if r.user_owner_id != Some(to) => Err(Error::Conflict(format!(
            "{} is reserved for {} ({}). Until that reservation ends or is cancelled, it can only go to them{}.",
            asset.asset_tag,
            r.user_name,
            reservation_span(r.start_date, r.end_date),
            if r.user_owner_id.is_none() {
                ", once their account is linked to an Owner entry"
            } else {
                ""
            }
        ))),
        _ => Ok(()),
    }
}

fn check_reservable(asset: &Asset) -> Result<()> {
    let tag = &asset.asset_tag;
    if asset.kind == AssetKind::System {
        return Err(Error::validation(
            "asset_id",
            format!("{tag} is a System. Reserve its Instruments instead."),
        ));
    }
    if asset.is_archived {
        return Err(Error::validation("asset_id", format!("{tag} is archived.")));
    }
    if asset.status_role == Some(StatusRole::Retired) {
        return Err(Error::validation("asset_id", format!("{tag} is retired.")));
    }
    Ok(())
}

async fn reservation_audit(
    conn: &mut Conn,
    ctx: &WriteContext,
    action: AuditAction,
    r: &Reservation,
    old: Option<String>,
    new: Option<String>,
) -> Result<()> {
    write_audit(
        conn,
        NewAuditEntry::by(ctx, action, AuditEntity::Asset)
            .subject(r.asset_id, &r.asset_tag)
            .asset(r.asset_id)
            .changes(vec![FieldChange {
                field: "Reservation".into(),
                old,
                new,
            }]),
    )
    .await
}

fn reservation_label(r: &Reservation) -> String {
    format!(
        "{} for {}",
        reservation_span(r.start_date, r.end_date),
        r.user_name
    )
}

// ---------------------------------------------------------------------------
// Attachments
// ---------------------------------------------------------------------------

macro_rules! attachment_select {
    () => {
        "SELECT f.id, f.asset_id, a.asset_tag, f.calibration_id, r.cal_date AS calibration_date,
                f.category, f.file_name, f.storage_folder, f.relative_path, f.size_bytes, f.sha256,
                f.content_type,
                f.description, f.uploaded_by, f.is_archived, f.row_version, f.created_at
         FROM attachments f
         JOIN assets a                    ON a.id = f.asset_id
         LEFT JOIN calibration_records r  ON r.id = f.calibration_id"
    };
}

fn map_attachment(row: &Row) -> Result<Attachment> {
    Ok(Attachment {
        id: row.get("id")?,
        asset_id: row.get("asset_id")?,
        asset_tag: row.get("asset_tag")?,
        calibration_id: row.get("calibration_id")?,
        calibration_date: row.get("calibration_date")?,
        category: row.get::<String>("category")?.parse()?,
        file_name: row.get("file_name")?,
        storage_folder: row.get::<String>("storage_folder")?.parse()?,
        relative_path: row.get("relative_path")?,
        size_bytes: row.get("size_bytes")?,
        sha256: row.get("sha256")?,
        content_type: row.get("content_type")?,
        description: row.get("description")?,
        uploaded_by: row.get("uploaded_by")?,
        is_archived: row.get("is_archived")?,
        row_version: row.get("row_version")?,
        created_at: row.get("created_at")?,
    })
}

async fn fetch_attachment(conn: &mut Conn, id: Id) -> Result<Attachment> {
    const SQL: &str = concat!(attachment_select!(), " WHERE f.id = ?1");
    let row = conn
        .fetch_optional(SQL, params![id])
        .await?
        .ok_or(Error::NotFound {
            entity: "attachment",
            id,
        })?;
    map_attachment(&row)
}

fn archive_action(archived: bool) -> AuditAction {
    if archived {
        AuditAction::Archive
    } else {
        AuditAction::Restore
    }
}

// ---------------------------------------------------------------------------
// Store implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl Store for SqlStore {
    async fn info(&self) -> Result<StoreInfo> {
        let mut conn = self.conn().await?;
        let schema_version = conn.scalar(&self.schema_version_sql(), params![]).await?;
        let asset_count = conn
            .scalar(
                "SELECT COUNT(*) AS n FROM assets WHERE is_archived = FALSE",
                params![],
            )
            .await?;
        let calibration_count = conn
            .scalar(
                "SELECT COUNT(*) AS n FROM calibration_records WHERE is_archived = FALSE",
                params![],
            )
            .await?;
        Ok(StoreInfo {
            backend: self.backend.into(),
            location: self.location.clone(),
            schema_version,
            asset_count,
            calibration_count,
        })
    }

    // --- Users and sessions ------------------------------------------------

    async fn user_count(&self) -> Result<i64> {
        self.conn()
            .await?
            .scalar("SELECT COUNT(*) AS n FROM users", params![])
            .await
    }

    async fn create_first_admin(&self, mut input: NewUser) -> Result<User> {
        input.role = Role::Admin;
        input.must_change_password = false;
        let policy = self.settings().await?.password_policy;
        input.normalize(&policy)?;
        let mut tx = self.begin().await?;
        let existing = tx
            .scalar("SELECT COUNT(*) AS n FROM users", params![])
            .await?;
        if existing > 0 {
            return Err(Error::Conflict(
                "PolarisCal has already been set up. Sign in instead.".into(),
            ));
        }
        let id = insert_user(&mut tx, &input).await?;
        let user = fetch_user(&mut tx, id).await?;
        let ctx = WriteContext::new(
            Actor {
                user_id: id,
                username: user.username.clone(),
            },
            None,
        );
        write_audit(
            &mut tx,
            NewAuditEntry::by(&ctx, AuditAction::Create, AuditEntity::User)
                .subject(id, &user.username)
                .changes(created(&user_snapshot(&user))),
        )
        .await?;
        tx.commit().await?;
        Ok(user)
    }

    async fn credentials(&self, username: &str) -> Result<Option<UserCredentials>> {
        let sql = format!(
            "SELECT {USER_COLUMNS}, password_hash FROM users WHERE LOWER(username) = LOWER(?1)"
        );
        let row = self
            .conn()
            .await?
            .fetch_optional(&sql, params![username.trim()])
            .await?;
        row.map(|r| {
            Ok(UserCredentials {
                user: map_user(&r)?,
                password_hash: r.get("password_hash")?,
            })
        })
        .transpose()
    }

    async fn record_sign_in(&self, attempted_username: &str, user: Option<&User>) -> Result<()> {
        let mut tx = self.begin().await?;
        let entry = match user {
            Some(u) => {
                tx.execute(
                    "UPDATE users SET last_sign_in_at = ?2 WHERE id = ?1",
                    params![u.id, now()],
                )
                .await?;
                NewAuditEntry {
                    user_id: Some(u.id),
                    username: u.username.clone(),
                    action: AuditAction::SignIn,
                    entity: AuditEntity::Session,
                    entity_id: Some(u.id),
                    asset_id: None,
                    entity_label: Some(u.username.clone()),
                    reason: None,
                    changes: vec![],
                }
            }
            None => NewAuditEntry {
                user_id: None,
                username: attempted_username.trim().chars().take(64).collect(),
                action: AuditAction::SignInFailed,
                entity: AuditEntity::Session,
                entity_id: None,
                asset_id: None,
                entity_label: None,
                reason: None,
                changes: vec![],
            },
        };
        write_audit(&mut tx, entry).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn expire_password(&self, user: &User) -> Result<User> {
        let mut tx = self.begin().await?;
        let before = fetch_user(&mut tx, user.id).await?;
        if before.must_change_password {
            return Ok(before);
        }
        tx.execute(
            "UPDATE users SET must_change_password = ?2, updated_at = ?3,
                              row_version = row_version + 1
             WHERE id = ?1",
            params![user.id, true, now()],
        )
        .await?;
        let after = fetch_user(&mut tx, user.id).await?;
        let ctx = WriteContext::new(
            Actor {
                user_id: user.id,
                username: user.username.clone(),
            },
            None,
        );
        write_audit(
            &mut tx,
            NewAuditEntry::by(&ctx, AuditAction::PasswordExpired, AuditEntity::User)
                .subject(user.id, &after.username)
                .changes(diff(&user_snapshot(&before), &user_snapshot(&after))),
        )
        .await?;
        tx.commit().await?;
        Ok(after)
    }

    async fn request_password_reset(&self, username: &str) -> Result<()> {
        let mut tx = self.begin().await?;
        let sql = format!("SELECT {USER_COLUMNS} FROM users WHERE LOWER(username) = LOWER(?1)");
        let Some(row) = tx.fetch_optional(&sql, params![username.trim()]).await? else {
            return Ok(());
        };
        let before = map_user(&row)?;
        if !before.is_active || before.reset_requested_at.is_some() {
            return Ok(());
        }
        tx.execute(
            "UPDATE users SET reset_requested_at = ?2 WHERE id = ?1",
            params![before.id, now()],
        )
        .await?;
        let after = fetch_user(&mut tx, before.id).await?;
        // Nobody is signed in, so the entry is in the name of the account it's about (as
        // a failed sign-in is in the name of the username tried).
        write_audit(
            &mut tx,
            NewAuditEntry {
                user_id: None,
                username: after.username.clone(),
                action: AuditAction::PasswordResetRequest,
                entity: AuditEntity::User,
                entity_id: Some(after.id),
                asset_id: None,
                entity_label: Some(after.username.clone()),
                reason: None,
                changes: diff(&user_snapshot(&before), &user_snapshot(&after)),
            },
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn record_sign_out(&self, actor: &Actor) -> Result<()> {
        let mut conn = self.conn().await?;
        let ctx = WriteContext::new(actor.clone(), None);
        write_audit(
            &mut conn,
            NewAuditEntry::by(&ctx, AuditAction::SignOut, AuditEntity::Session)
                .subject(actor.user_id, &actor.username),
        )
        .await
    }

    async fn list_users(&self) -> Result<Vec<User>> {
        let sql = format!(
            "SELECT {USER_COLUMNS} FROM users ORDER BY is_active DESC, LOWER(display_name), id"
        );
        self.conn()
            .await?
            .fetch_all(&sql, params![])
            .await?
            .iter()
            .map(map_user)
            .collect()
    }

    async fn get_user(&self, id: Id) -> Result<User> {
        fetch_user(&mut self.conn().await?, id).await
    }

    async fn create_user(&self, ctx: &WriteContext, mut input: NewUser) -> Result<User> {
        let policy = self.settings().await?.password_policy;
        input.normalize(&policy)?;
        let mut tx = self.begin().await?;
        let id = insert_user(&mut tx, &input).await?;
        let user = fetch_user(&mut tx, id).await?;
        write_audit(
            &mut tx,
            NewAuditEntry::by(ctx, AuditAction::Create, AuditEntity::User)
                .subject(id, &user.username)
                .changes(created(&user_snapshot(&user))),
        )
        .await?;
        tx.commit().await?;
        Ok(user)
    }

    async fn update_user(
        &self,
        ctx: &WriteContext,
        id: Id,
        expected_version: i64,
        mut input: UserUpdate,
    ) -> Result<User> {
        input.normalize()?;
        let mut tx = self.begin().await?;
        let before = fetch_user(&mut tx, id).await?;

        if id == ctx.actor.user_id && !input.is_active {
            return Err(Error::validation(
                "is_active",
                "You can't deactivate your own account.",
            ));
        }
        let loses_admin = before.role == Role::Admin
            && before.is_active
            && (input.role != Role::Admin || !input.is_active);
        if loses_admin && active_admin_count(&mut tx).await? <= 1 {
            return Err(Error::validation(
                "role",
                "This is the only active administrator. Make someone else an administrator first.",
            ));
        }

        let changed = tx
            .execute(
                "UPDATE users SET display_name = ?3, email = ?4, role = ?5, is_active = ?6,
                                  updated_at = ?7, row_version = row_version + 1
                 WHERE id = ?1 AND row_version = ?2",
                params![
                    id,
                    expected_version,
                    &input.display_name,
                    &input.email,
                    input.role.as_str(),
                    input.is_active,
                    now()
                ],
            )
            .await?;
        if changed == 0 {
            return Err(missing_or_stale(&mut tx, "users", "user", id).await);
        }
        let after = fetch_user(&mut tx, id).await?;
        write_audit(
            &mut tx,
            NewAuditEntry::by(ctx, AuditAction::Update, AuditEntity::User)
                .subject(id, &after.username)
                .changes(diff(&user_snapshot(&before), &user_snapshot(&after))),
        )
        .await?;
        if before.is_active != after.is_active {
            // Their Owner entry is active exactly when they are.
            tx.execute(
                "UPDATE owners SET is_active = ?2, updated_at = ?3 WHERE user_id = ?1",
                params![id, after.is_active, now()],
            )
            .await?;
        }
        tx.commit().await?;
        Ok(after)
    }

    async fn set_password(
        &self,
        ctx: &WriteContext,
        user_id: Id,
        new_password: &str,
        must_change: bool,
    ) -> Result<User> {
        let policy = self.settings().await?.password_policy;
        let mut tx = self.begin().await?;
        let before = fetch_user(&mut tx, user_id).await?;
        policy.check(&before.username, new_password)?;
        let own = ctx.actor.user_id == user_id;
        let must_change = !own && must_change;
        tx.execute(
            "UPDATE users SET password_hash = ?2, must_change_password = ?3, updated_at = ?4,
                              password_changed_at = ?4, reset_requested_at = NULL,
                              row_version = row_version + 1
             WHERE id = ?1",
            params![user_id, hash_password(new_password)?, must_change, now()],
        )
        .await?;
        let after = fetch_user(&mut tx, user_id).await?;
        let action = if own {
            AuditAction::PasswordChange
        } else {
            AuditAction::PasswordReset
        };
        write_audit(
            &mut tx,
            NewAuditEntry::by(ctx, action, AuditEntity::User)
                .subject(user_id, &after.username)
                .changes(diff(&user_snapshot(&before), &user_snapshot(&after))),
        )
        .await?;
        tx.commit().await?;
        Ok(after)
    }

    // --- Settings ------------------------------------------------------------

    async fn settings(&self) -> Result<AppSettings> {
        let rows = self
            .conn()
            .await?
            .fetch_all(
                "SELECT setting_key, setting_value FROM app_settings",
                params![],
            )
            .await?
            .iter()
            .map(|r| {
                Ok((
                    r.get::<String>("setting_key")?,
                    r.get::<String>("setting_value")?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let value = |key: &str| rows.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        Ok(AppSettings {
            idle_lock_minutes: value("idle_lock_minutes")
                .and_then(|v| v.parse().ok())
                .unwrap_or(AppSettings::DEFAULT_IDLE_LOCK_MINUTES),
            attachments_root: value("attachments_root").filter(|v| !v.is_empty()),
            photos_root: value("photos_root").filter(|v| !v.is_empty()),
            service_manuals_root: value("service_manuals_root").filter(|v| !v.is_empty()),
            documents_root: value("documents_root").filter(|v| !v.is_empty()),
            update_checks: value("update_checks").is_none_or(|v| v != "false"),
            currency_symbol: value("currency_symbol")
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| AppSettings::DEFAULT_CURRENCY.into()),
            date_format: value("date_format")
                .filter(|v| AppSettings::DATE_FORMATS.contains(&v.as_str()))
                .unwrap_or_else(|| AppSettings::DEFAULT_DATE_FORMAT.into()),
            password_policy: {
                let default = PasswordPolicy::default();
                let flag = |key: &str| value(key).is_some_and(|v| v == "true");
                PasswordPolicy {
                    min_length: value("password_min_length")
                        .and_then(|v| v.parse().ok())
                        .filter(|n| PasswordPolicy::MIN_LENGTH_RANGE.contains(n))
                        .unwrap_or(default.min_length),
                    require_uppercase: flag("password_require_uppercase"),
                    require_number: flag("password_require_number"),
                    require_symbol: flag("password_require_symbol"),
                    expiry_days: value("password_expiry_days")
                        .and_then(|v| v.parse().ok())
                        .filter(|n| PasswordPolicy::EXPIRY_DAYS_RANGE.contains(n)),
                }
            },
        })
    }

    async fn update_settings(
        &self,
        ctx: &WriteContext,
        mut settings: AppSettings,
    ) -> Result<AppSettings> {
        settings.normalize()?;
        let before = self.settings().await?;
        let mut tx = self.begin().await?;
        let now = now();
        let pairs = [
            ("idle_lock_minutes", settings.idle_lock_minutes.to_string()),
            (
                "attachments_root",
                settings.attachments_root.clone().unwrap_or_default(),
            ),
            (
                "photos_root",
                settings.photos_root.clone().unwrap_or_default(),
            ),
            (
                "service_manuals_root",
                settings.service_manuals_root.clone().unwrap_or_default(),
            ),
            (
                "documents_root",
                settings.documents_root.clone().unwrap_or_default(),
            ),
            ("update_checks", settings.update_checks.to_string()),
            ("currency_symbol", settings.currency_symbol.clone()),
            ("date_format", settings.date_format.clone()),
            (
                "password_min_length",
                settings.password_policy.min_length.to_string(),
            ),
            (
                "password_require_uppercase",
                settings.password_policy.require_uppercase.to_string(),
            ),
            (
                "password_require_number",
                settings.password_policy.require_number.to_string(),
            ),
            (
                "password_require_symbol",
                settings.password_policy.require_symbol.to_string(),
            ),
            (
                "password_expiry_days",
                settings
                    .password_policy
                    .expiry_days
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
            ),
        ];
        for (key, value) in pairs {
            let exists = tx
                .fetch_optional(
                    "SELECT 1 AS n FROM app_settings WHERE setting_key = ?1",
                    params![key],
                )
                .await?
                .is_some();
            let sql = if exists {
                "UPDATE app_settings SET setting_value = ?2, updated_at = ?3 WHERE setting_key = ?1"
            } else {
                "INSERT INTO app_settings (setting_key, setting_value, updated_at) VALUES (?1, ?2, ?3)"
            };
            tx.execute(sql, params![key, value, now]).await?;
        }
        let mut changes = Vec::new();
        if before.idle_lock_minutes != settings.idle_lock_minutes {
            changes.push(FieldChange {
                field: "Auto-lock after (minutes)".into(),
                old: Some(before.idle_lock_minutes.to_string()),
                new: Some(settings.idle_lock_minutes.to_string()),
            });
        }
        for (field, old, new) in [
            (
                "Calibration certificates folder",
                &before.attachments_root,
                &settings.attachments_root,
            ),
            ("Photos folder", &before.photos_root, &settings.photos_root),
            (
                "Service manuals folder",
                &before.service_manuals_root,
                &settings.service_manuals_root,
            ),
            (
                "Other documents folder",
                &before.documents_root,
                &settings.documents_root,
            ),
        ] {
            if old != new {
                changes.push(FieldChange {
                    field: field.into(),
                    old: old.clone(),
                    new: new.clone(),
                });
            }
        }
        if before.currency_symbol != settings.currency_symbol {
            changes.push(FieldChange {
                field: "Currency symbol".into(),
                old: Some(before.currency_symbol.clone()),
                new: Some(settings.currency_symbol.clone()),
            });
        }
        if before.date_format != settings.date_format {
            changes.push(FieldChange {
                field: "Date format".into(),
                old: Some(before.date_format.clone()),
                new: Some(settings.date_format.clone()),
            });
        }
        let yes_no = |b: bool| Some(if b { "Yes" } else { "No" }.to_string());
        let (old, new) = (&before.password_policy, &settings.password_policy);
        let days =
            |d: Option<u32>| Some(d.map_or_else(|| "Never".to_string(), |d| format!("{d} days")));
        for (field, old, new) in [
            (
                "Minimum password length",
                Some(old.min_length.to_string()),
                Some(new.min_length.to_string()),
            ),
            (
                "Passwords need a capital letter",
                yes_no(old.require_uppercase),
                yes_no(new.require_uppercase),
            ),
            (
                "Passwords need a number",
                yes_no(old.require_number),
                yes_no(new.require_number),
            ),
            (
                "Passwords need a special character",
                yes_no(old.require_symbol),
                yes_no(new.require_symbol),
            ),
            (
                "Passwords expire after",
                days(old.expiry_days),
                days(new.expiry_days),
            ),
        ] {
            if old != new {
                changes.push(FieldChange {
                    field: field.into(),
                    old,
                    new,
                });
            }
        }
        if before.update_checks != settings.update_checks {
            changes.push(FieldChange {
                field: "Check for updates".into(),
                old: yes_no(before.update_checks),
                new: yes_no(settings.update_checks),
            });
        }
        if !changes.is_empty() {
            write_audit(
                &mut tx,
                NewAuditEntry::by(ctx, AuditAction::Update, AuditEntity::Settings).changes(changes),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(settings)
    }

    // --- Audit trail -----------------------------------------------------------

    async fn list_audit(&self, filter: &AuditFilter) -> Result<Vec<AuditEntry>> {
        let sql = format!(
            "SELECT id, occurred_at, user_id, username, action, entity, entity_id, asset_id,
                    entity_label, reason, changes
             FROM audit_log
             WHERE (?1 IS NULL OR asset_id = ?1)
               AND (?2 IS NULL OR entity = ?2)
               AND (?3 IS NULL OR user_id = ?3)
               AND (?4 IS NULL OR id < ?4)
             ORDER BY id DESC
             {}",
            self.dialect().limit("?5")
        );
        self.conn()
            .await?
            .fetch_all(
                &sql,
                params![
                    filter.asset_id,
                    filter.entity.map(AuditEntity::as_str),
                    filter.user_id,
                    filter.before_id,
                    filter.effective_limit()
                ],
            )
            .await?
            .iter()
            .map(map_audit)
            .collect()
    }

    // --- Pick lists ------------------------------------------------------------

    async fn list_lookups(
        &self,
        kind: LookupKind,
        include_inactive: bool,
    ) -> Result<Vec<LookupItem>> {
        let sql = format!(
            "{} WHERE (?1 = TRUE OR is_active = TRUE) ORDER BY LOWER(name), id",
            lookup_select(kind)
        );
        self.conn()
            .await?
            .fetch_all(&sql, params![include_inactive])
            .await?
            .iter()
            .map(|r| map_lookup(r, kind))
            .collect()
    }

    async fn create_lookup(
        &self,
        ctx: &WriteContext,
        kind: LookupKind,
        mut input: LookupInput,
    ) -> Result<LookupItem> {
        input.normalize()?;
        let (table, extra_cols) = lookup_table(kind);
        let extra = lookup_extra(kind, &input);
        let cols: String = extra_cols.iter().map(|c| format!(", {c}")).collect();
        let marks: String = (0..extra_cols.len())
            .map(|i| format!(", ?{}", i + 6))
            .collect();
        let sql = format!(
            "INSERT INTO {table} (name, description, is_active, sort_order, created_at, updated_at{cols})
             VALUES (?1, ?2, ?3, ?4, ?5, ?5{marks})"
        );
        let mut values = params![
            &input.name,
            &input.description,
            input.is_active,
            input.sort_order,
            now()
        ];
        values.extend(extra.into_iter().map(Value::from));
        let mut tx = self.begin().await?;
        let id = tx
            .insert(&sql, values)
            .await
            .map_err(|e| write_err(e, || format!("'{}' is already in this list.", input.name)))?;
        let item = fetch_lookup(&mut tx, kind, id).await?;
        write_audit(
            &mut tx,
            NewAuditEntry::by(ctx, AuditAction::Create, AuditEntity::Lookup)
                .subject(id, lookup_label(&item))
                .changes(created(&lookup_snapshot(&item))),
        )
        .await?;
        tx.commit().await?;
        Ok(item)
    }

    async fn update_lookup(
        &self,
        ctx: &WriteContext,
        kind: LookupKind,
        id: Id,
        mut input: LookupInput,
    ) -> Result<LookupItem> {
        input.normalize()?;
        let (table, extra_cols) = lookup_table(kind);
        let extra_set: String = extra_cols
            .iter()
            .enumerate()
            .map(|(i, c)| format!(", {c} = ?{}", i + 7))
            .collect();
        let sql = format!(
            "UPDATE {table}
             SET name = ?2, description = ?3, is_active = ?4, sort_order = ?5, updated_at = ?6{extra_set}
             WHERE id = ?1"
        );
        let mut tx = self.begin().await?;
        let before = fetch_lookup(&mut tx, kind, id).await?;
        refuse_linked_owner(&before)?;
        // The built-in Calibration Extension type never counts as a calibration.
        if before.is_extension == Some(true) {
            input.counts_as_calibration = Some(false);
            input.is_repair = Some(false);
        }
        // Items away for service still show when overdue; retired ones never do.
        if let Some(role) = before.status_role {
            input.tracks_due = Some(role.is_out_for_service());
        }
        let extra = lookup_extra(kind, &input);
        let mut values = params![
            id,
            &input.name,
            &input.description,
            input.is_active,
            input.sort_order,
            now()
        ];
        values.extend(extra.into_iter().map(Value::from));
        tx.execute(&sql, values)
            .await
            .map_err(|e| write_err(e, || format!("'{}' is already in this list.", input.name)))?;
        let after = fetch_lookup(&mut tx, kind, id).await?;
        if after.counts_as_calibration != before.counts_as_calibration {
            let assets = tx
                .fetch_all(
                    "SELECT DISTINCT asset_id FROM calibration_records WHERE service_type_id = ?1",
                    params![id],
                )
                .await?
                .iter()
                .map(|r| r.get::<Id>("asset_id"))
                .collect::<Result<Vec<_>>>()?;
            for asset_id in assets {
                recompute_asset_dates(&mut tx, asset_id).await?;
            }
        }
        write_audit(
            &mut tx,
            NewAuditEntry::by(ctx, AuditAction::Update, AuditEntity::Lookup)
                .subject(id, lookup_label(&after))
                .changes(diff(&lookup_snapshot(&before), &lookup_snapshot(&after))),
        )
        .await?;
        tx.commit().await?;
        Ok(after)
    }

    async fn delete_lookup(&self, ctx: &WriteContext, kind: LookupKind, id: Id) -> Result<()> {
        let (table, _) = lookup_table(kind);
        let mut tx = self.begin().await?;
        let item = fetch_lookup(&mut tx, kind, id).await?;
        refuse_linked_owner(&item)?;
        if item.is_extension == Some(true) {
            return Err(Error::Conflict(format!(
                "'{}' is built in and can't be deleted. Make it inactive if you don't use extensions.",
                item.name
            )));
        }
        if item.status_role.is_some() {
            return Err(Error::Conflict(format!(
                "'{}' is built in and can't be deleted. You can rename it.",
                item.name
            )));
        }
        let mut uses = Vec::new();
        for (used_table, used_column, noun) in lookup_usage(kind) {
            let n = tx
                .scalar(
                    &format!("SELECT COUNT(*) AS n FROM {used_table} WHERE {used_column} = ?1"),
                    params![id],
                )
                .await?;
            if n > 0 {
                uses.push(format!("{n} {noun}{}", if n == 1 { "" } else { "s" }));
            }
        }
        if !uses.is_empty() {
            return Err(Error::Conflict(format!(
                "'{}' is used by {} (including archived ones), so it can't be deleted. Make it inactive instead.",
                item.name,
                uses.join(" and ")
            )));
        }
        if matches!(kind, LookupKind::Status | LookupKind::ServiceType)
            && tx
                .scalar(&format!("SELECT COUNT(*) AS n FROM {table}"), params![])
                .await?
                <= 1
        {
            return Err(Error::Conflict(if kind == LookupKind::Status {
                "This is the only status, and every item needs one.".into()
            } else {
                "This is the only service type, and every service record needs one.".into()
            }));
        }
        if kind == LookupKind::Status {
            tx.execute(
                "UPDATE assets SET previous_status_id = NULL WHERE previous_status_id = ?1",
                params![id],
            )
            .await?;
        }
        tx.execute(&format!("DELETE FROM {table} WHERE id = ?1"), params![id])
            .await
            .map_err(|e| match write_err(e, String::new) {
                // Something started using it between the check and the delete.
                Error::Validation(_) => Error::Conflict(format!(
                    "'{}' is in use, so it can't be deleted. Make it inactive instead.",
                    item.name
                )),
                other => other,
            })?;
        write_audit(
            &mut tx,
            NewAuditEntry::by(ctx, AuditAction::Delete, AuditEntity::Lookup)
                .subject(id, lookup_label(&item))
                .changes(deleted(&lookup_snapshot(&item))),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    // --- Instruments and Systems ---------------------------------------------

    async fn list_assets(&self, filter: &AssetFilter) -> Result<Vec<Asset>> {
        const SQL: &str = concat!(
            asset_select!(),
            " WHERE (?1 IS NULL OR a.kind = ?1)
                AND (?2 IS NULL OR a.parent_id = ?2)
                AND (?3 = TRUE OR a.is_archived = FALSE)
                AND (?4 IS NULL
                     OR LOWER(a.asset_tag)     LIKE LOWER(?4) ESCAPE '!'
                     OR LOWER(a.description)   LIKE LOWER(?4) ESCAPE '!'
                     OR LOWER(a.model_number)  LIKE LOWER(?4) ESCAPE '!'
                     OR LOWER(a.serial_number) LIKE LOWER(?4) ESCAPE '!')
              ORDER BY LOWER(a.asset_tag), a.id"
        );
        let search = filter
            .search
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(like_pattern);
        let today = today();
        self.conn()
            .await?
            .fetch_all(
                SQL,
                params![
                    filter.kind.map(AssetKind::as_str),
                    filter.parent_id,
                    filter.include_archived,
                    search
                ],
            )
            .await?
            .iter()
            .map(|r| map_asset(r, today))
            .collect()
    }

    async fn get_asset(&self, id: Id) -> Result<Asset> {
        fetch_asset(&mut self.conn().await?, id).await
    }

    async fn create_asset(&self, ctx: &WriteContext, mut input: AssetInput) -> Result<Asset> {
        input.normalize()?;
        let mut tx = self.begin().await?;
        check_hierarchy(&mut tx, None, &input).await?;
        inherit_from_system(&mut tx, &mut input).await?;

        let derived = derive_dates(
            input.first_cal_date,
            input.last_cal_date,
            &[],
            input.cal_interval,
        );
        let id = tx
            .insert(
                "INSERT INTO assets (
                    asset_tag, kind, parent_id, description, manufacturer_id, model_number, serial_number,
                    location_id, department_id, owner_id, status_id, warranty_end_date,
                    cal_interval_value, cal_interval_unit, in_service_date,
                    first_cal_date, last_cal_date, next_cal_date, notes, created_at, updated_at,
                    last_cal_date_entered, cal_interval_end_of_month, cal_aggregate, owner_since)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?20, ?21, ?22, ?23, ?24)",
                params![
                    &input.asset_tag,
                    input.kind.as_str(),
                    input.parent_id,
                    &input.description,
                    input.manufacturer_id,
                    &input.model_number,
                    &input.serial_number,
                    input.location_id,
                    input.department_id,
                    input.owner_id,
                    input.status_id,
                    input.warranty_end_date,
                    input.cal_interval.map(|i| i64::from(i.value)),
                    input.cal_interval.map(|i| i.unit.as_str()),
                    input.in_service_date,
                    derived.first_cal_date,
                    derived.last_cal_date,
                    derived.next_cal_date,
                    &input.notes,
                    now(),
                    input.last_cal_date,
                    input.cal_interval.is_some_and(|i| i.end_of_month),
                    input.cal_aggregate,
                    input.owner_id.map(|_| now())
                ],
            )
            .await
            .map_err(|e| write_err(e, || format!("Asset tag '{}' is already in use.", input.asset_tag)))?;

        let asset = fetch_asset(&mut tx, id).await?;
        write_audit(
            &mut tx,
            NewAuditEntry::by(ctx, AuditAction::Create, AuditEntity::Asset)
                .subject(id, &asset.asset_tag)
                .asset(id)
                .changes(created(&asset_snapshot(&asset))),
        )
        .await?;
        tx.commit().await?;
        Ok(asset)
    }

    async fn update_asset(
        &self,
        ctx: &WriteContext,
        id: Id,
        expected_version: i64,
        mut input: AssetInput,
    ) -> Result<Asset> {
        input.normalize()?;
        let mut tx = self.begin().await?;
        check_hierarchy(&mut tx, Some(id), &input).await?;
        inherit_from_system(&mut tx, &mut input).await?;
        let before = fetch_asset(&mut tx, id).await?;
        // A new owner from the edit form counts as checking it out to them now.
        let owner_since = if input.owner_id == before.owner_id {
            before.owner_since
        } else {
            input.owner_id.map(|_| now())
        };

        let changed = tx
            .execute(
                "UPDATE assets SET
                    asset_tag = ?3, kind = ?4, parent_id = ?5, description = ?6, manufacturer_id = ?7,
                    model_number = ?8, serial_number = ?9, location_id = ?10, department_id = ?11,
                    owner_id = ?12, status_id = ?13, warranty_end_date = ?14,
                    cal_interval_value = ?15, cal_interval_unit = ?16, in_service_date = ?17,
                    first_cal_date = ?18, last_cal_date_entered = ?19, notes = ?20, updated_at = ?21,
                    cal_interval_end_of_month = ?22, cal_aggregate = ?23, owner_since = ?24,
                    row_version = row_version + 1
                 WHERE id = ?1 AND row_version = ?2",
                params![
                    id,
                    expected_version,
                    &input.asset_tag,
                    input.kind.as_str(),
                    input.parent_id,
                    &input.description,
                    input.manufacturer_id,
                    &input.model_number,
                    &input.serial_number,
                    input.location_id,
                    input.department_id,
                    input.owner_id,
                    input.status_id,
                    input.warranty_end_date,
                    input.cal_interval.map(|i| i64::from(i.value)),
                    input.cal_interval.map(|i| i.unit.as_str()),
                    input.in_service_date,
                    input.first_cal_date,
                    input.last_cal_date,
                    &input.notes,
                    now(),
                    input.cal_interval.is_some_and(|i| i.end_of_month),
                    input.cal_aggregate,
                    owner_since
                ],
            )
            .await
            .map_err(|e| {
                write_err(e, || {
                    format!("Asset tag '{}' is already in use.", input.asset_tag)
                })
            })?;

        if changed == 0 {
            return Err(missing_or_stale(&mut tx, "assets", "asset", id).await);
        }
        recompute_asset_dates(&mut tx, id).await?;
        let after = fetch_asset(&mut tx, id).await?;
        write_audit(
            &mut tx,
            NewAuditEntry::by(ctx, AuditAction::Update, AuditEntity::Asset)
                .subject(id, &after.asset_tag)
                .asset(id)
                .changes(diff(&asset_snapshot(&before), &asset_snapshot(&after))),
        )
        .await?;
        if after.kind == AssetKind::System
            && (after.location_id != before.location_id
                || after.department_id != before.department_id)
        {
            cascade_to_instruments(&mut tx, ctx, &after).await?;
        }
        tx.commit().await?;
        Ok(after)
    }

    async fn change_owner(
        &self,
        ctx: &WriteContext,
        asset_ids: &[Id],
        change: OwnerChange,
    ) -> Result<Vec<Asset>> {
        let mut tx = self.begin().await?;
        let changed = apply_owner_change(&mut tx, ctx, asset_ids, change).await?;
        tx.commit().await?;
        Ok(changed)
    }

    async fn list_owner_requests(&self, filter: &OwnerRequestFilter) -> Result<Vec<OwnerRequest>> {
        const SQL: &str = concat!(
            owner_request_select!(),
            " WHERE (?1 IS NULL OR r.asset_id = ?1)
                AND (r.status = 'pending' OR (?2 IS NOT NULL AND r.decided_at >= ?2))"
        );
        let mut requests = self
            .conn()
            .await?
            .fetch_all(SQL, params![filter.asset_id, filter.decided_since])
            .await?
            .iter()
            .map(map_owner_request)
            .collect::<Result<Vec<_>>>()?;
        // Waiting ones first, oldest first (a queue); then decided ones, newest first.
        requests.sort_by(|a, b| {
            let waiting = |r: &OwnerRequest| r.status != OwnerRequestStatus::Pending;
            waiting(a).cmp(&waiting(b)).then_with(|| {
                if a.status == OwnerRequestStatus::Pending {
                    (a.created_at, a.id).cmp(&(b.created_at, b.id))
                } else {
                    (b.decided_at, b.id).cmp(&(a.decided_at, a.id))
                }
            })
        });
        Ok(requests)
    }

    async fn get_owner_request(&self, id: Id) -> Result<OwnerRequest> {
        fetch_owner_request(&mut self.conn().await?, id).await
    }

    async fn request_owner_change(
        &self,
        ctx: &WriteContext,
        asset_ids: &[Id],
        change: OwnerChange,
        note: Option<String>,
    ) -> Result<Vec<OwnerRequest>> {
        let (kind, owner_id) = match change {
            OwnerChange::CheckOut { owner_id } => (OwnerRequestKind::CheckOut, owner_id),
            OwnerChange::Transfer { owner_id } => (OwnerRequestKind::Transfer, owner_id),
            OwnerChange::CheckIn => {
                return Err(Error::validation(
                    "change",
                    "Checking items in is done by a Technician or Administrator.",
                ));
            }
        };
        let ids = unique_ids(asset_ids)?;
        let note = note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
        let mut tx = self.begin().await?;
        let owner_name = check_new_owner(&mut tx, owner_id).await?;
        let stamp = now();
        let mut created = Vec::with_capacity(ids.len());
        for id in ids {
            let asset = fetch_asset(&mut tx, id).await?;
            check_owner_change(&asset, change)?;
            check_reservation_hold(&mut tx, &asset, change).await?;
            let waiting = tx
                .fetch_optional(
                    "SELECT requested_by FROM owner_requests WHERE asset_id = ?1 AND status = 'pending'",
                    params![id],
                )
                .await?
                .map(|r| r.get::<String>("requested_by"))
                .transpose()?;
            if let Some(by) = waiting {
                return Err(Error::Conflict(format!(
                    "{} already has a request waiting (from {by}).",
                    asset.asset_tag
                )));
            }
            // A transfer is the holder's to agree to when their Owner entry is a user;
            // otherwise (a check-out, or a group owner) a Technician or Administrator decides.
            let approver: Option<Id> = match (kind, asset.owner_id) {
                (OwnerRequestKind::Transfer, Some(holder)) => tx
                    .fetch_optional("SELECT user_id FROM owners WHERE id = ?1", params![holder])
                    .await?
                    .map(|r| r.get::<Option<Id>>("user_id"))
                    .transpose()?
                    .flatten(),
                _ => None,
            };
            if approver == Some(ctx.actor.user_id) {
                return Err(Error::validation(
                    "asset_ids",
                    format!("{} is already checked out to you.", asset.asset_tag),
                ));
            }
            let request_id = tx
                .insert(
                    "INSERT INTO owner_requests (asset_id, kind, owner_id, requested_by_id,
                                                 requested_by, note, status, created_at, updated_at,
                                                 approver_user_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7, ?7, ?8)",
                    params![
                        id,
                        kind.as_str(),
                        owner_id,
                        ctx.actor.user_id,
                        &ctx.actor.username,
                        &note,
                        stamp,
                        approver
                    ],
                )
                .await?;
            write_audit(
                &mut tx,
                NewAuditEntry::by(
                    &WriteContext::new(ctx.actor.clone(), note.clone()),
                    AuditAction::Request,
                    AuditEntity::Asset,
                )
                .subject(id, &asset.asset_tag)
                .asset(id)
                .changes(vec![FieldChange {
                    field: "Request".into(),
                    old: None,
                    new: Some(request_label(kind, &owner_name)),
                }]),
            )
            .await?;
            created.push(fetch_owner_request(&mut tx, request_id).await?);
        }
        tx.commit().await?;
        Ok(created)
    }

    async fn decide_owner_request(
        &self,
        ctx: &WriteContext,
        id: Id,
        approve: bool,
        note: Option<String>,
    ) -> Result<OwnerRequest> {
        let note = note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
        let mut tx = self.begin().await?;
        let request = fetch_owner_request(&mut tx, id).await?;
        require_pending(&request)?;
        if approve {
            // The change itself, recorded on the item as a check-out or transfer, with who
            // asked and why.
            let mut reason = format!("Requested by {}", request.requested_by);
            if let Some(n) = &request.note {
                reason.push_str(&format!(": {n}"));
            }
            if let Some(n) = &note {
                reason.push_str(&format!(". Approved: {n}"));
            }
            let approval = WriteContext::new(ctx.actor.clone(), Some(reason));
            apply_owner_change(
                &mut tx,
                &approval,
                &[request.asset_id],
                request.kind.change(request.owner_id),
            )
            .await?;
        } else {
            write_audit(
                &mut tx,
                NewAuditEntry::by(
                    &WriteContext::new(ctx.actor.clone(), note.clone()),
                    AuditAction::Decline,
                    AuditEntity::Asset,
                )
                .subject(request.asset_id, &request.asset_tag)
                .asset(request.asset_id)
                .changes(vec![FieldChange {
                    field: "Request".into(),
                    old: Some(format!(
                        "{} (from {})",
                        request_label(request.kind, &request.owner_name),
                        request.requested_by
                    )),
                    new: Some("Declined".into()),
                }]),
            )
            .await?;
        }
        let status = if approve {
            OwnerRequestStatus::Approved
        } else {
            OwnerRequestStatus::Declined
        };
        close_owner_request(&mut tx, ctx, id, status, &note).await?;
        let decided = fetch_owner_request(&mut tx, id).await?;
        tx.commit().await?;
        Ok(decided)
    }

    async fn cancel_owner_request(&self, ctx: &WriteContext, id: Id) -> Result<OwnerRequest> {
        let mut tx = self.begin().await?;
        let request = fetch_owner_request(&mut tx, id).await?;
        require_pending(&request)?;
        close_owner_request(&mut tx, ctx, id, OwnerRequestStatus::Cancelled, &None).await?;
        write_audit(
            &mut tx,
            NewAuditEntry::by(ctx, AuditAction::Cancel, AuditEntity::Asset)
                .subject(request.asset_id, &request.asset_tag)
                .asset(request.asset_id)
                .changes(vec![FieldChange {
                    field: "Request".into(),
                    old: Some(format!(
                        "{} (from {})",
                        request_label(request.kind, &request.owner_name),
                        request.requested_by
                    )),
                    new: Some("Cancelled".into()),
                }]),
        )
        .await?;
        let cancelled = fetch_owner_request(&mut tx, id).await?;
        tx.commit().await?;
        Ok(cancelled)
    }

    async fn link_user_owner(
        &self,
        ctx: &WriteContext,
        user_id: Id,
        owner_id: Option<Id>,
    ) -> Result<User> {
        let mut tx = self.begin().await?;
        let before = fetch_user(&mut tx, user_id).await?;
        if let Some(owner_id) = owner_id {
            let exists = tx
                .fetch_optional("SELECT id FROM owners WHERE id = ?1", params![owner_id])
                .await?
                .is_some();
            if !exists {
                return Err(Error::validation(
                    "owner_id",
                    "That Owner entry doesn't exist.",
                ));
            }
        }
        tx.execute(
            "UPDATE owners SET user_id = NULL, updated_at = ?2 WHERE user_id = ?1",
            params![user_id, now()],
        )
        .await?;
        if let Some(owner_id) = owner_id {
            // A linked owner is active exactly when its user is.
            tx.execute(
                "UPDATE owners SET user_id = ?2, is_active = ?3, updated_at = ?4 WHERE id = ?1",
                params![owner_id, user_id, before.is_active, now()],
            )
            .await?;
        }
        let after = fetch_user(&mut tx, user_id).await?;
        if before.owner_id != after.owner_id {
            write_audit(
                &mut tx,
                NewAuditEntry::by(ctx, AuditAction::Update, AuditEntity::User)
                    .subject(user_id, &after.username)
                    .changes(vec![FieldChange {
                        field: "Owner entry".into(),
                        old: before.owner_name.clone(),
                        new: after.owner_name.clone(),
                    }]),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(after)
    }

    async fn list_reservations(&self, filter: &ReservationFilter) -> Result<Vec<Reservation>> {
        const SQL: &str = concat!(
            reservation_select!(),
            " WHERE (?1 IS NULL OR r.asset_id = ?1)
                AND (?2 IS NULL OR r.end_date >= ?2)
                AND (?3 IS NULL OR r.start_date <= ?3)
                AND (?4 = TRUE OR r.status IN ('active', 'pending'))
              ORDER BY r.start_date, r.id"
        );
        self.conn()
            .await?
            .fetch_all(
                SQL,
                params![
                    filter.asset_id,
                    filter.from,
                    filter.to,
                    filter.include_closed
                ],
            )
            .await?
            .iter()
            .map(map_reservation)
            .collect()
    }

    async fn get_reservation(&self, id: Id) -> Result<Reservation> {
        fetch_reservation(&mut self.conn().await?, id).await
    }

    async fn check_reservation(
        &self,
        asset_id: Id,
        start: NaiveDate,
        end: NaiveDate,
        exclude: Option<Id>,
    ) -> Result<ReservationCheck> {
        let mut conn = self.conn().await?;
        let asset = fetch_asset(&mut conn, asset_id).await?;
        check_reservable(&asset)?;
        let changing = match exclude {
            Some(id) => Some(fetch_reservation(&mut conn, id).await?),
            None => None,
        };
        reservation_check(&mut conn, &asset, start, end, changing.as_ref()).await
    }

    async fn update_reservation(
        &self,
        ctx: &WriteContext,
        id: Id,
        start: NaiveDate,
        end: NaiveDate,
        note: Option<String>,
    ) -> Result<Reservation> {
        let note = note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
        let mut tx = self.begin().await?;
        let before = fetch_reservation(&mut tx, id).await?;
        if before.status != ReservationStatus::Active {
            return Err(Error::Conflict(format!(
                "Only a booked reservation can be changed (this one is {}).",
                before.status
            )));
        }
        let today = today();
        if before.end_date < today {
            return Err(Error::Conflict(
                "This reservation has already finished.".into(),
            ));
        }
        if start != before.start_date {
            if before.start_date <= today {
                return Err(Error::validation(
                    "start_date",
                    "It has already started, so only its end date can change.",
                ));
            }
            if start < today {
                return Err(Error::validation(
                    "start_date",
                    "A reservation can't start in the past.",
                ));
            }
        }
        if end < today {
            return Err(Error::validation(
                "end_date",
                "A reservation can't end in the past. Cancel it instead.",
            ));
        }
        let asset = fetch_asset(&mut tx, before.asset_id).await?;
        let check = reservation_check(&mut tx, &asset, start, end, Some(&before)).await?;
        let tag = &asset.asset_tag;
        if let Some(holder) = &check.checked_out_to {
            return Err(Error::Conflict(format!(
                "{tag} is checked out to {holder}, so the reservation can't be moved onto it now."
            )));
        }
        if let Some(first) = check.conflicts.first() {
            return Err(Error::Conflict(format!(
                "{tag} is already reserved {} by {}.",
                reservation_span(first.start_date, first.end_date),
                first.user_name
            )));
        }
        tx.execute(
            "UPDATE reservations SET start_date = ?2, end_date = ?3, note = ?4, updated_at = ?5
             WHERE id = ?1",
            params![id, start, end, &note, now()],
        )
        .await?;
        let after = fetch_reservation(&mut tx, id).await?;
        let mut changes = Vec::new();
        if (before.start_date, before.end_date) != (after.start_date, after.end_date) {
            changes.push(FieldChange {
                field: "Reservation".into(),
                old: Some(reservation_label(&before)),
                new: Some(reservation_label(&after)),
            });
        }
        if before.note != after.note {
            changes.push(FieldChange {
                field: "Reservation note".into(),
                old: before.note.clone(),
                new: after.note.clone(),
            });
        }
        if !changes.is_empty() {
            write_audit(
                &mut tx,
                NewAuditEntry::by(ctx, AuditAction::Update, AuditEntity::Asset)
                    .subject(after.asset_id, &after.asset_tag)
                    .asset(after.asset_id)
                    .changes(changes),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(after)
    }

    async fn create_reservation(
        &self,
        ctx: &WriteContext,
        asset_id: Id,
        start: NaiveDate,
        end: NaiveDate,
        note: Option<String>,
        asks: Option<Id>,
    ) -> Result<Reservation> {
        let note = note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
        if start < today() {
            return Err(Error::validation(
                "start_date",
                "A reservation can't start in the past.",
            ));
        }
        let mut tx = self.begin().await?;
        let asset = fetch_asset(&mut tx, asset_id).await?;
        check_reservable(&asset)?;
        let check = reservation_check(&mut tx, &asset, start, end, None).await?;
        let tag = &asset.asset_tag;
        if let Some(holder) = &check.checked_out_to {
            return Err(Error::Conflict(format!(
                "{tag} is checked out to {holder}. It has to be returned before it can be reserved."
            )));
        }
        let status = match asks {
            None => {
                if let Some(first) = check.conflicts.first() {
                    return Err(Error::Conflict(format!(
                        "{tag} is already reserved {} by {}.",
                        reservation_span(first.start_date, first.end_date),
                        first.user_name
                    )));
                }
                ReservationStatus::Active
            }
            Some(asked) => {
                let Some(blocking) = check.can_ask.as_ref().filter(|b| b.id == asked) else {
                    return Err(Error::Conflict(format!(
                        "Those dates clash with more than the reservation you're asking about. Choose other dates for {tag}."
                    )));
                };
                if blocking.user_id == ctx.actor.user_id {
                    return Err(Error::validation(
                        "asks",
                        "That's your own reservation. Change its dates instead.",
                    ));
                }
                ReservationStatus::Pending
            }
        };
        let id = tx
            .insert(
                "INSERT INTO reservations (asset_id, user_id, start_date, end_date, note, status,
                                           asks_reservation_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                params![
                    asset_id,
                    ctx.actor.user_id,
                    start,
                    end,
                    &note,
                    status.as_str(),
                    asks,
                    now()
                ],
            )
            .await?;
        let created = fetch_reservation(&mut tx, id).await?;
        let audit_ctx = WriteContext::new(ctx.actor.clone(), note.clone());
        let (action, label) = match status {
            ReservationStatus::Pending => (
                AuditAction::Request,
                format!(
                    "{} (asks {} to end theirs early)",
                    reservation_label(&created),
                    created.asks_user_name.clone().unwrap_or_default()
                ),
            ),
            _ => (AuditAction::Reserve, reservation_label(&created)),
        };
        reservation_audit(&mut tx, &audit_ctx, action, &created, None, Some(label)).await?;
        tx.commit().await?;
        Ok(created)
    }

    async fn answer_reservation_request(
        &self,
        ctx: &WriteContext,
        id: Id,
        accept: bool,
        note: Option<String>,
    ) -> Result<Reservation> {
        let note = note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
        let mut tx = self.begin().await?;
        let request = fetch_reservation(&mut tx, id).await?;
        let (ReservationStatus::Pending, Some(asked)) =
            (request.status, request.asks_reservation_id)
        else {
            return Err(Error::Conflict(format!(
                "This reservation isn't waiting for an answer (it's {}).",
                request.status
            )));
        };
        let blocking = fetch_reservation(&mut tx, asked).await?;
        let audit_ctx = WriteContext::new(ctx.actor.clone(), note.clone());
        let stamp = now();
        if accept {
            if blocking.status == ReservationStatus::Active {
                let new_end = request.start_date - chrono::Duration::days(1);
                tx.execute(
                    "UPDATE reservations SET end_date = ?2, updated_at = ?3 WHERE id = ?1",
                    params![blocking.id, new_end, stamp],
                )
                .await?;
                let shortened = fetch_reservation(&mut tx, blocking.id).await?;
                reservation_audit(
                    &mut tx,
                    &audit_ctx,
                    AuditAction::Accept,
                    &shortened,
                    Some(reservation_label(&blocking)),
                    Some(format!(
                        "{} (ended early for {})",
                        reservation_label(&shortened),
                        request.user_name
                    )),
                )
                .await?;
            }
            // Anything else booked over these dates since? Then it can't go ahead.
            let clashes = overlapping_reservations(
                &mut tx,
                request.asset_id,
                request.start_date,
                request.end_date,
                Some(request.id),
            )
            .await?
            .into_iter()
            .filter(|r| r.id != request.id && r.status == ReservationStatus::Active)
            .count();
            if clashes > 0 {
                return Err(Error::Conflict(
                    "Another reservation now overlaps these dates, so this one can't go ahead. Decline it instead."
                        .into(),
                ));
            }
        }
        let status = if accept {
            ReservationStatus::Active
        } else {
            ReservationStatus::Declined
        };
        tx.execute(
            "UPDATE reservations SET status = ?2, decided_by = ?3, decided_at = ?4,
                                     decision_note = ?5, updated_at = ?4
             WHERE id = ?1",
            params![id, status.as_str(), &ctx.actor.username, stamp, &note],
        )
        .await?;
        let answered = fetch_reservation(&mut tx, id).await?;
        if !accept {
            reservation_audit(
                &mut tx,
                &audit_ctx,
                AuditAction::Decline,
                &answered,
                Some(reservation_label(&answered)),
                Some("Declined".into()),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(answered)
    }

    async fn cancel_reservation(&self, ctx: &WriteContext, id: Id) -> Result<Reservation> {
        let mut tx = self.begin().await?;
        let r = fetch_reservation(&mut tx, id).await?;
        if !matches!(
            r.status,
            ReservationStatus::Active | ReservationStatus::Pending
        ) {
            return Err(Error::Conflict(format!(
                "This reservation was already {}.",
                r.status
            )));
        }
        let stamp = now();
        tx.execute(
            "UPDATE reservations SET status = 'cancelled', decided_by = ?2, decided_at = ?3,
                                     updated_at = ?3
             WHERE id = ?1",
            params![id, &ctx.actor.username, stamp],
        )
        .await?;
        // Requests waiting on this one to end early no longer need to wait.
        tx.execute(
            "UPDATE reservations SET status = 'active', asks_reservation_id = NULL,
                                     decision_note = 'The reservation it asked about was cancelled.',
                                     updated_at = ?2
             WHERE asks_reservation_id = ?1 AND status = 'pending'",
            params![id, stamp],
        )
        .await?;
        let cancelled = fetch_reservation(&mut tx, id).await?;
        reservation_audit(
            &mut tx,
            ctx,
            AuditAction::Cancel,
            &cancelled,
            Some(reservation_label(&r)),
            Some("Cancelled".into()),
        )
        .await?;
        tx.commit().await?;
        Ok(cancelled)
    }

    async fn add_to_system(
        &self,
        ctx: &WriteContext,
        system_id: Id,
        instrument_ids: &[Id],
    ) -> Result<()> {
        let mut tx = self.begin().await?;
        let system = fetch_asset(&mut tx, system_id).await?;
        if system.kind != AssetKind::System {
            return Err(Error::validation(
                "parent_id",
                "Instruments can only belong to a System.",
            ));
        }
        if system.is_archived {
            return Err(Error::validation(
                "parent_id",
                "The selected System is archived.",
            ));
        }
        for &id in instrument_ids {
            let before = fetch_asset(&mut tx, id).await?;
            if before.parent_id == Some(system_id) {
                continue;
            }
            if before.kind != AssetKind::Instrument {
                return Err(Error::validation(
                    "children",
                    format!(
                        "{} is a System; only Instruments can be added.",
                        before.asset_tag
                    ),
                ));
            }
            if before.is_archived {
                return Err(Error::validation(
                    "children",
                    format!("{} is archived. Restore it first.", before.asset_tag),
                ));
            }
            tx.execute(
                "UPDATE assets SET parent_id = ?2, location_id = ?3, department_id = ?4,
                                   updated_at = ?5, row_version = row_version + 1
                 WHERE id = ?1",
                params![
                    id,
                    system_id,
                    system.location_id,
                    system.department_id,
                    now()
                ],
            )
            .await?;
            let after = fetch_asset(&mut tx, id).await?;
            write_audit(
                &mut tx,
                NewAuditEntry::by(ctx, AuditAction::Update, AuditEntity::Asset)
                    .subject(id, &after.asset_tag)
                    .asset(id)
                    .changes(diff(&asset_snapshot(&before), &asset_snapshot(&after))),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn set_asset_archived(
        &self,
        ctx: &WriteContext,
        id: Id,
        expected_version: i64,
        archived: bool,
    ) -> Result<Asset> {
        let mut tx = self.begin().await?;
        if archived {
            let children = tx
                .scalar(
                    "SELECT COUNT(*) AS n FROM assets WHERE parent_id = ?1 AND is_archived = FALSE",
                    params![id],
                )
                .await?;
            if children > 0 {
                return Err(Error::validation(
                    "",
                    format!(
                        "This System still contains {children} active item(s). Archive or move them first."
                    ),
                ));
            }
        }
        let before = fetch_asset(&mut tx, id).await?;
        let changed = tx
            .execute(
                "UPDATE assets SET is_archived = ?3, updated_at = ?4, row_version = row_version + 1
                 WHERE id = ?1 AND row_version = ?2",
                params![id, expected_version, archived, now()],
            )
            .await?;
        if changed == 0 {
            return Err(missing_or_stale(&mut tx, "assets", "asset", id).await);
        }
        let after = fetch_asset(&mut tx, id).await?;
        write_audit(
            &mut tx,
            NewAuditEntry::by(ctx, archive_action(archived), AuditEntity::Asset)
                .subject(id, &after.asset_tag)
                .asset(id)
                .changes(diff(&asset_snapshot(&before), &asset_snapshot(&after))),
        )
        .await?;
        tx.commit().await?;
        Ok(after)
    }

    // --- Attachments -------------------------------------------------------------

    async fn list_attachments(&self, filter: &AttachmentFilter) -> Result<Vec<Attachment>> {
        const SQL: &str = concat!(
            attachment_select!(),
            " WHERE (?1 IS NULL
                     OR f.asset_id = ?1
                     -- Service manuals from other items of the same manufacturer and model.
                     OR (?4 = TRUE AND ?2 IS NULL AND f.category = 'service_manual'
                         AND f.asset_id IN (
                             SELECT o.id FROM assets o JOIN assets t ON t.id = ?1
                              WHERE o.id <> t.id
                                AND t.manufacturer_id IS NOT NULL AND t.model_number IS NOT NULL
                                AND o.manufacturer_id = t.manufacturer_id
                                AND LOWER(o.model_number) = LOWER(t.model_number))))
                AND (?2 IS NULL OR f.calibration_id = ?2)
                AND (?3 = TRUE OR f.is_archived = FALSE)
              ORDER BY f.created_at DESC, f.id DESC"
        );
        self.conn()
            .await?
            .fetch_all(
                SQL,
                params![
                    filter.asset_id,
                    filter.calibration_id,
                    filter.include_archived,
                    filter.include_shared_manuals
                ],
            )
            .await?
            .iter()
            .map(map_attachment)
            .collect()
    }

    async fn get_attachment(&self, id: Id) -> Result<Attachment> {
        fetch_attachment(&mut self.conn().await?, id).await
    }

    async fn create_attachment(
        &self,
        ctx: &WriteContext,
        mut input: NewAttachment,
    ) -> Result<Attachment> {
        input.file_name = input.file_name.trim().to_string();
        input.description = input
            .description
            .take()
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty());
        if input.file_name.is_empty() || input.relative_path.trim().is_empty() {
            return Err(Error::validation("file_name", "The file has no name."));
        }
        let mut tx = self.begin().await?;
        let asset_archived = tx
            .fetch_optional(
                "SELECT is_archived FROM assets WHERE id = ?1",
                params![input.asset_id],
            )
            .await?
            .map(|r| r.get::<bool>("is_archived"))
            .transpose()?;
        match asset_archived {
            None => {
                return Err(Error::validation(
                    "asset_id",
                    "The selected item does not exist.",
                ));
            }
            Some(true) => {
                return Err(Error::validation(
                    "asset_id",
                    "The selected item is archived.",
                ));
            }
            Some(false) => {}
        }
        if let Some(cal_id) = input.calibration_id {
            let owner = tx
                .fetch_optional(
                    "SELECT asset_id FROM calibration_records WHERE id = ?1",
                    params![cal_id],
                )
                .await?
                .map(|r| r.get::<Id>("asset_id"))
                .transpose()?;
            if owner != Some(input.asset_id) {
                return Err(Error::validation(
                    "calibration_id",
                    "That calibration record doesn't belong to this item.",
                ));
            }
        }

        // The same file (by fingerprint) attached twice to the same item or record is
        // almost always a mistake, and the two would be indistinguishable in the list.
        let duplicate_sql = format!(
            "SELECT file_name FROM attachments
             WHERE asset_id = ?1
               AND ((?2 IS NULL AND calibration_id IS NULL) OR calibration_id = ?2)
               AND sha256 = ?3 AND is_archived = FALSE
             ORDER BY id
             {}",
            self.dialect().limit("1")
        );
        let already = tx
            .fetch_optional(
                &duplicate_sql,
                params![input.asset_id, input.calibration_id, &input.sha256],
            )
            .await?
            .map(|r| r.get::<String>("file_name"))
            .transpose()?;
        if let Some(existing) = already {
            return Err(Error::Duplicate(format!(
                "This file is already attached as {existing}."
            )));
        }

        let id = tx
            .insert(
                "INSERT INTO attachments (asset_id, calibration_id, category, file_name, relative_path,
                                          size_bytes, sha256, content_type, description,
                                          uploaded_by_id, uploaded_by, created_at, updated_at,
                                          storage_folder)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12, ?13)",
                params![
                    input.asset_id,
                    input.calibration_id,
                    input.category.as_str(),
                    &input.file_name,
                    &input.relative_path,
                    input.size_bytes,
                    &input.sha256,
                    &input.content_type,
                    &input.description,
                    ctx.actor.user_id,
                    &ctx.actor.username,
                    now(),
                    input.storage_folder.as_str()
                ],
            )
            .await
            .map_err(|e| {
                write_err(e, || {
                    format!("A file is already stored at {}.", input.relative_path)
                })
            })?;

        let attachment = fetch_attachment(&mut tx, id).await?;
        write_audit(
            &mut tx,
            NewAuditEntry::by(ctx, AuditAction::Create, AuditEntity::Attachment)
                .subject(id, &attachment.file_name)
                .asset(attachment.asset_id)
                .changes(created(&attachment_snapshot(&attachment))),
        )
        .await?;
        tx.commit().await?;
        Ok(attachment)
    }

    async fn set_attachment_archived(
        &self,
        ctx: &WriteContext,
        id: Id,
        expected_version: i64,
        archived: bool,
    ) -> Result<Attachment> {
        let mut tx = self.begin().await?;
        let before = fetch_attachment(&mut tx, id).await?;
        let changed = tx
            .execute(
                "UPDATE attachments SET is_archived = ?3, updated_at = ?4, row_version = row_version + 1
                 WHERE id = ?1 AND row_version = ?2",
                params![id, expected_version, archived, now()],
            )
            .await?;
        if changed == 0 {
            return Err(missing_or_stale(&mut tx, "attachments", "attachment", id).await);
        }
        let after = fetch_attachment(&mut tx, id).await?;
        write_audit(
            &mut tx,
            NewAuditEntry::by(ctx, archive_action(archived), AuditEntity::Attachment)
                .subject(id, &after.file_name)
                .asset(after.asset_id)
                .changes(diff(
                    &attachment_snapshot(&before),
                    &attachment_snapshot(&after),
                )),
        )
        .await?;
        tx.commit().await?;
        Ok(after)
    }

    // --- Calibration records ---------------------------------------------------

    async fn list_calibrations(
        &self,
        filter: &CalibrationFilter,
    ) -> Result<Vec<CalibrationRecord>> {
        const SQL: &str = concat!(
            calibration_select!(),
            " WHERE (?1 IS NULL OR r.asset_id = ?1) AND (?2 = TRUE OR r.is_archived = FALSE)
                AND (?3 IS NULL OR r.cal_date >= ?3) AND (?4 IS NULL OR r.cal_date <= ?4)
              ORDER BY r.cal_date DESC, r.id DESC"
        );
        self.conn()
            .await?
            .fetch_all(
                SQL,
                params![
                    filter.asset_id,
                    filter.include_archived,
                    filter.from_date,
                    filter.to_date
                ],
            )
            .await?
            .iter()
            .map(map_calibration)
            .collect()
    }

    async fn last_cost(
        &self,
        provider_id: Id,
        service_type_id: Id,
        asset_id: Id,
    ) -> Result<Option<LastCost>> {
        let sql = format!(
            "SELECT r.cost_cents, r.cal_date, a.asset_tag
             FROM calibration_records r
             JOIN assets a  ON a.id = r.asset_id
             JOIN assets me ON me.id = ?3
             WHERE r.provider_id = ?1 AND r.service_type_id = ?2
               AND r.cost_cents IS NOT NULL AND r.is_archived = FALSE
               AND a.manufacturer_id = me.manufacturer_id
               AND LOWER(a.model_number) = LOWER(me.model_number)
             ORDER BY r.cal_date DESC, r.id DESC
             {}",
            self.dialect().limit("1")
        );
        self.conn()
            .await?
            .fetch_optional(&sql, params![provider_id, service_type_id, asset_id])
            .await?
            .map(|r| {
                Ok(LastCost {
                    cost_cents: r.get("cost_cents")?,
                    cal_date: r.get("cal_date")?,
                    asset_tag: r.get("asset_tag")?,
                })
            })
            .transpose()
    }

    async fn get_calibration(&self, id: Id) -> Result<CalibrationRecord> {
        fetch_calibration(&mut self.conn().await?, id).await
    }

    async fn create_calibration(
        &self,
        ctx: &WriteContext,
        mut input: CalibrationInput,
    ) -> Result<CalibrationRecord> {
        input.normalize(today())?;
        let mut tx = self.begin().await?;
        let next_due = prepare_service(&mut tx, &mut input).await?;
        let asset_before = fetch_asset(&mut tx, input.asset_id).await?;

        let id = tx
            .insert(
                "INSERT INTO calibration_records (
                    asset_id, cal_date, provider_id, technician, cost_cents, certificate_number,
                    condition_received, condition_returned, next_due_date, notes, created_at, updated_at,
                    manufacturer_id, service_type_id, extension_days)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, ?12, ?13, ?14)",
                params![
                    input.asset_id,
                    input.cal_date,
                    calibrated_by_columns(&input).0,
                    &input.technician,
                    input.cost_cents,
                    &input.certificate_number,
                    input.condition_received.as_str(),
                    input.condition_returned.as_str(),
                    next_due,
                    &input.notes,
                    now(),
                    calibrated_by_columns(&input).1,
                    input.service_type_id,
                    input.extension_days
                ],
            )
            .await
            .map_err(|e| write_err(e, || "This service record already exists.".into()))?;

        recompute_asset_dates(&mut tx, input.asset_id).await?;
        let record = fetch_calibration(&mut tx, id).await?;
        write_audit(
            &mut tx,
            NewAuditEntry::by(ctx, AuditAction::Create, AuditEntity::Calibration)
                .subject(id, format!("{} on {}", record.asset_tag, record.cal_date))
                .asset(record.asset_id)
                .changes(created(&calibration_snapshot(&record))),
        )
        .await?;
        set_status_with_service(&mut tx, ctx, &asset_before, input.new_status_id).await?;
        tx.commit().await?;
        Ok(record)
    }

    async fn update_calibration(
        &self,
        ctx: &WriteContext,
        id: Id,
        expected_version: i64,
        mut input: CalibrationInput,
    ) -> Result<CalibrationRecord> {
        input.normalize(today())?;
        let mut tx = self.begin().await?;
        let before = fetch_calibration(&mut tx, id).await?;
        if before.asset_id != input.asset_id {
            return Err(Error::validation(
                "asset_id",
                "A calibration record cannot be moved to a different item. Archive it and create a new one.",
            ));
        }
        let next_due = prepare_service(&mut tx, &mut input).await?;
        let asset_before = fetch_asset(&mut tx, input.asset_id).await?;

        let changed = tx
            .execute(
                "UPDATE calibration_records SET
                    cal_date = ?3, provider_id = ?4, technician = ?5, cost_cents = ?6,
                    certificate_number = ?7, condition_received = ?8, condition_returned = ?9,
                    next_due_date = ?10, notes = ?11, updated_at = ?12, manufacturer_id = ?13,
                    service_type_id = ?14, extension_days = ?15, row_version = row_version + 1
                 WHERE id = ?1 AND row_version = ?2",
                params![
                    id,
                    expected_version,
                    input.cal_date,
                    calibrated_by_columns(&input).0,
                    &input.technician,
                    input.cost_cents,
                    &input.certificate_number,
                    input.condition_received.as_str(),
                    input.condition_returned.as_str(),
                    next_due,
                    &input.notes,
                    now(),
                    calibrated_by_columns(&input).1,
                    input.service_type_id,
                    input.extension_days
                ],
            )
            .await
            .map_err(|e| write_err(e, || "This service record already exists.".into()))?;

        if changed == 0 {
            return Err(
                missing_or_stale(&mut tx, "calibration_records", "calibration record", id).await,
            );
        }
        recompute_asset_dates(&mut tx, input.asset_id).await?;
        let after = fetch_calibration(&mut tx, id).await?;
        write_audit(
            &mut tx,
            NewAuditEntry::by(ctx, AuditAction::Update, AuditEntity::Calibration)
                .subject(id, format!("{} on {}", after.asset_tag, after.cal_date))
                .asset(after.asset_id)
                .changes(diff(
                    &calibration_snapshot(&before),
                    &calibration_snapshot(&after),
                )),
        )
        .await?;
        set_status_with_service(&mut tx, ctx, &asset_before, input.new_status_id).await?;
        tx.commit().await?;
        Ok(after)
    }

    async fn set_calibration_archived(
        &self,
        ctx: &WriteContext,
        id: Id,
        expected_version: i64,
        archived: bool,
    ) -> Result<CalibrationRecord> {
        let mut tx = self.begin().await?;
        let before = fetch_calibration(&mut tx, id).await?;
        let changed = tx
            .execute(
                "UPDATE calibration_records SET is_archived = ?3, updated_at = ?4, row_version = row_version + 1
                 WHERE id = ?1 AND row_version = ?2",
                params![id, expected_version, archived, now()],
            )
            .await?;
        if changed == 0 {
            return Err(
                missing_or_stale(&mut tx, "calibration_records", "calibration record", id).await,
            );
        }
        recompute_asset_dates(&mut tx, before.asset_id).await?;
        let after = fetch_calibration(&mut tx, id).await?;
        write_audit(
            &mut tx,
            NewAuditEntry::by(ctx, archive_action(archived), AuditEntity::Calibration)
                .subject(id, format!("{} on {}", after.asset_tag, after.cal_date))
                .asset(after.asset_id)
                .changes(diff(
                    &calibration_snapshot(&before),
                    &calibration_snapshot(&after),
                )),
        )
        .await?;
        tx.commit().await?;
        Ok(after)
    }

    // --- Moving data between databases -------------------------------------------

    // --- Certificates ------------------------------------------------------------

    async fn certificate_template(&self) -> Result<CertificateTemplate> {
        read_certificate_template(&mut self.conn().await?).await
    }

    async fn update_certificate_template(
        &self,
        ctx: &WriteContext,
        mut template: CertificateTemplate,
    ) -> Result<CertificateTemplate> {
        template.normalize()?;
        let mut tx = self.begin().await?;
        let before = read_certificate_template(&mut tx).await?;
        let json = serde_json::to_string(&template)
            .map_err(|e| Error::Database(format!("could not encode the template: {e}")))?;
        write_setting(&mut tx, TEMPLATE_KEY, &json).await?;
        write_certificate_counter(&mut tx, template.next_number).await?;
        let changes = template_changes(&before, &template);
        if !changes.is_empty() {
            write_audit(
                &mut tx,
                NewAuditEntry::by(ctx, AuditAction::Update, AuditEntity::Settings).changes(changes),
            )
            .await?;
        }
        let saved = read_certificate_template(&mut tx).await?;
        tx.commit().await?;
        Ok(saved)
    }

    async fn list_certificates(
        &self,
        filter: &CertificateFilter,
    ) -> Result<Vec<CertificateSummary>> {
        const SQL: &str = concat!(
            certificate_select!(),
            " WHERE (?1 IS NULL OR c.asset_id = ?1)
                AND (?2 IS NULL
                     OR LOWER(c.certificate_number) LIKE LOWER(?2) ESCAPE '!'
                     OR LOWER(c.subject)            LIKE LOWER(?2) ESCAPE '!'
                     OR LOWER(c.description)        LIKE LOWER(?2) ESCAPE '!')
              ORDER BY c.created_at DESC, c.id DESC"
        );
        let search = filter
            .search
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(like_pattern);
        self.conn()
            .await?
            .fetch_all(SQL, params![filter.asset_id, search])
            .await?
            .iter()
            .map(map_certificate_summary)
            .collect()
    }

    async fn get_certificate(&self, id: Id) -> Result<Certificate> {
        fetch_certificate(&mut self.conn().await?, id).await
    }

    async fn certificate_signers(&self) -> Result<Vec<CertificateSigner>> {
        self.conn()
            .await?
            .fetch_all(
                "SELECT id, display_name,
                        CASE WHEN signature IS NULL THEN 0 ELSE 1 END AS has_signature
                 FROM users WHERE is_active = TRUE
                 ORDER BY LOWER(display_name), id",
                params![],
            )
            .await?
            .iter()
            .map(|r| {
                Ok(CertificateSigner {
                    id: r.get("id")?,
                    display_name: r.get("display_name")?,
                    has_signature: r.get::<i64>("has_signature")? != 0,
                })
            })
            .collect()
    }

    async fn user_signature(&self, user_id: Id) -> Result<Option<String>> {
        fetch_signature(&mut self.conn().await?, user_id)
            .await?
            .ok_or(Error::NotFound {
                entity: "user",
                id: user_id,
            })
    }

    async fn set_user_signature(
        &self,
        ctx: &WriteContext,
        user_id: Id,
        signature: Option<String>,
    ) -> Result<()> {
        let signature = signature.filter(|s| !s.trim().is_empty());
        if let Some(image) = &signature {
            check_image("signature", image)?;
        }
        let mut tx = self.begin().await?;
        let user = tx
            .fetch_optional(
                "SELECT username, signature FROM users WHERE id = ?1",
                params![user_id],
            )
            .await?
            .ok_or(Error::NotFound {
                entity: "user",
                id: user_id,
            })?;
        let username: String = user.get("username")?;
        let had: Option<String> = user.get("signature")?;
        tx.execute(
            "UPDATE users SET signature = ?2, updated_at = ?3, row_version = row_version + 1
             WHERE id = ?1",
            params![user_id, &signature, now()],
        )
        .await?;
        let describe = |s: &Option<String>| s.as_ref().map(|_| "Saved".to_string());
        if had != signature {
            write_audit(
                &mut tx,
                NewAuditEntry::by(ctx, AuditAction::Update, AuditEntity::User)
                    .subject(user_id, &username)
                    .changes(vec![FieldChange {
                        field: "Signature".into(),
                        old: describe(&had),
                        new: if had.is_some() && signature.is_some() {
                            Some("Replaced".into())
                        } else {
                            describe(&signature)
                        },
                    }]),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn create_certificate(
        &self,
        ctx: &WriteContext,
        mut details: CertificateDetails,
    ) -> Result<Certificate> {
        details.normalize()?;
        let mut tx = self.begin().await?;
        if let Some(asset_id) = details.asset_id {
            let exists = tx
                .fetch_optional("SELECT 1 AS n FROM assets WHERE id = ?1", params![asset_id])
                .await?
                .is_some();
            if !exists {
                return Err(Error::validation(
                    "asset_id",
                    "The selected item does not exist.",
                ));
            }
        }
        resolve_signer(
            &mut tx,
            &mut details.calibrated_by_id,
            &mut details.calibrated_by_signature,
        )
        .await?;
        resolve_signer(
            &mut tx,
            &mut details.approved_by_id,
            &mut details.approved_by_signature,
        )
        .await?;
        let template = read_certificate_template(&mut tx).await?;
        let issued = now();
        let number = template.format_number(template.next_number, today());
        let encode = |e: serde_json::Error| {
            Error::Database(format!("could not encode the certificate: {e}"))
        };
        let details_json = serde_json::to_string(&details).map_err(encode)?;
        let template_json = serde_json::to_string(&template).map_err(encode)?;
        let id = tx
            .insert(
                "INSERT INTO certificates (certificate_number, kind, asset_id, subject, description,
                                           cal_date, details, template, issued_by_id, issued_by,
                                           created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    &number,
                    details.kind.as_str(),
                    details.asset_id,
                    details.subject(),
                    &details.description,
                    details.cal_date,
                    details_json,
                    template_json,
                    ctx.actor.user_id,
                    &ctx.actor.username,
                    issued
                ],
            )
            .await
            .map_err(|e| {
                write_err(e, || {
                    format!(
                        "Certificate number {number} is already used. Change the next number in \
                         Settings > Certificate Template."
                    )
                })
            })?;
        write_certificate_counter(&mut tx, template.next_number.saturating_add(1)).await?;
        let certificate = fetch_certificate(&mut tx, id).await?;
        let mut entry = NewAuditEntry::by(ctx, AuditAction::Create, AuditEntity::Certificate)
            .subject(id, &number)
            .changes(created(&certificate_snapshot(&certificate)));
        if let Some(asset_id) = details.asset_id {
            entry = entry.asset(asset_id);
        }
        write_audit(&mut tx, entry).await?;
        tx.commit().await?;
        Ok(certificate)
    }

    async fn table_counts(&self) -> Result<Vec<TableCount>> {
        crate::transfer::table_counts(&mut self.conn().await?).await
    }

    async fn export_data(&self) -> Result<DataSet> {
        crate::transfer::export(&mut self.begin().await?).await
    }

    async fn import_data(&self, data: &DataSet) -> Result<()> {
        let mut tx = self.begin().await?;
        crate::transfer::import(&mut tx, data).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn record_data_copy(
        &self,
        ctx: &WriteContext,
        action: AuditAction,
        description: &str,
    ) -> Result<()> {
        let mut entry = NewAuditEntry::by(ctx, action, AuditEntity::Database);
        entry.entity_label = Some(description.chars().take(500).collect());
        write_audit(&mut self.conn().await?, entry).await
    }
}
