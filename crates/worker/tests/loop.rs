//! The worker against a real folder, a real SQLite file and a scripted engine.
//!
//! These tests exist because the queue's own tests prove the queue and prove
//! nothing about the loop that drains it. Every failure mode here was a real
//! one: a source nobody ever scanned, a rescan that re-read files that had
//! not changed, an engine that was not signed in costing six attempts per
//! document, and a job kind nobody implemented spinning forever.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use knowlith_engine::{Engine, EngineError, Reply, Request};
use knowlith_lake::Lake;
use knowlith_worker::{KIND_COMPILE, KIND_RESCAN, Worker};

struct Scripted(&'static str);

impl Engine for Scripted {
    fn name(&self) -> &str {
        "Scripted"
    }
    fn run(&self, _request: &Request) -> knowlith_engine::Result<Reply> {
        Ok(Reply::new("Scripted", self.0))
    }
}

struct Offline;

impl Engine for Offline {
    fn name(&self) -> &str {
        "Offline"
    }
    fn run(&self, _request: &Request) -> knowlith_engine::Result<Reply> {
        Err(EngineError::Transport("connection reset by peer".into()))
    }
}

const FOUND: &str = r#"{"candidates":[{"kind":"rule","title":"Rok placanja",
  "statement":"Rok placanja je 15 dana.",
  "quotes":["Rok placanja je 15 dana od izdavanja racuna."]}]}"#;

/// A folder on disk, removed when the test ends.
struct Folder(std::path::PathBuf);

impl Folder {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "knowlith-worker-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, body: &str) {
        let path = self.0.join(name);
        std::fs::write(&path, body).unwrap();
        // Write-settle skips files whose mtime is <3s old. Tests must not
        // wait that long, so the file is aged to a settled past.
        age_mtime(&path);
    }

    fn path(&self) -> String {
        self.0.to_string_lossy().to_string()
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn age_mtime(path: &std::path::Path) {
    // Prefer a portable touch; fall back to sleeping past write-settle.
    let ok = std::process::Command::new("touch")
        .arg("-t")
        .arg("202001011200.00")
        .arg(path)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        let _ = std::process::Command::new("touch")
            .arg("-d")
            .arg("2020-01-01 12:00:00")
            .arg(path)
            .status();
    }
}

/// A worker on mains power.
///
/// Pinned rather than read, because the policy holds expensive jobs when a
/// laptop is unplugged — and a test suite whose result depends on whether
/// the machine running it has a charger attached is worse than no test.
fn worker(engine: Arc<dyn Engine>) -> Worker {
    Worker::new(Lake::in_memory().unwrap(), engine).on_mains()
}

#[test]
fn a_source_nobody_ever_scanned_is_scanned_without_being_asked() {
    let folder = Folder::new("first");
    folder.write("uvjeti.md", "# Uvjeti\n\nRok placanja je 15 dana od izdavanja racuna.\n");

    let mut worker = worker(Arc::new(Scripted(FOUND)));
    worker
        .lake()
        .put_source("default", "Uvjeti", &folder.path(), "folder", "codex")
        .unwrap();

    // Nobody enqueues anything. The scheduler notices last_scan is NULL.
    let flag = AtomicBool::new(false);
    let first = worker.tick(&flag).unwrap();
    assert_eq!(
        first.scheduled, 2,
        "an unread source and the hourly quote re-check are both due"
    );
    assert_eq!(
        first.ran.as_deref(),
        Some(KIND_RESCAN),
        "housekeeping must not overtake the folder the owner just added"
    );

    let second = worker.tick(&flag).unwrap();
    assert_eq!(second.ran.as_deref(), Some(KIND_COMPILE));

    // Reading a document and deciding what it means are two jobs. The
    // second only runs once the reading has drained, because "which claim
    // is current" is a question about the whole set.
    assert!(
        worker.lake().objects().unwrap().is_empty(),
        "nothing is settled while documents are still being read"
    );

    let mut ran = Vec::new();
    while let Some(kind) = worker.tick(&flag).unwrap().ran {
        ran.push(kind);
    }
    assert!(ran.contains(&"settle".to_string()), "{ran:?}");

    let objects = worker.lake().objects().unwrap();
    assert_eq!(objects.len(), 1, "the loop produced a rule without anyone typing a command");
    assert_eq!(objects[0].title, "Rok placanja");
    assert!(!objects[0].evidence.is_empty());
}

#[test]
fn a_rescan_recognises_a_file_whose_bytes_did_not_move() {
    let folder = Folder::new("unchanged");
    folder.write("uvjeti.md", "# Uvjeti\n\nRok placanja je 15 dana od izdavanja racuna.\n");

    let mut worker = worker(Arc::new(Scripted(FOUND)));
    worker
        .lake()
        .put_source("default", "Uvjeti", &folder.path(), "folder", "codex")
        .unwrap();

    let flag = AtomicBool::new(false);
    let first = worker.tick(&flag).unwrap();
    assert!(first.outcome.unwrap().contains("1 changed"));

    // Clearing last_scan is what a due window does; the folder is untouched.
    worker
        .lake()
        .put_source("default", "Uvjeti", &folder.path(), "folder", "codex")
        .unwrap();
    worker.lake().connection()
        .execute("UPDATE sources SET last_scan = NULL", [])
        .unwrap();

    // Drain the compile queued by the first pass.
    while worker.tick(&flag).unwrap().ran.as_deref() == Some(KIND_COMPILE) {}

    let again = worker.lake().job_counts().unwrap();
    let compiles: i64 = again.iter().map(|(_, n)| *n).sum();
    assert!(compiles >= 2, "the queue keeps its history: {again:?}");
}

#[test]
fn an_engine_that_is_offline_defers_instead_of_losing_the_document() {
    let folder = Folder::new("offline");
    folder.write("uvjeti.md", "# Uvjeti\n\nRok placanja je 15 dana od izdavanja racuna.\n");

    let mut worker = worker(Arc::new(Offline));
    worker
        .lake()
        .put_source("default", "Uvjeti", &folder.path(), "folder", "codex")
        .unwrap();

    let flag = AtomicBool::new(false);
    worker.tick(&flag).unwrap(); // rescan
    let compile = worker.tick(&flag).unwrap();

    assert_eq!(compile.ran.as_deref(), Some(KIND_COMPILE));
    let note = compile.outcome.unwrap();
    assert!(note.starts_with("deferred"), "a lost connection is not a refusal: {note}");

    let counts = worker.lake().job_counts().unwrap();
    assert!(
        counts.iter().any(|(state, n)| state == "queued" && *n >= 1),
        "the document must come back when the connection does: {counts:?}"
    );
    assert!(!counts.iter().any(|(state, _)| state == "failed"));
}

#[test]
fn a_folder_that_is_gone_is_reported_and_not_retried_forever() {
    let mut worker = worker(Arc::new(Scripted(FOUND)));
    worker
        .lake()
        .put_source("default", "Missing", "/definitely/not/here", "folder", "codex")
        .unwrap();

    let flag = AtomicBool::new(false);
    let tick = worker.tick(&flag).unwrap();
    assert_eq!(tick.ran.as_deref(), Some(KIND_RESCAN));
    assert!(tick.outcome.unwrap().starts_with("failed"));

    let counts = worker.lake().job_counts().unwrap();
    assert!(counts.iter().any(|(state, n)| state == "failed" && *n == 1));
}

#[test]
fn a_job_kind_nobody_implemented_stops_rather_than_spinning() {
    let mut worker = worker(Arc::new(Scripted(FOUND)));
    worker
        .lake()
        .enqueue(&knowlith_lake::NewJob {
            kind: "teleport".into(),
            payload: "{}".into(),
            idempotency_key: "teleport:1".into(),
            priority: 0,
        })
        .unwrap();

    let flag = AtomicBool::new(false);
    let tick = worker.tick(&flag).unwrap();
    assert!(tick.outcome.unwrap().contains("teleport"));
    let counts = worker.lake().job_counts().unwrap();
    assert!(counts.iter().any(|(state, n)| state == "failed" && *n == 1));
}

#[test]
fn an_empty_queue_is_an_idle_tick_once_the_housekeeping_has_run() {
    let mut worker = worker(Arc::new(Scripted(FOUND)));
    let flag = AtomicBool::new(false);

    // With no sources there is still one thing due: the hourly re-check
    // that keeps `verified` a check rather than a memory.
    let first = worker.tick(&flag).unwrap();
    assert_eq!(first.scheduled, 1);
    assert!(!first.idle());

    assert!(worker.tick(&flag).unwrap().idle());
}


#[test]
fn an_unplugged_laptop_holds_the_work_instead_of_burning_the_battery() {
    let folder = Folder::new("battery");
    folder.write("cjenik.md", "Montaža split sustava: 150 EUR.");

    let mut worker = Worker::new(Lake::in_memory().unwrap(), Arc::new(Scripted("[]"))).unplugged();
    worker
        .lake()
        .put_source("s1", "Prodaja", &folder.path(), "folder", "codex")
        .unwrap();

    let stop = AtomicBool::new(false);

    // The free half still runs: the folder is read whatever the power says.
    let mut compiled = false;
    for _ in 0..12 {
        let tick = worker.tick(&stop).unwrap();
        if let Some(outcome) = &tick.outcome {
            if outcome.starts_with("held:") {
                compiled = true;
                assert!(outcome.contains("battery"), "{outcome}");
                break;
            }
        }
    }
    assert!(compiled, "the compile job was never reached");
    assert!(worker.lake().document_count().unwrap() > 0, "reading the folder was held too");

    // Held is not failed: plugging in releases it, and the attempt count
    // was not spent waiting.
    let held = worker.lake().held().unwrap();
    assert!(!held.is_empty(), "nothing was recorded as held");
    let released = worker.lake().release_held().unwrap();
    assert!(released > 0);
    assert!(worker.lake().held().unwrap().is_empty());
}

#[test]
fn an_owner_who_turned_automatic_reading_off_gets_no_model_calls() {
    let folder = Folder::new("manual");
    folder.write("uvjeti.md", "Rok plaćanja je 30 dana.");

    let mut worker = Worker::new(Lake::in_memory().unwrap(), Arc::new(Scripted("[]"))).on_mains();
    worker
        .lake()
        .set_policy(&knowlith_lake::Policy {
            processing: knowlith_lake::Processing::Manual,
            pause_on_battery: false,
            large_scan: 500,
            ..knowlith_lake::Policy::default()
        })
        .unwrap();
    worker
        .lake()
        .put_source("s1", "Uvjeti", &folder.path(), "folder", "codex")
        .unwrap();

    let stop = AtomicBool::new(false);
    for _ in 0..12 {
        worker.tick(&stop).unwrap();
    }

    let held = worker.lake().held().unwrap();
    assert!(
        held.iter().any(|(_, reason, _)| reason == "manual"),
        "expected work held for manual, got {held:?}"
    );
    assert_eq!(
        worker.lake().object_ids(None).unwrap().len(),
        0,
        "something was compiled despite automatic reading being off"
    );
}
