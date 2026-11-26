//! Reading and writing every table row for row, for copies between backends.

use polaris_core::transfer::{Cell, DataSet, TableCount, TableData};
use polaris_core::{Error, Result};

use crate::sql::{Conn, Dialect, Row, Value, params};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColType {
    Int,
    Text,
    Bool,
    Date,
    Time,
}

pub(crate) struct TableSpec {
    pub name: &'static str,
    pub label: &'static str,
    /// Has an `id` identity column (every table but `app_settings`).
    pub identity: bool,
    pub columns: &'static [(&'static str, ColType)],
}

use ColType::{Bool, Date, Int, Text, Time};

macro_rules! lookup_columns {
    (@ty) => { Bool };
    (@ty $ty:ident) => { $ty };
    ($($extra:literal $(as $ty:ident)?),*) => {
        &[
            ("id", Int),
            ("name", Text),
            ("description", Text),
            ("is_active", Bool),
            ("sort_order", Int),
            ("created_at", Time),
            ("updated_at", Time),
            $(($extra, lookup_columns!(@ty $($ty)?)),)*
        ]
    };
}

/// Every table, parents before children so inserts satisfy foreign keys.
/// (`assets.parent_id` points at the same table; import fills it in a second pass.)
pub(crate) const TABLES: &[TableSpec] = &[
    TableSpec {
        name: "users",
        label: "Users",
        identity: true,
        columns: &[
            ("id", Int),
            ("username", Text),
            ("display_name", Text),
            ("email", Text),
            ("role", Text),
            ("password_hash", Text),
            ("must_change_password", Bool),
            ("is_active", Bool),
            ("last_sign_in_at", Time),
            ("row_version", Int),
            ("created_at", Time),
            ("updated_at", Time),
            ("signature", Text),
            ("reset_requested_at", Time),
            ("password_changed_at", Time),
        ],
    },
    TableSpec {
        name: "locations",
        label: "Locations",
        identity: true,
        columns: lookup_columns!(),
    },
    TableSpec {
        name: "departments",
        label: "Departments",
        identity: true,
        columns: lookup_columns!(),
    },
    TableSpec {
        name: "owners",
        label: "Owners",
        identity: true,
        columns: lookup_columns!(),
    },
    TableSpec {
        name: "manufacturers",
        label: "Manufacturers",
        identity: true,
        columns: lookup_columns!(),
    },
    TableSpec {
        name: "calibration_providers",
        label: "Calibration providers",
        identity: true,
        columns: lookup_columns!("is_internal"),
    },
    TableSpec {
        name: "asset_statuses",
        label: "Statuses",
        identity: true,
        columns: lookup_columns!("tracks_due", "builtin_role" as Text),
    },
    TableSpec {
        name: "service_types",
        label: "Service types",
        identity: true,
        columns: lookup_columns!("counts_as_calibration", "is_extension", "is_repair"),
    },
    TableSpec {
        name: "assets",
        label: "Instruments and Systems",
        identity: true,
        columns: &[
            ("id", Int),
            ("asset_tag", Text),
            ("kind", Text),
            ("parent_id", Int),
            ("description", Text),
            ("manufacturer_id", Int),
            ("model_number", Text),
            ("serial_number", Text),
            ("location_id", Int),
            ("department_id", Int),
            ("owner_id", Int),
            ("status_id", Int),
            ("warranty_end_date", Date),
            ("cal_interval_value", Int),
            ("cal_interval_unit", Text),
            ("cal_interval_end_of_month", Bool),
            ("in_service_date", Date),
            ("first_cal_date", Date),
            ("last_cal_date", Date),
            ("last_cal_date_entered", Date),
            ("next_cal_date", Date),
            ("cal_extended", Bool),
            ("previous_status_id", Int),
            ("cal_aggregate", Bool),
            ("owner_since", Time),
            ("notes", Text),
            ("is_archived", Bool),
            ("row_version", Int),
            ("created_at", Time),
            ("updated_at", Time),
        ],
    },
    TableSpec {
        name: "calibration_records",
        label: "Service records",
        identity: true,
        columns: &[
            ("id", Int),
            ("asset_id", Int),
            ("cal_date", Date),
            ("service_type_id", Int),
            ("provider_id", Int),
            ("manufacturer_id", Int),
            ("technician", Text),
            ("cost_cents", Int),
            ("certificate_number", Text),
            ("condition_received", Text),
            ("condition_returned", Text),
            ("next_due_date", Date),
            ("notes", Text),
            ("extension_days", Int),
            ("invoice_number", Text),
            ("invoice_line", Int),
            ("invoice_date", Date),
            ("is_archived", Bool),
            ("row_version", Int),
            ("created_at", Time),
            ("updated_at", Time),
        ],
    },
    TableSpec {
        name: "attachments",
        label: "Files",
        identity: true,
        columns: &[
            ("id", Int),
            ("asset_id", Int),
            ("calibration_id", Int),
            ("category", Text),
            ("file_name", Text),
            ("relative_path", Text),
            ("size_bytes", Int),
            ("sha256", Text),
            ("content_type", Text),
            ("description", Text),
            ("uploaded_by_id", Int),
            ("uploaded_by", Text),
            ("is_archived", Bool),
            ("row_version", Int),
            ("created_at", Time),
            ("updated_at", Time),
            ("storage_folder", Text),
        ],
    },
    TableSpec {
        name: "certificates",
        label: "Certificates",
        identity: true,
        columns: &[
            ("id", Int),
            ("certificate_number", Text),
            ("kind", Text),
            ("asset_id", Int),
            ("subject", Text),
            ("description", Text),
            ("cal_date", Date),
            ("details", Text),
            ("template", Text),
            ("issued_by_id", Int),
            ("issued_by", Text),
            ("created_at", Time),
        ],
    },
    TableSpec {
        name: "owner_requests",
        label: "Check-out requests",
        identity: true,
        columns: &[
            ("id", Int),
            ("asset_id", Int),
            ("kind", Text),
            ("owner_id", Int),
            ("requested_by_id", Int),
            ("requested_by", Text),
            ("note", Text),
            ("status", Text),
            ("decided_by_id", Int),
            ("decided_by", Text),
            ("decided_at", Time),
            ("decision_note", Text),
            ("created_at", Time),
            ("updated_at", Time),
            ("approver_user_id", Int),
        ],
    },
    TableSpec {
        name: "reservations",
        label: "Reservations",
        identity: true,
        columns: &[
            ("id", Int),
            ("asset_id", Int),
            ("user_id", Int),
            ("start_date", Date),
            ("end_date", Date),
            ("note", Text),
            ("status", Text),
            ("asks_reservation_id", Int),
            ("decided_by", Text),
            ("decided_at", Time),
            ("decision_note", Text),
            ("created_at", Time),
            ("updated_at", Time),
        ],
    },
    TableSpec {
        name: "audit_log",
        label: "Audit trail",
        identity: true,
        columns: &[
            ("id", Int),
            ("occurred_at", Time),
            ("user_id", Int),
            ("username", Text),
            ("action", Text),
            ("entity", Text),
            ("entity_id", Int),
            ("asset_id", Int),
            ("entity_label", Text),
            ("reason", Text),
            ("changes", Text),
        ],
    },
    TableSpec {
        name: "app_settings",
        label: "Settings",
        identity: false,
        columns: &[
            ("setting_key", Text),
            ("setting_value", Text),
            ("updated_at", Time),
        ],
    },
];

/// Tables a fresh database fills with defaults; an import replaces their contents.
const SEEDED: &[&str] = &["asset_statuses", "service_types", "app_settings"];

pub(crate) fn label(table: &str) -> String {
    TABLES
        .iter()
        .find(|t| t.name == table)
        .map_or_else(|| table.to_string(), |t| t.label.to_string())
}

fn spec(table: &str) -> Result<&'static TableSpec> {
    TABLES
        .iter()
        .find(|t| t.name == table)
        .ok_or_else(|| Error::Database(format!("unknown table {table} in copied data")))
}

pub(crate) async fn table_counts(conn: &mut Conn) -> Result<Vec<TableCount>> {
    let mut out = Vec::with_capacity(TABLES.len());
    for t in TABLES {
        let rows = conn
            .scalar(&format!("SELECT COUNT(*) AS n FROM {}", t.name), params![])
            .await?;
        out.push(TableCount {
            table: t.name.into(),
            rows,
        });
    }
    Ok(out)
}

fn read_cell(row: &Row, column: &str, ty: ColType) -> Result<Cell> {
    let cell = match ty {
        Int => row.get::<Option<i64>>(column)?.map(Cell::Int),
        Text => row.get::<Option<String>>(column)?.map(Cell::Text),
        Bool => row.get::<Option<bool>>(column)?.map(Cell::Bool),
        Date => row
            .get::<Option<chrono::NaiveDate>>(column)?
            .map(Cell::Date),
        Time => row
            .get::<Option<chrono::DateTime<chrono::Utc>>>(column)?
            .map(Cell::Time),
    };
    Ok(cell.unwrap_or(Cell::Null))
}

fn to_value(cell: &Cell, ty: ColType, table: &str, column: &str) -> Result<Value> {
    let mismatch = || {
        Error::Database(format!(
            "copied value for {table}.{column} doesn't match its column type"
        ))
    };
    Ok(match (ty, cell) {
        (Int, Cell::Null) => Value::Int(None),
        (Text, Cell::Null) => Value::Text(None),
        (Bool, Cell::Null) => Value::Bool(None),
        (Date, Cell::Null) => Value::Date(None),
        (Time, Cell::Null) => Value::Time(None),
        (Int, Cell::Int(i)) => Value::Int(Some(*i)),
        (Text, Cell::Text(s)) => Value::Text(Some(s.clone())),
        (Bool, Cell::Bool(b)) => Value::Bool(Some(*b)),
        (Bool, Cell::Int(i)) => Value::Bool(Some(*i != 0)),
        (Date, Cell::Date(d)) => Value::Date(Some(*d)),
        (Time, Cell::Time(t)) => Value::Time(Some(*t)),
        _ => return Err(mismatch()),
    })
}

/// Reads every table. Run it inside a transaction for a consistent snapshot.
pub(crate) async fn export(conn: &mut Conn) -> Result<DataSet> {
    let mut tables = Vec::with_capacity(TABLES.len());
    for t in TABLES {
        let columns: Vec<&str> = t.columns.iter().map(|(c, _)| *c).collect();
        let order = if t.identity { "id" } else { "setting_key" };
        let sql = format!(
            "SELECT {} FROM {} ORDER BY {order}",
            columns.join(", "),
            t.name
        );
        let rows = conn
            .fetch_all(&sql, params![])
            .await?
            .iter()
            .map(|row| {
                t.columns
                    .iter()
                    .map(|(c, ty)| read_cell(row, c, *ty))
                    .collect::<Result<Vec<_>>>()
            })
            .collect::<Result<Vec<_>>>()?;
        tables.push(TableData {
            table: t.name.into(),
            columns: columns.iter().map(|c| c.to_string()).collect(),
            rows,
        });
    }
    Ok(DataSet { tables })
}

/// The largest number of parameters in one multi-row INSERT (SQL Server allows 2100).
const MAX_PARAMS_PER_INSERT: usize = 1000;

/// Fills an empty database. Must run inside a transaction.
pub(crate) async fn import(conn: &mut Conn, data: &DataSet) -> Result<()> {
    let dialect = conn.dialect();

    // Refuse to merge into a database that's already in use.
    for t in TABLES.iter().filter(|t| !SEEDED.contains(&t.name)) {
        let rows = conn
            .scalar(&format!("SELECT COUNT(*) AS n FROM {}", t.name), params![])
            .await?;
        if rows > 0 {
            return Err(Error::Conflict(format!(
                "The target database already contains data ({} {}). Copy into a new, empty database.",
                rows,
                t.label.to_lowercase()
            )));
        }
    }
    for table in SEEDED {
        conn.execute(&format!("DELETE FROM {table}"), params![])
            .await?;
    }

    for table in &data.tables {
        let spec = spec(&table.table)?;
        // Columns are matched by name, so both sides need not list them in the same order.
        let types = table
            .columns
            .iter()
            .map(|c| {
                spec.columns
                    .iter()
                    .find(|(name, _)| name == c)
                    .map(|(name, ty)| (*name, *ty))
                    .ok_or_else(|| Error::Database(format!("unknown column {}.{c}", table.table)))
            })
            .collect::<Result<Vec<_>>>()?;
        if table.rows.is_empty() {
            continue;
        }
        let parent_col = (table.table == "assets")
            .then(|| types.iter().position(|(c, _)| *c == "parent_id"))
            .flatten();

        if dialect == Dialect::Mssql && spec.identity {
            conn.batch(&format!("SET IDENTITY_INSERT {} ON", table.table))
                .await?;
        }

        let per_insert = (MAX_PARAMS_PER_INSERT / types.len()).max(1);
        let column_list = types.iter().map(|(c, _)| *c).collect::<Vec<_>>().join(", ");
        for chunk in table.rows.chunks(per_insert) {
            let mut sql = format!("INSERT INTO {} ({column_list}) VALUES ", table.table);
            let mut values = Vec::with_capacity(chunk.len() * types.len());
            for (r, row) in chunk.iter().enumerate() {
                if row.len() != types.len() {
                    return Err(Error::Database(format!(
                        "a copied {} row has {} values for {} columns",
                        table.table,
                        row.len(),
                        types.len()
                    )));
                }
                sql.push_str(if r == 0 { "(" } else { ", (" });
                for (i, (cell, (column, ty))) in row.iter().zip(&types).enumerate() {
                    if i > 0 {
                        sql.push_str(", ");
                    }
                    // Parents are linked after every asset exists.
                    let value = if Some(i) == parent_col {
                        Value::Int(None)
                    } else {
                        to_value(cell, *ty, &table.table, column)?
                    };
                    values.push(value);
                    sql.push_str(&format!("?{}", values.len()));
                }
                sql.push(')');
            }
            conn.execute(&sql, values).await?;
        }

        if dialect == Dialect::Mssql && spec.identity {
            conn.batch(&format!("SET IDENTITY_INSERT {} OFF", table.table))
                .await?;
        }

        if let Some(p) = parent_col {
            let id_col = types.iter().position(|(c, _)| *c == "id");
            for row in &table.rows {
                if let (Some(Cell::Int(id)), Some(Cell::Int(parent))) =
                    (id_col.map(|i| &row[i]), row.get(p))
                {
                    conn.execute(
                        "UPDATE assets SET parent_id = ?2 WHERE id = ?1",
                        params![*id, *parent],
                    )
                    .await?;
                }
            }
        }

        // PostgreSQL identity sequences don't notice explicit ids; move them past the max.
        if dialect == Dialect::Postgres && spec.identity {
            conn.batch(&format!(
                "SELECT setval(pg_get_serial_sequence('{t}', 'id'), \
                 (SELECT COALESCE(MAX(id), 0) + 1 FROM {t}), false)",
                t = table.table
            ))
            .await?;
        }
    }
    Ok(())
}
