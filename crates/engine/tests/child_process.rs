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
use std::time::{Duration, Instant};

use knowlith_engine::{CliEngine, Engine, EngineError, Flavour, Request};

/// Writes an executable script and returns its path.
fn script(name: &str, body: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("knowlith-engine-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let mut file = std::fs::File::create(&path).unwrap();
    write!(file, "#!/bin/sh\n{body}").unwrap();
    file.sync_all().unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
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
    let path = script("hang", "sleep 300\n");
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
