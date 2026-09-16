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
                    "{} runs in the terminal. On Company brain, click a node and Ask AI — a live session opens beside the map.",
                    app.label()
                )
            }
            Outcome::NotInstalled => format!("{} is not installed on this computer.", app.label()),
            Outcome::OpenedInTerminal => format!(
                "Opened a Terminal session with {}. Send the question there if it is not already filled.",
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
/// Desktop hosts get a deep link. CLI hosts get a Terminal session unless
/// `embedded` is true — then the caller is embedding a live PTY and must
/// not also bounce Terminal.app.
pub fn open_with_prompt(app: App, prompt: &str) -> TryLaunch {
    open_with_prompt_opts(app, prompt, false)
}

/// Same as [`open_with_prompt`], with control over whether a system Terminal
/// window is opened for CLI hosts.
///
/// When `embedded` is set and a CLI binary exists, we take the terminal path
/// even if [`App::launch_surface`] would prefer a desktop app (Codex +
/// ChatGPT.app). The brain sidebar owns that PTY; bouncing Terminal.app or a
/// deep link would hide the live session the owner asked for.
pub fn open_with_prompt_opts(app: App, prompt: &str, embedded: bool) -> TryLaunch {
    let surface = if embedded && app.cli_binary().is_some() {
        crate::apps::LaunchSurface::Terminal
    } else {
        app.launch_surface()
    };
    match surface {
        crate::apps::LaunchSurface::Missing => TryLaunch {
            outcome: Outcome::NotInstalled,
            surface: "missing",
            command: None,
            embedded: false,
        },
        crate::apps::LaunchSurface::Desktop => {
            let Some(url) = prompt_deeplink(app, prompt) else {
                return TryLaunch {
                    outcome: Outcome::NoWindow,
                    surface: "desktop",
                    command: None,
                    embedded: false,
                };
            };
            open_url(&url);
            TryLaunch {
                outcome: Outcome::Opened,
                surface: "desktop",
                command: None,
                embedded: false,
            }
        }
        crate::apps::LaunchSurface::Terminal => {
            let Some(binary) = app.cli_binary() else {
                return TryLaunch {
                    outcome: Outcome::NotInstalled,
                    surface: "terminal",
                    command: None,
                    embedded: false,
                };
            };
            let command = terminal_command(app, &binary, prompt);
            if !embedded {
                open_in_terminal(&command);
            }
            TryLaunch {
                outcome: Outcome::OpenedInTerminal,
                surface: "terminal",
                command: Some(command),
                embedded,
            }
        }
    }
}

/// Argv for an embedded PTY session (no shell wrapping).
pub fn cli_pty_argv(app: App, prompt: &str) -> Option<(std::path::PathBuf, Vec<String>)> {
    let binary = app.cli_binary()?;
    let args = match app {
        App::Cursor => vec![
            "--mode=ask".into(),
            "--approve-mcps".into(),
            prompt.to_string(),
        ],
        App::ClaudeCode | App::Codex => vec![prompt.to_string()],
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
    /// True when the CLI was prepared for an in-app PTY, not Terminal.app.
    pub embedded: bool,
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

/// Opens the system Terminal with `command` already typed and running.
fn open_in_terminal(command: &str) {
    #[cfg(target_os = "macos")]
    {
        // AppleScript's `quoted form` would be ideal; we already shell-quoted
        // the prompt. Wrap the whole line once more for `do script`.
        let script = format!(
            "tell application \"Terminal\" to do script {}",
            apple_script_string(command)
        );
        detach("osascript", &["-e", &script]);
    }
    #[cfg(windows)]
    {
        // `start` opens a new console window running the command.
        detach("cmd", &["/C", "start", "cmd", "/K", command]);
    }
    #[cfg(all(not(target_os = "macos"), not(windows)))]
    {
        // Best-effort: x-terminal-emulator is the Debian alternative.
        // Failure is silent — the UI still shows the command to paste.
        detach("x-terminal-emulator", &["-e", "sh", "-c", command]);
    }
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
        assert!(message.contains("Ask AI"), "{message}");
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
}
