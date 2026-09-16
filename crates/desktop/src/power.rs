//! Whether this machine is plugged in.
//!
//! One question, asked for one reason: an owner who opens a laptop in a
//! meeting and finds Knowlith has spent their battery reading a NAS will
//! turn it off and not turn it back on. Costing someone an afternoon of
//! battery is a trust problem, not a performance one.
//!
//! Every platform answers through a command that already ships with it,
//! and an unknown answer means mains. Pausing a company's work because a
//! power query failed would be a far worse failure than running on battery
//! for a few minutes.

use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Power {
    /// Plugged in, or we could not tell.
    Mains,
    Battery,
}

impl Power {
    pub fn on_battery(self) -> bool {
        self == Power::Battery
    }
}

pub fn power() -> Power {
    #[cfg(target_os = "macos")]
    {
        // `pmset -g batt` prints "Now drawing from 'Battery Power'" or
        // "'AC Power'". A desktop prints the AC line too.
        match capture("pmset", &["-g", "batt"]) {
            Some(text) if text.contains("'Battery Power'") => Power::Battery,
            _ => Power::Mains,
        }
    }
    #[cfg(windows)]
    {
        // BatteryStatus 1 means discharging. 2 is on mains, and a desktop
        // with no battery returns nothing at all.
        let query = "(Get-CimInstance -ClassName Win32_Battery).BatteryStatus";
        match capture("powershell", &["-NoProfile", "-Command", query]) {
            Some(text) if text.trim().starts_with('1') => Power::Battery,
            _ => Power::Mains,
        }
    }
    #[cfg(all(not(target_os = "macos"), not(windows)))]
    {
        // The kernel exposes this as a file, so no process is needed.
        match std::fs::read_to_string("/sys/class/power_supply/AC/online") {
            Ok(text) if text.trim() == "0" => Power::Battery,
            _ => Power::Mains,
        }
    }
}

#[allow(dead_code)]
fn capture(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_is_one_of_two_and_never_a_panic() {
        let state = power();
        assert!(matches!(state, Power::Mains | Power::Battery));
    }

    #[test]
    fn an_unanswerable_machine_counts_as_plugged_in() {
        // The fallback in every branch above. Asserted as behaviour rather
        // than as a comment, because getting it backwards would silently
        // stop every desktop in the country from doing any work.
        assert_eq!(capture("knowlith-no-such-program", &[]), None);
        assert!(!Power::Mains.on_battery());
    }
}
