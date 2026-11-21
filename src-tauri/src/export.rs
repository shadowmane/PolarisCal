//! Writing reports to Excel workbooks.
//!
//! The UI sends the table it is showing (columns with a kind, and rows of cells), so
//! the file always matches the screen. Dates arrive as `YYYY-MM-DD` text and money as
//! cents; both become real Excel values with formats, so they sort and sum properly.

use std::path::Path;

use chrono::{Datelike, Local, NaiveDate};
use rust_xlsxwriter::{ExcelDateTime, Format, FormatAlign, FormatBorder, Workbook, XlsxError};
use serde::Deserialize;
use tauri::State;
use ts_rs::TS;

use crate::commands::{CmdResult, CommandError, ErrorKind, Need, require};
use crate::state::AppState;

/// How a column's cells are written and formatted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ColumnKind {
    Text,
    /// A plain number.
    Number,
    /// Integer cents, shown as currency.
    Money,
    /// `YYYY-MM-DD` text, written as an Excel date.
    Date,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct ExportColumn {
    pub header: String,
    pub kind: ColumnKind,
    /// Width in characters; a sensible default is used when absent.
    #[serde(default)]
    pub width: Option<f64>,
}

/// One cell: empty, a number, or text.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum ExportCell {
    Number(f64),
    Text(String),
    Empty(Option<()>),
}

/// One worksheet: a title, an optional line describing filters, and a table.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct ExportSheet {
    /// Also the worksheet tab name (shortened to Excel's 31-character limit).
    pub title: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    pub columns: Vec<ExportColumn>,
    pub rows: Vec<Vec<ExportCell>>,
    /// Shown before money values; "$" when absent.
    #[serde(default)]
    pub currency_symbol: Option<String>,
}

/// An Excel number format showing `symbol` before amounts, e.g. `"€"#,##0.00`.
fn money_format(symbol: &str) -> String {
    let quoted = symbol.replace('"', "");
    let space = if quoted.chars().all(|c| c.is_alphabetic()) && quoted.chars().count() > 1 {
        " "
    } else {
        ""
    };
    format!("\"{quoted}{space}\"#,##0.00")
}

/// Excel forbids some characters in sheet names and limits them to 31 characters.
fn sheet_name(title: &str, used: &mut Vec<String>) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| if "[]:*?/\\".contains(c) { '-' } else { c })
        .collect();
    let base: String = cleaned.trim().chars().take(28).collect();
    let base = if base.is_empty() {
        "Sheet".to_string()
    } else {
        base
    };
    let mut name = base.clone();
    let mut n = 2;
    while used.iter().any(|u| u.eq_ignore_ascii_case(&name)) {
        name = format!("{base} {n}");
        n += 1;
    }
    used.push(name.clone());
    name
}

const HEADER_ROW: u32 = 3;

pub fn write_workbook(path: &Path, sheets: &[ExportSheet]) -> Result<(), XlsxError> {
    let mut workbook = Workbook::new();
    let title_fmt = Format::new().set_bold().set_font_size(14);
    let subtitle_fmt = Format::new().set_italic().set_font_color(0x595959);
    let header_fmt = Format::new()
        .set_bold()
        .set_background_color(0xDCE6F1)
        .set_border_bottom(FormatBorder::Thin)
        .set_text_wrap()
        .set_align(FormatAlign::Top);
    let date_fmt = Format::new()
        .set_num_format("yyyy-mm-dd")
        .set_align(FormatAlign::Left);
    let number_fmt = Format::new().set_num_format("General");
    let text_fmt = Format::new().set_align(FormatAlign::Top);
    let generated = Local::now()
        .format("Generated %Y-%m-%d %H:%M by PolarisCal")
        .to_string();

    let mut used = Vec::new();
    for sheet in sheets {
        let money_fmt = Format::new().set_num_format(money_format(
            sheet.currency_symbol.as_deref().unwrap_or("$"),
        ));
        let ws = workbook.add_worksheet();
        ws.set_name(sheet_name(&sheet.title, &mut used))?;
        ws.write_string_with_format(0, 0, &sheet.title, &title_fmt)?;
        let subtitle = match &sheet.subtitle {
            Some(s) if !s.trim().is_empty() => format!("{s} · {generated}"),
            _ => generated.clone(),
        };
        ws.write_string_with_format(1, 0, &subtitle, &subtitle_fmt)?;

        for (c, col) in sheet.columns.iter().enumerate() {
            let c = c as u16;
            ws.write_string_with_format(HEADER_ROW, c, &col.header, &header_fmt)?;
            let default_width = match col.kind {
                ColumnKind::Date => 12.0,
                ColumnKind::Money | ColumnKind::Number => 12.0,
                ColumnKind::Text => (col.header.len() as f64 + 4.0).max(14.0),
            };
            ws.set_column_width(c, col.width.unwrap_or(default_width))?;
        }

        for (r, row) in sheet.rows.iter().enumerate() {
            let r = HEADER_ROW + 1 + r as u32;
            for (c, (cell, col)) in row.iter().zip(&sheet.columns).enumerate() {
                let c = c as u16;
                match (col.kind, cell) {
                    (_, ExportCell::Empty(_)) => {}
                    (ColumnKind::Money, ExportCell::Number(cents)) => {
                        ws.write_number_with_format(r, c, cents / 100.0, &money_fmt)?;
                    }
                    (ColumnKind::Date, ExportCell::Text(text)) => {
                        match NaiveDate::parse_from_str(text, "%Y-%m-%d") {
                            Ok(d) => {
                                let excel = ExcelDateTime::from_ymd(
                                    d.year() as u16,
                                    d.month() as u8,
                                    d.day() as u8,
                                )?;
                                ws.write_datetime_with_format(r, c, &excel, &date_fmt)?;
                            }
                            Err(_) => {
                                ws.write_string_with_format(r, c, text, &text_fmt)?;
                            }
                        }
                    }
                    (_, ExportCell::Number(n)) => {
                        ws.write_number_with_format(r, c, *n, &number_fmt)?;
                    }
                    (_, ExportCell::Text(text)) => {
                        ws.write_string_with_format(r, c, text, &text_fmt)?;
                    }
                }
            }
        }

        if !sheet.columns.is_empty() {
            let last_row = HEADER_ROW + sheet.rows.len() as u32;
            ws.autofilter(HEADER_ROW, 0, last_row, sheet.columns.len() as u16 - 1)?;
            ws.set_freeze_panes(HEADER_ROW + 1, 0)?;
        }
    }
    workbook.save(path)
}

/// Saves `sheets` as an Excel workbook at `path` (chosen by the person in a save dialog).
#[tauri::command(rename_all = "snake_case")]
pub async fn export_xlsx(
    state: State<'_, AppState>,
    path: String,
    sheets: Vec<ExportSheet>,
) -> CmdResult<()> {
    require(&state, Need::SignedIn).await?;
    let path = std::path::PathBuf::from(path.trim());
    let is_xlsx = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("xlsx"));
    if !path.is_absolute() || !is_xlsx {
        return Err(CommandError::new(
            ErrorKind::Validation,
            "Choose where to save the .xlsx file.",
        ));
    }
    tauri::async_runtime::spawn_blocking(move || write_workbook(&path, &sheets))
        .await
        .map_err(|e| CommandError::new(ErrorKind::Config, e.to_string()))?
        .map_err(|e| {
            CommandError::new(
                ErrorKind::Config,
                format!("Couldn't save the Excel file: {e}. If it's open in Excel, close it and try again."),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheet_names_are_valid_and_unique() {
        let mut used = Vec::new();
        assert_eq!(
            sheet_name("Due / overdue: [all]", &mut used),
            "Due - overdue- -all-"
        );
        assert_eq!(
            sheet_name("Due / overdue: [all]", &mut used),
            "Due - overdue- -all- 2"
        );
        assert_eq!(sheet_name("   ", &mut used), "Sheet");
        assert!(sheet_name(&"x".repeat(40), &mut used).chars().count() <= 31);
    }

    #[test]
    fn money_formats_show_the_symbol() {
        assert_eq!(money_format("€"), "\"€\"#,##0.00");
        assert_eq!(money_format("kr"), "\"kr \"#,##0.00");
        assert_eq!(money_format("R$"), "\"R$\"#,##0.00");
    }

    #[test]
    fn cells_deserialize_from_ui_json() {
        let cells: Vec<ExportCell> = serde_json::from_str(r#"[1.5, "text", null]"#).unwrap();
        assert_eq!(
            cells,
            [
                ExportCell::Number(1.5),
                ExportCell::Text("text".into()),
                ExportCell::Empty(None)
            ]
        );
    }

    #[test]
    fn writes_a_workbook() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report.xlsx");
        let sheet = ExportSheet {
            title: "Due list".into(),
            subtitle: Some("Next 30 days".into()),
            columns: vec![
                ExportColumn {
                    header: "Tag".into(),
                    kind: ColumnKind::Text,
                    width: None,
                },
                ExportColumn {
                    header: "Due".into(),
                    kind: ColumnKind::Date,
                    width: None,
                },
                ExportColumn {
                    header: "Cost".into(),
                    kind: ColumnKind::Money,
                    width: Some(10.0),
                },
            ],
            rows: vec![
                vec![
                    ExportCell::Text("DMM-1".into()),
                    ExportCell::Text("2026-12-01".into()),
                    ExportCell::Number(12_500.0),
                ],
                vec![
                    ExportCell::Text("RIG-1".into()),
                    ExportCell::Empty(None),
                    ExportCell::Empty(None),
                ],
            ],
            currency_symbol: Some("€".into()),
        };
        write_workbook(&path, &[sheet]).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..2], b"PK", "xlsx files are zip archives");
    }
}
