//! Moving a whole database between backends (SQLite to a server and back).
//!
//! A [`DataSet`] is a backend-neutral copy of every table, row for row with the
//! original ids, so links between records, the audit trail, and password hashes
//! survive the move unchanged.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use ts_rs::TS;

/// One value in a copied row.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Null,
    Bool(bool),
    Int(i64),
    Text(String),
    Date(NaiveDate),
    Time(DateTime<Utc>),
}

/// Every row of one table, in id order.
#[derive(Debug, Clone, PartialEq)]
pub struct TableData {
    pub table: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
}

/// Every table, in an order that satisfies foreign keys when inserted front to back.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DataSet {
    pub tables: Vec<TableData>,
}

impl DataSet {
    pub fn rows_in(&self, table: &str) -> i64 {
        self.tables
            .iter()
            .find(|t| t.table == table)
            .map_or(0, |t| t.rows.len() as i64)
    }
}

/// How many rows a table holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TableCount {
    pub table: String,
    pub rows: i64,
}

/// One table's result in a [`CopyReport`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TableCopy {
    pub table: String,
    /// A readable name such as "Calibration records".
    pub label: String,
    pub source_rows: i64,
    pub copied_rows: i64,
}

/// What a database copy did. Every table's copied count matched its source count,
/// or the copy would have failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct CopyReport {
    pub source: String,
    pub target: String,
    pub tables: Vec<TableCopy>,
}
