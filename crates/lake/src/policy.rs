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

/// How many CLI compile workers run beside the I/O track.
///
/// Extract/rescan stays on its own thread so a long walk never blocks
/// reading. Each of these workers owns one CLI child at a time; more than
/// four on a laptop mostly hits the provider's rate limit.
pub const DEFAULT_COMPILE_WORKERS: usize = 2;
pub const MAX_COMPILE_WORKERS: usize = 4;

/// How many documents one CLI invoke may read together.
///
/// Cold-start of `codex` / `claude` / `agent` dominates wall-clock on large
/// folders. Packing several documents into one process cuts that cost without
/// calling any external HTTP API.
pub const DEFAULT_COMPILE_BATCH: usize = 8;
pub const MAX_COMPILE_BATCH: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    pub processing: Processing,
    /// Stop calling models when the laptop is unplugged.
    pub pause_on_battery: bool,
    pub large_scan: usize,
    /// Which CLI reads documents for compile jobs: `auto`, `codex`,
    /// `claude-code`, `cursor-agent`, or `managed`.
    ///
    /// The worker binds the engine at daemon start. Changing this while
    /// Knowlith is running takes effect on the next restart — the API says
    /// so rather than pretending the switch is live.
    #[serde(default = "default_engine")]
    pub engine: String,
    /// Parallel CLI compile workers (I/O track is always separate).
    #[serde(default = "default_compile_workers")]
    pub compile_workers: usize,
    /// Documents packed into one CLI invoke.
    #[serde(default = "default_compile_batch")]
    pub compile_batch_size: usize,
    /// Run the relate pass only after the owner confirms the build quiz.
    ///
    /// While false, relate still runs right after compile and settle — which
    /// blocks the build supervisor on a whole-folder model call before the
    /// owner sees the quiz.
    #[serde(default = "default_relate_after_build")]
    pub relate_after_build: bool,
}

fn default_relate_after_build() -> bool {
    true
}

fn default_engine() -> String {
    "auto".into()
}

fn default_compile_workers() -> usize {
    DEFAULT_COMPILE_WORKERS
}

fn default_compile_batch() -> usize {
    DEFAULT_COMPILE_BATCH
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            processing: Processing::Automatic,
            pause_on_battery: true,
            large_scan: DEFAULT_LARGE_SCAN,
            engine: default_engine(),
            compile_workers: DEFAULT_COMPILE_WORKERS,
            compile_batch_size: DEFAULT_COMPILE_BATCH,
            relate_after_build: default_relate_after_build(),
        }
    }
}

impl Policy {
    /// Clamped worker / batch sizes so a hand-edited lake cannot spawn chaos.
    pub fn compile_workers_capped(&self) -> usize {
        self.compile_workers.clamp(1, MAX_COMPILE_WORKERS)
    }

    pub fn compile_batch_capped(&self) -> usize {
        self.compile_batch_size.clamp(1, MAX_COMPILE_BATCH)
    }

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

    /// Back from what was written on the job.
    ///
    /// The queue stores [`Held::as_str`], and the interface needs the
    /// sentence. Without this the owner is shown `too-much-at-once`, which
    /// is a slug, in a panel that is supposed to explain itself.
    pub fn from_slug(slug: &str) -> Option<Held> {
        match slug {
            "ask" => Some(Held::Ask),
            "manual" => Some(Held::Manual),
            "battery" => Some(Held::Battery),
            "too-much-at-once" => Some(Held::TooMuchAtOnce),
            _ => None,
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
            // Every slug the queue can store must come back as a sentence,
            // or the work panel prints the slug at the owner.
            assert_eq!(Held::from_slug(held.as_str()), Some(held));
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
            engine: "cursor-agent".into(),
            compile_workers: 3,
            compile_batch_size: 12,
            relate_after_build: true,
        };
        lake.set_policy(&mine).unwrap();
        assert_eq!(lake.policy(), mine);
    }

    #[test]
    fn a_policy_saved_before_engine_existed_defaults_to_auto() {
        let raw: Policy = serde_json::from_str(
            r#"{"processing":"automatic","pauseOnBattery":true,"largeScan":500}"#,
        )
        .unwrap();
        assert_eq!(raw.engine, "auto");
        assert_eq!(raw.compile_workers, DEFAULT_COMPILE_WORKERS);
        assert_eq!(raw.compile_batch_size, DEFAULT_COMPILE_BATCH);
        assert!(raw.relate_after_build);
    }

    #[test]
    fn relate_after_build_defaults_on() {
        assert!(Policy::default().relate_after_build);
    }

    #[test]
    fn worker_and_batch_knobs_are_capped() {
        let loose = Policy {
            compile_workers: 99,
            compile_batch_size: 0,
            ..Policy::default()
        };
        assert_eq!(loose.compile_workers_capped(), MAX_COMPILE_WORKERS);
        assert_eq!(loose.compile_batch_capped(), 1);
    }

    #[test]
    fn a_corrupt_setting_falls_back_rather_than_stopping_everything() {
        let lake = Lake::in_memory().unwrap();
        lake.set_setting(KEY, "{not json").unwrap();
        assert_eq!(lake.policy(), Policy::default());
    }
}

/// Whose company this is.
///
/// Kept in the lake rather than passed on the command line, because four
/// different entry points — the daemon, the gateway, the extension and the
/// connect command — all have to agree on it, and a default baked into each
/// of them is how three of them end up saying "Termoval d.o.o." on somebody
/// else's machine.
const COMPANY_KEY: &str = "company";

impl Lake {
    /// The company's name, or a reasonable stand-in.
    ///
    /// Falls back to the first source folder's own name: someone who dropped
    /// in `~/Documents/Termoval` has already told us what to call them, and
    /// asking again is a question with an answer already on screen.
    pub fn company(&self) -> String {
        if let Ok(Some(name)) = self.setting(COMPANY_KEY) {
            if !name.trim().is_empty() {
                return name;
            }
        }
        if let Ok(sources) = self.sources() {
            if let Some((_, name, ..)) = sources.first() {
                if !name.trim().is_empty() {
                    return name.clone();
                }
            }
        }
        "Your company".to_string()
    }

    pub fn set_company(&self, name: &str) -> Result<()> {
        self.set_setting(COMPANY_KEY, name.trim())
    }

    /// Whether the owner has ever said who they are.
    pub fn company_is_known(&self) -> bool {
        self.setting(COMPANY_KEY)
            .ok()
            .flatten()
            .map(|name| !name.trim().is_empty())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod company_tests {
    use super::*;

    #[test]
    fn an_empty_lake_has_a_neutral_name_not_a_demo_one() {
        let lake = Lake::in_memory().unwrap();
        assert_eq!(lake.company(), "Your company");
        assert!(!lake.company_is_known());
    }

    #[test]
    fn the_first_folder_names_the_company_until_the_owner_says_otherwise() {
        let lake = Lake::in_memory().unwrap();
        lake.put_source("s1", "Termoval", "/tmp/termoval", "folder", "codex")
            .unwrap();
        assert_eq!(lake.company(), "Termoval");
        // Still not "known": a folder name is a guess, and onboarding should
        // still ask.
        assert!(!lake.company_is_known());

        lake.set_company("Termoval d.o.o.").unwrap();
        assert_eq!(lake.company(), "Termoval d.o.o.");
        assert!(lake.company_is_known());
    }

    #[test]
    fn a_blank_name_does_not_overwrite_the_fallback() {
        let lake = Lake::in_memory().unwrap();
        lake.set_company("   ").unwrap();
        assert_eq!(lake.company(), "Your company");
    }
}
