//! Whether the gateway can honestly serve this company right now.
//!
//! An empty index that returns "nothing found" is a lie when the real answer
//! is "nothing has been compiled yet" or "work is still queued".

use knowlith_lake::Lake;

/// What the lake looks like from the gateway's side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Approved knowledge is available.
    Ready,
    /// Nothing approved yet — the owner has not finished onboarding.
    Empty,
    /// Documents are still being compiled or settled.
    Compiling { compile: i64, settle: i64 },
}

pub fn status(lake: &Lake) -> Status {
    let approved = lake
        .objects()
        .map(|objects| {
            objects
                .iter()
                .filter(|o| crate::gate::is_servable(o))
                .count()
        })
        .unwrap_or(0);

    if approved == 0 {
        return Status::Empty;
    }

    let compile = lake.pending_of_kind("compile_document").unwrap_or(0);
    let settle = lake.pending_of_kind("settle").unwrap_or(0);
    if compile > 0 || settle > 0 {
        return Status::Compiling { compile, settle };
    }

    Status::Ready
}

/// A sentence the model should read when the lake is not ready to answer.
pub fn message(status: &Status, company: &str) -> Option<String> {
    match status {
        Status::Ready => None,
        Status::Empty => Some(format!(
            "{company} has not approved anything in Knowlith yet. Say so plainly — do not answer from general knowledge."
        )),
        Status::Compiling { compile, settle } => Some(format!(
            "{company}'s knowledge is still being compiled ({compile} documents, {settle} settle jobs queued). \
             Say that the owner should wait for the work panel to finish, or ask about something already approved."
        )),
    }
}
