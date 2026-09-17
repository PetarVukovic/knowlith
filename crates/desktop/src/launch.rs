//! Opening and restarting the applications Knowlith connects to.
//!
//! Restarting someone else's application is an intrusive thing to do, so
//! everything here is gentle by construction: the quit is a request the
//! application can refuse, never a kill. If Claude Desktop has an unsaved
//! state or a dialog open it stays open, and the owner is told it is still
//! running rather than losing it.
//!
//! Nothing in this module is required for a connection to work. It exists so
//! that the owner does not have to know that Claude Desktop reads its config
//! at launch — the difference between a product that works and a product
//! that works once you know the trick.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::apps::App;
use crate::paths;

/// How long to wait for an application to actually go away after being asked.
const QUIT_GRACE: Duration = Duration::from_secs(8);
const POLL: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    /// It was not running, and now it is.
    Opened,
    /// It was running; it was asked to quit, it did, and it was reopened.
    Restarted,
    /// It was asked to quit and did not within the grace period. Nothing was
    /// forced. The owner has to close it themselves, and is told so.
    StillRunning,
    /// There is no window to open — Claude Code is a terminal program.
    NoWindow,
    /// The application is not installed on this machine.
    NotInstalled,
    /// A CLI session was started in Terminal (Claude Code / Codex / Cursor Agent).
    OpenedInTerminal,
}

impl Outcome {
    /// What to show the owner. Written as a result, not as a status code.
    pub fn message(self, app: App) -> String {
        match self {
            Outcome::Opened => format!("{} is open and can see your company.", app.label()),
            Outcome::Restarted => format!("{} restarted and reloaded your company.", app.label()),
            Outcome::StillRunning => format!(
                "{} did not close — it may be asking about something. Close it and open it again.",
                app.label()
            ),
            Outcome::NoWindow => {
                format!(
                    "{} runs in the terminal. Use Try in your AI or Ask on the company brain — Knowlith opens Terminal with the question ready.",
                    app.label()
                )
            }
            Outcome::NotInstalled => format!("{} is not installed on this computer.", app.label()),
            Outcome::OpenedInTerminal => format!(
                "Opened a small terminal window with {}. Send the question there, then come back when it has read Knowlith.",
                app.label()
            ),
        }
    }
}

/// Whether the application has a process running right now.
pub fn is_running(app: App) -> bool {
    let Some(process) = process_name(app) else {
        return false;
    };

    #[cfg(target_os = "macos")]
    {
        // `pgrep -x` matches the executable name exactly, so a browser tab
        // titled "Claude" does not count as the application running.
        run_quietly("pgrep", &["-x", process])
    }
    #[cfg(windows)]
    {
        let Some(output) = capture("tasklist", &["/FI", &format!("IMAGENAME eq {process}"), "/NH"]) else {
            return false;
        };
        output.to_lowercase().contains(&process.to_lowercase())
    }
    #[cfg(all(not(target_os = "macos"), not(windows)))]
    {
        run_quietly("pgrep", &["-x", process])
    }
}

/// Opens the application, restarting it first when it is already running and
/// only reads its configuration at launch.
///
/// CLI hosts (Claude Code, Cursor Agent, Codex CLI) have no window to bounce.
/// Saying "not installed" when `agent` is on PATH and Knowlith is connected
/// is the Connect-screen contradiction owners hit after Ask AI landed the CLI.
pub fn open_or_restart(app: App) -> Outcome {
    // Prefer the same surface Ask AI uses: a working CLI is not "missing"
    // just because there is no `.app` bundle.
    match app.launch_surface() {
        crate::apps::LaunchSurface::Missing => Outcome::NotInstalled,
        crate::apps::LaunchSurface::Terminal => {
            // No windowed host to restart. Opening Terminal.app from Connect
            // without a question is noise — the owner wants a live ask on the
            // brain map. Tell them honestly instead of claiming Missing.
            Outcome::NoWindow
        }
        crate::apps::LaunchSurface::Desktop => open_or_restart_desktop(app),
    }
}

fn open_or_restart_desktop(app: App) -> Outcome {
    let path = app.application_path().or_else(|| app.chatgpt_desktop());
    if path.is_none() {
        return Outcome::NotInstalled;
    }

    let running = is_running(app);
    if !running {
        open(app);
        return Outcome::Opened;
    }

    if !app.needs_restart() {
        // Already running and it re-reads on its own. Bringing the window
        // forward is the useful thing to do, not bouncing the process.
        open(app);
        return Outcome::Opened;
    }

    quit(app);
    let deadline = Instant::now() + QUIT_GRACE;
    while Instant::now() < deadline {
        if !is_running(app) {
            open(app);
            return Outcome::Restarted;
        }
        std::thread::sleep(POLL);
    }
    Outcome::StillRunning
}

/// Brings the application up without touching a running instance.
pub fn open(app: App) {
    let Some(path) = app.application_path().or_else(|| app.chatgpt_desktop()) else {
        return;
    };

    #[cfg(target_os = "macos")]
    {
        // `open` hands the launch to the window server, which is what makes
        // the application come to the front rather than starting a second,
        // headless copy owned by this process.
        detach("open", &["-a", &crate::paths::display(&path)]);
    }
    #[cfg(windows)]
    {
        detach_program(&path);
    }
    #[cfg(all(not(target_os = "macos"), not(windows)))]
    {
        detach("xdg-open", &[&crate::paths::display(&path)]);
    }
}

/// Opens the application with a prompt already sitting in its composer.
///
/// Desktop hosts get a deep link; CLI hosts get a Terminal window.
pub fn open_with_prompt(app: App, prompt: &str) -> TryLaunch {
    match app.launch_surface() {
        crate::apps::LaunchSurface::Missing => TryLaunch {
            outcome: Outcome::NotInstalled,
            surface: "missing",
            command: None,
        },
        crate::apps::LaunchSurface::Desktop => {
            let Some(url) = prompt_deeplink(app, prompt) else {
                return TryLaunch {
                    outcome: Outcome::NoWindow,
                    surface: "desktop",
                    command: None,
                };
            };
            open_url(&url);
            TryLaunch {
                outcome: Outcome::Opened,
                surface: "desktop",
                command: None,
            }
        }
        crate::apps::LaunchSurface::Terminal => {
            let Some(binary) = app.cli_binary() else {
                return TryLaunch {
                    outcome: Outcome::NotInstalled,
                    surface: "terminal",
                    command: None,
                };
            };
            let command = terminal_command(app, &binary, prompt);
            open_in_terminal(&command);
            TryLaunch {
                outcome: Outcome::OpenedInTerminal,
                surface: "terminal",
                command: Some(command),
            }
        }
    }
}

/// The shell line Terminal would run, for display when the owner asks to see it.
pub fn cli_command_line(app: App, prompt: &str) -> Option<String> {
    let binary = app.cli_binary()?;
    Some(terminal_command(app, &binary, prompt))
}

/// Non-interactive argv for a one-shot CLI run (tests and tooling).
///
/// `server_key` is the `mcpServers` key Connect wrote (`knowlith-<company>`).
pub fn cli_print_argv(app: App, server_key: &str, prompt: &str) -> Option<(std::path::PathBuf, Vec<String>)> {
    let binary = app.cli_binary()?;
    let args = match app {
        App::ClaudeCode => vec![
            "--print".into(),
            "--output-format".into(),
            "text".into(),
            // Headless `-p` starts in manual permission mode and there is no
            // host to approve a tool call, so every MCP read is denied and
            // Claude answers from context. `mcp__<server>` allows the tools
            // of that one server and nothing else.
            "--allowedTools".into(),
            format!("mcp__{server_key}"),
            // The daemon's cwd is whatever folder it was started from. A
            // headless `-p` run connects every server in that folder's
            // `.mcp.json` and runs its hooks — servers nobody trusted for
            // this question. Strict mode hands Claude exactly one server:
            // ours, pointed at this binary, so the answer can only have
            // been read from the company's own lake.
            "--strict-mcp-config".into(),
            "--mcp-config".into(),
            serde_json::json!({
                "mcpServers": {
                    server_key: { "type": "stdio", "command": paths::binary(), "args": ["mcp"] }
                }
            })
            .to_string(),
            prompt.to_string(),
        ],
        App::Codex => vec![
            "exec".into(),
            "--color".into(),
            "never".into(),
            // `exec` refuses to start outside a git repository, and the
            // daemon's cwd is the owner's home. The engine's own path already
            // passes this (crates/engine/src/cli.rs); the chat has to too.
            "--skip-git-repo-check".into(),
            // A chat question is not a session worth a rollout file each.
            "--ephemeral".into(),
            prompt.to_string(),
        ],
        App::Cursor => vec![
            "--print".into(),
            "--mode=ask".into(),
            "--output-format".into(),
            "text".into(),
            "--approve-mcps".into(),
            prompt.to_string(),
        ],
        App::ClaudeDesktop => return None,
    };
    Some((binary, args))
}

/// What [`open_with_prompt`] did, including the shell line for the UI panel.
#[derive(Debug, Clone)]
pub struct TryLaunch {
    pub outcome: Outcome,
    /// `desktop` | `terminal` | `missing`
    pub surface: &'static str,
    pub command: Option<String>,
}

/// The documented deep link that prefills `prompt` in this application's chat.
pub fn prompt_deeplink(app: App, prompt: &str) -> Option<String> {
    let encoded = urlencoding_lightweight(prompt);
    let url = match app {
        App::ClaudeDesktop => format!("claude://claude.ai/new?q={encoded}"),
        App::ClaudeCode => format!("claude-cli://open?q={encoded}"),
        App::Codex => format!("codex://threads/new?prompt={encoded}"),
        App::Cursor => {
            format!("cursor://anysphere.cursor-deeplink/prompt?text={encoded}")
        }
    };
    Some(url)
}

/// Opens the local interface in the owner's default browser.
pub fn open_browser(url: &str) {
    open_url(url);
}

/// Hands a URL to the operating system. Used for `claude://`, `codex://`,
/// `cursor://` and `claude-cli://` — schemes the AI apps register themselves.
pub fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    {
        detach("open", &[url]);
    }
    #[cfg(windows)]
    {
        // `cmd /C start "" <url>` is the documented way to open a registered
        // protocol without treating the URL as a path to search for.
        detach("cmd", &["/C", "start", "", url]);
    }
    #[cfg(all(not(target_os = "macos"), not(windows)))]
    {
        detach("xdg-open", &[url]);
    }
}

/// Shell line that starts the CLI with the prompt, matching each product's docs.
///
/// - Cursor Agent: `agent --mode=ask --approve-mcps -- '…'` (read-only ask + MCP)
/// - Claude Code: `claude -- '…'`
/// - Codex CLI: `codex -- '…'`
///
/// The `--` stops option parsing so a prompt that starts with `-` is not eaten.
fn terminal_command(app: App, binary: &std::path::Path, prompt: &str) -> String {
    let bin = crate::paths::display(binary);
    let quoted = shell_single_quote(prompt);
    match app {
        App::Cursor => format!("{bin} --mode=ask --approve-mcps -- {quoted}"),
        App::ClaudeCode | App::Codex => format!("{bin} -- {quoted}"),
        // Desktop never reaches this path; kept exhaustive.
        App::ClaudeDesktop => format!("{bin} -- {quoted}"),
    }
}

fn shell_single_quote(value: &str) -> String {
    // POSIX-safe: wrap in single quotes, escape embedded ones as '\''.
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Opens a compact, separate terminal window — never inside the browser.
fn open_in_terminal(command: &str) {
    #[cfg(target_os = "macos")]
    {
        // `do script` always creates its own Terminal.app window. Bounds keep
        // it postcard-sized so it reads as a popup beside Knowlith.
        let script = format!(
            "tell application \"Terminal\"
  activate
  set win to do script {}
  delay 0.25
  try
    set bounds of front window to {{460, 140, 960, 400}}
    set custom title of front window to \"Knowlith\"
  end try
end tell",
            apple_script_string(command)
        );
        detach("osascript", &["-e", &script]);
    }
    #[cfg(windows)]
    {
        if open_in_windows_terminal(command) {
            return;
        }
        // Classic console: new window, fixed column/row count inside the session.
        let inner = format!("mode con: cols=92 lines=22 & {command}");
        detach("cmd", &["/C", "start", "Knowlith", "cmd", "/K", &inner]);
    }
    #[cfg(all(not(target_os = "macos"), not(windows)))]
    {
        // Best-effort: x-terminal-emulator is the Debian alternative.
        // Failure is silent — the UI still shows the command to paste.
        detach("x-terminal-emulator", &["-e", "sh", "-c", command]);
    }
}

/// Windows Terminal when installed — small floating tab, separate from the browser.
#[cfg(windows)]
fn open_in_windows_terminal(command: &str) -> bool {
    let Ok(output) = std::process::Command::new("where")
        .arg("wt")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    detach(
        "wt",
        &[
            "--pos",
            "460,140",
            "--size",
            "92,22",
            "cmd",
            "/K",
            command,
        ],
    );
    true
}

fn apple_script_string(value: &str) -> String {
    // AppleScript double-quoted string with backslash and quote escaped.
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' | '"' => {
                out.push('\\');
                out.push(ch);
            }
            '\n' => out.push_str("\\n"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// Percent-encode for a query value. Avoids pulling a URL crate into this
/// crate for one encode path that only ever carries prompt text.
fn urlencoding_lightweight(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            b' ' => out.push_str("%20"),
            b => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Asks the application to quit. Never forces it.
pub fn quit(app: App) {
    let Some(process) = process_name(app) else {
        return;
    };

    #[cfg(target_os = "macos")]
    {
        // A Quit Apple event is the same thing Cmd-Q sends: the application
        // gets to save, prompt and refuse.
        let script = format!("tell application \"{}\" to quit", process);
        detach("osascript", &["-e", &script]);
    }
    #[cfg(windows)]
    {
        // Without `/F` this closes the window the way the close button does.
        detach("taskkill", &["/IM", process]);
    }
    #[cfg(all(not(target_os = "macos"), not(windows)))]
    {
        detach("pkill", &["-TERM", "-x", process]);
    }
}

/// The name of the running process, as the operating system reports it.
fn process_name(app: App) -> Option<&'static str> {
    match app {
        App::ClaudeDesktop => Some(if cfg!(windows) { "claude.exe" } else { "Claude" }),
        App::Codex => Some(if cfg!(windows) { "Codex.exe" } else { "Codex" }),
        App::Cursor => Some(if cfg!(windows) { "Cursor.exe" } else { "Cursor" }),
        // A terminal program has no single process to look for, and the one
        // the owner cares about is the shell they are typing into.
        App::ClaudeCode => None,
    }
}

/// Runs a command and throws away everything it says.
///
/// Output is discarded rather than inherited on purpose: when the gateway is
/// the caller, anything written to stdout is protocol traffic, and a stray
/// line from `pgrep` would disconnect the client.
fn run_quietly(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn capture(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Starts a command and does not wait for it.
///
/// The child is deliberately orphaned. Knowlith's own process may be a
/// short-lived CLI invocation, and an application that dies because the tool
/// that opened it exited would be worse than not opening it at all.
fn detach(program: &str, args: &[&str]) {
    let _ = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

#[cfg(windows)]
fn detach_program(path: &std::path::Path) {
    let _ = Command::new(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_code_has_no_window_to_open() {
        assert_eq!(open_or_restart(App::ClaudeCode), Outcome::NoWindow);
    }

    #[test]
    fn a_cli_agent_is_not_reported_as_missing_when_its_binary_is_present() {
        // Cursor Agent is `agent` on PATH — Connect's Open must not say
        // "not installed" while the row already shows Connected.
        if App::Cursor.cli_binary().is_none() {
            return;
        }
        assert_eq!(open_or_restart(App::Cursor), Outcome::NoWindow);
        let message = Outcome::NoWindow.message(App::Cursor);
        assert!(message.contains("Ask") || message.contains("Try in your AI"), "{message}");
        assert!(!message.contains("not installed"), "{message}");
    }

    #[test]
    fn every_outcome_says_something_a_person_can_act_on() {
        for outcome in [
            Outcome::Opened,
            Outcome::Restarted,
            Outcome::StillRunning,
            Outcome::NoWindow,
            Outcome::NotInstalled,
            Outcome::OpenedInTerminal,
        ] {
            let message = outcome.message(App::ClaudeDesktop);
            assert!(message.ends_with('.'), "{message:?}");
            assert!(message.contains("Claude Desktop"));
            assert!(!message.contains("Mac"), "the product ships on Windows too: {message}");
        }
    }

    #[test]
    fn a_terminal_program_has_no_process_to_watch() {
        assert!(process_name(App::ClaudeCode).is_none());
        assert!(!is_running(App::ClaudeCode));
    }

    #[test]
    fn every_supported_app_has_a_prompt_deeplink() {
        let prompt = "Use Knowlith's \"Ways to pay\" skill";
        for app in App::ALL {
            let url = prompt_deeplink(app, prompt).expect("every app has a scheme");
            assert!(url.contains("Use%20Knowlith"), "{url}");
            assert!(
                url.starts_with("claude://")
                    || url.starts_with("claude-cli://")
                    || url.starts_with("codex://")
                    || url.starts_with("cursor://"),
                "{url}"
            );
        }
    }

    #[test]
    fn a_prompt_deeplink_does_not_submit_itself() {
        // The schemes prefill; they must not encode an auto-send flag we
        // invent. If a host adds one later, this product still refuses it —
        // a sent prompt is a claim about what the model did.
        let url = prompt_deeplink(App::ClaudeDesktop, "hello").unwrap();
        assert!(!url.contains("autosubmit"));
        assert!(!url.contains("submit=true"));
    }

    #[test]
    fn cursor_agent_cli_matches_documented_ask_invocation() {
        // https://cursor.com/docs/cli/using — `agent --mode=ask "…"`.
        // `--approve-mcps` so Knowlith is usable without an extra confirm.
        let cmd = terminal_command(
            App::Cursor,
            std::path::Path::new("/Users/me/.local/bin/agent"),
            "What do we charge?",
        );
        assert!(cmd.starts_with("/Users/me/.local/bin/agent "));
        assert!(cmd.contains("--mode=ask"));
        assert!(cmd.contains("--approve-mcps"));
        assert!(cmd.contains("'What do we charge?'"));
    }

    #[test]
    fn claude_and_codex_cli_take_the_prompt_as_documented() {
        // `claude [prompt]` and `codex [PROMPT]` — positional after `--`.
        let claude = terminal_command(App::ClaudeCode, std::path::Path::new("/opt/bin/claude"), "hi");
        assert_eq!(claude, "/opt/bin/claude -- 'hi'");
        let codex = terminal_command(App::Codex, std::path::Path::new("/opt/bin/codex"), "hi");
        assert_eq!(codex, "/opt/bin/codex -- 'hi'");
    }

    #[test]
    fn chat_print_argv_is_non_interactive() {
        // Brain chat must not open a TUI — that painted boxes into bubbles.
        let (_, claude) = cli_print_argv(App::ClaudeCode, "knowlith-bb", "What do we charge?").unwrap();
        assert!(claude.iter().any(|a| a == "--print"));
        assert!(claude.ends_with(&["What do we charge?".into()]));
        // Without this every read is denied and Claude answers from its head.
        assert!(claude.windows(2).any(|w| w[0] == "--allowedTools" && w[1] == "mcp__knowlith-bb"));
        // Only our server, whatever `.mcp.json` sits in the daemon's cwd.
        assert!(claude.iter().any(|a| a == "--strict-mcp-config"));
        let config = claude
            .windows(2)
            .find(|w| w[0] == "--mcp-config")
            .map(|w| w[1].clone())
            .expect("an inline mcp config");
        let parsed: serde_json::Value = serde_json::from_str(&config).unwrap();
        assert_eq!(parsed["mcpServers"]["knowlith-bb"]["args"][0], "mcp");
        assert_eq!(parsed["mcpServers"].as_object().unwrap().len(), 1);

        let (_, codex) = cli_print_argv(App::Codex, "knowlith-bb", "What do we charge?").unwrap();
        assert_eq!(codex[0], "exec");
        assert!(codex.iter().any(|a| a == "never"));
        // The daemon's cwd is not a git repo; exec would refuse to start.
        assert!(codex.iter().any(|a| a == "--skip-git-repo-check"));

        let (_, cursor) = cli_print_argv(App::Cursor, "knowlith-bb", "What do we charge?").unwrap();
        assert!(cursor.iter().any(|a| a == "--print"));
        assert!(cursor.iter().any(|a| a == "--approve-mcps"));
    }
}
