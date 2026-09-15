//! Running the owner's own CLI as a child process.
//!
//! This is what "the reading happens on your Mac, on your subscription"
//! means concretely: Knowlith spawns `codex` or `claude`, hands it one
//! document on stdin, reads the answer, and the process exits. Nothing is
//! kept running between jobs — a long-lived child would hold state that does
//! not survive a crash, and every piece of state in this system has to live
//! in the lake instead.
//!
//! Both CLIs are invoked read-only and non-interactive. Neither is given a
//! writable workspace, because the compiler never needs one and a sandbox
//! escape is not a risk worth carrying for a convenience nobody asked for.
//!
//! Both are also invoked with the owner's own configuration switched off.
//! That is not tidiness. A developer's `CLAUDE.md`, hooks, plugins and agent
//! memory are loaded by these tools on every run, and they will happily
//! appear in the reply: the first real run of this code returned a hook's
//! warning about an unrelated tool as part of the answer. Worse than the
//! noise is what it implies — the compiler's output would depend on which
//! machine it ran on, and text the owner never wrote for us would be steering
//! a model that is reading their contracts.
//!
//! The isolation stops short of one flag on purpose. Claude Code's `--bare`
//! switches off the most, but it also refuses to read OAuth or the keychain,
//! so it only works with an API key — which would quietly turn "runs on your
//! existing subscription" into "bring your own billing". `--restricted`
//! plus `--strict-mcp-config` plus an empty working directory gets the same
//! isolation while the owner's login keeps working.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::{Engine, EngineError, Reply, Request, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavour {
    Codex,
    ClaudeCode,
}

impl Flavour {
    pub fn program(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::ClaudeCode => "Claude Code",
        }
    }

    /// Whether the CLI can enforce a JSON Schema itself. Codex takes
    /// `--output-schema`; Claude Code does not, so the schema goes into the
    /// prompt instead.
    fn schema_is_native(self) -> bool {
        matches!(self, Self::Codex)
    }
}

pub struct CliEngine {
    flavour: Flavour,
    /// The binary to run. Overridable so the tests can point at a script that
    /// behaves like a CLI without needing a provider account.
    program: String,
    model: Option<String>,
    name: String,
}

impl CliEngine {
    pub fn new(flavour: Flavour) -> Self {
        Self {
            flavour,
            program: flavour.program().to_string(),
            model: None,
            name: flavour.label().to_string(),
        }
    }

    /// Point the engine at another binary. Used by the tests, and by an owner
    /// whose CLI is not on `PATH`.
    pub fn with_program(mut self, program: impl Into<String>) -> Self {
        self.program = program.into();
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    fn arguments(&self, schema_path: Option<&str>, last_message_path: Option<&str>) -> Vec<String> {
        let mut args: Vec<String> = Vec::new();
        match self.flavour {
            Flavour::Codex => {
                args.push("exec".into());
                // Read-only, outside a git repo, and leaving no session files
                // behind: the compiler is a pure function of the document.
                args.push("--skip-git-repo-check".into());
                args.push("--ephemeral".into());
                args.push("--sandbox".into());
                args.push("read-only".into());
                // The owner's own Codex configuration and rule files stay
                // out of it; authentication still comes from CODEX_HOME.
                args.push("--ignore-user-config".into());
                args.push("--ignore-rules".into());
                args.push("--color".into());
                args.push("never".into());
                if let Some(path) = schema_path {
                    args.push("--output-schema".into());
                    args.push(path.into());
                }
                if let Some(path) = last_message_path {
                    // The final message, on its own, with none of the
                    // progress output around it.
                    args.push("--output-last-message".into());
                    args.push(path.into());
                }
                if let Some(model) = &self.model {
                    args.push("--model".into());
                    args.push(model.clone());
                }
                // A bare `-` makes Codex read the prompt from stdin.
                args.push("-".into());
            }
            Flavour::ClaudeCode => {
                args.push("--print".into());
                args.push("--output-format".into());
                args.push("text".into());
                // No user, project or local settings — which is where hooks
                // live — no MCP servers, and none of the tools that run
                // commands or code, because reading a document needs none.
                args.push("--restricted".into());
                args.push("--strict-mcp-config".into());
                if let Some(model) = &self.model {
                    args.push("--model".into());
                    args.push(model.clone());
                }
            }
        }
        args
    }
}

impl Engine for CliEngine {
    fn name(&self) -> &str {
        &self.name
    }

    fn run(&self, request: &Request) -> Result<Reply> {
        // Codex takes the schema as a file. It lives as long as the child.
        let schema_file = match (self.flavour.schema_is_native(), &request.schema) {
            (true, Some(schema)) => Some(write_schema(schema)?),
            _ => None,
        };
        let schema_path = schema_file.as_ref().map(|p| p.to_string_lossy().into_owned());

        let reply_file = match self.flavour {
            Flavour::Codex => Some(
                tempish::TempFile::with_contents("knowlith-reply", ".txt", b"")
                    .map_err(|e| EngineError::Transport(format!("could not create the reply file: {e}")))?,
            ),
            Flavour::ClaudeCode => None,
        };
        let reply_path = reply_file.as_ref().map(|p| p.to_string_lossy().into_owned());

        // An empty directory with nothing in it to discover. Project
        // instruction files and repository context are found by walking up
        // from the working directory, and the compiler must not inherit
        // whatever happens to be above the folder it was launched from.
        let workdir = tempish::TempDir::new("knowlith-run")
            .map_err(|e| EngineError::Transport(format!("could not create a working directory: {e}")))?;

        let mut child = Command::new(&self.program)
            .args(self.arguments(schema_path.as_deref(), reply_path.as_deref()))
            .current_dir(workdir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| match e.kind() {
                // Not installed is the owner's problem to fix, and retrying
                // it every five minutes would never fix it.
                std::io::ErrorKind::NotFound => EngineError::Unavailable(format!(
                    "{} is not installed, or not on PATH. Install it, or choose another processor.",
                    self.flavour.label()
                )),
                _ => EngineError::Transport(format!("could not start {}: {e}", self.program)),
            })?;

        let prompt = request.prompt(self.flavour.schema_is_native());
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| EngineError::Transport("the child process has no stdin".into()))?;
        // A separate thread, because a prompt larger than the pipe buffer
        // deadlocks against a child that is already writing to stdout.
        let writer = std::thread::spawn(move || {
            let _ = stdin.write_all(prompt.as_bytes());
            drop(stdin);
        });

        let output = wait_with_timeout(child, request.timeout, self.flavour.label())?;
        let _ = writer.join();
        drop(schema_file);
        drop(workdir);

        if !output.status.success() {
            // Some failures are reported on stdout, and an error with no
            // message is the least useful thing this can return.
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let said = if stderr.is_empty() {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            } else {
                stderr
            };
            let said = if said.is_empty() {
                "it gave no reason".to_string()
            } else {
                said
            };
            return Err(classify(self.flavour, output.status.code(), &said));
        }

        // Codex was asked to put its final message in a file; anything it
        // printed along the way is progress, not the answer.
        let text = reply_file
            .as_ref()
            .and_then(|f| std::fs::read_to_string(f.path()).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| String::from_utf8_lossy(&output.stdout).trim().to_string());
        drop(reply_file);

        if text.is_empty() {
            return Err(EngineError::Refused(format!(
                "{} exited successfully but said nothing",
                self.flavour.label()
            )));
        }

        Ok(Reply {
            text,
            engine: self.name.clone(),
        })
    }
}

/// Waits for the child, and kills it if it stops answering.
///
/// `std::process::Child` has no timed wait, so this polls `try_wait` against
/// a deadline while two threads drain stdout and stderr — draining them is
/// not optional, because a child that fills its pipe buffer blocks forever
/// and would never reach the deadline at all.
///
/// On timeout the child is actually killed. Letting it run would hold a job
/// lease that never returns, and keep a model answering a question nobody is
/// listening to any more.
fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
    label: &str,
) -> Result<std::process::Output> {
    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let out_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(pipe) = out_pipe.as_mut() {
            let _ = pipe.read_to_end(&mut buffer);
        }
        buffer
    });
    let err_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(pipe) = err_pipe.as_mut() {
            let _ = pipe.read_to_end(&mut buffer);
        }
        buffer
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(EngineError::Transport(format!(
                        "{label} did not answer within {} seconds, and was stopped",
                        timeout.as_secs()
                    )));
                }
                std::thread::sleep(POLL);
            }
            Err(e) => return Err(EngineError::Transport(format!("{label} could not be waited on: {e}"))),
        }
    };

    Ok(std::process::Output {
        status,
        stdout: out_reader.join().unwrap_or_default(),
        stderr: err_reader.join().unwrap_or_default(),
    })
}

/// How often the wait checks whether the child has finished. Short enough
/// that a fast reply is not delayed, long enough that a ten-minute run costs
/// nothing to watch.
const POLL: Duration = Duration::from_millis(25);

/// Decides whether the queue waits or a person is asked.
///
/// The distinction is read out of what the CLI actually said, because the
/// exit code alone does not carry it: both CLIs exit 1 for "rate limited" and
/// for "that prompt is invalid", and those need opposite responses.
fn classify(flavour: Flavour, code: Option<i32>, stderr: &str) -> EngineError {
    let lower = stderr.to_lowercase();
    let transport = [
        "rate limit",
        "429",
        "timeout",
        "timed out",
        "connection",
        "network",
        "unreachable",
        "temporarily unavailable",
        "503",
        "502",
        "overloaded",
        "usage limit",
    ];
    let unavailable = ["not logged in", "sign in", "login", "unauthorized", "401", "no credentials"];

    if unavailable.iter().any(|needle| lower.contains(needle)) {
        return EngineError::Unavailable(format!(
            "{} is installed but not signed in. Run `{} login`.",
            flavour.label(),
            flavour.program()
        ));
    }
    if transport.iter().any(|needle| lower.contains(needle)) {
        return EngineError::Transport(trim(stderr));
    }
    EngineError::Refused(format!(
        "{} exited with {} — {}",
        flavour.label(),
        code.map(|c| c.to_string()).unwrap_or_else(|| "a signal".into()),
        trim(stderr)
    ))
}

/// Pulls the actual complaint out of what a CLI printed.
///
/// Codex writes a banner — version, model, sandbox, session id — to stderr on
/// every single run, successful or not. Reading the first few hundred
/// characters therefore reports the banner and hides the error, which is
/// exactly what happened the first time a real quota limit was hit. The
/// error-looking lines win; failing that, the end wins, because that is where
/// a CLI puts what went wrong.
fn trim(s: &str) -> String {
    let complaints: Vec<&str> = s
        .lines()
        .map(str::trim)
        .filter(|line| {
            let lower = line.to_lowercase();
            lower.starts_with("error") || lower.contains("error:") || lower.starts_with("fatal")
        })
        .collect();

    let chosen = if complaints.is_empty() {
        s.trim().to_string()
    } else {
        // The same error repeated is still one error.
        let mut seen = Vec::new();
        for line in complaints {
            if !seen.contains(&line) {
                seen.push(line);
            }
        }
        seen.join(" ")
    };

    let flat = chosen.split_whitespace().collect::<Vec<_>>().join(" ");
    let count = flat.chars().count();
    if count <= 300 {
        return flat;
    }
    // The tail, not the head.
    let skipped = count - 300;
    format!("…{}", flat.chars().skip(skipped).collect::<String>())
}

fn write_schema(schema: &str) -> Result<tempish::TempFile> {
    tempish::TempFile::with_contents("knowlith-schema", ".json", schema.as_bytes())
        .map_err(|e| EngineError::Transport(format!("could not write the schema file: {e}")))
}

/// A temporary file that removes itself.
///
/// Small enough to own rather than take a dependency for, and owning it means
/// the file is gone the moment the child exits rather than at some later
/// cleanup that may never run.
mod tempish {
    use std::io::Write;
    use std::path::{Path, PathBuf};

    pub struct TempFile(PathBuf);

    impl TempFile {
        pub fn with_contents(prefix: &str, suffix: &str, bytes: &[u8]) -> std::io::Result<Self> {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let path = std::env::temp_dir().join(format!("{prefix}-{}-{unique}{suffix}", std::process::id()));
            let mut file = std::fs::File::create(&path)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            Ok(Self(path))
        }

        pub fn to_string_lossy(&self) -> std::borrow::Cow<'_, str> {
            self.0.to_string_lossy()
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    /// An empty directory that removes itself.
    pub struct TempDir(PathBuf);

    impl TempDir {
        pub fn new(prefix: &str) -> std::io::Result<Self> {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let path = std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()));
            std::fs::create_dir_all(&path)?;
            Ok(Self(path))
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_runs_read_only_and_leaves_nothing_behind() {
        let args = CliEngine::new(Flavour::Codex).arguments(None, None);
        assert!(args.contains(&"--sandbox".to_string()) && args.contains(&"read-only".to_string()));
        assert!(args.contains(&"--ephemeral".to_string()));
        assert!(args.contains(&"--skip-git-repo-check".to_string()));
        assert_eq!(args.last().unwrap(), "-", "the prompt goes in on stdin");
    }

    #[test]
    fn claude_runs_non_interactively() {
        let args = CliEngine::new(Flavour::ClaudeCode).arguments(None, None);
        assert!(args.contains(&"--print".to_string()));
    }

    #[test]
    fn neither_cli_inherits_the_owners_own_configuration() {
        let codex = CliEngine::new(Flavour::Codex).arguments(None, None);
        assert!(codex.contains(&"--ignore-user-config".to_string()));
        assert!(codex.contains(&"--ignore-rules".to_string()));

        let claude = CliEngine::new(Flavour::ClaudeCode).arguments(None, None);
        assert!(claude.contains(&"--restricted".to_string()));
        assert!(claude.contains(&"--strict-mcp-config".to_string()));
        assert!(
            !claude.contains(&"--bare".to_string()),
            "--bare would stop the owner's own subscription from being used"
        );
    }

    #[test]
    fn a_schema_file_is_only_passed_to_the_cli_that_understands_one() {
        assert!(
            CliEngine::new(Flavour::Codex)
                .arguments(Some("/tmp/s.json"), None)
                .contains(&"--output-schema".to_string())
        );
        assert!(
            !CliEngine::new(Flavour::ClaudeCode)
                .arguments(Some("/tmp/s.json"), None)
                .contains(&"--output-schema".to_string())
        );
    }

    #[test]
    fn the_codex_banner_does_not_hide_the_error() {
        let stderr = "OpenAI Codex v0.154.0\n--------\nworkdir: /tmp/x\nmodel: gpt-6-astra\nprovider: openai\napproval: never\nsandbox: read-only\nsession id: 01a0a6c5\n--------\nERROR: You've hit your usage limit. Try again at Sep 20th.";
        let message = trim(stderr);
        assert!(message.contains("usage limit"), "got: {message}");
        assert!(!message.contains("workdir"), "the banner is not the error");
    }

    #[test]
    fn the_same_error_repeated_is_reported_once() {
        let stderr = "ERROR: rate limited\nERROR: rate limited";
        assert_eq!(trim(stderr), "ERROR: rate limited");
    }

    #[test]
    fn a_rate_limit_is_waited_out_and_a_bad_prompt_is_not() {
        assert!(classify(Flavour::Codex, Some(1), "Error: 429 rate limit exceeded").is_retryable());
        assert!(!classify(Flavour::Codex, Some(1), "Error: invalid model name").is_retryable());
    }

    #[test]
    fn being_signed_out_asks_the_owner_rather_than_retrying() {
        let err = classify(Flavour::ClaudeCode, Some(1), "You are not logged in.");
        assert!(matches!(err, EngineError::Unavailable(_)));
        assert!(!err.is_retryable());
        assert!(format!("{err}").contains("claude login"));
    }

    #[test]
    fn a_missing_binary_is_reported_as_something_to_install() {
        let engine = CliEngine::new(Flavour::Codex).with_program("knowlith-no-such-binary");
        let err = engine
            .run(&Request::new("candidates", "x", "y"))
            .expect_err("a binary that does not exist cannot be run");
        assert!(matches!(err, EngineError::Unavailable(_)));
        assert!(!err.is_retryable(), "installing software is not a retry");
    }
}
