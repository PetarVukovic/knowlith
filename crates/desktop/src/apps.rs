//! The AI applications on this machine, and where each keeps its settings.
//!
//! Every vendor made a different choice, and none of them are guessable:
//! Claude Desktop reads one JSON file under Application Support, Claude Code
//! reads a different JSON file in the home directory, Codex reads TOML. The
//! whole point of this module is that the owner never learns any of that.
//!
//! Detection is deliberately generous. An application counts as present when
//! either the program or its configuration directory exists, because a
//! machine where Claude Desktop was opened once and quit still has a config
//! directory, and refusing to connect it would be wrong. What is *not*
//! generous is the reporting: a connection is only claimed after the file has
//! been written and read back.

use std::path::PathBuf;

use crate::paths;

/// How Knowlith reaches an AI tool when the owner presses "try" / "ask".
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LaunchSurface {
    /// Claude Desktop, Codex.app, ChatGPT.app, Cursor.app — open outside.
    Desktop,
    /// Claude Code, Codex CLI, Cursor Agent — Terminal session.
    Terminal,
    /// Neither a windowed app nor a CLI binary is present.
    Missing,
}

/// One application Knowlith can hand itself to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum App {
    /// The Claude chat application, the one most owners already have open.
    ClaudeDesktop,
    /// The `claude` command line, user scope so every project sees it.
    ClaudeCode,
    /// The `codex` command line and the Codex application, which share
    /// `~/.codex/config.toml`.
    Codex,
    /// Cursor Agent (`agent` CLI) and/or the Cursor IDE.
    Cursor,
}

/// How an application stores its server list. The shape decides how a
/// connection is merged in, and merging is the only operation here that can
/// damage something the owner wrote by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// A JSON object with an `mcpServers` map.
    JsonServers,
    /// TOML with an `[mcp_servers.<name>]` table.
    TomlServers,
}

impl App {
    pub const ALL: [App; 4] = [App::ClaudeDesktop, App::ClaudeCode, App::Codex, App::Cursor];

    /// What the owner calls it.
    pub fn label(self) -> &'static str {
        match self {
            App::ClaudeDesktop => "Claude Desktop",
            App::ClaudeCode => "Claude Code",
            App::Codex => "Codex",
            App::Cursor => "Cursor Agent",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            App::ClaudeDesktop => "claude-desktop",
            App::ClaudeCode => "claude-code",
            App::Codex => "codex",
            App::Cursor => "cursor",
        }
    }

    pub fn parse(slug: &str) -> Option<App> {
        App::ALL.into_iter().find(|app| app.slug() == slug)
    }

    /// Which application a client says it is, from the `clientInfo.name` it
    /// sends in `initialize`.
    ///
    /// The field is free text chosen by whoever wrote the client, so this is
    /// a guess by construction. It is kept here, in one function with its
    /// own tests, rather than spread through the gateway: when a client
    /// renames itself the fix is one line and the screens that count reads
    /// keep working.
    ///
    /// `None` means a client we do not recognise. That is reported as
    /// "another tool" rather than folded into one of these four, because
    /// telling an owner that Codex read their rules when it was something
    /// else is worse than telling them nothing.
    pub fn from_client_name(name: &str) -> Option<App> {
        let name = name.to_lowercase();
        // Order matters: "claude-code" contains "claude", so the more
        // specific names are tested first.
        if name.contains("codex") {
            return Some(App::Codex);
        }
        if name.contains("cursor") {
            return Some(App::Cursor);
        }
        if name.contains("claude-code") || name.contains("claude code") || name == "claude_code" {
            return Some(App::ClaudeCode);
        }
        if name.contains("claude") {
            return Some(App::ClaudeDesktop);
        }
        None
    }

    pub fn format(self) -> Format {
        match self {
            App::ClaudeDesktop | App::ClaudeCode | App::Cursor => Format::JsonServers,
            App::Codex => Format::TomlServers,
        }
    }

    /// The file whose contents decide whether this application can see
    /// Knowlith.
    ///
    /// `None` only when the operating system gives us nowhere to write, which
    /// in practice means an unsupported platform rather than a broken machine.
    pub fn config_file(self) -> Option<PathBuf> {
        match self {
            App::ClaudeDesktop => Some(paths::app_support("Claude")?.join("claude_desktop_config.json")),
            App::ClaudeCode => {
                // Claude Code reads `.claude.json` from `CLAUDE_CONFIG_DIR`
                // when that is set, and from the home directory otherwise.
                // Writing to the wrong one is silent, and a developer who set
                // the variable is exactly the owner most likely to check.
                let base = std::env::var("CLAUDE_CONFIG_DIR")
                    .ok()
                    .filter(|v| !v.trim().is_empty())
                    .map(PathBuf::from)
                    .unwrap_or_else(paths::home);
                Some(base.join(".claude.json"))
            }
            App::Codex => {
                // Same story as `CLAUDE_CONFIG_DIR`: Codex reads `config.toml`
                // from `CODEX_HOME` when set. The ChatGPT desktop app, the
                // CLI and the IDE extension all share that one file.
                let base = std::env::var("CODEX_HOME")
                    .ok()
                    .filter(|v| !v.trim().is_empty())
                    .map(PathBuf::from)
                    .unwrap_or_else(|| paths::home().join(".codex"));
                Some(base.join("config.toml"))
            }
            App::Cursor => Some(paths::home().join(".cursor").join("mcp.json")),
        }
    }

    /// Whether this machine looks like it has the application.
    ///
    /// Cursor Agent ships as `agent` on PATH (not `cursor`), and Codex may
    /// be CLI-only with ChatGPT.app as the desktop shell. Counting only the
    /// `.app` bundle called this product "not installed" on machines that
    /// run agents every day.
    pub fn installed(self) -> bool {
        if self.application_path().is_some() {
            return true;
        }
        if self.chatgpt_desktop().is_some() {
            return true;
        }
        if self.cli_binary().is_some() {
            return true;
        }
        self.config_file()
            .and_then(|f| f.parent().map(|d| d.is_dir()))
            .unwrap_or(false)
    }

    /// The command-line binary to spawn for a terminal session, when any.
    ///
    /// Cursor Agent is `agent`; older installs may still have `cursor`.
    pub fn cli_binary(self) -> Option<PathBuf> {
        for name in self.cli_names() {
            if let Some(path) = program_on_path(name) {
                return Some(path);
            }
        }
        None
    }

    fn cli_names(self) -> &'static [&'static str] {
        match self {
            App::ClaudeDesktop => &[],
            App::ClaudeCode => &["claude"],
            App::Codex => &["codex"],
            App::Cursor => &["agent"],
        }
    }

    /// ChatGPT Desktop, when present — Codex's windowed host on many machines.
    pub fn chatgpt_desktop(self) -> Option<PathBuf> {
        if !matches!(self, App::Codex) {
            return None;
        }
        #[cfg(target_os = "macos")]
        {
            let candidates = [
                PathBuf::from("/Applications/ChatGPT.app"),
                paths::home().join("Applications/ChatGPT.app"),
            ];
            return candidates.into_iter().find(|p| p.exists());
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }

    /// How "try" / "ask" should reach this tool on *this* machine.
    ///
    /// Desktop apps open outside Knowlith. CLIs open a Terminal session the
    /// owner can see — Claude Code, Codex CLI, Cursor Agent. Preferring a
    /// missing `.app` over a working CLI is what produced "Cursor is not
    /// installed" while `agent` was on PATH.
    pub fn launch_surface(self) -> LaunchSurface {
        match self {
            App::ClaudeDesktop => {
                if self.application_path().is_some() {
                    LaunchSurface::Desktop
                } else {
                    LaunchSurface::Missing
                }
            }
            App::ClaudeCode => {
                if self.cli_binary().is_some() {
                    LaunchSurface::Terminal
                } else {
                    LaunchSurface::Missing
                }
            }
            App::Codex => {
                if self.application_path().is_some() || self.chatgpt_desktop().is_some() {
                    LaunchSurface::Desktop
                } else if self.cli_binary().is_some() {
                    LaunchSurface::Terminal
                } else {
                    LaunchSurface::Missing
                }
            }
            App::Cursor => {
                // Prefer `agent` on PATH: the label is Cursor Agent, and Ask AI
                // embeds a live PTY. Cursor.app alone is a last resort deep link.
                if self.cli_binary().is_some() {
                    LaunchSurface::Terminal
                } else if self.application_path().is_some() {
                    LaunchSurface::Desktop
                } else {
                    LaunchSurface::Missing
                }
            }
        }
    }

    /// The installed application bundle or executable, when there is one.
    ///
    /// Only Claude Desktop and Codex ship a window; Claude Code is a terminal
    /// program and correctly has none.
    pub fn application_path(self) -> Option<PathBuf> {
        let candidates: Vec<PathBuf> = match self {
            App::ClaudeDesktop => {
                #[cfg(target_os = "macos")]
                {
                    vec![
                        PathBuf::from("/Applications/Claude.app"),
                        paths::home().join("Applications/Claude.app"),
                    ]
                }
                #[cfg(windows)]
                {
                    local_app_data()
                        .map(|base| {
                            vec![
                                base.join("AnthropicClaude").join("claude.exe"),
                                base.join("Programs").join("Claude").join("Claude.exe"),
                            ]
                        })
                        .unwrap_or_default()
                }
                #[cfg(all(not(target_os = "macos"), not(windows)))]
                {
                    Vec::new()
                }
            }
            App::Codex => {
                #[cfg(target_os = "macos")]
                {
                    vec![
                        PathBuf::from("/Applications/Codex.app"),
                        paths::home().join("Applications/Codex.app"),
                    ]
                }
                #[cfg(windows)]
                {
                    local_app_data()
                        .map(|base| vec![base.join("Programs").join("Codex").join("Codex.exe")])
                        .unwrap_or_default()
                }
                #[cfg(all(not(target_os = "macos"), not(windows)))]
                {
                    Vec::new()
                }
            }
            App::Cursor => {
                #[cfg(target_os = "macos")]
                {
                    vec![
                        PathBuf::from("/Applications/Cursor.app"),
                        paths::home().join("Applications/Cursor.app"),
                    ]
                }
                #[cfg(windows)]
                {
                    local_app_data()
                        .map(|base| {
                            vec![
                                base.join("Programs").join("cursor").join("Cursor.exe"),
                                base.join("cursor").join("Cursor.exe"),
                            ]
                        })
                        .unwrap_or_default()
                }
                #[cfg(all(not(target_os = "macos"), not(windows)))]
                {
                    Vec::new()
                }
            }
            App::ClaudeCode => Vec::new(),
        };
        candidates.into_iter().find(|p| p.exists())
    }

    /// Whether restarting the application is what makes a new server appear.
    ///
    /// Claude Desktop reads its config once at launch. Cursor's desktop app
    /// does too — but Cursor *Agent* (`agent` on PATH) starts a new process
    /// each time, so restarting a missing IDE would be the wrong instruction.
    pub fn needs_restart(self) -> bool {
        match self {
            App::ClaudeDesktop => true,
            App::Cursor => self.application_path().is_some(),
            App::ClaudeCode | App::Codex => false,
        }
    }

    /// What to tell the owner to do after connecting.
    pub fn refresh_hint(self) -> &'static str {
        match self {
            App::ClaudeDesktop => "Claude Desktop reads this when it starts, so it has to be restarted once.",
            App::ClaudeCode => "Open a new terminal, or type /mcp in a running session and reconnect.",
            App::Codex => "The next `codex` run picks it up; nothing to restart.",
            App::Cursor if self.application_path().is_some() => {
                "Cursor reads MCP when a window starts — reload the window or restart Cursor once."
            }
            App::Cursor => "The next `agent` session picks Knowlith up; nothing to restart.",
        }
    }
}

/// A `PATH` lookup that knows about Windows.
///
/// `claude` on Windows is `claude.cmd`, and looking only for a file named
/// exactly `claude` finds nothing on a machine where it is installed. This is
/// the same bug in every cross-platform tool that was written on a Mac.
pub fn program_on_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let extensions: Vec<String> = {
        #[cfg(windows)]
        {
            let pathext = std::env::var("PATHEXT")
                .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
            std::iter::once(String::new())
                .chain(pathext.split(';').map(|e| e.to_lowercase()))
                .filter(|e| e.is_empty() || e.starts_with('.'))
                .collect()
        }
        #[cfg(not(windows))]
        {
            vec![String::new()]
        }
    };

    for dir in std::env::split_paths(&path) {
        for extension in &extensions {
            let candidate = dir.join(format!("{program}{extension}"));
            if is_executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &std::path::Path) -> bool {
    path.is_file()
}

#[cfg(windows)]
fn local_app_data() -> Option<PathBuf> {
    std::env::var("LOCALAPPDATA")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| Some(paths::home().join("AppData").join("Local")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_app_round_trips_through_its_slug() {
        for app in App::ALL {
            assert_eq!(App::parse(app.slug()), Some(app));
        }
    }

    #[test]
    fn every_app_knows_where_its_config_lives() {
        for app in App::ALL {
            assert!(app.config_file().is_some(), "{} has nowhere to write", app.label());
        }
    }

    #[test]
    fn only_the_desktop_app_needs_a_restart() {
        assert!(App::ClaudeDesktop.needs_restart());
        assert!(!App::ClaudeCode.needs_restart());
        assert!(!App::Codex.needs_restart());
        // Cursor Agent CLI does not; Cursor.app would. The assertion is the
        // CLI-shaped machine this product is usually developed on.
        if App::Cursor.application_path().is_none() {
            assert!(!App::Cursor.needs_restart());
        }
    }

    #[test]
    fn cursor_cli_is_the_agent_binary() {
        // Cursor's terminal product is `agent`, not `cursor` — see
        // https://cursor.com/docs/cli/overview.
        assert_eq!(App::Cursor.cli_names(), &["agent"]);
    }

    #[test]
    fn path_lookup_finds_a_real_program() {
        // Something every machine this runs on has, under its own name.
        let program = if cfg!(windows) { "cmd" } else { "sh" };
        assert!(program_on_path(program).is_some());
    }

    #[test]
    fn path_lookup_does_not_invent_programs() {
        assert!(program_on_path("knowlith-definitely-not-installed").is_none());
    }
}

#[cfg(test)]
mod client_name_tests {
    use super::App;

    #[test]
    fn the_names_these_three_clients_actually_send() {
        // Taken from what each client puts in `clientInfo.name`.
        assert_eq!(App::from_client_name("claude-ai"), Some(App::ClaudeDesktop));
        assert_eq!(App::from_client_name("Claude"), Some(App::ClaudeDesktop));
        assert_eq!(App::from_client_name("claude-code"), Some(App::ClaudeCode));
        assert_eq!(App::from_client_name("Claude Code"), Some(App::ClaudeCode));
        assert_eq!(App::from_client_name("codex"), Some(App::Codex));
        assert_eq!(App::from_client_name("codex-cli"), Some(App::Codex));
        // What the Codex CLI / ChatGPT desktop app actually send today.
        assert_eq!(App::from_client_name("codex-mcp-client"), Some(App::Codex));
        assert_eq!(App::from_client_name("cursor"), Some(App::Cursor));
        assert_eq!(App::from_client_name("cursor-vscode"), Some(App::Cursor));
    }

    #[test]
    fn a_client_we_do_not_know_is_not_guessed_into_one_of_ours() {
        // Saying "Codex read your rules" when it was something else is worse
        // than saying nothing at all.
        assert_eq!(App::from_client_name("mcp-inspector"), None);
        assert_eq!(App::from_client_name(""), None);
    }
}
