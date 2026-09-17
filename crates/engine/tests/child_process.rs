//! The child-process path, exercised against real processes.
//!
//! These spawn actual binaries — shell scripts that behave the way `codex`
//! and `claude` behave in the cases that matter — so the parts that only go
//! wrong at runtime are covered: a prompt larger than a pipe buffer, a child
//! that never exits, a child that writes to stderr and fails, one that
//! succeeds and says nothing.
//!
//! No provider account, no network, no cost. Running the real CLIs here would
//! make the suite slow, non-deterministic and billable, and would still not
//! test the hang.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use knowlith_engine::{CliEngine, Engine, EngineError, Flavour, Request};

static SCRIPT_SEQ: AtomicU64 = AtomicU64::new(0);

/// Writes an executable script and returns its path.
fn script(name: &str, body: &str) -> PathBuf {
    // Own directory per call: tests in this file run on threads that share a
    // PID, and Linux refuses to exec a file still open for write (ETXTBSY).
    // A shared folder plus an undropped File is how CI lost
    // `claude_json_is_the_only_source_of_tokens_and_price`.
    let dir = std::env::temp_dir().join(format!(
        "knowlith-engine-{}-{}-{}",
        std::process::id(),
        SCRIPT_SEQ.fetch_add(1, Ordering::Relaxed),
        name
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    {
        let mut file = std::fs::File::create(&path).unwrap();
        write!(file, "#!/bin/sh\n{body}").unwrap();
        file.sync_all().unwrap();
    }
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn pid_is_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn engine(path: &PathBuf) -> CliEngine {
    CliEngine::new(Flavour::ClaudeCode).with_program(path.to_string_lossy().into_owned())
}

#[test]
fn the_document_reaches_the_child_on_stdin_and_the_answer_comes_back() {
    let path = script("echo-back", "cat\n");
    let request = Request::new("candidates", "Find the rules.", "Popust od 5% za stalne kupce.");

    let reply = engine(&path).run(&request).expect("the child answered");

    assert!(reply.text.contains("Find the rules."), "instructions were sent");
    assert!(
        reply.text.contains("Popust od 5% za stalne kupce."),
        "the document was sent"
    );
    assert_eq!(reply.engine, "Claude Code");
}

#[test]
fn a_document_larger_than_a_pipe_buffer_does_not_deadlock() {
    // A pipe buffer is 64 KB on macOS. A child that writes while we write is
    // exactly the shape that deadlocks when either side is done on the main
    // thread, and a real price list is comfortably this big.
    let path = script("echo-back-big", "cat\n");
    let big = "Stavka | Cijena\n".repeat(40_000);
    let request = Request::new("candidates", "Read this.", big.clone());

    let reply = engine(&path)
        .run(&request.clone().with_timeout(Duration::from_secs(20)))
        .expect("a large prompt must still complete");

    assert!(reply.text.len() > 600_000, "the whole document came back");
}

#[test]
fn a_child_that_never_answers_is_stopped() {
    let path = script("hang", "sleep 8\n");
    let request = Request::new("candidates", "x", "y").with_timeout(Duration::from_millis(400));

    let started = Instant::now();
    let err = engine(&path).run(&request).expect_err("a hung child must not hang the daemon");
    let waited = started.elapsed();

    assert!(err.is_retryable(), "a hang is a transport problem, not a refusal");
    assert!(
        waited < Duration::from_secs(5),
        "the wait must end at the timeout, not at the child's own pace; took {waited:?}"
    );
    match &err {
        EngineError::Transport(message) => assert!(
            message.contains("was stopped") || message.contains("could not be waited on"),
            "unexpected transport message: {message}"
        ),
        other => panic!("expected a transport error, got {other:?}"),
    }
}

#[test]
fn a_rate_limit_from_the_child_is_waited_out() {
    let path = script("rate-limited", "echo 'Error: 429 rate limit exceeded' >&2\nexit 1\n");
    let err = engine(&path)
        .run(&Request::new("candidates", "x", "y"))
        .expect_err("a failing child is an error");
    assert!(err.is_retryable());
}

#[test]
fn a_bad_prompt_from_the_child_asks_a_person() {
    let path = script("bad-prompt", "echo 'Error: unknown model xyz' >&2\nexit 2\n");
    let err = engine(&path)
        .run(&Request::new("candidates", "x", "y"))
        .expect_err("a failing child is an error");
    assert!(matches!(err, EngineError::Refused(_)));
    assert!(!err.is_retryable());
}

#[test]
fn being_signed_out_is_reported_as_something_to_fix() {
    let path = script("signed-out", "echo 'You are not logged in. Run claude login.' >&2\nexit 1\n");
    let err = engine(&path)
        .run(&Request::new("candidates", "x", "y"))
        .expect_err("a failing child is an error");
    assert!(matches!(err, EngineError::Unavailable(_)));
    assert!(!err.is_retryable(), "signing in is not something a retry achieves");
}

#[test]
fn a_silent_success_is_not_treated_as_an_answer() {
    let path = script("silent", "cat > /dev/null\nexit 0\n");
    let err = engine(&path)
        .run(&Request::new("candidates", "x", "y"))
        .expect_err("an empty answer is not an answer");
    assert!(matches!(err, EngineError::Refused(_)));
}

#[test]
fn the_child_inherits_no_working_directory_it_could_write_to() {
    // The engine never passes a writable workspace flag. This pins that:
    // a future change that adds one has to change this test too.
    let path = script("dump-args", "echo \"$@\"\n");
    let reply = CliEngine::new(Flavour::Codex)
        .with_program(path.to_string_lossy().into_owned())
        .run(&Request::new("candidates", "x", "y"))
        .expect("the child answered");
    assert!(reply.text.contains("--sandbox read-only"));
    assert!(!reply.text.contains("workspace-write"));
    assert!(!reply.text.contains("--add-dir"));
    assert!(!reply.text.contains("dangerously"));
}

#[test]
fn claude_json_is_the_only_source_of_tokens_and_price() {
    let path = script(
        "claude-json",
        r#"cat >/dev/null
printf '%s\n' '{"type":"result","result":"{\"candidates\":[]}","usage":{"input_tokens":12,"output_tokens":3},"total_cost_usd":0.01,"model":"claude-sonnet"}'
"#,
    );
    let reply = engine(&path)
        .run(&Request::new("candidates", "x", "y"))
        .expect("the child answered");
    assert_eq!(reply.text, "{\"candidates\":[]}");
    let usage = reply.usage.expect("the envelope carried a bill");
    assert_eq!(usage.input_tokens, Some(12));
    assert_eq!(usage.output_tokens, Some(3));
    assert_eq!(usage.cost_usd, Some(0.01));
}

#[test]
fn codex_progress_jsonl_does_not_replace_the_answer() {
    let path = script(
        "codex-jsonl",
        r#"last=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--output-last-message" ]; then
    shift
    last=$1
  fi
  shift
done
cat >/dev/null
printf '%s\n' '{"candidates":[]}' > "$last"
printf '%s\n' '{"payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":50,"cached_input_tokens":4,"output_tokens":7}}}}'
"#,
    );
    let reply = CliEngine::new(Flavour::Codex)
        .with_program(path.to_string_lossy().into_owned())
        .run(&Request::new("candidates", "x", "y"))
        .expect("the child answered");
    assert_eq!(reply.text, "{\"candidates\":[]}");
    let usage = reply.usage.expect("jsonl carried tokens");
    assert_eq!(usage.input_tokens, Some(50));
    assert_eq!(usage.output_tokens, Some(7));
    assert_eq!(usage.cache_tokens, Some(4));
    assert!(usage.cost_usd.is_none());
}

#[test]
fn excessive_child_output_is_refused_without_unbounded_allocation() {
    let path = script("noisy", "head -c 9000000 /dev/zero\n");
    let result = engine(&path).run(&Request::new("candidates", "x", "y").with_timeout(Duration::from_secs(5)));
    assert!(matches!(result, Err(EngineError::Refused(_))), "oversized output must not become a reply");
}

#[test]
fn a_grandchild_holding_output_open_is_also_bounded_by_the_deadline() {
    let pid_file = std::env::temp_dir().join(format!(
        "knowlith-grandchild-{}-{}",
        std::process::id(),
        SCRIPT_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let path = script(
        "inherited-pipe",
        &format!(
            "sleep 8 &\nprintf '%s\\n' \"$!\" > '{}'\nexit 0\n",
            pid_file.display()
        ),
    );
    let path_for_thread = path.clone();
    let start = Instant::now();
    let handle = std::thread::spawn(move || {
        engine(&path_for_thread)
            .run(&Request::new("candidates", "x", "y").with_timeout(Duration::from_secs(2)))
    });
    let pid = loop {
        if let Ok(text) = std::fs::read_to_string(&pid_file) {
            if let Ok(pid) = text.trim().parse::<u32>() {
                break pid;
            }
        }
        if start.elapsed() > Duration::from_secs(2) {
            let result = handle.join();
            panic!(
                "grandchild pid was never written to {}; engine={result:?}",
                pid_file.display()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let result = handle.join().expect("the wait thread must return");
    assert!(result.is_err(), "expected a timeout, got {result:?}");
    assert!(start.elapsed() < Duration::from_secs(5));
    std::thread::sleep(Duration::from_millis(80));
    assert!(
        !pid_is_alive(pid),
        "grandchild {pid} must die with the process group, or cargo test hangs on Linux"
    );
}

#[test]
fn a_cancelled_run_kills_the_child() {
    let path = script("hang", "sleep 8\n");
    let cancel = Arc::new(AtomicBool::new(false));
    let request = Request::new("candidates", "x", "y")
        .with_timeout(Duration::from_secs(30))
        .with_cancel(Arc::clone(&cancel));
    let path_for_thread = path.clone();
    let started = Instant::now();
    let handle = std::thread::spawn(move || engine(&path_for_thread).run(&request));
    std::thread::sleep(Duration::from_millis(80));
    cancel.store(true, Ordering::Relaxed);
    let result = handle.join().expect("the wait thread must return");
    assert!(result.is_err(), "a cancelled run must not look like a reply: {result:?}");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the child must die on cancel, not at the timeout; took {:?}",
        started.elapsed()
    );
}

#[test]
fn a_success_exit_with_a_cli_error_envelope_is_not_a_model_reply() {
    let path = script("quota-envelope", "printf '%s\n' '{\"type\":\"result\",\"is_error\":true,\"result\":\"Credit balance is too low\"}'\n");
    let result = engine(&path).run(&Request::new("candidates", "x", "y"));
    assert!(matches!(result, Err(EngineError::Unavailable(_))), "error envelope must pause the account: {result:?}");
}

#[test]
fn stdout_account_error_is_not_hidden_by_stderr_progress() {
    let path = script("quota-stdout", "echo 'starting reader' >&2\necho '429 insufficient_quota'\nexit 1\n");
    let result = engine(&path).run(&Request::new("candidates", "x", "y"));
    assert!(matches!(result, Err(EngineError::Unavailable(_))));
}

#[test]
fn claude_does_not_run_two_children_at_once() {
    // Claude Code's own ProcessTransport races when two `claude` processes
    // write at once. Knowlith must serialise those children; the busy-file
    // is how we see an overlap the wall clock could miss on a fast machine.
    let dir = std::env::temp_dir().join(format!(
        "knowlith-claude-lock-{}-{}",
        std::process::id(),
        SCRIPT_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let busy = dir.join("busy");
    let overlap = dir.join("overlap");
    let path = script(
        "locked",
        &format!(
            r#"busy='{}'
overlap='{}'
if ! mkdir "$busy" 2>/dev/null; then echo overlap > "$overlap"; fi
sleep 0.3
rmdir "$busy" 2>/dev/null || true
printf '%s\n' '{{"type":"result","result":"ok"}}'
"#,
            busy.display(),
            overlap.display()
        ),
    );
    let a = engine(&path);
    let b = engine(&path);
    let start = Instant::now();
    let left = std::thread::spawn(move || a.run(&Request::new("candidates", "x", "y")));
    let right = std::thread::spawn(move || b.run(&Request::new("candidates", "x", "y")));
    left.join().unwrap().expect("first child");
    right.join().unwrap().expect("second child");
    assert!(
        !overlap.exists(),
        "two Claude children overlapped — the exclusive lock is missing"
    );
    assert!(
        start.elapsed() >= Duration::from_millis(500),
        "two 300ms children that ran together would finish sooner; took {:?}",
        start.elapsed()
    );
}

#[test]
fn cursor_children_may_run_together() {
    let dir = std::env::temp_dir().join(format!(
        "knowlith-cursor-lock-{}-{}",
        std::process::id(),
        SCRIPT_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let started = dir.join("started");
    let path = script(
        "together",
        &format!(
            r#"started='{}'
echo x >> "$started"
n=0
while [ "$(wc -l < "$started" | tr -d ' ')" -lt 2 ] && [ "$n" -lt 40 ]; do
  n=$((n + 1))
  sleep 0.05
done
sleep 0.15
printf '%s\n' '{{"type":"result","result":"ok"}}'
"#,
            started.display()
        ),
    );
    let a = CliEngine::new(Flavour::CursorAgent).with_program(path.to_string_lossy().into_owned());
    let b = CliEngine::new(Flavour::CursorAgent).with_program(path.to_string_lossy().into_owned());
    let start = Instant::now();
    let left = std::thread::spawn(move || a.run(&Request::new("candidates", "x", "y")));
    let right = std::thread::spawn(move || b.run(&Request::new("candidates", "x", "y")));
    left.join().unwrap().expect("first child");
    right.join().unwrap().expect("second child");
    assert!(
        start.elapsed() < Duration::from_millis(1500),
        "Cursor is allowed to overlap; a global lock would serialise these past 1.5s; took {:?}",
        start.elapsed()
    );
    let lines = std::fs::read_to_string(&started).unwrap_or_default();
    assert!(
        lines.lines().count() >= 2,
        "both Cursor children should have started: {lines:?}"
    );
}

#[test]
fn killing_the_child_leaves_what_it_already_said_on_disk() {
    let path = script("partial", "printf 'PARTIAL-ANSWER\\n'; sleep 8\n");
    let journal = std::env::temp_dir().join(format!(
        "knowlith-journal-{}-{}",
        std::process::id(),
        SCRIPT_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let cancel = Arc::new(AtomicBool::new(false));
    let request = Request::new("candidates", "x", "y")
        .with_timeout(Duration::from_secs(30))
        .with_cancel(Arc::clone(&cancel))
        .with_journal(journal.clone());
    let path_for_thread = path.clone();
    let handle = std::thread::spawn(move || engine(&path_for_thread).run(&request));
    let started = Instant::now();
    loop {
        let body = std::fs::read_to_string(&journal).unwrap_or_default();
        if body.contains("PARTIAL-ANSWER") {
            break;
        }
        if started.elapsed() > Duration::from_secs(3) {
            panic!("the journal was never written; contents={body:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    cancel.store(true, Ordering::Relaxed);
    let _ = handle.join();
    let body = std::fs::read_to_string(&journal).expect("the journal must survive the child");
    assert!(
        body.contains("PARTIAL-ANSWER"),
        "a killed child must leave its session on disk, got {body:?}"
    );
}
