//! Copying all data from one database to another (SQLite to a server, or back).

use polaris_core::audit::WriteContext;
use polaris_core::model::AuditAction;
use polaris_core::transfer::{CopyReport, TableCopy};
use polaris_core::{Error, Result, Store};

use crate::transfer::label;

/// Copies every row from `source` into the empty database `target`, keeping ids,
/// then checks that each table's row count matches. The copy is audited on both
/// sides. `source_name` and `target_name` are credential-free descriptions.
///
/// The import runs in one transaction, so a failure leaves the target unchanged.
/// Nobody should be editing the source while it runs.
pub async fn copy_database(
    source: &dyn Store,
    target: &dyn Store,
    ctx: &WriteContext,
    source_name: &str,
    target_name: &str,
) -> Result<CopyReport> {
    let data = source.export_data().await?;
    target.import_data(&data).await?;

    let target_counts = target.table_counts().await?;
    let mut tables = Vec::with_capacity(data.tables.len());
    for t in &data.tables {
        let copied = target_counts
            .iter()
            .find(|c| c.table == t.table)
            .map_or(0, |c| c.rows);
        let expected = t.rows.len() as i64;
        if copied != expected {
            return Err(Error::Database(format!(
                "Verification failed: {} has {expected} rows in the source but {copied} in the target.",
                label(&t.table)
            )));
        }
        tables.push(TableCopy {
            table: t.table.clone(),
            label: label(&t.table),
            source_rows: expected,
            copied_rows: copied,
        });
    }

    let rows: i64 = tables.iter().map(|t| t.copied_rows).sum();
    source
        .record_data_copy(
            ctx,
            AuditAction::Export,
            &format!("Copied all data ({rows} rows) to {target_name}"),
        )
        .await?;
    target
        .record_data_copy(
            ctx,
            AuditAction::Import,
            &format!("Copied all data ({rows} rows) from {source_name}"),
        )
        .await?;

    Ok(CopyReport {
        source: source_name.to_string(),
        target: target_name.to_string(),
        tables,
    })
}
