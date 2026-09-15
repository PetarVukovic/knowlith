//! Spreadsheets.
//!
//! This is the format that matters most and goes wrong most quietly. A price
//! read by similarity search returns a *similar* row; a price read by row
//! number returns that row or nothing. Every cell here keeps its row and its
//! sheet so the value can be looked up rather than recalled.

use calamine::{Data, Reader, Xlsx, open_workbook_from_rs};
use knowlith_core::BlockKind;

use crate::builder::{Piece, Rendition};
use crate::error::ExtractError;

/// Reads every worksheet into one rendition.
///
/// Returns the header row of the first sheet as the document's columns —
/// enough for a preview to draw a table, without pretending a workbook with
/// differently shaped sheets has one schema.
pub fn parse(bytes: &[u8]) -> Result<(Option<Vec<String>>, Rendition), ExtractError> {
    let cursor = std::io::Cursor::new(bytes.to_vec());
    let mut workbook: Xlsx<_> = open_workbook_from_rs(cursor)
        .map_err(|e: calamine::XlsxError| ExtractError::Parse(e.to_string()))?;

    let mut rendition = Rendition::new();
    let mut columns: Option<Vec<String>> = None;
    let names = workbook.sheet_names().to_vec();
    let multi_sheet = names.len() > 1;

    for name in names {
        let Ok(range) = workbook.worksheet_range(&name) else {
            continue;
        };
        let mut header_seen = false;

        for (index, row) in range.rows().enumerate() {
            // calamine indexes from the first populated row, so add back the
            // offset to get the row number the owner sees in Excel.
            let row_number = (range.start().map(|(r, _)| r).unwrap_or(0) + index as u32) + 1;
            let cells: Vec<String> = row.iter().map(cell_text).collect();
            if cells.iter().all(|c| c.is_empty()) {
                continue;
            }
            if !header_seen {
                header_seen = true;
                if columns.is_none() {
                    columns = Some(cells.clone());
                }
                continue;
            }
            let locator = if multi_sheet {
                format!("{name} row {row_number}")
            } else {
                format!("row {row_number}")
            };
            rendition.push(
                Piece::new(locator, BlockKind::TableRow, cells.join(" | ")).row(
                    Some(&name),
                    row_number,
                    cells,
                ),
            );
        }
    }

    Ok((columns, rendition))
}

/// Renders one cell.
///
/// Floats are printed without a trailing `.0` so that `450` in the file does
/// not become `450.0` in a quote the owner is asked to recognise. Anything
/// beyond that — thousands separators, currency, decimal commas — is
/// formatting, and formatting belongs to whoever displays the value, not to
/// the record of what the file says.
fn cell_text(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.trim().to_string(),
        Data::Float(f) => {
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{}", *f as i64)
            } else {
                format!("{f}")
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(d) => d
            .as_datetime()
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| d.to_string()),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
        Data::Error(e) => format!("#{e:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_floats_keep_their_written_form() {
        assert_eq!(cell_text(&Data::Float(450.0)), "450");
        assert_eq!(cell_text(&Data::Float(450.5)), "450.5");
    }
}
