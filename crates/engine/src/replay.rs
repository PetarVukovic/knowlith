//! Recorded answers, played back.
//!
//! Without this the pipeline could only be tested by paying a provider and
//! accepting a different answer every run — which means it could not be
//! tested at all. A cassette is one recorded reply, keyed by the exact
//! request that produced it, so the compiler can be exercised end to end on
//! any machine, offline, with the same result every time.
//!
//! The key deliberately covers the instructions as well as the document: a
//! changed prompt is a different question, and silently replaying the old
//! answer to it would make a prompt regression invisible.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::{Engine, EngineError, Reply, Request, Result};

/// The cassette filename for a request.
pub fn cassette_key(request: &Request) -> String {
    let mut hasher = Sha256::new();
    hasher.update(request.stage.as_bytes());
    hasher.update([0]);
    hasher.update(request.instructions.as_bytes());
    hasher.update([0]);
    hasher.update(request.schema.as_deref().unwrap_or("").as_bytes());
    hasher.update([0]);
    hasher.update(request.input.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("{}-{}", slug(&request.stage), &digest[..16])
}

fn slug(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect()
}

/// Plays back recorded replies and never reaches anything.
pub struct ReplayEngine {
    dir: PathBuf,
    name: String,
}

impl ReplayEngine {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            name: "Replay".into(),
        }
    }

    fn path(&self, request: &Request) -> PathBuf {
        self.dir.join(format!("{}.txt", cassette_key(request)))
    }
}

impl Engine for ReplayEngine {
    fn name(&self) -> &str {
        &self.name
    }

    fn run(&self, request: &Request) -> Result<Reply> {
        let path = self.path(request);
        match std::fs::read_to_string(&path) {
            Ok(text) => Ok(Reply::new(self.name.clone(), text)),
            // Not a transport problem: no amount of waiting produces a
            // recording. The message says exactly how to make one, because
            // the person hitting this is always in the middle of changing a
            // prompt.
            Err(_) => Err(EngineError::Refused(format!(
                "no recording for this request at {}. Re-record with a real engine wrapped in RecordingEngine.",
                path.display()
            ))),
        }
    }
}

/// Wraps a real engine and writes down everything it says.
///
/// This is how the cassettes are made: run the suite once against Codex or
/// Claude Code with the recorder in place, then commit what lands in the
/// directory. From then on CI needs neither.
pub struct RecordingEngine<E: Engine> {
    inner: E,
    dir: PathBuf,
}

impl<E: Engine> RecordingEngine<E> {
    pub fn new(inner: E, dir: impl Into<PathBuf>) -> Self {
        Self {
            inner,
            dir: dir.into(),
        }
    }

    pub fn directory(&self) -> &Path {
        &self.dir
    }
}

impl<E: Engine> Engine for RecordingEngine<E> {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn run(&self, request: &Request) -> Result<Reply> {
        let reply = self.inner.run(request)?;
        if let Err(e) = std::fs::create_dir_all(&self.dir) {
            return Err(EngineError::Transport(format!(
                "could not create the cassette directory {}: {e}",
                self.dir.display()
            )));
        }
        let path = self.dir.join(format!("{}.txt", cassette_key(request)));
        if let Err(e) = std::fs::write(&path, &reply.text) {
            return Err(EngineError::Transport(format!(
                "could not write {}: {e}",
                path.display()
            )));
        }
        Ok(reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Canned(&'static str);

    impl Engine for Canned {
        fn name(&self) -> &str {
            "Canned"
        }
        fn run(&self, _request: &Request) -> Result<Reply> {
            Ok(Reply::new("Canned", self.0))
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("knowlith-replay-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_recording_plays_back_identically() {
        let dir = scratch("roundtrip");
        let request = Request::new("candidates", "Find the rules.", "Popust je 5%.");

        let recorder = RecordingEngine::new(Canned("{\"rules\":[]}"), &dir);
        let recorded = recorder.run(&request).unwrap();

        let replay = ReplayEngine::new(&dir);
        assert_eq!(replay.run(&request).unwrap().text, recorded.text);
    }

    #[test]
    fn changing_the_prompt_invalidates_the_recording() {
        let dir = scratch("prompt-change");
        let original = Request::new("candidates", "Find the rules.", "Popust je 5%.");
        RecordingEngine::new(Canned("old answer"), &dir).run(&original).unwrap();

        let reworded = Request::new("candidates", "Find every rule, and quote it.", "Popust je 5%.");
        let err = ReplayEngine::new(&dir)
            .run(&reworded)
            .expect_err("a reworded prompt must not silently replay the old answer");
        assert!(!err.is_retryable());
    }

    #[test]
    fn the_same_document_in_a_different_stage_is_a_different_recording() {
        let a = Request::new("candidates", "Same.", "Same document.");
        let b = Request::new("consolidation", "Same.", "Same document.");
        assert_ne!(cassette_key(&a), cassette_key(&b));
    }

    #[test]
    fn a_missing_recording_is_never_retried() {
        let err = ReplayEngine::new(scratch("empty"))
            .run(&Request::new("candidates", "x", "y"))
            .unwrap_err();
        assert!(!err.is_retryable(), "waiting does not create a recording");
    }
}
