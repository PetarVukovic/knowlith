//! Knowing when to stop asking.
//!
//! A provider outage during a scan of two thousand documents produces two
//! thousand failing jobs, each of which retries. Without a breaker that is a
//! few thousand spawned processes and a great deal of somebody's rate limit
//! spent discovering the same fact over and over.
//!
//! After a run of transport failures the engine is held open: requests are
//! refused immediately, cheaply, and still as [`EngineError::Transport`] so
//! the queue keeps waiting rather than giving up. One success closes it.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::{Engine, EngineError, Reply, Request, Result};

/// Consecutive transport failures before the engine is held open.
const THRESHOLD: u32 = 5;
/// How long to stay open before letting one request through to find out.
const COOLDOWN: Duration = Duration::from_secs(60);

#[derive(Debug, Default)]
struct State {
    consecutive: u32,
    opened_at: Option<Instant>,
}

pub struct Breaker<E: Engine> {
    inner: E,
    state: Mutex<State>,
}

impl<E: Engine> Breaker<E> {
    pub fn new(inner: E) -> Self {
        Self {
            inner,
            state: Mutex::new(State::default()),
        }
    }

    /// Whether the engine is currently being held open. The status bar reads
    /// this to say "worker paused" rather than leaving the owner watching a
    /// queue that is not moving for no visible reason.
    pub fn is_open(&self) -> bool {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        match state.opened_at {
            Some(at) => at.elapsed() < COOLDOWN,
            None => false,
        }
    }
}

impl<E: Engine> Engine for Breaker<E> {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn run(&self, request: &Request) -> Result<Reply> {
        if self.is_open() {
            return Err(EngineError::Transport(format!(
                "{} is not answering; waiting before trying again",
                self.inner.name()
            )));
        }

        let result = self.inner.run(request);

        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        match &result {
            Ok(_) => {
                state.consecutive = 0;
                state.opened_at = None;
            }
            Err(e) if e.is_retryable() => {
                state.consecutive += 1;
                if state.consecutive >= THRESHOLD {
                    state.opened_at = Some(Instant::now());
                }
            }
            // A refusal is about this one request, not about the engine, so
            // it must not count towards opening the breaker — otherwise five
            // malformed documents would take the whole scan down.
            Err(_) => {}
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct Counting {
        calls: AtomicU32,
        result: fn() -> Result<Reply>,
    }

    impl Counting {
        fn new(result: fn() -> Result<Reply>) -> Self {
            Self {
                calls: AtomicU32::new(0),
                result,
            }
        }
    }

    impl Engine for Counting {
        fn name(&self) -> &str {
            "Counting"
        }
        fn run(&self, _request: &Request) -> Result<Reply> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            (self.result)()
        }
    }

    fn request() -> Request {
        Request::new("candidates", "x", "y")
    }

    #[test]
    fn an_outage_stops_being_asked_after_five_tries() {
        let breaker = Breaker::new(Counting::new(|| Err(EngineError::Transport("no network".into()))));
        for _ in 0..12 {
            let _ = breaker.run(&request());
        }
        assert!(breaker.is_open());
        assert_eq!(
            breaker.inner.calls.load(Ordering::SeqCst),
            THRESHOLD,
            "the engine must stop being spawned once it is clearly down"
        );
    }

    #[test]
    fn the_queue_still_waits_rather_than_giving_up() {
        let breaker = Breaker::new(Counting::new(|| Err(EngineError::Transport("no network".into()))));
        for _ in 0..THRESHOLD {
            let _ = breaker.run(&request());
        }
        let err = breaker.run(&request()).unwrap_err();
        assert!(err.is_retryable(), "an open breaker must not look like a permanent failure");
    }

    #[test]
    fn bad_documents_do_not_take_the_engine_down() {
        let breaker = Breaker::new(Counting::new(|| Err(EngineError::Refused("bad json".into()))));
        for _ in 0..20 {
            let _ = breaker.run(&request());
        }
        assert!(!breaker.is_open());
        assert_eq!(breaker.inner.calls.load(Ordering::SeqCst), 20);
    }

    #[test]
    fn one_success_clears_the_count() {
        let failing = Breaker::new(Counting::new(|| Err(EngineError::Transport("blip".into()))));
        let _ = failing.run(&request());
        let _ = failing.run(&request());
        assert!(!failing.is_open());

        let working = Breaker::new(Counting::new(|| {
            Ok(Reply {
                text: "ok".into(),
                engine: "Counting".into(),
            })
        }));
        assert!(working.run(&request()).is_ok());
        assert!(!working.is_open());
    }
}
