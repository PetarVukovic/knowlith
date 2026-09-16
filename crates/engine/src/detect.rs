//! Finding out what is already installed.
//!
//! The onboarding screen says "Knowlith found two AI tools already installed
//! on this Mac". That sentence has to be true on the machine it is shown on,
//! so it is answered by looking, not by assuming — and being signed in is
//! checked separately from being installed, because they fail differently and
//! the owner fixes them differently.

use std::process::{Command, Stdio};
use std::time::Duration;

use crate::cli::Flavour;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detected {
    pub flavour: Flavour,
    pub label: &'static str,
    pub program: &'static str,
    /// Where the binary is, when it was found.
    pub path: Option<String>,
    pub version: Option<String>,
}

impl Detected {
    pub fn installed(&self) -> bool {
        self.path.is_some()
    }
}

/// Looks for every CLI Knowlith can run.
///
/// Always returns one entry per flavour, installed or not, so the onboarding
/// screen can show what is missing as easily as what is there.
pub fn detect() -> Vec<Detected> {
    [Flavour::Codex, Flavour::ClaudeCode, Flavour::CursorAgent]
        .into_iter()
        .map(|flavour| {
            let path = which(flavour.program());
            let version = path.as_ref().and_then(|_| version_of(flavour.program()));
            Detected {
                flavour,
                label: flavour.label(),
                program: flavour.program(),
                path,
                version,
            }
        })
        .collect()
}

/// `PATH` lookup done here rather than by shelling out to `which`, so a
/// missing tool costs a few `stat` calls instead of a process.
fn which(program: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| is_executable(candidate))
        .map(|p| p.to_string_lossy().into_owned())
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

/// Runs `<program> --version`, briefly.
///
/// A CLI that hangs on `--version` is a CLI that would hang on real work, and
/// finding that out during onboarding costs five seconds instead of a scan.
fn version_of(program: &str) -> Option<String> {
    let child = Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });

    let output = rx.recv_timeout(Duration::from_secs(5)).ok()?.ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_flavour_is_always_reported() {
        let found = detect();
        assert_eq!(found.len(), 3);
        assert!(found.iter().any(|d| d.flavour == Flavour::Codex));
        assert!(found.iter().any(|d| d.flavour == Flavour::ClaudeCode));
        assert!(found.iter().any(|d| d.flavour == Flavour::CursorAgent));
    }

    #[test]
    fn a_program_that_does_not_exist_is_not_found() {
        assert!(which("knowlith-definitely-not-a-program").is_none());
    }

    #[test]
    fn something_that_certainly_exists_is_found() {
        // `sh` is on PATH on every machine this is meant to run on.
        assert!(which("sh").is_some());
    }
}
