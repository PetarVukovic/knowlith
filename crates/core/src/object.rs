//! The canonical unit of company knowledge.
//!
//! A `ContextObject` is one thing the company has decided, written in the
//! owner's own words, carrying the source spans it was derived from. It is what
//! the MCP gateway serves and what the review queue approves. The Markdown file
//! under `~/Knowlith/` is its serialisation; this is its shape.

use serde::{Deserialize, Serialize};

use crate::evidence::Evidence;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ObjectKind {
    /// A decision with a limit: a discount ceiling, a deadline, who approves.
    Rule,
    /// An ordered procedure the company follows.
    Process,
    /// A word this company uses in a particular way.
    Term,
    /// A value that is true until the document behind it changes.
    Fact,
    /// A procedure written for an agent to execute rather than for a person
    /// to read. Never extracted from a document: a skill is assembled from
    /// objects the owner has already approved, which is why it is the one
    /// kind the candidate schema cannot produce.
    Skill,
}

/// A quiet label under the object's title. Never a level of navigation — the
/// owner should not have to learn a taxonomy to find their own price list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Subtype {
    Term,
    Product,
    Policy,
    Reference,
    Template,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ObjectStatus {
    /// Compiled, waiting for a person. Never served to an agent.
    Proposed,
    /// Approved and in effect.
    Approved,
    /// Two documents disagree; the owner has to choose.
    Conflicted,
    /// Replaced by a later version, kept so "what applied in March" has an
    /// answer.
    Superseded,
    /// Refused. Kept so the same claim is not re-proposed every scan.
    Rejected,
}

/// How much the sources agree, from 0.0 to 1.0.
///
/// Calibrated from source agreement and span quality, never from a model's own
/// estimate of itself — a model asked how sure it is will say "high", and that
/// number would be a decoration. The UI renders this as language in Simple mode
/// and as a number only for engineers.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Confidence(pub f32);

impl Confidence {
    /// The wording the owner sees. Thresholds live here so every surface says
    /// the same thing about the same number.
    pub fn phrase(self) -> &'static str {
        match self.0 {
            c if c >= 0.85 => "Clearly stated",
            c if c >= 0.6 => "Stated, in one place",
            c if c >= 0.35 => "Implied, not written",
            _ => "Weakly supported",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    /// This object needs the target to be true.
    DependsOn,
    /// The target breaks if this object changes. The reverse of `DependsOn`,
    /// stored explicitly so impact queries do not have to scan.
    UsedBy,
    /// This object was computed from the target.
    DerivedFrom,
    /// This object disagrees with the target.
    ConflictsWith,
}

/// Where an edge came from. Structure is cheap and reliable; a model-proposed
/// edge is a claim like any other and needs a person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelationOrigin {
    /// Derived from filenames, shared codes, folder layout, explicit references.
    Structural,
    /// Proposed by the compiler's engine.
    Model,
    /// Drawn by a person.
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relation {
    pub target_id: String,
    pub target_label: String,
    #[serde(rename = "type")]
    pub kind: RelationType,
    pub origin: RelationOrigin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextObject {
    /// Stable and readable: `rule:sales.discount`. Survives re-compilation, so
    /// history and relations are not orphaned by a rescan.
    pub id: String,
    pub kind: ObjectKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtype: Option<Subtype>,
    pub title: String,
    /// Markdown, in the owner's language.
    pub body: String,
    pub status: ObjectStatus,
    pub confidence: Confidence,
    pub version: u32,
    /// RFC 3339. When this version started applying — which is the date the
    /// document says, not the date Knowlith read it.
    pub valid_from: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_by: Option<String>,
    /// True when the owner changed the wording before approving. Worth keeping:
    /// a high edit rate is the honest measure of how good the compiler is.
    #[serde(default)]
    pub edited_on_approval: bool,
    /// Never empty for an approved object. An object that cannot be traced to a
    /// span is not knowledge, it is a guess.
    pub evidence: Vec<Evidence>,
    pub relations: Vec<Relation>,
    /// Path of the canonical Markdown under the Lake root.
    pub path: String,
    pub updated_at: String,
}

impl ContextObject {
    /// Whether this object may be served to an agent.
    pub fn is_servable(&self) -> bool {
        self.status == ObjectStatus::Approved && !self.evidence.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_wording_is_monotonic() {
        assert_eq!(Confidence(0.95).phrase(), "Clearly stated");
        assert_eq!(Confidence(0.7).phrase(), "Stated, in one place");
        assert_eq!(Confidence(0.4).phrase(), "Implied, not written");
        assert_eq!(Confidence(0.1).phrase(), "Weakly supported");
    }
}
