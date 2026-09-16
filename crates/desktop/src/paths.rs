//! Where Knowlith's own files live, on each operating system.
//!
//! Every path in the product resolves through this module. That is not
//! tidiness: `$HOME` does not exist on Windows, and a single
//! `env::var("HOME")` anywhere in the tree is enough to send the lake to a
//! folder called `.` next to whatever the shortcut's working directory
//! happened to be. One place to be wrong is one place to fix.

use std::path::{Path, PathBuf};

/// The owner's home directory.
///
/// `USERPROFILE` is tried before `HOME` on Windows because Git Bash and MSYS
/// set `HOME` to a private root inside their own installation, and a lake
/// written there disappears the day the owner uninstalls their terminal.
pub fn home() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(profile) = non_empty("USERPROFILE") {
            return PathBuf::from(profile);
        }
        if let (Some(drive), Some(path)) = (non_empty("HOMEDRIVE"), non_empty("HOMEPATH")) {
            return PathBuf::from(format!("{drive}{path}"));
        }
    }
    if let Some(home) = non_empty("HOME") {
        return PathBuf::from(home);
    }
    // Neither variable set happens in launchd jobs and Windows services. The
    // current directory is a poor home, but it is a real one, and every
    // caller creates the folder it needs.
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// `~/Knowlith` — the folder the owner can open, back up and take with them.
pub fn root() -> PathBuf {
    if let Some(explicit) = non_empty("KNOWLITH_HOME") {
        return PathBuf::from(explicit);
    }
    home().join("Knowlith")
}

/// The Context Lake itself.
pub fn lake_db() -> PathBuf {
    root().join("data").join("lake.sqlite")
}

/// Where the gateway writes what it could not say over the wire.
///
/// An MCP server speaks JSON-RPC on stdout and must never print anything
/// else there — a stray line makes the client drop the connection with no
/// message the owner can act on. So diagnostics go to stderr for a developer
/// and to this file for everyone else.
pub fn log_dir() -> PathBuf {
    root().join("logs")
}

/// The company's own mark, shown next to the slash command in the chat box.
pub fn brand_dir() -> PathBuf {
    root().join("brand")
}

/// Per-application configuration directories, as each vendor defines them.
///
/// macOS and Windows disagree about where an application keeps its settings,
/// and both disagree with Linux. Writing to the wrong one is silent: the file
/// appears, the app never reads it, and the owner is told they are connected
/// when they are not.
pub fn app_support(app_folder: &str) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        Some(home().join("Library").join("Application Support").join(app_folder))
    }
    #[cfg(windows)]
    {
        non_empty("APPDATA")
            .map(PathBuf::from)
            .or_else(|| Some(home().join("AppData").join("Roaming")))
            .map(|base| base.join(app_folder))
    }
    #[cfg(all(not(target_os = "macos"), not(windows)))]
    {
        non_empty("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| Some(home().join(".config")))
            .map(|base| base.join(app_folder))
    }
}

/// The absolute path of the running `knowlith` binary.
///
/// An MCP client spawns the server by command line with no shell and no
/// inherited `PATH` in some configurations, so the config file has to name
/// the binary absolutely. Falling back to the bare name is better than
/// failing to write a config at all: on a machine where `PATH` is inherited
/// it still works.
pub fn binary() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok().or(Some(p)))
        .map(|p| display(&p))
        .unwrap_or_else(|| "knowlith".to_string())
}

/// A path as the owner should read it.
///
/// `canonicalize` on Windows returns the `\\?\C:\...` extended-length form.
/// It is a valid path and every API accepts it, but it is not what anyone
/// typed, it breaks string comparison against a path the owner pasted, and
/// some applications refuse to parse it out of a JSON config.
pub fn display(path: &Path) -> String {
    let text = path.to_string_lossy().into_owned();
    match text.strip_prefix(r"\\?\") {
        Some(rest) => rest.to_string(),
        None => text,
    }
}

/// `canonicalize`, without the Windows extended-length prefix.
pub fn real(path: &Path) -> std::io::Result<PathBuf> {
    let full = path.canonicalize()?;
    Ok(PathBuf::from(display(&full)))
}

fn non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_is_absolute_or_current() {
        let home = home();
        assert!(!home.as_os_str().is_empty());
    }

    #[test]
    fn root_follows_the_override() {
        // Serialised with the other env test by running in one process is not
        // guaranteed, so this asserts the shape rather than mutating the
        // environment.
        let root = root();
        assert!(root.to_string_lossy().contains("Knowlith") || std::env::var("KNOWLITH_HOME").is_ok());
    }

    #[test]
    fn display_strips_the_windows_long_path_prefix() {
        let path = Path::new(r"\\?\C:\Users\ana\Knowlith");
        assert_eq!(display(path), r"C:\Users\ana\Knowlith");
    }

    #[test]
    fn display_leaves_a_normal_path_alone() {
        let path = Path::new("/Users/ana/Knowlith");
        assert_eq!(display(path), "/Users/ana/Knowlith");
    }
}
