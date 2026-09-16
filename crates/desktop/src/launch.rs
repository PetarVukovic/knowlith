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
                format!("{} runs in a terminal. Open a new one and it is there.", app.label())
            }
            Outcome::NotInstalled => format!("{} is not installed on this computer.", app.label()),
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
pub fn open_or_restart(app: App) -> Outcome {
    if app.application_path().is_none() {
        return match app {
            App::ClaudeCode => Outcome::NoWindow,
            _ => Outcome::NotInstalled,
        };
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
    let Some(path) = app.application_path() else {
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
    fn every_outcome_says_something_a_person_can_act_on() {
        for outcome in [
            Outcome::Opened,
            Outcome::Restarted,
            Outcome::StillRunning,
            Outcome::NoWindow,
            Outcome::NotInstalled,
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
}
