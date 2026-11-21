//! SQL Server support via tiberius: a small connection pool, value conversion,
//! and a schema-migration runner (sqlx, used for the other backends, has no SQL
//! Server driver).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use tiberius::{Client, ColumnData, Config, FromSql, Query, SqlBrowser};
use tokio::net::TcpStream;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

use super::{DbError, Value};

type TdsClient = Client<Compat<TcpStream>>;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// Settings applied to every new connection. XACT_ABORT makes any error roll back
/// the whole transaction; the ANSI options are what indexes and triggers expect.
const SESSION_SETUP: &str = "SET XACT_ABORT ON; SET ANSI_NULLS ON; SET ANSI_PADDING ON; \
     SET ANSI_WARNINGS ON; SET ARITHABORT ON; SET CONCAT_NULL_YIELDS_NULL ON; \
     SET QUOTED_IDENTIFIER ON; SET NUMERIC_ROUNDABORT OFF;";

/// Opens one connection, resolving a named instance through SQL Server Browser.
pub(crate) async fn connect(config: &Config, named_instance: bool) -> Result<TdsClient, DbError> {
    let attempt = async {
        let tcp = if named_instance {
            TcpStream::connect_named(config).await?
        } else {
            TcpStream::connect(config.get_addr())
                .await
                .map_err(|e| DbError::other(format!("cannot reach {}: {e}", config.get_addr())))?
        };
        let _ = tcp.set_nodelay(true);
        let mut client = match Client::connect(config.clone(), tcp.compat_write()).await {
            Ok(client) => client,
            // Azure SQL and availability groups may redirect to another server.
            Err(tiberius::error::Error::Routing { host, port }) => {
                let mut redirected = config.clone();
                redirected.host(&host);
                redirected.port(port);
                let tcp = TcpStream::connect(redirected.get_addr())
                    .await
                    .map_err(|e| DbError::other(format!("cannot reach {host}:{port}: {e}")))?;
                Client::connect(redirected, tcp.compat_write()).await?
            }
            Err(e) => return Err(e.into()),
        };
        client
            .simple_query(SESSION_SETUP)
            .await?
            .into_results()
            .await?;
        Ok(client)
    };
    tokio::time::timeout(CONNECT_TIMEOUT, attempt)
        .await
        .map_err(|_| DbError::other("timed out connecting to SQL Server"))?
}

/// A minimal pool: idle connections are reused; a connection that was left inside
/// a transaction or hit a network error is closed instead of returned.
pub(crate) struct Pool {
    config: Config,
    named_instance: bool,
    idle: Mutex<Vec<TdsClient>>,
    permits: Arc<Semaphore>,
}

impl Pool {
    pub fn new(config: Config, named_instance: bool, max_connections: usize) -> Arc<Self> {
        Arc::new(Self {
            config,
            named_instance,
            idle: Mutex::new(Vec::new()),
            permits: Arc::new(Semaphore::new(max_connections)),
        })
    }

    fn idle(&self) -> std::sync::MutexGuard<'_, Vec<TdsClient>> {
        self.idle.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub async fn get(self: &Arc<Self>) -> Result<PooledConn, DbError> {
        let permit = tokio::time::timeout(CONNECT_TIMEOUT, self.permits.clone().acquire_owned())
            .await
            .map_err(|_| DbError::other("timed out waiting for a SQL Server connection"))?
            .map_err(|_| DbError::other("the SQL Server connection pool is closed"))?;
        let reused = self.idle().pop();
        let client = match reused {
            Some(client) => client,
            None => connect(&self.config, self.named_instance).await?,
        };
        Ok(PooledConn {
            client: Some(client),
            pool: Arc::clone(self),
            _permit: permit,
            in_transaction: false,
            broken: false,
        })
    }

    pub fn close(&self) {
        self.permits.close();
        self.idle().clear();
    }
}

pub(crate) struct PooledConn {
    client: Option<TdsClient>,
    pool: Arc<Pool>,
    _permit: OwnedSemaphorePermit,
    in_transaction: bool,
    broken: bool,
}

impl Drop for PooledConn {
    fn drop(&mut self) {
        // Closing a connection that's still in a transaction rolls it back on the server.
        if let Some(client) = self.client.take()
            && !self.in_transaction
            && !self.broken
            && !self.pool.permits.is_closed()
        {
            self.pool.idle().push(client);
        }
    }
}

impl PooledConn {
    fn client(&mut self) -> &mut TdsClient {
        self.client.as_mut().expect("connection present until drop")
    }

    /// Remembers whether an error left the connection unusable.
    fn track<T>(&mut self, result: Result<T, tiberius::error::Error>) -> Result<T, DbError> {
        if let Err(e) = &result
            && !matches!(e, tiberius::error::Error::Server(_))
        {
            self.broken = true;
        }
        result.map_err(DbError::from)
    }

    pub async fn begin(&mut self) -> Result<(), DbError> {
        let r = self.client().begin_transaction().await;
        self.track(r)?;
        self.in_transaction = true;
        Ok(())
    }

    pub async fn commit(mut self) -> Result<(), DbError> {
        if self.in_transaction {
            let r = self.client().commit_transaction().await;
            self.track(r)?;
            self.in_transaction = false;
        }
        Ok(())
    }

    fn build<'a>(sql: &'a str, params: Vec<Value>) -> Query<'a> {
        let mut q = Query::new(sql);
        for v in params {
            match v {
                Value::Bool(b) => q.bind(b),
                Value::Int(i) => q.bind(i),
                Value::Text(s) => q.bind(s),
                Value::Date(d) => q.bind(d),
                Value::Time(t) => q.bind(t),
            }
        }
        q
    }

    pub async fn query(
        &mut self,
        sql: &str,
        params: Vec<Value>,
    ) -> Result<Vec<tiberius::Row>, DbError> {
        let q = Self::build(sql, params);
        let result = match q.query(self.client()).await {
            Ok(stream) => stream.into_results().await,
            Err(e) => Err(e),
        };
        Ok(self.track(result)?.into_iter().flatten().collect())
    }

    pub async fn execute(&mut self, sql: &str, params: Vec<Value>) -> Result<u64, DbError> {
        let q = Self::build(sql, params);
        let result = q.execute(self.client()).await.map(|r| r.total());
        self.track(result)
    }

    /// Runs a batch with no parameters, outside `sp_executesql`.
    pub async fn simple(&mut self, sql: &str) -> Result<(), DbError> {
        let result = match self.client().simple_query(sql).await {
            Ok(stream) => stream.into_results().await.map(|_| ()),
            Err(e) => Err(e),
        };
        self.track(result)
    }
}

// ---------------------------------------------------------------------------
// Reading values
// ---------------------------------------------------------------------------

fn cell<'a>(row: &'a tiberius::Row, col: &str) -> Result<&'a ColumnData<'static>, DbError> {
    row.cells()
        .find(|(c, _)| c.name().eq_ignore_ascii_case(col))
        .map(|(_, data)| data)
        .ok_or_else(|| DbError::other(format!("no column named {col}")))
}

fn mismatch(col: &str, data: &ColumnData<'_>) -> DbError {
    DbError::other(format!("column {col} has unexpected type {data:?}"))
}

pub(crate) fn read_int(row: &tiberius::Row, col: &str) -> Result<Option<i64>, DbError> {
    Ok(match cell(row, col)? {
        ColumnData::I64(v) => *v,
        ColumnData::I32(v) => v.map(i64::from),
        ColumnData::I16(v) => v.map(i64::from),
        ColumnData::U8(v) => v.map(i64::from),
        ColumnData::Bit(v) => v.map(i64::from),
        other => return Err(mismatch(col, other)),
    })
}

pub(crate) fn read_bool(row: &tiberius::Row, col: &str) -> Result<Option<bool>, DbError> {
    Ok(match cell(row, col)? {
        ColumnData::Bit(v) => *v,
        ColumnData::I32(v) => v.map(|i| i != 0),
        ColumnData::I64(v) => v.map(|i| i != 0),
        ColumnData::U8(v) => v.map(|i| i != 0),
        other => return Err(mismatch(col, other)),
    })
}

pub(crate) fn read_text(row: &tiberius::Row, col: &str) -> Result<Option<String>, DbError> {
    Ok(match cell(row, col)? {
        ColumnData::String(v) => v.as_ref().map(|s| s.to_string()),
        // A NULL literal (e.g. `NULL AS x`) arrives as an integer column.
        ColumnData::I32(None) => None,
        other => return Err(mismatch(col, other)),
    })
}

pub(crate) fn read_date(row: &tiberius::Row, col: &str) -> Result<Option<NaiveDate>, DbError> {
    let data = cell(row, col)?;
    if matches!(data, ColumnData::I32(None)) {
        return Ok(None);
    }
    Ok(NaiveDate::from_sql(data)?)
}

pub(crate) fn read_time(row: &tiberius::Row, col: &str) -> Result<Option<DateTime<Utc>>, DbError> {
    let data = cell(row, col)?;
    if matches!(data, ColumnData::I32(None)) {
        return Ok(None);
    }
    Ok(DateTime::<Utc>::from_sql(data)?)
}

// ---------------------------------------------------------------------------
// Schema migrations
// ---------------------------------------------------------------------------

/// SQL Server migrations: (version, description, script). Versions match the other
/// backends' so "schema version" means the same thing everywhere. Scripts are split
/// into batches on lines containing only `GO`.
const MIGRATIONS: &[(i64, &str, &str)] = &[
    (
        5,
        "polariscal schema",
        include_str!("../../migrations/mssql/0005_polariscal_schema.sql"),
    ),
    (
        6,
        "interval end of month",
        include_str!("../../migrations/mssql/0006_interval_end_of_month.sql"),
    ),
    (
        7,
        "calibrated by manufacturer",
        include_str!("../../migrations/mssql/0007_calibrated_by_manufacturer.sql"),
    ),
    (
        8,
        "service types",
        include_str!("../../migrations/mssql/0008_service_types.sql"),
    ),
];

pub(crate) const VERSION_TABLE: &str = "polaris_schema_migrations";

fn batches(script: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for line in script.lines() {
        if line.trim().eq_ignore_ascii_case("GO") {
            if !current.trim().is_empty() {
                out.push(std::mem::take(&mut current));
            }
            current.clear();
        } else {
            current.push_str(line);
            current.push('\n');
        }
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

/// Brings the schema up to date, one transaction per migration, holding an
/// application lock so two computers starting at once don't both apply it.
pub(crate) async fn migrate(pool: &Arc<Pool>) -> Result<(), DbError> {
    let mut conn = pool.get().await?;
    let create = format!(
        "IF OBJECT_ID(N'{VERSION_TABLE}', N'U') IS NULL
         CREATE TABLE {VERSION_TABLE} (
             version      BIGINT        NOT NULL PRIMARY KEY,
             description  NVARCHAR(200) NOT NULL,
             installed_on DATETIME2     NOT NULL DEFAULT SYSUTCDATETIME()
         )"
    );
    if let Err(e) = conn.simple(&create).await
        && !e.message.contains("already an object named")
    {
        return Err(e);
    }
    for (version, description, script) in MIGRATIONS {
        conn.begin().await?;
        conn.simple(
            "DECLARE @r INT; EXEC @r = sp_getapplock @Resource = 'polariscal_schema', \
             @LockMode = 'Exclusive', @LockOwner = 'Transaction', @LockTimeout = 60000; \
             IF @r < 0 THROW 50001, 'Another computer is upgrading the database. Try again shortly.', 1;",
        )
        .await?;
        let done = conn
            .query(
                &format!("SELECT COUNT(*) AS n FROM {VERSION_TABLE} WHERE version = @P1"),
                vec![Value::Int(Some(*version))],
            )
            .await?;
        let applied = done
            .first()
            .map(|r| read_int(r, "n"))
            .transpose()?
            .flatten()
            .unwrap_or(0);
        if applied == 0 {
            for batch in batches(script) {
                conn.simple(&batch).await?;
            }
            conn.execute(
                &format!("INSERT INTO {VERSION_TABLE} (version, description) VALUES (@P1, @P2)"),
                vec![
                    Value::Int(Some(*version)),
                    Value::Text(Some((*description).to_string())),
                ],
            )
            .await?;
        }
        // Commit consumes the connection, so take a fresh one for the next migration.
        conn.commit().await?;
        conn = pool.get().await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_split_on_go_lines() {
        let parts = batches(
            "CREATE TABLE a (x INT);\nGO\n  go  \nCREATE TRIGGER t ON a AFTER UPDATE AS SELECT 1;\nGO\n",
        );
        assert_eq!(parts.len(), 2);
        assert!(parts[0].starts_with("CREATE TABLE a"));
        assert!(parts[1].starts_with("CREATE TRIGGER"));
    }
}
