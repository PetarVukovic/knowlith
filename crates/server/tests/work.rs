//! What the work panel is told.
//!
//! The panel is the first thing an owner sees after adding a folder, and
//! everything on it is a claim: that work is happening, how far along it
//! is, and what came out of each document. These tests hold those claims
//! to the queue rather than to a stored field that could drift from it.

use axum::body::Body;
use axum::http::Request;
use knowlith_lake::{Lake, NewJob, PRIORITY_NORMAL};
use knowlith_server::{AppState, auth::Token, router};
use serde_json::Value;
use tower::ServiceExt;

const SECRET: &str = "token";

fn lake() -> Lake {
    // A counter, not only a timestamp: two tests on two threads read the
    // same nanosecond, shared one lake file, and the second one's job was
    // silently swallowed by the idempotency key the first had already
    // used. That failed about one run in ten and looked like a queue bug.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "knowlith-work-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("a directory");
    Lake::open(&dir.join("lake.sqlite")).expect("a lake")
}

fn queue(lake: &Lake, kind: &str, key: &str, payload: &str) {
    lake.enqueue(&NewJob {
        kind: kind.into(),
        payload: payload.into(),
        idempotency_key: key.into(),
        priority: PRIORITY_NORMAL,
    })
    .expect("queued");
}

async fn work(lake: Lake) -> Value {
    let app = router(AppState::with_token(lake, "Test Company", Token::from_value(SECRET)));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/work")
                .header(knowlith_server::auth::HEADER, SECRET)
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("a response");
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("a body");
    serde_json::from_slice(&bytes).expect("json")
}

#[tokio::test]
async fn an_empty_queue_says_so_rather_than_inventing_progress() {
    let out = work(lake()).await;
    assert_eq!(out["stage"], "idle");
    assert_eq!(out["done"], 0);
    assert_eq!(out["total"], 0);
    assert!(out["lines"].as_array().expect("lines").is_empty());
}

#[tokio::test]
async fn the_stage_is_the_earliest_thing_still_outstanding() {
    // A relate job sits behind three documents. The owner is reading, not
    // preparing skills, whatever order the queue happens to be in.
    let lake = lake();
    queue(&lake, "relate", "rel", "{}");
    queue(&lake, "compile_document", "c1", "{}");
    queue(&lake, "compile_document", "c2", "{}");
    let out = work(lake).await;
    assert_eq!(out["stage"], "reading");
    assert_eq!(out["doing"], "Reading the documents");
    assert_eq!(out["total"], 3);
}

#[tokio::test]
async fn walking_the_folder_is_its_own_stage() {
    let lake = lake();
    queue(&lake, "rescan", "r", "{}");
    let out = work(lake).await;
    assert_eq!(out["stage"], "reading");
    assert_eq!(out["doing"], "Looking through your folder");
}

#[tokio::test]
async fn settling_is_not_reported_as_reading() {
    let lake = lake();
    queue(&lake, "settle", "s", "{}");
    let out = work(lake).await;
    assert_eq!(out["stage"], "thinking");
}

#[tokio::test]
async fn skills_and_connections_are_the_last_stage() {
    let lake = lake();
    queue(&lake, "draft_skills", "s", "{}");
    let out = work(lake).await;
    assert_eq!(out["stage"], "preparing");
}

/// The bug this exists to prevent: `From doc:9982ba86ea32837c` describes
/// the owner's own file in a vocabulary they have no way to read. The same
/// mistake already shipped once on the activity feed.
#[tokio::test]
async fn a_line_names_the_document_rather_than_its_id() {
    let mut lake = lake();
    lake.put_source("src-1", "Prodaja", "/tmp/prodaja", "folder", "codex")
        .expect("a source");
    let document = knowlith_extract::extract_bytes(
        std::path::Path::new("/tmp/prodaja/Cjenik 2026.md"),
        b"# Cjenik\n\nPopust je 5%.\n",
        "2026-01-15T09:00:00Z",
    )
    .expect("a document");
    lake.put_document("src-1", &document).expect("stored");

    queue(
        &lake,
        "compile_document",
        "c",
        &format!(r#"{{"document_id":"{}"}}"#, document.id),
    );
    let job = lake.lease().expect("lease").expect("a job");
    lake.finish(job.id, "14 claims · 2 not read").expect("finished");

    let out = work(lake).await;
    let line = &out["lines"][0];
    assert_eq!(line["subject"], "Cjenik 2026.md");
    assert_eq!(line["note"], "14 claims · 2 not read");
    assert_eq!(line["state"], "done");
}

/// A job about the whole lake has no document to name, and "a document
/// that is no longer there" would be a lie about a job that went fine.
#[tokio::test]
async fn work_on_everything_is_named_as_such() {
    let lake = lake();
    queue(&lake, "settle", "s", "{}");
    let job = lake.lease().expect("lease").expect("a job");
    lake.finish(job.id, "51 kept · 3 conflicts").expect("finished");

    let out = work(lake).await;
    assert_eq!(out["lines"][0]["subject"], "Everything read so far");
}

#[tokio::test]
async fn a_document_that_would_not_read_is_reported_with_its_reason() {
    let lake = lake();
    queue(&lake, "compile_document", "c", r#"{"document_id":"doc:gone"}"#);
    let job = lake.lease().expect("lease").expect("a job");
    lake.fail(job.id, "the file is password protected").expect("failed");

    let out = work(lake).await;
    assert_eq!(out["lines"][0]["state"], "failed");
    assert_eq!(out["lines"][0]["note"], "the file is password protected");
}

/// The queue stores `battery`; the owner is owed a sentence.
#[tokio::test]
async fn held_work_explains_itself_in_words() {
    let lake = lake();
    queue(&lake, "compile_document", "c", "{}");
    let job = lake.lease().expect("lease").expect("a job");
    lake.hold(job.id, knowlith_lake::Held::Battery.as_str()).expect("held");

    let out = work(lake).await;
    assert_eq!(out["held"]["reason"], "on battery");
    assert_eq!(out["held"]["count"], 1);
    // And it is not repeated as a line of its own for every job.
    assert!(out["lines"].as_array().expect("lines").is_empty());
}

/// The worker puts the file's name in front of its own sentence, which is
/// right on a command line and wrong in a table that already has a column
/// for it.
#[tokio::test]
async fn the_document_name_is_not_printed_twice_on_one_row() {
    let mut lake = lake();
    lake.put_source("src-1", "Prodaja", "/tmp/prodaja", "folder", "codex")
        .expect("a source");
    let document = knowlith_extract::extract_bytes(
        std::path::Path::new("/tmp/prodaja/Cjenik.md"),
        b"# Cjenik\n\nPopust je 5%.\n",
        "2026-01-15T09:00:00Z",
    )
    .expect("a document");
    lake.put_document("src-1", &document).expect("stored");

    queue(
        &lake,
        "compile_document",
        "c",
        &format!(r#"{{"document_id":"{}"}}"#, document.id),
    );
    let job = lake.lease().expect("lease").expect("a job");
    lake.finish(job.id, "Cjenik.md: 4 claims · 0 not read").expect("finished");

    let out = work(lake).await;
    assert_eq!(out["lines"][0]["subject"], "Cjenik.md");
    assert_eq!(out["lines"][0]["note"], "4 claims · 0 not read");
}

/// A note that merely begins with the same word must survive intact.
#[tokio::test]
async fn only_an_exact_prefix_is_dropped() {
    let lake = lake();
    queue(&lake, "settle", "s", "{}");
    let job = lake.lease().expect("lease").expect("a job");
    lake.finish(job.id, "Everything read so far was fine").expect("finished");

    let out = work(lake).await;
    assert_eq!(out["lines"][0]["note"], "Everything read so far was fine");
}

/// The hourly re-check of stored quotes runs for the life of the
/// installation. Reported when it found nothing, it would be the only
/// line on an idle machine's panel — renewed every hour, saying nothing.
#[tokio::test]
async fn housekeeping_that_found_nothing_is_not_news() {
    let lake = lake();
    queue(&lake, "recheck", "h", "{}");
    let job = lake.lease().expect("lease").expect("a job");
    lake.finish(job.id, "").expect("finished");

    let out = work(lake).await;
    assert!(out["lines"].as_array().expect("lines").is_empty());
}
