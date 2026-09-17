//! Running the owner's own CLI as a child process.
//!
//! This is what "the reading happens on your Mac, on your subscription"
//! means concretely: Knowlith spawns `codex`, `claude` or `agent`, hands it
//! one document or a small batch of them, reads the answer, and the process
//! exits. Nothing is kept running between jobs — a long-lived child would
//! hold state that does not survive a crash, and every piece of state in this
//! system has to live in the lake instead.
//!
//! All three CLIs are invoked read-only and non-interactive. None is given
//! a writable workspace, because the compiler never needs one and a sandbox
//! escape is not a risk worth carrying for a convenience nobody asked for.
//!
//! Codex and Claude are also invoked with the owner's own configuration
//! switched off. That is not tidiness. A developer's `CLAUDE.md`, hooks,
//! plugins and agent memory are loaded by these tools on every run, and they
//! will happily appear in the reply: the first real run of this code returned
//! a hook's warning about an unrelated tool as part of the answer. Worse than
//! the noise is what it implies — the compiler's output would depend on which
//! machine it ran on, and text the owner never wrote for us would be steering
//! a model that is reading their contracts.
//!
//! The isolation stops short of one flag on purpose. Claude Code's `--bare`
//! switches off the most, but it also refuses to read OAuth or the keychain,
//! so it only works with an API key — which would quietly turn "runs on your
//! existing subscription" into "bring your own billing". `--restricted`
//! plus `--strict-mcp-config` plus an empty working directory gets the same
//! isolation while the owner's login keeps working.
//!
//! Cursor Agent uses `--mode=ask` (no writes) and never `--approve-mcps`:
//! compile must not pull Knowlith MCP into the extraction loop.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::{Engine, EngineError, Reply, Request, Result};
use crate::usage::{parse_json_reply, scrape_usage_only};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavour {
    Codex,
    ClaudeCode,
    /// Cursor's `agent` CLI — headless ask mode for compile jobs.
    CursorAgent,
}

impl Flavour {
    pub fn program(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude",
            Self::CursorAgent => "agent",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::ClaudeCode => "Claude Code",
            Self::CursorAgent => "Cursor Agent",
        }
    }

    /// Slug stored on sources / policy (`cursor-agent`, not the binary name).
    pub fn slug(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude-code",
            Self::CursorAgent => "cursor-agent",
        }
    }

    /// The CLI the owner named, including the aliases Settings and onboarding use.
    ///
    /// `auto` is not a flavour — it means "use whatever the daemon bound".
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug.trim() {
            "codex" => Some(Self::Codex),
            "claude" | "claude-code" => Some(Self::ClaudeCode),
            "agent" | "cursor" | "cursor-agent" => Some(Self::CursorAgent),
            _ => None,
        }
    }

    /// Whether the CLI can enforce a JSON Schema itself. Codex takes
    /// `--output-schema`; Claude Code and Cursor Agent do not, so the schema
    /// goes into the prompt instead.
    fn schema_is_native(self) -> bool {
        matches!(self, Self::Codex)
    }

    /// Cursor Agent takes the prompt as a CLI argument; Codex/Claude read stdin.
    fn prompt_as_argument(self) -> bool {
        matches!(self, Self::CursorAgent)
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
                // JSON carries `result` plus `usage` / `total_cost_usd`.
                // Text mode discards the bill, and the owner then has
                // nothing honest to show for what the run cost.
                args.push("json".into());
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
            Flavour::CursorAgent => {
                // Headless, ask-only: no file writes, no MCP approval. Compile
                // must not loop Knowlith back into itself via tools.
                args.push("-p".into());
                args.push("--mode=ask".into());
                args.push("--output-format".into());
                args.push("json".into());
                args.push("--trust".into());
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
            Flavour::ClaudeCode | Flavour::CursorAgent => None,
        };
        let reply_path = reply_file.as_ref().map(|p| p.to_string_lossy().into_owned());

        // An empty directory with nothing in it to discover. Project
        // instruction files and repository context are found by walking up
        // from the working directory, and the compiler must not inherit
        // whatever happens to be above the folder it was launched from.
        let workdir = tempish::TempDir::new("knowlith-run")
            .map_err(|e| EngineError::Transport(format!("could not create a working directory: {e}")))?;

        let prompt = request.prompt(self.flavour.schema_is_native());
        let mut args = self.arguments(schema_path.as_deref(), reply_path.as_deref());
        if self.flavour.prompt_as_argument() {
            args.push(prompt.clone());
        }

        let mut command = Command::new(&self.program);
        command
            .args(&args)
            .current_dir(workdir.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if self.flavour.prompt_as_argument() {
            command.stdin(Stdio::null());
        } else {
            command.stdin(Stdio::piped());
        }

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn().map_err(|e| match e.kind() {
            // Not installed is the owner's problem to fix, and retrying
            // it every five minutes would never fix it.
            std::io::ErrorKind::NotFound => EngineError::Unavailable(format!(
                "{} is not installed, or not on PATH. Install it, or choose another processor.",
                self.flavour.label()
            )),
            _ => EngineError::Transport(format!("could not start {}: {e}", self.program)),
        })?;

        let writer = if self.flavour.prompt_as_argument() {
            None
        } else {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| EngineError::Transport("the child process has no stdin".into()))?;
            // A separate thread, because a prompt larger than the pipe buffer
            // deadlocks against a child that is already writing to stdout.
            Some(std::thread::spawn(move || {
                let _ = stdin.write_all(prompt.as_bytes());
                drop(stdin);
            }))
        };

        let output = wait_with_timeout(child, request.timeout, self.flavour.label())?;
        if let Some(writer) = writer {
            let _ = writer.join();
        }
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
        // printed along the way is progress, not the answer. Claude and
        // Cursor were asked for JSON so the bill comes back with the text.
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let (text, usage) = match self.flavour {
            Flavour::Codex => {
                let text = reply_file
                    .as_ref()
                    .and_then(|f| std::fs::read_to_string(f.path()).ok())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| stdout.trim().to_string());
                drop(reply_file);
                (text, scrape_usage_only(&stdout, &stderr))
            }
            Flavour::ClaudeCode | Flavour::CursorAgent => {
                drop(reply_file);
                parse_json_reply(&stdout, &stderr)
            }
        };

        if text.is_empty() {
            return Err(EngineError::Refused(format!(
                "{} exited successfully but said nothing",
                self.flavour.label()
            )));
        }

        Ok(Reply {
            text,
            engine: self.name.clone(),
            usage,
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
const MAX_OUTPUT: u64 = 8 * 1024 * 1024;

fn stop_child_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        // All CLI descendants inherit our dedicated process group.
        let _ = Command::new("/bin/kill").args(["-9", &format!("-{}", child.id())])
            .stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill").args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
    label: &str,
) -> Result<std::process::Output> {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (tx, rx) = std::sync::mpsc::channel();
    fn drain(pipe: impl Read) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        pipe.take(MAX_OUTPUT + 1).read_to_end(&mut buffer)
            .map_err(|e| EngineError::Transport(format!("could not read CLI output: {e}")))?;
        if buffer.len() as u64 > MAX_OUTPUT {
            return Err(EngineError::Refused("The CLI output exceeded the 8 MiB limit. Use a smaller batch.".into()));
        }
        Ok(buffer)
    }
    let out_tx = tx.clone();
    std::thread::spawn(move || { let _ = out_tx.send((true, stdout.map(drain).unwrap_or_else(|| Ok(vec![])))); });
    std::thread::spawn(move || { let _ = tx.send((false, stderr.map(drain).unwrap_or_else(|| Ok(vec![])))); });
    let deadline = Instant::now() + timeout;
    let mut status = None;
    let mut out = None;
    let mut err = None;
    loop {
        while let Ok((is_stdout, result)) = rx.try_recv() {
            match result {
                Ok(bytes) => { if is_stdout { out = Some(bytes); } else { err = Some(bytes); } }
                Err(error) => { stop_child_tree(&mut child); return Err(error); }
            }
        }
        if status.is_none() {
            match child.try_wait() {
                Ok(result) => status = result,
                Err(error) => {
                    stop_child_tree(&mut child);
                    return Err(EngineError::Transport(format!("{label} could not be waited on: {error}")));
                }
            }
        }
        if status.is_some() && out.is_some() && err.is_some() {
            // Also closes stdin inherited by any lingering grandchild.
            stop_child_tree(&mut child);
            return Ok(std::process::Output { status: status.unwrap(), stdout: out.unwrap(), stderr: err.unwrap() });
        }
        if Instant::now() >= deadline {
            stop_child_tree(&mut child);
            return Err(EngineError::Transport(format!("{label} did not answer within {} ms, and was stopped", timeout.as_millis())));
        }
        std::thread::sleep(POLL);
    }
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
    fn cursor_agent_runs_headless_ask_without_mcp() {
        let args = CliEngine::new(Flavour::CursorAgent).arguments(None, None);
        assert!(args.contains(&"-p".to_string()));
        assert!(args.contains(&"--mode=ask".to_string()));
        assert!(args.contains(&"--output-format".to_string()));
        assert!(args.contains(&"json".to_string()));
        assert!(args.contains(&"--trust".to_string()));
        assert!(
            !args.iter().any(|a| a.contains("approve-mcp")),
            "compile must not auto-approve MCP servers"
        );
    }

    #[test]
    fn claude_runs_non_interactively() {
        let args = CliEngine::new(Flavour::ClaudeCode).arguments(None, None);
        assert!(args.contains(&"--print".to_string()));
        assert!(args.contains(&"--output-format".to_string()));
        assert!(args.contains(&"json".to_string()));
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
        assert!(
            !CliEngine::new(Flavour::CursorAgent)
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
