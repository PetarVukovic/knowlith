//! Portable Markdown export of approved knowledge.
//!
//! The lake stays operational; files under `knowledge/` are the owner's
//! backup and the rebuildable truth second-brain expects.

use std::fs;
use std::path::{Path, PathBuf};

use knowlith_core::{ContextObject, ObjectStatus};

use crate::{Lake, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportReport {
    pub root: PathBuf,
    pub written: usize,
    pub skipped: usize,
}

/// Writes every approved object with evidence to `base`, one Markdown file per
/// `object.path`. Existing files are overwritten when the body changed.
pub fn export_approved(base: &Path, lake: &Lake) -> Result<ExportReport> {
    fs::create_dir_all(base)?;
    let mut written = 0usize;
    let mut skipped = 0usize;
    for object in lake.objects()? {
        if object.status != ObjectStatus::Approved || object.evidence.is_empty() {
            skipped += 1;
            continue;
        }
        let rel = object.path.trim_start_matches('/');
        let dest = base.join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = render_markdown(&object);
        let existing = fs::read_to_string(&dest).unwrap_or_default();
        if existing != text {
            fs::write(&dest, text)?;
            written += 1;
        } else {
            skipped += 1;
        }
    }
    Ok(ExportReport {
        root: base.to_path_buf(),
        written,
        skipped,
    })
}

fn render_markdown(object: &ContextObject) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("id: {}\n", object.id));
    out.push_str(&format!("kind: {:?}\n", object.kind).to_lowercase());
    out.push_str(&format!("status: approved\n"));
    out.push_str(&format!("confidence: {}\n", object.confidence.0));
    out.push_str(&format!("updated_at: {}\n", object.updated_at));
    out.push_str("---\n\n");
    out.push_str(&format!("# {}\n\n", object.title));
    out.push_str(&object.body);
    if !object.evidence.is_empty() {
        out.push_str("\n\n## Evidence\n\n");
        for span in &object.evidence {
            out.push_str(&format!(
                "- **{}** `{}` — \"{}\"\n",
                span.document_id, span.locator, span.quote.replace('\n', " ")
            ));
        }
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use knowlith_core::{Confidence, ContextObject, Evidence, ObjectKind, ObjectStatus};
    use knowlith_extract::extract_bytes;
    use std::path::Path;

    #[test]
    fn approved_objects_land_as_markdown_under_the_export_root() {
        let export_root = std::env::temp_dir().join(format!("knowlith-export-{}", std::process::id()));
        let _ = fs::remove_dir_all(&export_root);
        let mut lake = Lake::in_memory().unwrap();
        lake.put_source("main", "Main", "/tmp/main", "folder", "codex")
            .unwrap();

        let quote = "Discounts may not exceed ten percent without approval.";
        let doc = extract_bytes(
            Path::new("/tmp/main/policy.md"),
            quote.as_bytes(),
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        lake.put_document("main", &doc).unwrap();
        let (start, end) = knowlith_core::locate(&doc, quote).unwrap();

        let object = ContextObject {
            id: "rule:sales.discount".into(),
            kind: ObjectKind::Rule,
            subtype: None,
            title: "Discount ceiling".into(),
            body: "No discount above 10% without manager approval.".into(),
            status: ObjectStatus::Approved,
            confidence: Confidence(0.9),
            version: 1,
            valid_from: "2026-01-01".into(),
            valid_to: None,
            supersedes: None,
            decided_by: None,
            edited_on_approval: false,
            evidence: vec![Evidence {
                document_id: doc.id.clone(),
                locator: "policy.md".into(),
                start_byte: start,
                end_byte: end,
                quote: quote.into(),
            }],
            relations: vec![],
            path: "rules/sales/discount.md".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        };
        lake.put_object(&object).unwrap();

        let report = export_approved(&export_root, &lake).unwrap();
        assert_eq!(report.written, 1);
        let file = export_root.join("rules/sales/discount.md");
        assert!(file.is_file());
        let text = fs::read_to_string(file).unwrap();
        assert!(text.contains("Discount ceiling"));
        assert!(text.contains("Evidence"));
        let _ = fs::remove_dir_all(&export_root);
    }
}
