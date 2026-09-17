//! Event-driven rescans for watched source folders.
//!
//! Periodic rescans remain the safety net; this layer notices a file landing
//! on disk and queues a walk after write-settle, Renfield-style, without
//! waiting for the next six-hour tick.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use knowlith_lake::Lake;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::{enqueue_rescan, WRITE_SETTLE_SECS};

const REFRESH_ROOTS: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_secs(1);

/// Starts a background thread that watches active source roots.
pub fn spawn(db: PathBuf, stop: Arc<AtomicBool>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        if let Err(e) = run(&db, stop) {
            eprintln!("source watcher stopped: {e}");
        }
    })
}

fn run(db: &Path, stop: Arc<AtomicBool>) -> Result<(), String> {
    let (tx, rx) = channel();
    let mut watcher = RecommendedWatcher::new(
        move |result| {
            if let Ok(event) = result {
                let _ = tx.send(event);
            }
        },
        Config::default(),
    )
    .map_err(|e| e.to_string())?;

    let mut roots: HashMap<String, PathBuf> = HashMap::new();
    let mut pending: HashMap<String, Instant> = HashMap::new();
    let mut last_refresh = Instant::now()
        .checked_sub(REFRESH_ROOTS)
        .unwrap_or(Instant::now());

    while !stop.load(Ordering::Relaxed) {
        if last_refresh.elapsed() >= REFRESH_ROOTS {
            refresh_roots(db, &mut watcher, &mut roots)?;
            last_refresh = Instant::now();
        }

        match rx.recv_timeout(POLL) {
            Ok(event) => {
                if !event_interesting(&event.kind) {
                    continue;
                }
                for path in &event.paths {
                    if let Some(source_id) = source_for_path(path, &roots) {
                        pending.insert(source_id, Instant::now());
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        let due_after = Duration::from_secs(WRITE_SETTLE_SECS);
        let now = Instant::now();
        let due: Vec<String> = pending
            .iter()
            .filter(|(_, started)| now.duration_since(**started) >= due_after)
            .map(|(id, _)| id.clone())
            .collect();
        for id in due {
            pending.remove(&id);
            if let Some(root) = roots.get(&id) {
                if let Ok(lake) = Lake::open(db) {
                    let root = root.to_string_lossy().into_owned();
                    let _ = enqueue_rescan(&lake, &id, &root);
                }
            }
        }
    }
    Ok(())
}

fn refresh_roots(
    db: &Path,
    watcher: &mut RecommendedWatcher,
    roots: &mut HashMap<String, PathBuf>,
) -> Result<(), String> {
    let lake = Lake::open(db).map_err(|e| e.to_string())?;
    let sources = lake.sources().map_err(|e| e.to_string())?;
    let mut live = HashMap::new();
    for (id, _name, root, _kind, status, _last) in sources {
        if status != "active" {
            continue;
        }
        let path = PathBuf::from(&root);
        if !path.is_dir() {
            continue;
        }
        live.insert(id.clone(), path.clone());
        if !roots.contains_key(&id) {
            let _ = watcher.watch(&path, RecursiveMode::Recursive);
            roots.insert(id, path);
        }
    }
    for id in roots.keys().cloned().collect::<Vec<_>>() {
        if !live.contains_key(&id) {
            if let Some(path) = roots.remove(&id) {
                let _ = watcher.unwatch(&path);
            }
        }
    }
    Ok(())
}

fn event_interesting(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

fn source_for_path(path: &Path, roots: &HashMap<String, PathBuf>) -> Option<String> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    roots
        .iter()
        .filter(|(_, root)| path.starts_with(root))
        .max_by_key(|(_, root)| root.components().count())
        .map(|(id, _)| id.clone())
}
