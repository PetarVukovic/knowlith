//! What Knowlith knows about the machine it is installed on.
//!
//! Everything operating-system-shaped lives here: where the lake goes, where
//! each AI application keeps its settings, how to hand ourselves to one, how
//! to bring its window up afterwards, and what the company's mark looks like
//! in its chat box.
//!
//! Keeping it in one crate is what makes the Windows story checkable. The
//! rest of the product asks this crate for a path and never for an
//! environment variable, so "does this work on Windows" is a question about
//! four files rather than about eleven thousand lines.

pub mod apps;
pub mod autostart;
pub mod bundle;
pub mod connect;
pub mod guidance;
pub mod icon;
pub mod launch;
pub mod paths;
pub mod pick;
pub mod power;

pub use apps::{App, Format, program_on_path};
pub use autostart::Autostart;
pub use bundle::Bundle;
pub use guidance::Guide;
pub use connect::{ConnectError, SERVER_NAME, Status, connect, disconnect, status, status_all};
pub use icon::{Icon, company_icon};
pub use launch::{Outcome, open_or_restart};
pub use pick::PickError;
pub use power::{Power, power};
