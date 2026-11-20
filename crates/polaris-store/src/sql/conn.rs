//! Connection pools, connections (optionally inside a transaction), and rows for
//! every backend, behind one enum-based interface.

use std::sync::Arc;

use chrono::{DateTime, NaiveDate, Utc};
use polaris_core::{Error, Id, Result};
use sqlx::mysql::{MySqlPool, MySqlRow};
use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgPool, PgRow};
use sqlx::sqlite::{SqlitePool, SqliteRow};
use sqlx::{AssertSqlSafe, Column as _, MySql, Postgres, Row as _, Sqlite, Transaction};

use super::{DbError, Dialect, Rewritten, Value, mssql};

/// A pool of connections to one database.
pub(crate) enum Pool {
    Sqlite(SqlitePool),
    Postgres(PgPool),
    MySql(MySqlPool),
    Mssql(Arc<mssql::Pool>),
}

impl Pool {
    pub fn dialect(&self) -> Dialect {
        match self {
            Pool::Sqlite(_) => Dialect::Sqlite,
            Pool::Postgres(_) => Dialect::Postgres,
            Pool::MySql(_) => Dialect::MySql,
            Pool::Mssql(_) => Dialect::Mssql,
        }
    }

    /// A connection inside a new transaction. Dropping it without [`Conn::commit`]
    /// rolls the transaction back.
    pub async fn begin(&self) -> Result<Conn> {
        Ok(match self {
            Pool::Sqlite(p) => Conn::Sqlite(Handle::Tx(p.begin().await.map_err(DbError::from)?)),
            Pool::Postgres(p) => {
                Conn::Postgres(Handle::Tx(p.begin().await.map_err(DbError::from)?))
            }
            Pool::MySql(p) => Conn::MySql(Handle::Tx(p.begin().await.map_err(DbError::from)?)),
            Pool::Mssql(p) => {
                let mut conn = p.get().await?;
                conn.begin().await?;
                Conn::Mssql(Box::new(conn))
            }
        })
    }

    /// A connection for reads that don't need a transaction.
    pub async fn acquire(&self) -> Result<Conn> {
        Ok(match self {
            Pool::Sqlite(p) => {
                Conn::Sqlite(Handle::Plain(p.acquire().await.map_err(DbError::from)?))
            }
            Pool::Postgres(p) => {
                Conn::Postgres(Handle::Plain(p.acquire().await.map_err(DbError::from)?))
            }
            Pool::MySql(p) => Conn::MySql(Handle::Plain(p.acquire().await.map_err(DbError::from)?)),
            Pool::Mssql(p) => Conn::Mssql(Box::new(p.get().await?)),
        })
    }

    pub async fn close(&self) {
        match self {
            Pool::Sqlite(p) => p.close().await,
            Pool::Postgres(p) => p.close().await,
            Pool::MySql(p) => p.close().await,
            Pool::Mssql(p) => p.close(),
        }
    }
}

pub(crate) enum Handle<DB: sqlx::Database> {
    Tx(Transaction<'static, DB>),
    Plain(PoolConnection<DB>),
}

impl<DB: sqlx::Database> Handle<DB> {
    fn conn(&mut self) -> &mut DB::Connection {
        match self {
            Handle::Tx(tx) => tx,
            Handle::Plain(c) => c,
        }
    }

    async fn commit(self) -> std::result::Result<(), DbError> {
        match self {
            Handle::Tx(tx) => tx.commit().await.map_err(DbError::from),
            Handle::Plain(_) => Ok(()),
        }
    }
}

/// One database connection, possibly inside a transaction.
pub(crate) enum Conn {
    Sqlite(Handle<Sqlite>),
    Postgres(Handle<Postgres>),
    MySql(Handle<MySql>),
    /// Boxed: a SQL Server connection is much larger than the others.
    Mssql(Box<mssql::PooledConn>),
}

/// The parameters in the order the rewritten SQL expects them.
fn ordered(rw: &Rewritten, params: Vec<Value>) -> Vec<Value> {
    match &rw.order {
        None => params,
        Some(order) => order.iter().map(|&i| params[i].clone()).collect(),
    }
}

/// Builds a sqlx query with the parameters bound.
macro_rules! sqlx_query {
    ($sql:expr, $params:expr) => {{
        let mut q = sqlx::query(AssertSqlSafe($sql));
        for v in $params {
            q = match v {
                Value::Bool(b) => q.bind(b),
                Value::Int(i) => q.bind(i),
                Value::Text(s) => q.bind(s),
                Value::Date(d) => q.bind(d),
                Value::Time(t) => q.bind(t),
            };
        }
        q
    }};
}

impl Conn {
    pub fn dialect(&self) -> Dialect {
        match self {
            Conn::Sqlite(_) => Dialect::Sqlite,
            Conn::Postgres(_) => Dialect::Postgres,
            Conn::MySql(_) => Dialect::MySql,
            Conn::Mssql(_) => Dialect::Mssql,
        }
    }

    /// Runs a query written in portable SQL and returns every row.
    pub async fn fetch_all(
        &mut self,
        sql: &str,
        params: Vec<Value>,
    ) -> std::result::Result<Vec<Row>, DbError> {
        let rw = self.dialect().rewrite(sql);
        let params = ordered(&rw, params);
        Ok(match self {
            Conn::Sqlite(h) => sqlx_query!(rw.sql, params)
                .fetch_all(h.conn())
                .await?
                .into_iter()
                .map(Row::Sqlite)
                .collect(),
            Conn::Postgres(h) => sqlx_query!(rw.sql, params)
                .fetch_all(h.conn())
                .await?
                .into_iter()
                .map(Row::Postgres)
                .collect(),
            Conn::MySql(h) => sqlx_query!(rw.sql, params)
                .fetch_all(h.conn())
                .await?
                .into_iter()
                .map(Row::MySql)
                .collect(),
            Conn::Mssql(c) => c
                .query(&rw.sql, params)
                .await?
                .into_iter()
                .map(Row::Mssql)
                .collect(),
        })
    }

    pub async fn fetch_optional(
        &mut self,
        sql: &str,
        params: Vec<Value>,
    ) -> std::result::Result<Option<Row>, DbError> {
        Ok(self.fetch_all(sql, params).await?.into_iter().next())
    }

    /// The first column of the first row as an integer (for `COUNT(*)` and the like).
    pub async fn scalar(&mut self, sql: &str, params: Vec<Value>) -> Result<i64> {
        let row = self
            .fetch_optional(sql, params)
            .await?
            .ok_or_else(|| Error::Database("the query returned no rows".into()))?;
        row.int_at(0)
    }

    /// Runs a statement and returns how many rows it changed.
    pub async fn execute(
        &mut self,
        sql: &str,
        params: Vec<Value>,
    ) -> std::result::Result<u64, DbError> {
        let rw = self.dialect().rewrite(sql);
        let params = ordered(&rw, params);
        Ok(match self {
            Conn::Sqlite(h) => sqlx_query!(rw.sql, params)
                .execute(h.conn())
                .await?
                .rows_affected(),
            Conn::Postgres(h) => sqlx_query!(rw.sql, params)
                .execute(h.conn())
                .await?
                .rows_affected(),
            Conn::MySql(h) => sqlx_query!(rw.sql, params)
                .execute(h.conn())
                .await?
                .rows_affected(),
            Conn::Mssql(c) => c.execute(&rw.sql, params).await?,
        })
    }

    /// Runs an `INSERT` into a table with an `id` identity column and returns the new id.
    pub async fn insert(
        &mut self,
        sql: &str,
        params: Vec<Value>,
    ) -> std::result::Result<Id, DbError> {
        let missing = || DbError::other("the database did not return the new row's id");
        match self.dialect() {
            Dialect::Sqlite | Dialect::Postgres => {
                let row = self
                    .fetch_optional(&format!("{sql} RETURNING id"), params)
                    .await?
                    .ok_or_else(missing)?;
                row.int_at(0).map_err(|e| DbError::other(e.to_string()))
            }
            Dialect::Mssql => {
                let sql = format!("{sql}; SELECT CAST(SCOPE_IDENTITY() AS BIGINT) AS id");
                let row = self
                    .fetch_optional(&sql, params)
                    .await?
                    .ok_or_else(missing)?;
                row.int_at(0).map_err(|e| DbError::other(e.to_string()))
            }
            Dialect::MySql => {
                let rw = Dialect::MySql.rewrite(sql);
                let params = ordered(&rw, params);
                let Conn::MySql(h) = self else { unreachable!() };
                let result = sqlx_query!(rw.sql, params).execute(h.conn()).await?;
                i64::try_from(result.last_insert_id()).map_err(|_| missing())
            }
        }
    }

    /// Runs SQL with no parameters as a plain batch. On SQL Server this keeps session
    /// settings such as `SET IDENTITY_INSERT` in effect for later statements.
    pub async fn batch(&mut self, sql: &str) -> std::result::Result<(), DbError> {
        use sqlx::Executor as _;
        match self {
            Conn::Sqlite(h) => {
                h.conn().execute(AssertSqlSafe(sql.to_string())).await?;
            }
            Conn::Postgres(h) => {
                h.conn().execute(AssertSqlSafe(sql.to_string())).await?;
            }
            Conn::MySql(h) => {
                h.conn().execute(AssertSqlSafe(sql.to_string())).await?;
            }
            Conn::Mssql(c) => c.simple(sql).await?,
        }
        Ok(())
    }

    /// Commits the transaction (a no-op for a plain connection).
    pub async fn commit(self) -> std::result::Result<(), DbError> {
        match self {
            Conn::Sqlite(h) => h.commit().await,
            Conn::Postgres(h) => h.commit().await,
            Conn::MySql(h) => h.commit().await,
            Conn::Mssql(c) => (*c).commit().await,
        }
    }
}

/// A result row from any backend.
pub(crate) enum Row {
    Sqlite(SqliteRow),
    Postgres(PgRow),
    MySql(MySqlRow),
    Mssql(tiberius::Row),
}

/// Reads an integer column, accepting any integer width the server chose.
macro_rules! lenient_int {
    ($row:expr, $col:expr) => {
        $row.try_get::<Option<i64>, _>($col)
            .or_else(|_| {
                $row.try_get::<Option<i32>, _>($col)
                    .map(|v| v.map(i64::from))
            })
            .or_else(|_| {
                $row.try_get::<Option<i16>, _>($col)
                    .map(|v| v.map(i64::from))
            })
            .map_err(DbError::from)
    };
}

impl Row {
    /// Reads a column by name, converting it to `T`.
    pub fn get<T: FromColumn>(&self, column: &str) -> Result<T> {
        T::from_column(self, column)
    }

    fn first_column_name(&self) -> Result<String> {
        let name = match self {
            Row::Sqlite(r) => r.columns().first().map(|c| c.name().to_string()),
            Row::Postgres(r) => r.columns().first().map(|c| c.name().to_string()),
            Row::MySql(r) => r.columns().first().map(|c| c.name().to_string()),
            Row::Mssql(r) => r.columns().first().map(|c| c.name().to_string()),
        };
        name.ok_or_else(|| Error::Database("the query returned no columns".into()))
    }

    /// The integer in column `index` (only 0 is supported, for scalar queries).
    pub fn int_at(&self, index: usize) -> Result<i64> {
        debug_assert_eq!(index, 0);
        let name = self.first_column_name()?;
        match self {
            // Unnamed expression columns can't be looked up by name everywhere.
            Row::Sqlite(r) => Ok(r.try_get::<i64, _>(index).map_err(DbError::from)?),
            Row::Postgres(r) => lenient_int!(r, index)?
                .ok_or_else(|| Error::Database("the query returned an empty value".into())),
            Row::MySql(r) => lenient_int!(r, index)?
                .ok_or_else(|| Error::Database("the query returned an empty value".into())),
            Row::Mssql(_) => self.get::<i64>(&name),
        }
    }

    fn read_int(&self, col: &str) -> std::result::Result<Option<i64>, DbError> {
        match self {
            Row::Sqlite(r) => Ok(r.try_get::<Option<i64>, _>(col)?),
            Row::Postgres(r) => lenient_int!(r, col),
            Row::MySql(r) => lenient_int!(r, col),
            Row::Mssql(r) => mssql::read_int(r, col),
        }
    }

    fn read_bool(&self, col: &str) -> std::result::Result<Option<bool>, DbError> {
        match self {
            Row::Sqlite(r) => Ok(r.try_get::<Option<bool>, _>(col)?),
            Row::Postgres(r) => Ok(r.try_get::<Option<bool>, _>(col)?),
            Row::MySql(r) => r
                .try_get::<Option<bool>, _>(col)
                .or_else(|_| r.try_get::<Option<i64>, _>(col).map(|v| v.map(|i| i != 0)))
                .map_err(DbError::from),
            Row::Mssql(r) => mssql::read_bool(r, col),
        }
    }

    fn read_text(&self, col: &str) -> std::result::Result<Option<String>, DbError> {
        match self {
            Row::Sqlite(r) => Ok(r.try_get::<Option<String>, _>(col)?),
            Row::Postgres(r) => Ok(r.try_get::<Option<String>, _>(col)?),
            Row::MySql(r) => Ok(r.try_get::<Option<String>, _>(col)?),
            Row::Mssql(r) => mssql::read_text(r, col),
        }
    }

    fn read_date(&self, col: &str) -> std::result::Result<Option<NaiveDate>, DbError> {
        match self {
            Row::Sqlite(r) => Ok(r.try_get::<Option<NaiveDate>, _>(col)?),
            Row::Postgres(r) => Ok(r.try_get::<Option<NaiveDate>, _>(col)?),
            Row::MySql(r) => Ok(r.try_get::<Option<NaiveDate>, _>(col)?),
            Row::Mssql(r) => mssql::read_date(r, col),
        }
    }

    fn read_time(&self, col: &str) -> std::result::Result<Option<DateTime<Utc>>, DbError> {
        match self {
            Row::Sqlite(r) => Ok(r.try_get::<Option<DateTime<Utc>>, _>(col)?),
            Row::Postgres(r) => Ok(r.try_get::<Option<DateTime<Utc>>, _>(col)?),
            Row::MySql(r) => Ok(r.try_get::<Option<DateTime<Utc>>, _>(col)?),
            Row::Mssql(r) => mssql::read_time(r, col),
        }
    }
}

/// A Rust type a column can be read into. `Option<T>` accepts NULL; `T` does not.
pub(crate) trait FromColumn: Sized {
    fn from_column(row: &Row, column: &str) -> Result<Self>;
}

fn required<T>(value: Option<T>, column: &str) -> Result<T> {
    value.ok_or_else(|| Error::Database(format!("unexpected empty value in column {column}")))
}

macro_rules! from_column {
    ($($ty:ty => |$row:ident, $col:ident| $read:expr),* $(,)?) => {$(
        impl FromColumn for Option<$ty> {
            fn from_column($row: &Row, $col: &str) -> Result<Self> {
                Ok($read?)
            }
        }
        impl FromColumn for $ty {
            fn from_column(row: &Row, column: &str) -> Result<Self> {
                required(<Option<$ty>>::from_column(row, column)?, column)
            }
        }
    )*};
}

fn narrow<T: TryFrom<i64>>(v: Option<i64>, col: &str) -> std::result::Result<Option<T>, DbError> {
    v.map(|i| {
        T::try_from(i)
            .map_err(|_| DbError::other(format!("value {i} in column {col} is out of range")))
    })
    .transpose()
}

from_column!(
    i64 => |row, col| row.read_int(col),
    i32 => |row, col| row.read_int(col).and_then(|v| narrow(v, col)),
    bool => |row, col| row.read_bool(col),
    String => |row, col| row.read_text(col),
    NaiveDate => |row, col| row.read_date(col),
    DateTime<Utc> => |row, col| row.read_time(col),
);
