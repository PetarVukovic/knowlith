//! What the Sources screen's buttons do.
//!
//! "Paused" and "Remove source" were, for a while, badges the browser set on
//! its own copy of the list while the worker kept walking the folder. These
//! tests hold each button to the row the worker actually reads.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use knowlith_lake::Lake;
use knowlith_server::{AppState, auth::Token, router};
use serde_json::Value;
use tower::ServiceExt;

const SECRET: &str = "token";

/// One lake on disk, opened once for the test's own assertions and once per
/// request for the router — the same file, the way the daemon and the
/// worker share it.
struct Fixture {
    path: std::path::PathBuf,
    lake: Lake,
}

fn lake() -> Fixture {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "knowlith-sources-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("a directory");
    let path = dir.join("lake.sqlite");
    let lake = Lake::open(&path).expect("a lake");
    lake.put_source("invoices-1", "Invoices", "/tmp/invoices", "folder", "codex")
        .expect("a source");
    Fixture { path, lake }
}

async fn call(fixture: &Fixture, method: Method, uri: &str, body: &str) -> (StatusCode, Value) {
    let app = router(AppState::with_token(
        Lake::open(&fixture.path).expect("the same lake"),
        "Test Company",
        Token::from_value(SECRET),
    ));
    let response = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header(knowlith_server::auth::HEADER, SECRET)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .expect("a request"),
        )
        .await
        .expect("a response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("a body");
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, json)
}

fn targets(fixture: &Fixture) -> Vec<String> {
    fixture
        .lake
        .scan_targets()
        .expect("targets")
        .into_iter()
        .map(|t| t.id)
        .collect()
}

#[tokio::test]
async fn pausing_a_folder_takes_it_off_the_workers_list() {
    let lake = lake();
    assert_eq!(targets(&lake), vec!["invoices-1"]);

    let (status, out) = call(&lake, Method::PUT, "/api/sources/invoices-1/status", r#"{"paused":true}"#).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(out["status"], "paused");
    assert!(targets(&lake).is_empty(), "a paused folder must not be walked");

    let (_, listed) = call(&lake, Method::GET, "/api/sources", "").await;
    assert_eq!(listed[0]["status"], "paused");

    let (_, out) = call(&lake, Method::PUT, "/api/sources/invoices-1/status", r#"{"paused":false}"#).await;
    assert_eq!(out["status"], "active");
    assert_eq!(targets(&lake), vec!["invoices-1"]);
}

#[tokio::test]
async fn a_walk_that_finishes_after_pause_does_not_resume_the_folder() {
    let lake = lake();
    lake.lake.set_source_paused("invoices-1", true).expect("paused");
    lake.lake.mark_scanned("invoices-1").expect("scanned");
    assert!(targets(&lake).is_empty());
}

#[tokio::test]
async fn removing_a_folder_hides_it_but_keeps_what_was_read() {
    let lake = lake();
    let (status, _) = call(&lake, Method::DELETE, "/api/sources/invoices-1", "").await;
    assert_eq!(status, StatusCode::OK);

    assert!(targets(&lake).is_empty(), "a removed folder must not be walked");
    let (_, listed) = call(&lake, Method::GET, "/api/sources", "").await;
    assert!(listed.as_array().expect("a list").is_empty());
    // The row is still there — documents and evidence cascade from it.
    assert!(lake.lake.sources().expect("rows").iter().any(|(id, ..)| id == "invoices-1"));
}

#[tokio::test]
async fn a_folder_that_was_never_read_does_not_carry_a_date() {
    let lake = lake();
    let (_, listed) = call(&lake, Method::GET, "/api/sources", "").await;
    assert!(listed[0]["lastAnalyzed"].is_null(), "no walk, no date — the screen printed Invalid Date");
    assert_eq!(listed[0]["processor"], "codex");
}

#[tokio::test]
async fn a_source_lists_files_and_what_quotes_them() {
    let mut lake = lake();
    let doc = knowlith_core::Document {
        id: "doc:abc123".into(),
        path: "/tmp/invoices/INV-001.pdf".into(),
        name: "INV-001.pdf".into(),
        kind: knowlith_core::DocumentKind::Pdf,
        byte_len: 100,
        sha256: "abc123".repeat(8),
        text: "Total due in 15 days.".into(),
        text_sha256: "def".into(),
        verbatim: false,
        modified: "2026-01-01T00:00:00Z".into(),
        columns: None,
        blocks: vec![knowlith_core::Block {
            locator: "page 1".into(),
            kind: knowlith_core::BlockKind::Paragraph,
            text: "Total due in 15 days.".into(),
            start_byte: 0,
            end_byte: 22,
            page: Some(1),
            sheet: None,
            row: None,
            cells: None,
        }],
    };
    lake.lake.put_document("invoices-1", &doc).expect("stored");
    let (status, out) = call(&lake, Method::GET, "/api/sources/invoices-1/documents", "").await;
    assert_eq!(status, StatusCode::OK);
    let files = out.as_array().expect("files");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["name"], "INV-001.pdf");
    assert!(files[0]["quotedBy"].as_array().expect("quotes").is_empty());
}

#[tokio::test]
async fn a_folder_nobody_added_cannot_be_paused_or_removed() {
    let lake = lake();
    let (status, _) = call(&lake, Method::PUT, "/api/sources/nope/status", r#"{"paused":true}"#).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(&lake, Method::DELETE, "/api/sources/nope", "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
