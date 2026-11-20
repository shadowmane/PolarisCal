//! The one [`Store`] implementation, shared by every database backend.

use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Local, NaiveDate, SubsecRound, Utc};
use polaris_core::audit::{
    Actor, AuditEntry, AuditFilter, FieldChange, NewAuditEntry, WriteContext, asset_snapshot,
    attachment_snapshot, calibration_snapshot, created, diff, lookup_snapshot, user_snapshot,
};
use polaris_core::auth::{
    NewUser, User, UserCredentials, UserUpdate, check_password_rules, hash_password,
};
use polaris_core::model::{
    AppSettings, Asset, AssetFilter, AssetInput, AssetKind, Attachment, AttachmentFilter,
    AuditAction, AuditEntity, CalInterval, CalibrationFilter, CalibrationInput, CalibrationRecord,
    LookupInput, LookupItem, LookupKind, NewAttachment, Role, StoreInfo,
};
use polaris_core::schedule::{add_interval, derive_dates, due_status};
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

fn interval_from(value: Option<i64>, unit: Option<String>) -> Result<Option<CalInterval>> {
    match (value, unit) {
        (Some(v), Some(u)) => Ok(Some(CalInterval {
            value: u32::try_from(v).map_err(|_| Error::Database(format!("bad interval {v}")))?,
            unit: u.parse()?,
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
                a.owner_id, o.name AS owner_name,
                a.status_id, s.name AS status_name, s.tracks_due AS status_tracks_due,
                a.warranty_end_date, a.cal_interval_value, a.cal_interval_unit,
                a.in_service_date, a.first_cal_date, a.last_cal_date, a.last_cal_date_entered, a.next_cal_date,
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

fn map_asset(row: &Row, today: NaiveDate) -> Result<Asset> {
    let next_cal_date: Option<NaiveDate> = row.get("next_cal_date")?;
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
        status_id: row.get("status_id")?,
        status_name: row.get("status_name")?,
        warranty_end_date: row.get("warranty_end_date")?,
        cal_interval: interval_from(
            row.get("cal_interval_value")?,
            row.get("cal_interval_unit")?,
        )?,
        in_service_date: row.get("in_service_date")?,
        first_cal_date: row.get("first_cal_date")?,
        last_cal_date: row.get("last_cal_date")?,
        last_cal_date_entered: row.get("last_cal_date_entered")?,
        next_cal_date,
        due_status: due_status(next_cal_date, tracks_due, today),
        child_count: row.get("child_count")?,
        notes: row.get("notes")?,
        is_archived: row.get("is_archived")?,
        row_version: row.get("row_version")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
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
            "SELECT first_cal_date, last_cal_date_entered, cal_interval_value, cal_interval_unit
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
            "SELECT cal_date FROM calibration_records WHERE asset_id = ?1 AND is_archived = FALSE",
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
        interval_from(
            row.get("cal_interval_value")?,
            row.get("cal_interval_unit")?,
        )?,
    );

    conn.execute(
        "UPDATE assets SET first_cal_date = ?2, last_cal_date = ?3, next_cal_date = ?4 WHERE id = ?1",
        params![
            asset_id,
            derived.first_cal_date,
            derived.last_cal_date,
            derived.next_cal_date
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
        "SELECT r.id, r.asset_id, a.asset_tag, r.cal_date, r.provider_id, p.name AS provider_name,
                r.technician, r.cost_cents, r.certificate_number,
                r.condition_received, r.condition_returned, r.next_due_date, r.notes,
                (SELECT COUNT(*) FROM attachments f
                  WHERE f.calibration_id = r.id AND f.is_archived = FALSE) AS attachment_count,
                r.is_archived, r.row_version, r.created_at, r.updated_at
         FROM calibration_records r
         JOIN assets a                ON a.id = r.asset_id
         JOIN calibration_providers p ON p.id = r.provider_id"
    };
}

fn map_calibration(row: &Row) -> Result<CalibrationRecord> {
    Ok(CalibrationRecord {
        id: row.get("id")?,
        asset_id: row.get("asset_id")?,
        asset_tag: row.get("asset_tag")?,
        cal_date: row.get("cal_date")?,
        provider_id: row.get("provider_id")?,
        provider_name: row.get("provider_name")?,
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
async fn due_date_for(
    conn: &mut Conn,
    asset_id: Id,
    cal_date: NaiveDate,
) -> Result<Option<NaiveDate>> {
    let row = conn
        .fetch_optional(
            "SELECT cal_interval_value, cal_interval_unit, is_archived FROM assets WHERE id = ?1",
            params![asset_id],
        )
        .await?
        .ok_or_else(|| Error::validation("asset_id", "The selected item does not exist."))?;
    if row.get::<bool>("is_archived")? {
        return Err(Error::validation(
            "asset_id",
            "The selected item is archived.",
        ));
    }
    let interval = interval_from(
        row.get("cal_interval_value")?,
        row.get("cal_interval_unit")?,
    )?;
    Ok(interval.and_then(|i| add_interval(cal_date, i)))
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
                            last_sign_in_at, row_version, created_at, updated_at";

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
                            must_change_password, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
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
// Attachments
// ---------------------------------------------------------------------------

macro_rules! attachment_select {
    () => {
        "SELECT f.id, f.asset_id, a.asset_tag, f.calibration_id, r.cal_date AS calibration_date,
                f.category, f.file_name, f.relative_path, f.size_bytes, f.sha256, f.content_type,
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
        input.normalize()?;
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
        input.normalize()?;
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
        let mut tx = self.begin().await?;
        let before = fetch_user(&mut tx, user_id).await?;
        check_password_rules(&before.username, new_password)?;
        let own = ctx.actor.user_id == user_id;
        let must_change = !own && must_change;
        tx.execute(
            "UPDATE users SET password_hash = ?2, must_change_password = ?3, updated_at = ?4,
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
        if before.attachments_root != settings.attachments_root {
            changes.push(FieldChange {
                field: "Attachments folder".into(),
                old: before.attachments_root.clone(),
                new: settings.attachments_root.clone(),
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
            "{} WHERE (?1 = TRUE OR is_active = TRUE) ORDER BY sort_order, LOWER(name), id",
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
        let (table, extra_col) = lookup_table(kind);
        let extra = lookup_extra(kind, &input);
        let sql = match extra_col {
            Some(col) => format!(
                "INSERT INTO {table} (name, description, is_active, sort_order, created_at, updated_at, {col})
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)"
            ),
            None => format!(
                "INSERT INTO {table} (name, description, is_active, sort_order, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)"
            ),
        };
        let mut values = params![
            &input.name,
            &input.description,
            input.is_active,
            input.sort_order,
            now()
        ];
        if let Some(value) = extra {
            values.push(Value::from(value));
        }
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
        let (table, extra_col) = lookup_table(kind);
        let extra = lookup_extra(kind, &input);
        let extra_set = extra_col.map(|c| format!(", {c} = ?7")).unwrap_or_default();
        let sql = format!(
            "UPDATE {table}
             SET name = ?2, description = ?3, is_active = ?4, sort_order = ?5, updated_at = ?6{extra_set}
             WHERE id = ?1"
        );
        let mut tx = self.begin().await?;
        let before = fetch_lookup(&mut tx, kind, id).await?;
        let mut values = params![
            id,
            &input.name,
            &input.description,
            input.is_active,
            input.sort_order,
            now()
        ];
        if let Some(value) = extra {
            values.push(Value::from(value));
        }
        tx.execute(&sql, values)
            .await
            .map_err(|e| write_err(e, || format!("'{}' is already in this list.", input.name)))?;
        let after = fetch_lookup(&mut tx, kind, id).await?;
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
                    last_cal_date_entered)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?20, ?21)",
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
                    input.last_cal_date
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
        ctx.require_reason()?;
        let mut tx = self.begin().await?;
        check_hierarchy(&mut tx, Some(id), &input).await?;
        let before = fetch_asset(&mut tx, id).await?;

        let changed = tx
            .execute(
                "UPDATE assets SET
                    asset_tag = ?3, kind = ?4, parent_id = ?5, description = ?6, manufacturer_id = ?7,
                    model_number = ?8, serial_number = ?9, location_id = ?10, department_id = ?11,
                    owner_id = ?12, status_id = ?13, warranty_end_date = ?14,
                    cal_interval_value = ?15, cal_interval_unit = ?16, in_service_date = ?17,
                    first_cal_date = ?18, last_cal_date_entered = ?19, notes = ?20, updated_at = ?21,
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
                    now()
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
        tx.commit().await?;
        Ok(after)
    }

    async fn set_asset_archived(
        &self,
        ctx: &WriteContext,
        id: Id,
        expected_version: i64,
        archived: bool,
    ) -> Result<Asset> {
        ctx.require_reason()?;
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
            " WHERE (?1 IS NULL OR f.asset_id = ?1)
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
                    filter.include_archived
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
                                          uploaded_by_id, uploaded_by, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
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
                    now()
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
        ctx.require_reason()?;
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
              ORDER BY r.cal_date DESC, r.id DESC"
        );
        self.conn()
            .await?
            .fetch_all(SQL, params![filter.asset_id, filter.include_archived])
            .await?
            .iter()
            .map(map_calibration)
            .collect()
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
        let next_due = due_date_for(&mut tx, input.asset_id, input.cal_date).await?;

        let id = tx
            .insert(
                "INSERT INTO calibration_records (
                    asset_id, cal_date, provider_id, technician, cost_cents, certificate_number,
                    condition_received, condition_returned, next_due_date, notes, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
                params![
                    input.asset_id,
                    input.cal_date,
                    input.provider_id,
                    &input.technician,
                    input.cost_cents,
                    &input.certificate_number,
                    input.condition_received.as_str(),
                    input.condition_returned.as_str(),
                    next_due,
                    &input.notes,
                    now()
                ],
            )
            .await
            .map_err(|e| write_err(e, || "This calibration record already exists.".into()))?;

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
        ctx.require_reason()?;
        let mut tx = self.begin().await?;
        let before = fetch_calibration(&mut tx, id).await?;
        if before.asset_id != input.asset_id {
            return Err(Error::validation(
                "asset_id",
                "A calibration record cannot be moved to a different item. Archive it and create a new one.",
            ));
        }
        let next_due = due_date_for(&mut tx, input.asset_id, input.cal_date).await?;

        let changed = tx
            .execute(
                "UPDATE calibration_records SET
                    cal_date = ?3, provider_id = ?4, technician = ?5, cost_cents = ?6,
                    certificate_number = ?7, condition_received = ?8, condition_returned = ?9,
                    next_due_date = ?10, notes = ?11, updated_at = ?12, row_version = row_version + 1
                 WHERE id = ?1 AND row_version = ?2",
                params![
                    id,
                    expected_version,
                    input.cal_date,
                    input.provider_id,
                    &input.technician,
                    input.cost_cents,
                    &input.certificate_number,
                    input.condition_received.as_str(),
                    input.condition_returned.as_str(),
                    next_due,
                    &input.notes,
                    now()
                ],
            )
            .await
            .map_err(|e| write_err(e, || "This calibration record already exists.".into()))?;

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
        ctx.require_reason()?;
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
