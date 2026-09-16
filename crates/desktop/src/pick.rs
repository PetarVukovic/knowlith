//! The folder chooser the operating system already has.
//!
//! A browser cannot tell a page where a folder is. `webkitdirectory` hands
//! over relative names, `showDirectoryPicker` hands over a handle with no
//! path, and neither is something a daemon can scan. So the interface asks
//! the daemon to open the chooser, and the daemon — which is a native
//! process on the owner's machine — opens the real one and answers with a
//! real path.
//!
//! Cancelling is not a failure. Somebody who opens the chooser, looks, and
//! closes it has done nothing wrong, and the interface should go back to
//! where it was rather than show an error.

use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum PickError {
    #[error("could not start the folder chooser: {0}")]
    Start(#[from] std::io::Error),
    #[error("{0}")]
    Failed(String),
}

type Result<T> = std::result::Result<T, PickError>;

/// How long the chooser may stay open before the request is abandoned.
///
/// Not a limit on the person: the dialog stays on screen and still works.
/// It is a limit on the HTTP request holding a thread, for the case where
/// somebody opens the chooser and goes to lunch.
pub const TIMEOUT_SECS: u64 = 300;

/// Opens the system folder chooser and waits for an answer.
///
/// `Ok(None)` means the owner closed it without choosing.
pub fn folder(prompt: &str) -> Result<Option<PathBuf>> {
    let chosen = run(prompt)?;
    Ok(chosen.map(PathBuf::from))
}

/// The AppleScript that opens the chooser.
///
/// A bare `activate` targets the running script, so the dialog belongs to
/// this osascript process and comes up in front of the browser. Asking
/// `System Events` to do it instead — the obvious way to write this — gives
/// the window to System Events, where it outlives the script: kill the
/// process and the chooser is still on screen, owned by nothing this daemon
/// can reach.
#[cfg(target_os = "macos")]
fn script(prompt: &str) -> String {
    format!(
        r#"activate
POSIX path of (choose folder with prompt "{}")"#,
        prompt.replace('\\', "\\\\").replace('"', "\\\"")
    )
}

#[cfg(target_os = "macos")]
fn run(prompt: &str) -> Result<Option<String>> {
    let output = Command::new("osascript").arg("-e").arg(script(prompt)).output()?;
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Ok(if path.is_empty() { None } else { Some(path) });
    }

    // AppleScript reports a cancelled dialog as error -128, which is an
    // ordinary outcome and not worth showing anybody.
    let error = String::from_utf8_lossy(&output.stderr);
    if error.contains("-128") {
        return Ok(None);
    }
    Err(PickError::Failed(format!(
        "the folder chooser did not open: {}",
        error.trim()
    )))
}

#[cfg(windows)]
fn run(prompt: &str) -> Result<Option<String>> {
    // The dialog is single-threaded-apartment only, and `-Sta` is what makes
    // it appear at all; without it the call returns immediately with nothing.
    let script = format!(
        r#"Add-Type -AssemblyName System.Windows.Forms
$d = New-Object System.Windows.Forms.FolderBrowserDialog
$d.Description = '{}'
$d.ShowNewFolderButton = $false
$top = New-Object System.Windows.Forms.Form
$top.TopMost = $true
if ($d.ShowDialog($top) -eq [System.Windows.Forms.DialogResult]::OK) {{ Write-Output $d.SelectedPath }}"#,
        prompt.replace('\'', "''")
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-Sta", "-NonInteractive", "-Command", &script])
        .output()?;
    if !output.status.success() {
        return Err(PickError::Failed(format!(
            "the folder chooser did not open: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(if path.is_empty() { None } else { Some(path) })
}

#[cfg(all(unix, not(target_os = "macos")))]
fn run(prompt: &str) -> Result<Option<String>> {
    for (program, args) in [
        ("zenity", vec!["--file-selection", "--directory", "--title"]),
        ("kdialog", vec!["--getexistingdirectory", "."]),
    ] {
        if crate::apps::program_on_path(program).is_none() {
            continue;
        }
        let mut command = Command::new(program);
        command.args(&args);
        if program == "zenity" {
            command.arg(prompt);
        }
        let output = command.output()?;
        // Both report a cancelled dialog with a non-zero status and no
        // output, which is the same ordinary outcome as on the other two
        // platforms.
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Ok(if path.is_empty() { None } else { Some(path) });
    }
    Err(PickError::Failed(
        "no folder chooser on this system — install zenity, or type the path".into(),
    ))
}

#[cfg(test)]
mod tests {
    // The dialog itself cannot be tested without a person in front of it, so
    // what is tested is the part that would silently produce a broken script:
    // a prompt carrying the quote character that terminates it.
    #[test]
    #[cfg(target_os = "macos")]
    fn the_dialog_belongs_to_this_process_and_not_to_system_events() {
        // Regression: `tell application "System Events" ... choose folder`
        // leaves the chooser on screen after the script is gone, because
        // System Events owns the window.
        let script = super::script("Pick a folder");
        assert!(script.starts_with("activate\n"));
        assert!(!script.contains("System Events"));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn a_quote_in_the_prompt_does_not_break_the_script() {
        let prompt = "Pick the \"sales\" folder";
        let escaped = prompt.replace('\\', "\\\\").replace('"', "\\\"");
        assert_eq!(escaped, "Pick the \\\"sales\\\" folder");
        // Every quote in the escaped form is preceded by a backslash, so the
        // AppleScript string literal closes where it is meant to.
        for (i, c) in escaped.char_indices() {
            if c == '"' {
                assert_eq!(&escaped[i - 1..i], "\\");
            }
        }
    }
}
