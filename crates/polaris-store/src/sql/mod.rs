//! A thin layer over the four database drivers so the store logic is written once.
//!
//! Queries are written in one portable SQL dialect:
//! * numbered placeholders `?1`, `?2`, ... (a number may repeat);
//! * `TRUE` / `FALSE` for booleans;
//! * `LOWER(...)` instead of case-insensitive collations;
//! * no `RETURNING`, `LIMIT`, or upsert syntax: use [`Conn::insert`] and [`Dialect::limit`].
//!
//! [`Dialect::rewrite`] turns that into each server's syntax just before it runs.

pub(crate) mod conn;
pub(crate) mod mssql;

use chrono::{DateTime, NaiveDate, Utc};
use polaris_core::Error;

pub(crate) use conn::{Conn, Pool, Row};

/// A query parameter. Nulls keep their type: PostgreSQL needs to know it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Bool(Option<bool>),
    Int(Option<i64>),
    Text(Option<String>),
    Date(Option<NaiveDate>),
    Time(Option<DateTime<Utc>>),
}

macro_rules! value_from {
    ($($ty:ty => $variant:ident($conv:expr)),* $(,)?) => {$(
        impl From<$ty> for Value {
            fn from(v: $ty) -> Self {
                #[allow(clippy::redundant_closure_call)]
                Value::$variant(Some(($conv)(v)))
            }
        }
        impl From<Option<$ty>> for Value {
            fn from(v: Option<$ty>) -> Self {
                #[allow(clippy::redundant_closure_call)]
                Value::$variant(v.map($conv))
            }
        }
    )*};
}

value_from!(
    bool => Bool(|v| v),
    i64 => Int(|v| v),
    i32 => Int(i64::from),
    u32 => Int(i64::from),
    String => Text(|v| v),
    &str => Text(str::to_string),
    &String => Text(String::clone),
    NaiveDate => Date(|v| v),
    DateTime<Utc> => Time(|v| v),
);

impl From<&Option<String>> for Value {
    fn from(v: &Option<String>) -> Self {
        Value::Text(v.clone())
    }
}

/// Builds a `Vec<Value>` from anything convertible.
macro_rules! params {
    ($($v:expr),* $(,)?) => { vec![$($crate::sql::Value::from($v)),*] };
}
pub(crate) use params;

/// The SQL flavor of a backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dialect {
    Sqlite,
    Postgres,
    MySql,
    Mssql,
}

/// A query in a backend's own syntax, plus which parameter goes in each slot when
/// the backend cannot refer to a parameter by number (MySQL).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Rewritten {
    pub sql: String,
    /// 0-based parameter indexes in placeholder order; `None` means "bind in order".
    pub order: Option<Vec<usize>>,
}

impl Dialect {
    /// `LIMIT n` in this dialect. The query must have an `ORDER BY` (SQL Server needs it).
    pub fn limit(self, n: &str) -> String {
        match self {
            Dialect::Mssql => format!("OFFSET 0 ROWS FETCH NEXT {n} ROWS ONLY"),
            _ => format!("LIMIT {n}"),
        }
    }

    /// Translates portable SQL (see the module docs) into this dialect.
    pub fn rewrite(self, sql: &str) -> Rewritten {
        let mut out = String::with_capacity(sql.len() + 16);
        let mut order = Vec::new();
        let chars: Vec<char> = sql.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '\'' {
                // Copy a string literal verbatim ('' is an escaped quote).
                out.push(c);
                i += 1;
                while i < chars.len() {
                    out.push(chars[i]);
                    if chars[i] == '\'' {
                        if chars.get(i + 1) == Some(&'\'') {
                            out.push('\'');
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                continue;
            }
            if c == '?' && chars.get(i + 1).is_some_and(char::is_ascii_digit) {
                let start = i + 1;
                let mut end = start;
                while end < chars.len() && chars[end].is_ascii_digit() {
                    end += 1;
                }
                let number: String = chars[start..end].iter().collect();
                let n: usize = number.parse().expect("placeholder number");
                match self {
                    Dialect::Sqlite => {
                        out.push('?');
                        out.push_str(&number);
                    }
                    Dialect::Postgres => {
                        out.push('$');
                        out.push_str(&number);
                    }
                    Dialect::Mssql => {
                        out.push_str("@P");
                        out.push_str(&number);
                    }
                    Dialect::MySql => {
                        out.push('?');
                        order.push(n - 1);
                    }
                }
                i = end;
                continue;
            }
            if self == Dialect::Mssql && (c.is_ascii_alphabetic() || c == '_') {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let prev_is_ident = start > 0
                    && (chars[start - 1].is_ascii_alphanumeric()
                        || matches!(chars[start - 1], '_' | '@' | '.'));
                match word.as_str() {
                    "TRUE" if !prev_is_ident => out.push('1'),
                    "FALSE" if !prev_is_ident => out.push('0'),
                    _ => out.push_str(&word),
                }
                continue;
            }
            out.push(c);
            i += 1;
        }
        Rewritten {
            sql: out,
            order: (self == Dialect::MySql).then_some(order),
        }
    }
}

/// A driver error, classified so writes can explain constraint violations.
#[derive(Debug)]
pub(crate) struct DbError {
    pub kind: DbErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DbErrorKind {
    Unique,
    ForeignKey,
    Other,
}

impl DbError {
    pub fn other(message: impl Into<String>) -> Self {
        Self {
            kind: DbErrorKind::Other,
            message: message.into(),
        }
    }
}

impl From<sqlx::Error> for DbError {
    fn from(e: sqlx::Error) -> Self {
        let kind = match e.as_database_error() {
            Some(d) if d.is_unique_violation() => DbErrorKind::Unique,
            Some(d) if d.is_foreign_key_violation() => DbErrorKind::ForeignKey,
            _ => DbErrorKind::Other,
        };
        Self {
            kind,
            message: e.to_string(),
        }
    }
}

impl From<tiberius::error::Error> for DbError {
    fn from(e: tiberius::error::Error) -> Self {
        let kind = match &e {
            // 2627: unique constraint, 2601: unique index, 547: FK or CHECK constraint.
            tiberius::error::Error::Server(t) if matches!(t.code(), 2627 | 2601) => {
                DbErrorKind::Unique
            }
            tiberius::error::Error::Server(t)
                if t.code() == 547 && t.message().contains("FOREIGN KEY") =>
            {
                DbErrorKind::ForeignKey
            }
            _ => DbErrorKind::Other,
        };
        Self {
            kind,
            message: e.to_string(),
        }
    }
}

impl From<DbError> for Error {
    fn from(e: DbError) -> Self {
        Error::Database(e.message)
    }
}

/// Maps constraint violations on writes to errors a user can act on.
pub(crate) fn write_err(e: DbError, duplicate_message: impl FnOnce() -> String) -> Error {
    match e.kind {
        DbErrorKind::Unique => Error::Duplicate(duplicate_message()),
        DbErrorKind::ForeignKey => Error::validation(
            "",
            "One of the selected items no longer exists. Reload and try again.",
        ),
        DbErrorKind::Other => Error::Database(e.message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQL: &str = "SELECT * FROM t WHERE (?1 IS NULL OR a = ?1) AND b = ?2 AND c = TRUE \
                       AND d = 'it''s ?3 TRUE' AND e = FALSE AND f.TRUE_x = ?3";

    #[test]
    fn sqlite_keeps_numbered_placeholders() {
        let r = Dialect::Sqlite.rewrite(SQL);
        assert_eq!(r.sql, SQL);
        assert_eq!(r.order, None);
    }

    #[test]
    fn postgres_uses_dollar_numbers() {
        let r = Dialect::Postgres.rewrite(SQL);
        assert!(
            r.sql.contains("($1 IS NULL OR a = $1) AND b = $2"),
            "{}",
            r.sql
        );
        assert!(r.sql.contains("'it''s ?3 TRUE'"), "{}", r.sql);
        assert!(r.sql.ends_with("f.TRUE_x = $3"), "{}", r.sql);
    }

    #[test]
    fn mysql_expands_repeated_placeholders() {
        let r = Dialect::MySql.rewrite(SQL);
        assert!(
            r.sql.contains("(? IS NULL OR a = ?) AND b = ?"),
            "{}",
            r.sql
        );
        assert_eq!(r.order, Some(vec![0, 0, 1, 2]));
    }

    #[test]
    fn mssql_uses_named_parameters_and_bit_literals() {
        let r = Dialect::Mssql.rewrite(SQL);
        assert!(
            r.sql.contains("(@P1 IS NULL OR a = @P1) AND b = @P2"),
            "{}",
            r.sql
        );
        assert!(r.sql.contains("c = 1"), "{}", r.sql);
        assert!(r.sql.contains("e = 0"), "{}", r.sql);
        assert!(r.sql.contains("'it''s ?3 TRUE'"), "{}", r.sql);
        assert!(r.sql.ends_with("f.TRUE_x = @P3"), "{}", r.sql);
    }

    #[test]
    fn limit_syntax() {
        assert_eq!(Dialect::Postgres.limit("?5"), "LIMIT ?5");
        assert_eq!(
            Dialect::Mssql.limit("?5"),
            "OFFSET 0 ROWS FETCH NEXT ?5 ROWS ONLY"
        );
    }
}
