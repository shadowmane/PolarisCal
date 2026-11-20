//! SQLite backend: a single local database file.

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{Local, NaiveDate, Utc};
use polaris_core::model::{
    Asset, AssetFilter, AssetInput, AssetKind, CalInterval, CalibrationFilter, CalibrationInput,
    CalibrationRecord, LookupInput, LookupItem, LookupKind, StoreInfo,
};
use polaris_core::schedule::{add_interval, derive_dates, due_status};
use polaris_core::{Error, Id, Result, Store};
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteRow,
    SqliteSynchronous,
};
use sqlx::{AssertSqlSafe, Row, Sqlite, Transaction};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations/sqlite");

pub struct SqliteStore {
    pool: SqlitePool,
    path: PathBuf,
}

impl SqliteStore {
    /// Opens (creating if needed) the database file and runs pending migrations.
    pub async fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| {
                Error::Config(format!("cannot create folder {}: {e}", parent.display()))
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
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(journal)
            .synchronous(sync)
            .busy_timeout(Duration::from_secs(10));

        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .map_err(|e| Error::Config(format!("cannot open {}: {e}", path.display())))?;

        MIGRATOR
            .run(&pool)
            .await
            .map_err(|e| Error::Database(format!("database upgrade failed: {e}")))?;

        Ok(Self {
            pool,
            path: path.to_path_buf(),
        })
    }

    async fn begin(&self) -> Result<Transaction<'static, Sqlite>> {
        self.pool.begin().await.map_err(db)
    }
}

// ---------------------------------------------------------------------------
// Error and row helpers
// ---------------------------------------------------------------------------

fn db(e: sqlx::Error) -> Error {
    Error::Database(e.to_string())
}

/// Maps constraint violations on writes to errors a user can act on.
fn write_err(e: sqlx::Error, duplicate_message: impl FnOnce() -> String) -> Error {
    if let Some(d) = e.as_database_error() {
        if d.is_unique_violation() {
            return Error::Duplicate(duplicate_message());
        }
        if d.is_foreign_key_violation() {
            return Error::validation(
                "",
                "One of the selected items no longer exists. Reload and try again.",
            );
        }
    }
    db(e)
}

fn get<'r, T>(row: &'r SqliteRow, column: &str) -> Result<T>
where
    T: sqlx::Decode<'r, Sqlite> + sqlx::Type<Sqlite>,
{
    row.try_get(column).map_err(db)
}

fn interval_from(value: Option<i64>, unit: Option<String>) -> Result<Option<CalInterval>> {
    match (value, unit) {
        (Some(v), Some(u)) => Ok(Some(CalInterval {
            value: u32::try_from(v).map_err(|_| Error::Database(format!("bad interval {v}")))?,
            unit: u.parse()?,
        })),
        _ => Ok(None),
    }
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// Escapes LIKE wildcards in user search text (paired with `ESCAPE '\'`).
fn like_pattern(search: &str) -> String {
    let mut out = String::with_capacity(search.len() + 2);
    out.push('%');
    for c in search.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

/// After an UPDATE matched no rows: is the row gone, or did its version move?
async fn missing_or_stale(
    tx: &mut Transaction<'static, Sqlite>,
    table: &'static str,
    entity: &'static str,
    id: Id,
) -> Error {
    let sql = format!("SELECT 1 FROM {table} WHERE id = ?1");
    match sqlx::query(AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
    {
        Ok(Some(_)) => Error::stale(entity),
        Ok(None) => Error::NotFound { entity, id },
        Err(e) => db(e),
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
                a.owner_id, o.name AS owner_name,
                a.status_id, s.name AS status_name, s.tracks_due AS status_tracks_due,
                a.warranty_end_date, a.cal_interval_value, a.cal_interval_unit,
                a.in_service_date, a.first_cal_date, a.last_cal_date, a.last_cal_date_entered, a.next_cal_date,
                (SELECT COUNT(*) FROM assets c WHERE c.parent_id = a.id AND c.is_archived = 0) AS child_count,
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

fn map_asset(row: &SqliteRow, today: NaiveDate) -> Result<Asset> {
    let next_cal_date: Option<NaiveDate> = get(row, "next_cal_date")?;
    let tracks_due: bool = get(row, "status_tracks_due")?;
    Ok(Asset {
        id: get(row, "id")?,
        asset_tag: get(row, "asset_tag")?,
        kind: get::<String>(row, "kind")?.parse()?,
        parent_id: get(row, "parent_id")?,
        parent_tag: get(row, "parent_tag")?,
        description: get(row, "description")?,
        manufacturer_id: get(row, "manufacturer_id")?,
        manufacturer_name: get(row, "manufacturer_name")?,
        model_number: get(row, "model_number")?,
        serial_number: get(row, "serial_number")?,
        location_id: get(row, "location_id")?,
        location_name: get(row, "location_name")?,
        department_id: get(row, "department_id")?,
        department_name: get(row, "department_name")?,
        owner_id: get(row, "owner_id")?,
        owner_name: get(row, "owner_name")?,
        status_id: get(row, "status_id")?,
        status_name: get(row, "status_name")?,
        warranty_end_date: get(row, "warranty_end_date")?,
        cal_interval: interval_from(
            get(row, "cal_interval_value")?,
            get(row, "cal_interval_unit")?,
        )?,
        in_service_date: get(row, "in_service_date")?,
        first_cal_date: get(row, "first_cal_date")?,
        last_cal_date: get(row, "last_cal_date")?,
        last_cal_date_entered: get(row, "last_cal_date_entered")?,
        next_cal_date,
        due_status: due_status(next_cal_date, tracks_due, today),
        child_count: get(row, "child_count")?,
        notes: get(row, "notes")?,
        is_archived: get(row, "is_archived")?,
        row_version: get(row, "row_version")?,
        created_at: get(row, "created_at")?,
        updated_at: get(row, "updated_at")?,
    })
}

/// Enforces the System/Instrument hierarchy rules that need the database.
async fn check_hierarchy(
    tx: &mut Transaction<'static, Sqlite>,
    self_id: Option<Id>,
    input: &AssetInput,
) -> Result<()> {
    if let Some(parent_id) = input.parent_id {
        if Some(parent_id) == self_id {
            return Err(Error::validation(
                "parent_id",
                "An item cannot belong to itself.",
            ));
        }
        let parent = sqlx::query("SELECT kind, is_archived FROM assets WHERE id = ?1")
            .bind(parent_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(db)?
            .ok_or_else(|| Error::validation("parent_id", "The selected System does not exist."))?;
        if get::<String>(&parent, "kind")?.parse::<AssetKind>()? != AssetKind::System {
            return Err(Error::validation(
                "parent_id",
                "Instruments can only belong to a System.",
            ));
        }
        if get::<bool>(&parent, "is_archived")? {
            return Err(Error::validation(
                "parent_id",
                "The selected System is archived.",
            ));
        }
    }
    if let (Some(id), AssetKind::Instrument) = (self_id, input.kind) {
        let children: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM assets WHERE parent_id = ?1")
            .bind(id)
            .fetch_one(&mut **tx)
            .await
            .map_err(db)?;
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
async fn recompute_asset_dates(tx: &mut Transaction<'static, Sqlite>, asset_id: Id) -> Result<()> {
    let row = sqlx::query(
        "SELECT first_cal_date, last_cal_date_entered, cal_interval_value, cal_interval_unit
         FROM assets WHERE id = ?1",
    )
    .bind(asset_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db)?
    .ok_or(Error::NotFound {
        entity: "asset",
        id: asset_id,
    })?;

    let record_dates: Vec<NaiveDate> = sqlx::query_scalar(
        "SELECT cal_date FROM calibration_records WHERE asset_id = ?1 AND is_archived = 0",
    )
    .bind(asset_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(db)?;

    let derived = derive_dates(
        get(&row, "first_cal_date")?,
        get(&row, "last_cal_date_entered")?,
        &record_dates,
        interval_from(
            get(&row, "cal_interval_value")?,
            get(&row, "cal_interval_unit")?,
        )?,
    );

    sqlx::query(
        "UPDATE assets SET first_cal_date = ?2, last_cal_date = ?3, next_cal_date = ?4 WHERE id = ?1",
    )
    .bind(asset_id)
    .bind(derived.first_cal_date)
    .bind(derived.last_cal_date)
    .bind(derived.next_cal_date)
    .execute(&mut **tx)
    .await
    .map_err(db)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Calibration records
// ---------------------------------------------------------------------------

macro_rules! calibration_select {
    () => {
        "SELECT r.id, r.asset_id, a.asset_tag, r.cal_date, r.provider_id, p.name AS provider_name,
                r.technician, r.cost_cents, r.certificate_number,
                r.condition_received, r.condition_returned, r.next_due_date, r.notes,
                r.is_archived, r.row_version, r.created_at, r.updated_at
         FROM calibration_records r
         JOIN assets a                ON a.id = r.asset_id
         JOIN calibration_providers p ON p.id = r.provider_id"
    };
}

fn map_calibration(row: &SqliteRow) -> Result<CalibrationRecord> {
    Ok(CalibrationRecord {
        id: get(row, "id")?,
        asset_id: get(row, "asset_id")?,
        asset_tag: get(row, "asset_tag")?,
        cal_date: get(row, "cal_date")?,
        provider_id: get(row, "provider_id")?,
        provider_name: get(row, "provider_name")?,
        technician: get(row, "technician")?,
        cost_cents: get(row, "cost_cents")?,
        certificate_number: get(row, "certificate_number")?,
        condition_received: get::<String>(row, "condition_received")?.parse()?,
        condition_returned: get::<String>(row, "condition_returned")?.parse()?,
        next_due_date: get(row, "next_due_date")?,
        notes: get(row, "notes")?,
        is_archived: get(row, "is_archived")?,
        row_version: get(row, "row_version")?,
        created_at: get(row, "created_at")?,
        updated_at: get(row, "updated_at")?,
    })
}

/// The due date a calibration on `cal_date` establishes for this asset.
async fn due_date_for(
    tx: &mut Transaction<'static, Sqlite>,
    asset_id: Id,
    cal_date: NaiveDate,
) -> Result<Option<NaiveDate>> {
    let row = sqlx::query(
        "SELECT cal_interval_value, cal_interval_unit, is_archived FROM assets WHERE id = ?1",
    )
    .bind(asset_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db)?
    .ok_or_else(|| Error::validation("asset_id", "The selected item does not exist."))?;
    if get::<bool>(&row, "is_archived")? {
        return Err(Error::validation(
            "asset_id",
            "The selected item is archived.",
        ));
    }
    let interval = interval_from(
        get(&row, "cal_interval_value")?,
        get(&row, "cal_interval_unit")?,
    )?;
    Ok(interval.and_then(|i| add_interval(cal_date, i)))
}

async fn calibration_asset_id(tx: &mut Transaction<'static, Sqlite>, id: Id) -> Result<Id> {
    sqlx::query_scalar("SELECT asset_id FROM calibration_records WHERE id = ?1")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db)?
        .ok_or(Error::NotFound {
            entity: "calibration record",
            id,
        })
}

// ---------------------------------------------------------------------------
// Lookups
// ---------------------------------------------------------------------------

/// Table name and the kind-specific extra column, if any. Both are fixed
/// strings chosen by `kind`, never user input, so formatting them into SQL is safe.
fn lookup_table(kind: LookupKind) -> (&'static str, Option<&'static str>) {
    match kind {
        LookupKind::Location => ("locations", None),
        LookupKind::Department => ("departments", None),
        LookupKind::Owner => ("owners", None),
        LookupKind::Manufacturer => ("manufacturers", None),
        LookupKind::Provider => ("calibration_providers", Some("is_internal")),
        LookupKind::Status => ("asset_statuses", Some("tracks_due")),
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
    format!(
        "SELECT id, name, description, is_active, sort_order,
                {internal} AS is_internal, {tracks} AS tracks_due
         FROM {table}"
    )
}

fn map_lookup(row: &SqliteRow, kind: LookupKind) -> Result<LookupItem> {
    Ok(LookupItem {
        id: get(row, "id")?,
        kind,
        name: get(row, "name")?,
        description: get(row, "description")?,
        is_active: get(row, "is_active")?,
        sort_order: get(row, "sort_order")?,
        is_internal: get(row, "is_internal")?,
        tracks_due: get(row, "tracks_due")?,
    })
}

/// The value for a kind's extra column, applying its default when unset.
fn lookup_extra(kind: LookupKind, input: &LookupInput) -> Option<bool> {
    match kind {
        LookupKind::Provider => Some(input.is_internal.unwrap_or(false)),
        LookupKind::Status => Some(input.tracks_due.unwrap_or(true)),
        _ => None,
    }
}

impl SqliteStore {
    async fn get_lookup(&self, kind: LookupKind, id: Id) -> Result<LookupItem> {
        let sql = format!("{} WHERE id = ?1", lookup_select(kind));
        let row = sqlx::query(AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or(Error::NotFound {
                entity: "list item",
                id,
            })?;
        map_lookup(&row, kind)
    }
}

// ---------------------------------------------------------------------------
// Store implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl Store for SqliteStore {
    async fn info(&self) -> Result<StoreInfo> {
        let schema_version: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations WHERE success = 1",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        let asset_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM assets WHERE is_archived = 0")
                .fetch_one(&self.pool)
                .await
                .map_err(db)?;
        let calibration_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM calibration_records WHERE is_archived = 0")
                .fetch_one(&self.pool)
                .await
                .map_err(db)?;
        Ok(StoreInfo {
            backend: "sqlite".into(),
            location: self.path.display().to_string(),
            schema_version,
            asset_count,
            calibration_count,
        })
    }

    async fn list_lookups(
        &self,
        kind: LookupKind,
        include_inactive: bool,
    ) -> Result<Vec<LookupItem>> {
        let sql = format!(
            "{} WHERE (?1 = 1 OR is_active = 1) ORDER BY sort_order, name COLLATE NOCASE",
            lookup_select(kind)
        );
        sqlx::query(AssertSqlSafe(sql))
            .bind(include_inactive)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(|r| map_lookup(r, kind))
            .collect()
    }

    async fn create_lookup(&self, kind: LookupKind, mut input: LookupInput) -> Result<LookupItem> {
        input.normalize()?;
        let (table, extra_col) = lookup_table(kind);
        let extra = lookup_extra(kind, &input);
        let sql = match extra_col {
            Some(col) => format!(
                "INSERT INTO {table} (name, description, is_active, sort_order, created_at, updated_at, {col})
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6) RETURNING id"
            ),
            None => format!(
                "INSERT INTO {table} (name, description, is_active, sort_order, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5) RETURNING id"
            ),
        };
        let mut query = sqlx::query_scalar(AssertSqlSafe(sql))
            .bind(&input.name)
            .bind(&input.description)
            .bind(input.is_active)
            .bind(input.sort_order)
            .bind(Utc::now());
        if let Some(value) = extra {
            query = query.bind(value);
        }
        let id: Id = query
            .fetch_one(&self.pool)
            .await
            .map_err(|e| write_err(e, || format!("'{}' is already in this list.", input.name)))?;
        self.get_lookup(kind, id).await
    }

    async fn update_lookup(
        &self,
        kind: LookupKind,
        id: Id,
        mut input: LookupInput,
    ) -> Result<LookupItem> {
        input.normalize()?;
        let (table, extra_col) = lookup_table(kind);
        let extra = lookup_extra(kind, &input);
        let extra_set = extra_col.map(|c| format!(", {c} = ?7")).unwrap_or_default();
        let sql = format!(
            "UPDATE {table}
             SET name = ?2, description = ?3, is_active = ?4, sort_order = ?5, updated_at = ?6{extra_set}
             WHERE id = ?1"
        );
        let mut query = sqlx::query(AssertSqlSafe(sql))
            .bind(id)
            .bind(&input.name)
            .bind(&input.description)
            .bind(input.is_active)
            .bind(input.sort_order)
            .bind(Utc::now());
        if let Some(value) = extra {
            query = query.bind(value);
        }
        let result = query
            .execute(&self.pool)
            .await
            .map_err(|e| write_err(e, || format!("'{}' is already in this list.", input.name)))?;
        if result.rows_affected() == 0 {
            return Err(Error::NotFound {
                entity: "list item",
                id,
            });
        }
        self.get_lookup(kind, id).await
    }

    async fn list_assets(&self, filter: &AssetFilter) -> Result<Vec<Asset>> {
        const SQL: &str = concat!(
            asset_select!(),
            " WHERE (?1 IS NULL OR a.kind = ?1)
                AND (?2 IS NULL OR a.parent_id = ?2)
                AND (?3 = 1 OR a.is_archived = 0)
                AND (?4 IS NULL
                     OR a.asset_tag     LIKE ?4 ESCAPE '\\'
                     OR a.description   LIKE ?4 ESCAPE '\\'
                     OR a.model_number  LIKE ?4 ESCAPE '\\'
                     OR a.serial_number LIKE ?4 ESCAPE '\\')
              ORDER BY a.asset_tag COLLATE NOCASE"
        );
        let search = filter
            .search
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(like_pattern);
        let today = today();
        sqlx::query(SQL)
            .bind(filter.kind.map(AssetKind::as_str))
            .bind(filter.parent_id)
            .bind(filter.include_archived)
            .bind(search)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(|r| map_asset(r, today))
            .collect()
    }

    async fn get_asset(&self, id: Id) -> Result<Asset> {
        const SQL: &str = concat!(asset_select!(), " WHERE a.id = ?1");
        let row = sqlx::query(SQL)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or(Error::NotFound {
                entity: "asset",
                id,
            })?;
        map_asset(&row, today())
    }

    async fn create_asset(&self, mut input: AssetInput) -> Result<Asset> {
        input.normalize()?;
        let mut tx = self.begin().await?;
        check_hierarchy(&mut tx, None, &input).await?;

        let derived = derive_dates(
            input.first_cal_date,
            input.last_cal_date,
            &[],
            input.cal_interval,
        );
        let id: Id = sqlx::query_scalar(
            "INSERT INTO assets (
                asset_tag, kind, parent_id, description, manufacturer_id, model_number, serial_number,
                location_id, department_id, owner_id, status_id, warranty_end_date,
                cal_interval_value, cal_interval_unit, in_service_date,
                first_cal_date, last_cal_date, next_cal_date, notes, created_at, updated_at,
                last_cal_date_entered)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?20, ?21)
             RETURNING id",
        )
        .bind(&input.asset_tag)
        .bind(input.kind.as_str())
        .bind(input.parent_id)
        .bind(&input.description)
        .bind(input.manufacturer_id)
        .bind(&input.model_number)
        .bind(&input.serial_number)
        .bind(input.location_id)
        .bind(input.department_id)
        .bind(input.owner_id)
        .bind(input.status_id)
        .bind(input.warranty_end_date)
        .bind(input.cal_interval.map(|i| i64::from(i.value)))
        .bind(input.cal_interval.map(|i| i.unit.as_str()))
        .bind(input.in_service_date)
        .bind(derived.first_cal_date)
        .bind(derived.last_cal_date)
        .bind(derived.next_cal_date)
        .bind(&input.notes)
        .bind(Utc::now())
        .bind(input.last_cal_date)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| write_err(e, || format!("Asset tag '{}' is already in use.", input.asset_tag)))?;

        tx.commit().await.map_err(db)?;
        self.get_asset(id).await
    }

    async fn update_asset(
        &self,
        id: Id,
        expected_version: i64,
        mut input: AssetInput,
    ) -> Result<Asset> {
        input.normalize()?;
        let mut tx = self.begin().await?;
        check_hierarchy(&mut tx, Some(id), &input).await?;

        let result = sqlx::query(
            "UPDATE assets SET
                asset_tag = ?3, kind = ?4, parent_id = ?5, description = ?6, manufacturer_id = ?7,
                model_number = ?8, serial_number = ?9, location_id = ?10, department_id = ?11,
                owner_id = ?12, status_id = ?13, warranty_end_date = ?14,
                cal_interval_value = ?15, cal_interval_unit = ?16, in_service_date = ?17,
                first_cal_date = ?18, last_cal_date_entered = ?19, notes = ?20, updated_at = ?21,
                row_version = row_version + 1
             WHERE id = ?1 AND row_version = ?2",
        )
        .bind(id)
        .bind(expected_version)
        .bind(&input.asset_tag)
        .bind(input.kind.as_str())
        .bind(input.parent_id)
        .bind(&input.description)
        .bind(input.manufacturer_id)
        .bind(&input.model_number)
        .bind(&input.serial_number)
        .bind(input.location_id)
        .bind(input.department_id)
        .bind(input.owner_id)
        .bind(input.status_id)
        .bind(input.warranty_end_date)
        .bind(input.cal_interval.map(|i| i64::from(i.value)))
        .bind(input.cal_interval.map(|i| i.unit.as_str()))
        .bind(input.in_service_date)
        .bind(input.first_cal_date)
        .bind(input.last_cal_date)
        .bind(&input.notes)
        .bind(Utc::now())
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            write_err(e, || {
                format!("Asset tag '{}' is already in use.", input.asset_tag)
            })
        })?;

        if result.rows_affected() == 0 {
            return Err(missing_or_stale(&mut tx, "assets", "asset", id).await);
        }
        recompute_asset_dates(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        self.get_asset(id).await
    }

    async fn set_asset_archived(
        &self,
        id: Id,
        expected_version: i64,
        archived: bool,
    ) -> Result<Asset> {
        let mut tx = self.begin().await?;
        if archived {
            let children: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM assets WHERE parent_id = ?1 AND is_archived = 0",
            )
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
            if children > 0 {
                return Err(Error::validation(
                    "",
                    format!(
                        "This System still contains {children} active item(s). Archive or move them first."
                    ),
                ));
            }
        }
        let result = sqlx::query(
            "UPDATE assets SET is_archived = ?3, updated_at = ?4, row_version = row_version + 1
             WHERE id = ?1 AND row_version = ?2",
        )
        .bind(id)
        .bind(expected_version)
        .bind(archived)
        .bind(Utc::now())
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        if result.rows_affected() == 0 {
            return Err(missing_or_stale(&mut tx, "assets", "asset", id).await);
        }
        tx.commit().await.map_err(db)?;
        self.get_asset(id).await
    }

    async fn list_calibrations(
        &self,
        filter: &CalibrationFilter,
    ) -> Result<Vec<CalibrationRecord>> {
        const SQL: &str = concat!(
            calibration_select!(),
            " WHERE (?1 IS NULL OR r.asset_id = ?1) AND (?2 = 1 OR r.is_archived = 0)
              ORDER BY r.cal_date DESC, r.id DESC"
        );
        sqlx::query(SQL)
            .bind(filter.asset_id)
            .bind(filter.include_archived)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(map_calibration)
            .collect()
    }

    async fn get_calibration(&self, id: Id) -> Result<CalibrationRecord> {
        const SQL: &str = concat!(calibration_select!(), " WHERE r.id = ?1");
        let row = sqlx::query(SQL)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or(Error::NotFound {
                entity: "calibration record",
                id,
            })?;
        map_calibration(&row)
    }

    async fn create_calibration(&self, mut input: CalibrationInput) -> Result<CalibrationRecord> {
        input.normalize(today())?;
        let mut tx = self.begin().await?;
        let next_due = due_date_for(&mut tx, input.asset_id, input.cal_date).await?;

        let id: Id = sqlx::query_scalar(
            "INSERT INTO calibration_records (
                asset_id, cal_date, provider_id, technician, cost_cents, certificate_number,
                condition_received, condition_returned, next_due_date, notes, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
             RETURNING id",
        )
        .bind(input.asset_id)
        .bind(input.cal_date)
        .bind(input.provider_id)
        .bind(&input.technician)
        .bind(input.cost_cents)
        .bind(&input.certificate_number)
        .bind(input.condition_received.as_str())
        .bind(input.condition_returned.as_str())
        .bind(next_due)
        .bind(&input.notes)
        .bind(Utc::now())
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| write_err(e, || "This calibration record already exists.".into()))?;

        recompute_asset_dates(&mut tx, input.asset_id).await?;
        tx.commit().await.map_err(db)?;
        self.get_calibration(id).await
    }

    async fn update_calibration(
        &self,
        id: Id,
        expected_version: i64,
        mut input: CalibrationInput,
    ) -> Result<CalibrationRecord> {
        input.normalize(today())?;
        let mut tx = self.begin().await?;
        let existing_asset = calibration_asset_id(&mut tx, id).await?;
        if existing_asset != input.asset_id {
            return Err(Error::validation(
                "asset_id",
                "A calibration record cannot be moved to a different item. Archive it and create a new one.",
            ));
        }
        let next_due = due_date_for(&mut tx, input.asset_id, input.cal_date).await?;

        let result = sqlx::query(
            "UPDATE calibration_records SET
                cal_date = ?3, provider_id = ?4, technician = ?5, cost_cents = ?6,
                certificate_number = ?7, condition_received = ?8, condition_returned = ?9,
                next_due_date = ?10, notes = ?11, updated_at = ?12, row_version = row_version + 1
             WHERE id = ?1 AND row_version = ?2",
        )
        .bind(id)
        .bind(expected_version)
        .bind(input.cal_date)
        .bind(input.provider_id)
        .bind(&input.technician)
        .bind(input.cost_cents)
        .bind(&input.certificate_number)
        .bind(input.condition_received.as_str())
        .bind(input.condition_returned.as_str())
        .bind(next_due)
        .bind(&input.notes)
        .bind(Utc::now())
        .execute(&mut *tx)
        .await
        .map_err(|e| write_err(e, || "This calibration record already exists.".into()))?;

        if result.rows_affected() == 0 {
            return Err(
                missing_or_stale(&mut tx, "calibration_records", "calibration record", id).await,
            );
        }
        recompute_asset_dates(&mut tx, input.asset_id).await?;
        tx.commit().await.map_err(db)?;
        self.get_calibration(id).await
    }

    async fn set_calibration_archived(
        &self,
        id: Id,
        expected_version: i64,
        archived: bool,
    ) -> Result<CalibrationRecord> {
        let mut tx = self.begin().await?;
        let asset_id = calibration_asset_id(&mut tx, id).await?;
        let result = sqlx::query(
            "UPDATE calibration_records SET is_archived = ?3, updated_at = ?4, row_version = row_version + 1
             WHERE id = ?1 AND row_version = ?2",
        )
        .bind(id)
        .bind(expected_version)
        .bind(archived)
        .bind(Utc::now())
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        if result.rows_affected() == 0 {
            return Err(
                missing_or_stale(&mut tx, "calibration_records", "calibration record", id).await,
            );
        }
        recompute_asset_dates(&mut tx, asset_id).await?;
        tx.commit().await.map_err(db)?;
        self.get_calibration(id).await
    }
}
