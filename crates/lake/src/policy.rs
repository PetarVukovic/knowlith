//! What the background service is allowed to do on its own.
//!
//! Every job in the queue is either free or it is not. Reading a folder,
//! comparing documents and re-checking quotes cost nothing but electricity.
//! Asking a model costs the owner money, or their subscription's allowance,
//! and it does it without anyone watching.
//!
//! An owner who discovers that Knowlith spent an afternoon and a sizeable
//! bill on a folder they dragged in by accident will not use it again. So
//! the expensive half is governed by three settings, all of them the
//! owner's, all of them changeable while work is queued.

use serde::{Deserialize, Serialize};

use crate::{Lake, Result};

/// When the service may call a model without being asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Processing {
    /// Read whatever changes, as it changes. The default, because a system
    /// the owner has to remember to run is one that goes stale.
    Automatic,
    /// Queue the work and wait to be told. The queue is durable, so
    /// "later" can be tomorrow.
    Ask,
    /// Never on its own. Only what the owner starts by hand.
    Manual,
}

/// How many changed files count as a big enough job to ask about first.
///
/// Someone pointing Knowlith at a NAS share for the first time may have
/// forty thousand files behind that folder. Reading them is the right thing
/// to do and it is also the single most expensive thing this product can
/// do, and the difference between those is a question worth asking once.
pub const DEFAULT_LARGE_SCAN: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    pub processing: Processing,
    /// Stop calling models when the laptop is unplugged.
    pub pause_on_battery: bool,
    pub large_scan: usize,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            processing: Processing::Automatic,
            pause_on_battery: true,
            large_scan: DEFAULT_LARGE_SCAN,
        }
    }
}

impl Policy {
    /// Whether a job that needs a model may start right now.
    ///
    /// Takes the power state rather than reading it, so the decision is a
    /// pure function and the tests do not need a laptop.
    pub fn may_run_ai(&self, on_battery: bool) -> std::result::Result<(), Held> {
        match self.processing {
            Processing::Manual => Err(Held::Manual),
            Processing::Ask => Err(Held::Ask),
            Processing::Automatic if on_battery && self.pause_on_battery => Err(Held::Battery),
            Processing::Automatic => Ok(()),
        }
    }
}

/// Why a job is sitting still. Each of these is a different sentence in the
/// interface and a different thing for the owner to do about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Held {
    /// The owner asked to be asked.
    Ask,
    /// The owner turned automatic work off entirely.
    Manual,
    /// Unplugged.
    Battery,
    /// More changed at once than the owner said to do without asking.
    TooMuchAtOnce,
}

impl Held {
    pub fn reason(self) -> &'static str {
        match self {
            Held::Ask => "waiting for you to say go",
            Held::Manual => "automatic reading is off",
            Held::Battery => "on battery",
            Held::TooMuchAtOnce => "more files changed than you allow at once",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Held::Ask => "ask",
            Held::Manual => "manual",
            Held::Battery => "battery",
            Held::TooMuchAtOnce => "too-much-at-once",
        }
    }
}

const KEY: &str = "policy";

impl Lake {
    /// The owner's settings, or the defaults when they have not chosen.
    pub fn policy(&self) -> Policy {
        self.setting(KEY)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn set_policy(&self, policy: &Policy) -> Result<()> {
        self.set_setting(KEY, &serde_json::to_string(policy)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_on_mains_runs() {
        assert!(Policy::default().may_run_ai(false).is_ok());
    }

    #[test]
    fn automatic_on_battery_waits_by_default() {
        assert_eq!(Policy::default().may_run_ai(true), Err(Held::Battery));
    }

    #[test]
    fn an_owner_who_does_not_mind_the_battery_is_obeyed() {
        let policy = Policy {
            pause_on_battery: false,
            ..Policy::default()
        };
        assert!(policy.may_run_ai(true).is_ok());
    }

    #[test]
    fn manual_never_runs_whatever_the_power_says() {
        let policy = Policy {
            processing: Processing::Manual,
            pause_on_battery: false,
            ..Policy::default()
        };
        assert_eq!(policy.may_run_ai(false), Err(Held::Manual));
        assert_eq!(policy.may_run_ai(true), Err(Held::Manual));
    }

    #[test]
    fn every_reason_is_a_sentence_the_owner_can_act_on() {
        for held in [Held::Ask, Held::Manual, Held::Battery, Held::TooMuchAtOnce] {
            assert!(!held.reason().is_empty());
            assert!(!held.as_str().contains(' '));
        }
    }

    #[test]
    fn the_policy_survives_a_restart() {
        let lake = Lake::in_memory().unwrap();
        assert_eq!(lake.policy(), Policy::default());

        let mine = Policy {
            processing: Processing::Ask,
            pause_on_battery: false,
            large_scan: 50,
        };
        lake.set_policy(&mine).unwrap();
        assert_eq!(lake.policy(), mine);
    }

    #[test]
    fn a_corrupt_setting_falls_back_rather_than_stopping_everything() {
        let lake = Lake::in_memory().unwrap();
        lake.set_setting(KEY, "{not json").unwrap();
        assert_eq!(lake.policy(), Policy::default());
    }
}
