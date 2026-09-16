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
    pub const ALL: [App; 3] = [App::ClaudeDesktop, App::ClaudeCode, App::Codex];

    /// What the owner calls it.
    pub fn label(self) -> &'static str {
        match self {
            App::ClaudeDesktop => "Claude Desktop",
            App::ClaudeCode => "Claude Code",
            App::Codex => "Codex",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            App::ClaudeDesktop => "claude-desktop",
            App::ClaudeCode => "claude-code",
            App::Codex => "codex",
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
    /// "another tool" rather than folded into one of these three, because
    /// telling an owner that Codex read their rules when it was something
    /// else is worse than telling them nothing.
    pub fn from_client_name(name: &str) -> Option<App> {
        let name = name.to_lowercase();
        // Order matters: "claude-code" contains "claude", so the more
        // specific names are tested first.
        if name.contains("codex") {
            return Some(App::Codex);
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
            App::ClaudeDesktop | App::ClaudeCode => Format::JsonServers,
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
            App::Codex => Some(paths::home().join(".codex").join("config.toml")),
        }
    }

    /// Whether this machine looks like it has the application.
    pub fn installed(self) -> bool {
        if self.application_path().is_some() {
            return true;
        }
        if program_on_path(self.program()).is_some() {
            return true;
        }
        self.config_file()
            .and_then(|f| f.parent().map(|d| d.is_dir()))
            .unwrap_or(false)
    }

    /// The command line this application ships, if any.
    fn program(self) -> &'static str {
        match self {
            App::ClaudeDesktop | App::ClaudeCode => "claude",
            App::Codex => "codex",
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
            App::ClaudeCode => Vec::new(),
        };
        candidates.into_iter().find(|p| p.exists())
    }

    /// Whether restarting the application is what makes a new server appear.
    ///
    /// Claude Desktop reads its config once at launch, so a connection made
    /// while it is running does nothing until it is restarted — and an owner
    /// who is told "connected" and then sees no tools concludes the product
    /// is broken. Claude Code reads on each session start and Codex on each
    /// run, so for those a new terminal is enough.
    pub fn needs_restart(self) -> bool {
        matches!(self, App::ClaudeDesktop)
    }

    /// What to tell the owner to do after connecting.
    pub fn refresh_hint(self) -> &'static str {
        match self {
            App::ClaudeDesktop => "Claude Desktop reads this when it starts, so it has to be restarted once.",
            App::ClaudeCode => "Open a new terminal, or type /mcp in a running session and reconnect.",
            App::Codex => "The next `codex` run picks it up; nothing to restart.",
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
    }

    #[test]
    fn a_client_we_do_not_know_is_not_guessed_into_one_of_ours() {
        // Saying "Codex read your rules" when it was Cursor is worse than
        // saying nothing at all.
        assert_eq!(App::from_client_name("cursor-vscode"), None);
        assert_eq!(App::from_client_name("mcp-inspector"), None);
        assert_eq!(App::from_client_name(""), None);
    }
}
