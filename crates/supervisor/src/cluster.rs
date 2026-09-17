//! Deterministic community grouping before the supervisor reads anything.

use std::collections::BTreeMap;

use knowlith_core::Document;
use knowlith_lake::CommunityRow;

/// Groups documents by parent folder name and invoice-like filename patterns.
pub fn communities(documents: &[Document]) -> Vec<CommunityRow> {
    let mut buckets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for doc in documents {
        let label = community_label(doc);
        buckets.entry(label).or_default().push(doc.id.clone());
    }
    buckets
        .into_iter()
        .enumerate()
        .map(|(i, (label, document_ids))| CommunityRow {
            id: format!("community:{i}"),
            label,
            document_ids,
        })
        .collect()
}

fn community_label(doc: &Document) -> String {
    let path = std::path::Path::new(&doc.path);
    let parent = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("documents");
    let name = doc.name.to_ascii_lowercase();
    if name.contains("inv") || name.contains("invoice") || name.contains("racun") {
        return format!("{parent} · invoices");
    }
    if name.ends_with(".xlsx") || name.ends_with(".csv") {
        return format!("{parent} · spreadsheets");
    }
    if name.ends_with(".pdf") {
        return format!("{parent} · PDF");
    }
    parent.to_string()
}
