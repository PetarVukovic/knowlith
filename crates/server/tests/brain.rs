//! What the company brain draws.
//!
//! Every node and edge on the map is a claim that the company knows
//! something and where it came from. These tests hold the map to the lake:
//! only approved objects, only the documents they actually quote, and
//! edges labelled in words the owner reads rather than in the relation
//! enum's names.

use axum::body::Body;
use axum::http::Request;
use knowlith_core::{Confidence, ContextObject, Evidence, ObjectKind, ObjectStatus};
use knowlith_lake::Lake;
use knowlith_server::{AppState, auth::Token, router};
use serde_json::Value;
use tower::ServiceExt;

const SECRET: &str = "token";

fn lake() -> Lake {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "knowlith-brain-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("a directory");
    Lake::open(&dir.join("lake.sqlite")).expect("a lake")
}

fn object(id: &str, title: &str, status: ObjectStatus, evidence: Vec<Evidence>) -> ContextObject {
    ContextObject {
        id: id.into(),
        kind: ObjectKind::Rule,
        subtype: None,
        title: title.into(),
        body: title.into(),
        status,
        confidence: Confidence(0.9),
        version: 1,
        valid_from: "2026-01-01T00:00:00Z".into(),
        valid_to: None,
        supersedes: None,
        decided_by: None,
        edited_on_approval: false,
        evidence,
        relations: Vec::new(),
        path: format!("rules/{id}.md"),
        updated_at: "2026-01-01T00:00:00Z".into(),
    }
}

async fn brain(lake: Lake) -> Value {
    projection(lake, "/api/brain").await
}

async fn projection(lake: Lake, uri: &str) -> Value {
    let app = router(AppState::with_token(lake, "Test Company", Token::from_value(SECRET)));
    let response = app
        .oneshot(
            Request::builder()
                .uri(uri)
                .header(knowlith_server::auth::HEADER, SECRET)
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("a response");
    assert_eq!(response.status(), 200);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("a body");
    serde_json::from_slice(&bytes).expect("json")
}

/// A folder of forty files and three approved rules is a brain of three
/// rules and the one or two files they quote — not forty-three things.
#[tokio::test]
async fn only_documents_something_approved_quotes_are_drawn_and_by_name() {
    let mut lake = lake();
    lake.put_source("s1", "Prodaja", "/tmp/prodaja", "folder", "codex")
        .expect("a source");
    let quoted = knowlith_extract::extract_bytes(
        std::path::Path::new("/tmp/prodaja/Uvjeti prodaje.md"),
        b"# Uvjeti\n\nPopust za stalne kupce iznosi 5%.\n",
        "2026-01-15T09:00:00Z",
    )
    .expect("a document");
    let unquoted = knowlith_extract::extract_bytes(
        std::path::Path::new("/tmp/prodaja/Biljeske.md"),
        "# Bilješke\n\nNešto nevažno.\n".as_bytes(),
        "2026-01-15T09:00:00Z",
    )
    .expect("a document");
    lake.put_document("s1", &quoted).expect("stored");
    lake.put_document("s1", &unquoted).expect("stored");

    let quote = "Popust za stalne kupce iznosi 5%.";
    let start = quoted.text.find(quote).expect("the quote");
    let span = Evidence {
        document_id: quoted.id.clone(),
        locator: "§1".into(),
        start_byte: start,
        end_byte: start + quote.len(),
        quote: quote.into(),
    };
    lake.put_object(&object("rule:discount", "Popust", ObjectStatus::Approved, vec![span.clone()]))
        .expect("stored");
    // A draft quoting the same file must not be what puts the file on the map.
    lake.put_object(&object("rule:draft", "Nacrt", ObjectStatus::Proposed, vec![span]))
        .expect("stored");

    let out = brain(lake).await;
    let nodes = out["nodes"].as_array().expect("nodes");
    let titles: Vec<&str> = nodes.iter().map(|n| n["title"].as_str().unwrap()).collect();
    assert_eq!(titles, vec!["Popust", "Uvjeti prodaje.md"], "{nodes:#?}");
    assert_eq!(nodes[1]["kind"], "document");

    let edges = out["edges"].as_array().expect("edges");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["from"], "rule:discount");
    assert_eq!(edges[0]["to"], quoted.id);
    assert_eq!(edges[0]["type"], "quoted_in");
    assert_eq!(edges[0]["label"], "quoted in");
}

/// Evidence ids already carry `doc:`. Doubling the prefix broke document nodes.
#[tokio::test]
async fn a_document_id_that_already_starts_with_doc_is_not_doubled() {
    let mut lake = lake();
    let quoted = knowlith_extract::extract_bytes(
        std::path::Path::new("/tmp/inv.md"),
        b"# Invoice\n\nAmount: 100 EUR.\n",
        "2026-01-15T09:00:00Z",
    )
    .expect("a document");
    lake.put_source("s1", "Invoices", "/tmp", "folder", "codex")
        .expect("a source");
    lake.put_document("s1", &quoted).expect("stored");

    let quote = "Amount: 100 EUR.";
    let start = quoted.text.find(quote).expect("the quote");
    let span = Evidence {
        document_id: quoted.id.clone(),
        locator: "§1".into(),
        start_byte: start,
        end_byte: start + quote.len(),
        quote: quote.into(),
    };
    lake.put_object(&object("fact:amount", "Amount", ObjectStatus::Approved, vec![span]))
        .expect("stored");

    let out = brain(lake).await;
    let nodes = out["nodes"].as_array().expect("nodes");
    let doc = nodes.iter().find(|n| n["kind"] == "document").expect("a document node");
    assert_eq!(doc["id"], quoted.id);
    assert_eq!(out["edges"][0]["to"], quoted.id);
}

/// `depends_on` is the relation's name in the code. On the map the owner
/// reads "needs" — and the same word everywhere the arrow appears.
#[tokio::test]
async fn an_edge_between_two_rules_carries_the_owners_word_for_it() {
    let mut lake = lake();
    lake.put_object(&object("rule:a", "A", ObjectStatus::Approved, Vec::new()))
        .expect("stored");
    let mut b = object("rule:b", "B", ObjectStatus::Approved, Vec::new());
    b.relations = vec![knowlith_core::Relation {
        target_id: "rule:a".into(),
        target_label: "A".into(),
        kind: knowlith_core::RelationType::DependsOn,
        origin: knowlith_core::RelationOrigin::Model,
        why: None,
        edge_confidence: None,
    }];
    lake.put_object(&b).expect("stored");

    let out = brain(lake).await;
    let edges = out["edges"].as_array().expect("edges");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["type"], "depends_on");
    assert_eq!(edges[0]["label"], "needs");
}

#[tokio::test]
async fn build_projection_shows_real_discoveries_without_approving_them() {
    let mut lake = lake();
    lake.put_source("s", "Company", "/tmp", "folder", "codex").unwrap();
    let doc = knowlith_extract::extract_bytes(std::path::Path::new("/tmp/terms.md"), b"Payment terms.", "2026-09-17T00:00:00Z").unwrap();
    lake.put_document("s", &doc).unwrap();
    let span = Evidence { document_id: doc.id.clone(), locator: "terms".into(), start_byte: 0, end_byte: 14, quote: "Payment terms.".into() };
    lake.put_object(&object("rule:new", "New rule", ObjectStatus::Proposed, vec![span.clone()])).unwrap();
    lake.put_object(&object("rule:no", "Rejected rule", ObjectStatus::Rejected, vec![span])).unwrap();
    let result = projection(lake, "/api/brain/build").await;
    let nodes = result["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 2);
    assert!(nodes.iter().any(|n| n["id"] == "rule:new" && n["status"] == "draft"));
    assert!(nodes.iter().any(|n| n["id"] == doc.id && n["status"] == "extracted"));
    assert_eq!(result["edges"].as_array().unwrap().len(), 1);
    assert_eq!(result["counts"]["approved"], 0);
    assert_eq!(result["counts"]["discoveries"], 1);
    assert_eq!(result["counts"]["documents"], 1);
}

#[tokio::test]
async fn build_projection_includes_documents_before_any_claim_exists() {
    let mut lake = lake();
    lake.put_source("s", "Company", "/tmp", "folder", "codex").unwrap();
    let doc = knowlith_extract::extract_bytes(std::path::Path::new("/tmp/read.md"), b"First file.", "2026-09-17T00:00:00Z").unwrap();
    lake.put_document("s", &doc).unwrap();
    let result = projection(lake, "/api/brain/build").await;
    assert_eq!(result["nodes"].as_array().unwrap().len(), 1);
    assert!(result["edges"].as_array().unwrap().is_empty());
}
