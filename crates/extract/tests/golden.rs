//! Extraction must not drift.
//!
//! Every offset stored in the lake points into the text these parsers
//! produce, so a change here silently invalidates evidence that was verified
//! months ago. This test pins the output against a checked-in snapshot of a
//! folder that looks like a real company's shared drive: Word, Excel, PDF,
//! Markdown, a Windows-1250 note with CRLF line endings, and a
//! semicolon-delimited CSV.
//!
//! No model runs here and nothing reaches the network, so it belongs on every
//! commit rather than in a nightly job.
//!
//! When a parser change is intended:
//!
//! ```sh
//! UPDATE_GOLDEN=1 cargo test -p knowlith-extract --test golden
//! ```
//!
//! and read the diff. Any moved offset in that diff is stored evidence that
//! will need rechecking — `knowlith doctor` reports which.

use std::path::{Path, PathBuf};

use knowlith_extract::extract_file;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct GoldenBlock {
    locator: String,
    kind: String,
    start_byte: usize,
    end_byte: usize,
    text: String,
}

/// The document's identity and file hash are deliberately not pinned: they
/// follow the bytes of the fixture, and a zip-based format rewritten by a
/// different toolchain changes them without changing a single offset.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct GoldenDocument {
    name: String,
    kind: String,
    verbatim: bool,
    text_sha256: String,
    columns: Option<Vec<String>>,
    blocks: Vec<GoldenBlock>,
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/termoval")
        .canonicalize()
        .expect("the fixture folder is checked in next to the crates")
}

fn readable_files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[test]
fn the_termoval_folder_extracts_the_same_way_every_time() {
    let root = fixtures();
    let source = root.join("source");
    let expected_path = root.join("expected/blocks.json");

    let mut documents: Vec<GoldenDocument> = Vec::new();
    for path in readable_files(&source) {
        let Ok(doc) = extract_file(&path) else {
            continue;
        };
        documents.push(GoldenDocument {
            name: doc.name.clone(),
            kind: format!("{:?}", doc.kind).to_lowercase(),
            verbatim: doc.verbatim,
            text_sha256: doc.text_sha256.clone(),
            columns: doc.columns.clone(),
            blocks: doc
                .blocks
                .iter()
                .map(|b| GoldenBlock {
                    locator: b.locator.clone(),
                    kind: format!("{:?}", b.kind).to_lowercase(),
                    start_byte: b.start_byte,
                    end_byte: b.end_byte,
                    text: b.text.clone(),
                })
                .collect(),
        });
    }
    documents.sort_by(|a, b| a.name.cmp(&b.name));

    assert!(
        documents.len() >= 10,
        "the fixture folder should yield a dozen readable documents, got {}",
        documents.len()
    );

    let rendered = serde_json::to_string_pretty(&documents).unwrap();

    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(expected_path.parent().unwrap()).unwrap();
        std::fs::write(&expected_path, format!("{rendered}\n")).unwrap();
        return;
    }

    let stored = std::fs::read_to_string(&expected_path).unwrap_or_else(|_| {
        panic!(
            "no snapshot at {}. Run UPDATE_GOLDEN=1 cargo test -p knowlith-extract --test golden",
            expected_path.display()
        )
    });
    let expected: Vec<GoldenDocument> = serde_json::from_str(&stored).unwrap();

    for (got, want) in documents.iter().zip(expected.iter()) {
        assert_eq!(got.name, want.name);
        assert_eq!(got.blocks, want.blocks, "blocks moved in {}", got.name);
        assert_eq!(got.text_sha256, want.text_sha256, "rendition changed in {}", got.name);
    }
    assert_eq!(documents.len(), expected.len(), "a document appeared or disappeared");
}

/// The property that makes every offset in the lake meaningful.
#[test]
fn every_block_addresses_its_own_text() {
    for path in readable_files(&fixtures().join("source")) {
        let Ok(doc) = extract_file(&path) else {
            continue;
        };
        for block in &doc.blocks {
            assert!(
                doc.text.is_char_boundary(block.start_byte) && doc.text.is_char_boundary(block.end_byte),
                "{} · {} does not land on a character boundary",
                doc.name,
                block.locator
            );
            assert_eq!(
                &doc.text[block.start_byte..block.end_byte],
                block.text,
                "{} · {} points at the wrong bytes",
                doc.name,
                block.locator
            );
        }
    }
}

/// Croatian text typed on an office PC arrives in Windows-1250, and losing a
/// ć there means losing it from a quote the owner is asked to recognise.
#[test]
fn diacritics_survive_a_legacy_encoding() {
    let note = fixtures().join("source/Termoval - Prodaja/04 Servis/Vrijeme-izlaska-na-teren.txt");
    let doc = extract_file(&note).expect("the note is readable");
    assert!(doc.text.contains("48 sati"), "the note states the contradicting figure");
    assert!(!doc.text.contains('\u{fffd}'), "no character was replaced during decoding");
}

/// A scan is not an empty document. Reporting it as one would quietly drop a
/// contract out of the company's knowledge.
#[test]
fn a_scan_is_refused_with_a_reason() {
    let scan = fixtures().join("source/Termoval - Prodaja/Skenirani-ugovor-Hotel-Adriatic.pdf");
    let err = extract_file(&scan).expect_err("a PDF with no text layer cannot be read");
    assert!(matches!(err, knowlith_extract::ExtractError::NoText));
}

/// A spreadsheet row has to stay addressable as a row, because that is how a
/// price is looked up rather than recalled.
#[test]
fn spreadsheet_rows_keep_their_sheet_and_row_number() {
    let sheet = fixtures().join("source/Termoval - Prodaja/02 Cjenici/Cjenik-2026.xlsx");
    let doc = extract_file(&sheet).expect("the price list is readable");

    let row = doc
        .blocks
        .iter()
        .find(|b| b.text.contains("FTXM35R"))
        .expect("the price list contains this model");
    assert_eq!(row.sheet.as_deref(), Some("Klima"));
    assert_eq!(row.locator, "Klima row 3");
    assert_eq!(
        row.cells.as_deref().unwrap().last().unwrap(),
        "892",
        "a whole price must not grow a decimal point it never had"
    );
}
