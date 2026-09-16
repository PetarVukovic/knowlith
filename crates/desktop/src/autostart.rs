//! Keeping the daemon alive when the window is closed.
//!
//! This is what makes the product's central promise true. A company puts a
//! NAS folder in and expects it to stay read; if reading only happens while
//! a window is open, the owner has not installed a system, they have
//! installed an application they must remember to run.
//!
//! So the background service is registered with the operating system's own
//! login mechanism — `launchd` on macOS, Task Scheduler on Windows,
//! `systemd --user` on Linux — and the window becomes what it should be:
//! a client that attaches to something already running.
//!
//! Every one of these is a per-user registration. None of them needs an
//! administrator, none of them installs anything outside the owner's own
//! account, and all three can be removed by the owner without us.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::paths;

/// Reverse-DNS on macOS, a plain name elsewhere. Both are what their
/// platform expects to see in a process list.
pub const SERVICE_LABEL: &str = "eu.knowlith.agent";
#[cfg(windows)]
const TASK_NAME: &str = "Knowlith";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Autostart {
    /// Whether the service is registered to start at login.
    pub enabled: bool,
    /// Where the registration lives, so an owner can see and delete it.
    pub location: Option<String>,
    /// Whether it is running right now.
    pub running: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum AutostartError {
    #[error("could not write {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Refused(String),
    #[error("this system has no per-user login service Knowlith knows how to use")]
    Unsupported,
}

type Result<T> = std::result::Result<T, AutostartError>;

pub fn status() -> Autostart {
    let location = registration_path().map(|p| paths::display(&p));
    let enabled = registration_path().map(|p| p.exists()).unwrap_or(false) || task_registered();
    Autostart {
        enabled,
        location,
        running: is_running(),
    }
}

/// Registers the daemon to start at login, and starts it now.
pub fn enable(port: u16) -> Result<Autostart> {
    #[cfg(target_os = "macos")]
    {
        enable_launchd(port)?;
    }
    #[cfg(windows)]
    {
        enable_scheduled_task(port)?;
    }
    #[cfg(all(not(target_os = "macos"), not(windows)))]
    {
        enable_systemd(port)?;
    }
    Ok(status())
}

/// Removes the registration and stops the daemon.
pub fn disable() -> Result<Autostart> {
    #[cfg(target_os = "macos")]
    {
        let plist = registration_path().ok_or(AutostartError::Unsupported)?;
        let _ = run("launchctl", &["bootout", &gui_target(), &paths::display(&plist)]);
        let _ = run("launchctl", &["unload", "-w", &paths::display(&plist)]);
        let _ = fs::remove_file(&plist);
    }
    #[cfg(windows)]
    {
        let _ = run("schtasks", &["/End", "/TN", TASK_NAME]);
        let _ = run("schtasks", &["/Delete", "/TN", TASK_NAME, "/F"]);
    }
    #[cfg(all(not(target_os = "macos"), not(windows)))]
    {
        let _ = run("systemctl", &["--user", "disable", "--now", "knowlith.service"]);
        if let Some(unit) = registration_path() {
            let _ = fs::remove_file(unit);
        }
    }
    Ok(status())
}

/// Where the registration file lives on this platform.
pub fn registration_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        Some(
            paths::home()
                .join("Library")
                .join("LaunchAgents")
                .join(format!("{SERVICE_LABEL}.plist")),
        )
    }
    #[cfg(windows)]
    {
        // Task Scheduler keeps its own store; the wrapper script is the
        // only file of ours on disk, and it is what the task runs.
        Some(paths::root().join("bin").join("knowlith-agent.cmd"))
    }
    #[cfg(all(not(target_os = "macos"), not(windows)))]
    {
        Some(
            paths::home()
                .join(".config")
                .join("systemd")
                .join("user")
                .join("knowlith.service"),
        )
    }
}

/// Whether the daemon is answering on its port.
///
/// The registration existing and the daemon running are different facts,
/// and an interface that conflates them tells the owner everything is fine
/// while nothing is being read.
pub fn is_running() -> bool {
    use std::net::{Ipv4Addr, SocketAddr, TcpStream};
    use std::time::Duration;

    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, 7717));
    TcpStream::connect_timeout(&address, Duration::from_millis(300)).is_ok()
}

// ------------------------------------------------------------------ macOS --

#[cfg(target_os = "macos")]
fn enable_launchd(port: u16) -> Result<()> {
    let plist_path = registration_path().ok_or(AutostartError::Unsupported)?;
    if let Some(parent) = plist_path.parent() {
        fs::create_dir_all(parent).map_err(|source| AutostartError::Write {
            path: paths::display(parent),
            source,
        })?;
    }

    let logs = paths::log_dir();
    let _ = fs::create_dir_all(&logs);

    let plist = launchd_plist(&paths::binary(), port, &logs);
    fs::write(&plist_path, plist).map_err(|source| AutostartError::Write {
        path: paths::display(&plist_path),
        source,
    })?;

    // `bootout` first so re-enabling after an upgrade picks up the new
    // binary path instead of silently keeping the old one loaded.
    let _ = run("launchctl", &["bootout", &gui_target(), &paths::display(&plist_path)]);
    let loaded = run(
        "launchctl",
        &["bootstrap", &gui_target(), &paths::display(&plist_path)],
    );
    if !loaded {
        // `bootstrap` is the modern verb and is not on every macOS this
        // will meet.
        if !run("launchctl", &["load", "-w", &paths::display(&plist_path)]) {
            return Err(AutostartError::Refused(
                "macOS refused to register the background service. The file is written; logging out and back in will start it.".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn gui_target() -> String {
    // The per-user domain. `gui/<uid>` is what makes this a login item for
    // one account rather than a system daemon needing an administrator.
    format!("gui/{}", libc_getuid())
}

#[cfg(target_os = "macos")]
fn libc_getuid() -> u32 {
    // One call, declared here rather than taking a dependency on `libc`
    // for it. The signature is stable ABI on every Unix.
    unsafe extern "C" {
        fn getuid() -> u32;
    }
    unsafe { getuid() }
}

#[cfg(target_os = "macos")]
fn launchd_plist(binary: &str, port: u16, logs: &std::path::Path) -> String {
    let log = paths::display(&logs.join("agent.log"));
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{SERVICE_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{binary}</string>
    <string>serve</string>
    <string>--port</string>
    <string>{port}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <dict>
    <key>SuccessfulExit</key>
    <false/>
  </dict>
  <key>ProcessType</key>
  <string>Background</string>
  <key>StandardOutPath</key>
  <string>{log}</string>
  <key>StandardErrorPath</key>
  <string>{log}</string>
</dict>
</plist>
"#
    )
}

// ---------------------------------------------------------------- Windows --

#[cfg(windows)]
fn enable_scheduled_task(port: u16) -> Result<()> {
    let script_path = registration_path().ok_or(AutostartError::Unsupported)?;
    if let Some(parent) = script_path.parent() {
        fs::create_dir_all(parent).map_err(|source| AutostartError::Write {
            path: paths::display(parent),
            source,
        })?;
    }

    let logs = paths::log_dir();
    let _ = fs::create_dir_all(&logs);
    let log = paths::display(&logs.join("agent.log"));

    // A wrapper rather than the binary directly, so the log redirection and
    // the arguments live in a file the owner can read and edit.
    let script = format!(
        "@echo off\r\nstart \"\" /B \"{}\" serve --port {port} >> \"{log}\" 2>&1\r\n",
        paths::binary()
    );
    fs::write(&script_path, script).map_err(|source| AutostartError::Write {
        path: paths::display(&script_path),
        source,
    })?;

    let _ = run("schtasks", &["/Delete", "/TN", TASK_NAME, "/F"]);
    let created = run(
        "schtasks",
        &[
            "/Create",
            "/TN",
            TASK_NAME,
            "/TR",
            &format!("\"{}\"", paths::display(&script_path)),
            "/SC",
            "ONLOGON",
            // Runs as the logged-in user with their normal rights. A task
            // that asked for elevation would prompt at every login.
            "/RL",
            "LIMITED",
            "/F",
        ],
    );
    if !created {
        return Err(AutostartError::Refused(
            "Windows refused to create the scheduled task. The start script is written to your Knowlith folder; running it starts the service.".into(),
        ));
    }
    let _ = run("schtasks", &["/Run", "/TN", TASK_NAME]);
    Ok(())
}

#[cfg(windows)]
fn task_registered() -> bool {
    run("schtasks", &["/Query", "/TN", TASK_NAME])
}

#[cfg(not(windows))]
fn task_registered() -> bool {
    false
}

// ------------------------------------------------------------------ Linux --

#[cfg(all(not(target_os = "macos"), not(windows)))]
fn enable_systemd(port: u16) -> Result<()> {
    let unit_path = registration_path().ok_or(AutostartError::Unsupported)?;
    if let Some(parent) = unit_path.parent() {
        fs::create_dir_all(parent).map_err(|source| AutostartError::Write {
            path: paths::display(parent),
            source,
        })?;
    }

    let unit = format!(
        "[Unit]\n\
         Description=Knowlith — keeps this company's knowledge current\n\n\
         [Service]\n\
         ExecStart={} serve --port {port}\n\
         Restart=on-failure\n\
         RestartSec=5\n\n\
         [Install]\n\
         WantedBy=default.target\n",
        paths::binary()
    );
    fs::write(&unit_path, unit).map_err(|source| AutostartError::Write {
        path: paths::display(&unit_path),
        source,
    })?;

    let _ = run("systemctl", &["--user", "daemon-reload"]);
    if !run("systemctl", &["--user", "enable", "--now", "knowlith.service"]) {
        return Err(AutostartError::Refused(
            "systemd refused to enable the service. The unit file is written; `systemctl --user enable --now knowlith` will start it.".into(),
        ));
    }
    Ok(())
}

/// Runs a command, discarding its output, and reports whether it succeeded.
fn run(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registration_has_a_place_on_this_platform() {
        let path = registration_path().expect("nowhere to register");
        assert!(path.is_absolute(), "{path:?}");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_plist_starts_the_daemon_at_login_and_keeps_it_up() {
        let plist = launchd_plist("/usr/local/bin/knowlith", 7717, std::path::Path::new("/tmp"));
        assert!(plist.contains("<key>RunAtLoad</key>\n  <true/>"));
        assert!(plist.contains("/usr/local/bin/knowlith"));
        assert!(plist.contains("<string>7717</string>"));
        // Restarted when it dies, but not when the owner stops it on
        // purpose — `SuccessfulExit false` is exactly that distinction.
        assert!(plist.contains("SuccessfulExit"));
        assert!(plist.contains(SERVICE_LABEL));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_plist_is_valid_xml_as_far_as_the_system_is_concerned() {
        let plist = launchd_plist("/usr/local/bin/knowlith", 7717, std::path::Path::new("/tmp"));
        let path = std::env::temp_dir().join(format!("knowlith-test-{}.plist", std::process::id()));
        fs::write(&path, &plist).unwrap();
        // `plutil` ships with macOS and is the same parser launchd uses.
        let ok = run("plutil", &["-lint", &paths::display(&path)]);
        let _ = fs::remove_file(&path);
        assert!(ok, "launchd would refuse this plist");
    }

    #[test]
    fn status_never_panics_on_a_machine_with_nothing_installed() {
        let status = status();
        assert_eq!(status.enabled, status.enabled);
    }
}
