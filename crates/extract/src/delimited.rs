//! CSV and TSV.
//!
//! A delimited file is addressed by row, because that is how the owner talks
//! about it: "the price of installation" is a row, not a character range. The
//! rendition joins each row's cells with ` | ` so a citation reads as a line of
//! the table rather than as raw CSV.

use knowlith_core::BlockKind;

use crate::builder::{Piece, Rendition};

/// Picks the delimiter the file actually uses.
///
/// Croatian locale spreadsheets export with `;` because `,` is the decimal
/// separator, so sniffing is not optional here — assuming `,` turns every row
/// into a single cell and every price into text.
pub fn delimiter(name: &str, text: &str) -> u8 {
    if name.to_ascii_lowercase().ends_with(".tsv") {
        return b'\t';
    }
    let first = text.lines().next().unwrap_or_default();
    let counts = [
        (b';', first.matches(';').count()),
        (b',', first.matches(',').count()),
        (b'\t', first.matches('\t').count()),
    ];
    counts
        .iter()
        .max_by_key(|(_, n)| *n)
        .filter(|(_, n)| *n > 0)
        .map(|(d, _)| *d)
        .unwrap_or(b',')
}

/// Returns the column headers and the rendition.
pub fn parse(name: &str, text: &str) -> (Option<Vec<String>>, Rendition) {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter(name, text))
        .flexible(true)
        .has_headers(false)
        .from_reader(text.as_bytes());

    let mut rendition = Rendition::new();
    let mut columns = None;
    // 1-based and counting the header, so the number matches what the owner
    // sees in the row gutter of their own spreadsheet application.
    let mut row_number = 0u32;

    for record in reader.records().flatten() {
        row_number += 1;
        let cells: Vec<String> = record.iter().map(|c| c.trim().to_string()).collect();
        if cells.iter().all(String::is_empty) {
            continue;
        }
        if columns.is_none() {
            columns = Some(cells.clone());
            continue;
        }
        let joined = cells.join(" | ");
        rendition.push(
            Piece::new(format!("row {row_number}"), BlockKind::TableRow, joined)
                .row(None, row_number, cells),
        );
    }

    (columns, rendition)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semicolon_files_are_not_read_as_one_column() {
        let text = "Stavka;Cijena\nUgradnja;450,00\n";
        assert_eq!(delimiter("cjenik.csv", text), b';');
        let (columns, rendition) = parse("cjenik.csv", text);
        assert_eq!(columns.unwrap(), ["Stavka", "Cijena"]);
        let (_, blocks) = rendition.finish();
        assert_eq!(blocks[0].cells.as_deref().unwrap(), ["Ugradnja", "450,00"]);
    }

    #[test]
    fn row_numbers_count_the_header() {
        let text = "a,b\n1,2\n3,4\n";
        let (_, rendition) = parse("t.csv", text);
        let (_, blocks) = rendition.finish();
        assert_eq!(blocks[0].locator, "row 2");
        assert_eq!(blocks[1].locator, "row 3");
    }

    #[test]
    fn spans_address_their_own_text() {
        let text = "a,b\n1,2\n3,4\n";
        let (_, rendition) = parse("t.csv", text);
        let (rendered, blocks) = rendition.finish();
        for b in &blocks {
            assert_eq!(&rendered[b.start_byte..b.end_byte], b.text);
        }
    }
}
