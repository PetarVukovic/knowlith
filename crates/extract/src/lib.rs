//! Structural extraction: bytes in, addressable blocks out.
//!
//! This crate is the floor the whole trust argument stands on, and it contains
//! no inference whatsoever. It never decides what a document means, never asks
//! a model anything, and never reaches the network. Given the same bytes it
//! produces the same text, the same blocks and the same offsets on every
//! machine — which is what lets the evidence gate be a check rather than an
//! opinion, and what lets the golden tests run in CI without a provider key.

pub mod builder;
pub mod delimited;
pub mod docx;
pub mod error;
pub mod inventory;
pub mod pdf;
pub mod prose;
pub mod sheet;

use std::path::Path;

use knowlith_core::{Document, DocumentKind, document_id, sha256_hex};

pub use error::{ExtractError, Skipped};

/// Reads one file from disk.
pub fn extract_file(path: &Path) -> Result<Document, ExtractError> {
    let bytes = std::fs::read(path)?;
    let modified = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
        .unwrap_or_default();
    extract_bytes(path, &bytes, &modified)
}

/// Reads one file that is already in memory.
///
/// Taking the bytes rather than the path is what makes golden tests possible:
/// a fixture can be checked in, hashed and replayed without touching a clock
/// or a filesystem.
pub fn extract_bytes(path: &Path, bytes: &[u8], modified: &str) -> Result<Document, ExtractError> {
    if bytes.is_empty() {
        return Err(ExtractError::Empty);
    }

    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
    let kind = DocumentKind::from_extension(ext).ok_or(ExtractError::UnsupportedType)?;

    let (text, blocks, columns) = match kind {
        DocumentKind::Markdown | DocumentKind::Text => {
            let text = decode(bytes);
            let blocks = prose::blocks(&text);
            (text, blocks, None)
        }
        DocumentKind::Csv => {
            let raw = decode(bytes);
            let (columns, rendition) = delimited::parse(&name, &raw);
            let (text, blocks) = rendition.finish();
            (text, blocks, columns)
        }
        DocumentKind::Xlsx => {
            let (columns, rendition) = sheet::parse(bytes)?;
            let (text, blocks) = rendition.finish();
            (text, blocks, columns)
        }
        DocumentKind::Docx => {
            let (text, blocks) = docx::parse(bytes)?.finish();
            (text, blocks, None)
        }
        DocumentKind::Pdf => {
            let (text, blocks) = pdf::parse(bytes)?.finish();
            (text, blocks, None)
        }
    };

    if blocks.is_empty() {
        return Err(ExtractError::NoText);
    }

    let sha256 = sha256_hex(bytes);
    Ok(Document {
        id: document_id(&sha256),
        path: path.to_string_lossy().into_owned(),
        name,
        kind,
        byte_len: bytes.len() as u64,
        text_sha256: sha256_hex(text.as_bytes()),
        verbatim: text.as_bytes() == bytes,
        sha256,
        text,
        modified: modified.to_string(),
        columns,
        blocks,
    })
}

/// Decodes text that is probably, but not certainly, UTF-8.
///
/// Files written on Windows in this region are often Windows-1250, and a lossy
/// UTF-8 read turns every č and ć into a replacement character — which then
/// travels into a quote the owner is asked to recognise. When the bytes are
/// valid UTF-8 this returns them unchanged, so the rendition stays verbatim.
fn decode(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    let (text, _, _) = encoding_rs::WINDOWS_1250.decode(bytes);
    text.into_owned()
}

/// Files that are never worth reading, recognised by name alone.
///
/// Dotfiles and Office lock files are noise. Keys, certs and env files are
/// secrets — skipping them is the allowlist/denylist pattern from filesystem
/// MCP servers, applied at the edge before anything reaches the lake.
pub fn is_noise(name: &str) -> bool {
    name.starts_with("~$")
        || name.starts_with('.')
        || name.eq_ignore_ascii_case("Thumbs.db")
        || name.eq_ignore_ascii_case("desktop.ini")
        || is_secret(name)
}

/// Names that must not enter the lake even if the extension is readable.
pub fn is_secret(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "id_rsa"
            | "id_dsa"
            | "id_ecdsa"
            | "id_ed25519"
            | "credentials.json"
            | "serviceaccount.json"
            | "secrets.json"
            | "auth.json"
    ) || lower.ends_with(".pem")
        || lower.ends_with(".key")
        || lower.ends_with(".pfx")
        || lower.ends_with(".p12")
        || lower.ends_with(".keystore")
        || lower.ends_with(".jks")
        || lower.ends_with(".env")
        || lower.ends_with(".env.local")
        || lower.ends_with(".env.production")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_stays_verbatim() {
        let text = "# Uvjeti\n\nPopust je 5%.\n";
        let doc = extract_bytes(
            Path::new("/tmp/uvjeti.md"),
            text.as_bytes(),
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        assert!(doc.verbatim);
        assert_eq!(doc.text, text);
        assert_eq!(doc.kind, DocumentKind::Markdown);
    }

    #[test]
    fn a_built_rendition_is_not_verbatim() {
        let doc = extract_bytes(
            Path::new("/tmp/c.csv"),
            b"Stavka,Cijena\nUgradnja,450\n",
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        assert!(!doc.verbatim);
        assert_eq!(doc.columns.as_deref().unwrap(), ["Stavka", "Cijena"]);
    }

    #[test]
    fn windows_1250_diacritics_survive() {
        // "plaćanje" in Windows-1250: ć is 0xE6.
        let bytes = [b'p', b'l', b'a', 0xE6, b'a', b'n', b'j', b'e'];
        assert_eq!(decode(&bytes), "plaćanje");
    }

    #[test]
    fn identity_of_a_document_follows_its_content() {
        let a = extract_bytes(Path::new("/a/x.md"), b"# Isti\n", "2026-01-01T00:00:00Z").unwrap();
        let b = extract_bytes(Path::new("/b/y.md"), b"# Isti\n", "2027-06-06T00:00:00Z").unwrap();
        assert_eq!(a.id, b.id, "moving or renaming a file must not orphan it");
    }

    #[test]
    fn unsupported_types_are_refused_by_name() {
        let err = extract_bytes(Path::new("/tmp/a.key"), b"x", "").unwrap_err();
        assert!(matches!(err, ExtractError::UnsupportedType));
    }

    #[test]
    fn json_is_read_as_text() {
        let doc = extract_bytes(
            Path::new("/tmp/partneri-osiguravatelji.json"),
            br#"{"ime":"Allianz","uvjeti":"cjenik usluga"}"#,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        assert_eq!(doc.kind, DocumentKind::Text);
        assert!(doc.text.contains("Allianz"), "company JSON must reach the lake");
        assert!(is_secret("credentials.json"));
        assert!(!is_secret("partneri-osiguravatelji.json"));
    }

    #[test]
    fn noise_is_recognised() {
        assert!(is_noise("~$ponuda.docx"));
        assert!(is_noise(".DS_Store"));
        assert!(is_noise("id_rsa"));
        assert!(is_noise("server.pem"));
        assert!(is_noise("prod.env"));
        assert!(!is_noise("Cjenik-2026.xlsx"));
    }
}
