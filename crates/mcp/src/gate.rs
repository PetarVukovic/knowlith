//! What may be said to an agent, and in what form.
//!
//! This is the product decision the rest of the gateway is built around, so
//! it is one small module rather than a condition repeated in eleven tool
//! handlers.
//!
//! The rule that is easy to get wrong is the middle one. An object the owner
//! has not settled — two documents disagree, or nobody has approved it yet —
//! is **named but not given**. Hiding it entirely feels safer and is worse:
//! an agent told nothing about payment terms does not stay silent, it writes
//! "30 days" because that is what payment terms usually are. An agent told
//! "there is an unsettled question about payment terms" says exactly that
//! and asks. Silence does not prevent a wrong answer; it only removes the
//! one signal that could have stopped it.

use knowlith_core::{ContextObject, ObjectStatus};

/// How much of an object an agent is allowed to see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Access {
    /// Text, figures, quote and source.
    Full,
    /// The subject only: "there is an open question about X".
    NameOnly(&'static str),
    /// Nothing at all. A rejected claim and a superseded version are not
    /// weaker knowledge, they are decisions that this is not the answer.
    Withheld,
}

/// The gate itself.
pub fn access(object: &ContextObject) -> Access {
    match object.status {
        ObjectStatus::Approved if object.evidence.is_empty() => {
            // Approval requires evidence, so this is a lake that was edited
            // by something other than Knowlith. Refusing is the only safe
            // reading: an approved object with no source is the exact shape
            // of a claim that was never checked.
            Access::Withheld
        }
        ObjectStatus::Approved => Access::Full,
        ObjectStatus::Conflicted => Access::NameOnly("two documents disagree and the owner has not chosen"),
        ObjectStatus::Proposed => Access::NameOnly("waiting for the owner to approve it"),
        ObjectStatus::Superseded | ObjectStatus::Rejected => Access::Withheld,
    }
}

pub fn is_servable(object: &ContextObject) -> bool {
    access(object) == Access::Full
}

/// Objects an agent may read in full.
pub fn servable(objects: Vec<ContextObject>) -> Vec<ContextObject> {
    objects.into_iter().filter(is_servable).collect()
}

/// Objects an agent may only be told the subject of.
pub fn unsettled(objects: &[ContextObject]) -> Vec<(&ContextObject, &'static str)> {
    objects
        .iter()
        .filter_map(|object| match access(object) {
            Access::NameOnly(why) => Some((object, why)),
            _ => None,
        })
        .collect()
}

/// The sentence attached to an answer that rests on something that moved.
///
/// Serving a stale answer silently and withholding one silently are both
/// worse than saying which it is, so the flag travels with the text rather
/// than being a field the agent has to think to look at.
pub fn stale_warning(stale_since: Option<&str>) -> Option<String> {
    stale_since.map(|at| {
        format!(
            "Check this one: something it rests on changed on {} and the owner has not reviewed it since.",
            at.split('T').next().unwrap_or(at)
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use knowlith_core::{Confidence, Evidence, ObjectKind};

    fn object(status: ObjectStatus, spans: usize) -> ContextObject {
        ContextObject {
            id: "rule:test".into(),
            kind: ObjectKind::Rule,
            subtype: None,
            title: "Rok plaćanja".into(),
            body: "30 dana od izdavanja računa.".into(),
            status,
            confidence: Confidence(0.9),
            version: 1,
            valid_from: "2026-01-01T00:00:00Z".into(),
            valid_to: None,
            supersedes: None,
            decided_by: None,
            edited_on_approval: false,
            evidence: (0..spans)
                .map(|n| Evidence {
                    document_id: "doc:1".into(),
                    locator: format!("§{n}"),
                    start_byte: 0,
                    end_byte: 4,
                    quote: "text".into(),
                })
                .collect(),
            relations: Vec::new(),
            path: "rules/test.md".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn approved_with_a_source_is_served() {
        assert_eq!(access(&object(ObjectStatus::Approved, 1)), Access::Full);
    }

    #[test]
    fn approved_without_a_source_is_not_served() {
        assert_eq!(access(&object(ObjectStatus::Approved, 0)), Access::Withheld);
    }

    #[test]
    fn an_unsettled_question_is_named_never_answered() {
        for status in [ObjectStatus::Conflicted, ObjectStatus::Proposed] {
            match access(&object(status, 1)) {
                Access::NameOnly(why) => assert!(!why.is_empty()),
                other => panic!("{status:?} came back as {other:?}"),
            }
        }
    }

    #[test]
    fn a_decision_that_this_is_not_the_answer_is_silence() {
        assert_eq!(access(&object(ObjectStatus::Rejected, 1)), Access::Withheld);
        assert_eq!(access(&object(ObjectStatus::Superseded, 1)), Access::Withheld);
    }

    #[test]
    fn a_stale_answer_says_so_with_a_date_a_person_can_read() {
        let warning = stale_warning(Some("2026-03-04T11:20:00Z")).unwrap();
        assert!(warning.contains("2026-03-04"));
        assert!(!warning.contains('T'), "{warning}");
        assert!(stale_warning(None).is_none());
    }
}
