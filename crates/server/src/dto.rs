//! The wire shape the interface already expects.
//!
//! `knowlith/src/lib/types.ts` is the contract, and it was written before any
//! of this existed. These types exist to satisfy it exactly rather than to
//! make the Rust side convenient: the interface should not have to learn a
//! second vocabulary because the storage layer happens to name things
//! differently.
//!
//! The two places the names genuinely differ are translated here and nowhere
//! else — `proposed` becomes `draft` and `conflicted` becomes `conflict`,
//! because those are the words the screens use.

use knowlith_core::object::{RelationOrigin, RelationType};
use knowlith_core::{Block, ContextObject, Document, DocumentKind, Evidence, ObjectKind, ObjectStatus, Subtype};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceDto {
    pub id: String,
    pub document_id: String,
    pub document_name: String,
    pub locator: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub quote: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Always true on the wire. Evidence that does not verify is never
    /// stored, so an unverified span cannot reach this point — and the field
    /// stays, because the interface shows the mark and a future recheck
    /// failure has to be able to clear it.
    pub verified: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationDto {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub target_id: String,
    pub target_title: String,
    /// `structural` — one object's title appears in another's text.
    /// `model` — the engine proposed it, and nothing can check it against a
    /// sentence, so it is shown as suggested and can be removed.
    /// `manual` — a person drew it.
    pub origin: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectDto {
    pub id: String,
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtype: Option<&'static str>,
    pub title: String,
    pub body: String,
    pub path: String,
    pub evidence: Vec<EvidenceDto>,
    pub relations: Vec<RelationDto>,
    pub confidence: f32,
    pub status: &'static str,
    pub version: u32,
    pub valid_from: String,
    pub valid_to: Option<String>,
    pub supersedes: Option<String>,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_by: Option<String>,
    pub edited_on_approval: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictSideDto {
    pub label: String,
    pub value: String,
    pub evidence: EvidenceDto,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictDto {
    pub summary: String,
    pub sides: Vec<ConflictSideDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewItemDto {
    pub id: String,
    pub object_id: String,
    pub kind: &'static str,
    pub title: String,
    pub before: Option<String>,
    pub after: String,
    pub evidence: Vec<EvidenceDto>,
    pub confidence: f32,
    pub affects: Vec<RelationDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict: Option<ConflictDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<CoverageDto>,
    pub compiled_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTypeDto {
    pub ext: String,
    pub count: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDigestDto {
    pub at: String,
    pub walked: usize,
    pub changed: usize,
    pub unchanged: usize,
    pub unreadable: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDto {
    pub id: String,
    pub name: String,
    pub path: String,
    pub kind: &'static str,
    pub access: &'static str,
    pub file_count: usize,
    pub bytes: u64,
    pub last_sync: String,
    pub processor: &'static str,
    pub status: String,
    pub last_analyzed: String,
    pub changes_found: usize,
    pub conflicts_found: usize,
    pub file_types: Vec<FileTypeDto>,
    /// Last walk summary, when the worker has recorded one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_digest: Option<SourceDigestDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceBlockDto {
    pub locator: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub heading: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cells: Option<Vec<String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDocumentDto {
    pub name: String,
    pub kind: &'static str,
    pub path: String,
    pub modified: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<String>>,
    pub blocks: Vec<SourceBlockDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryDto {
    pub rules: usize,
    pub processes: usize,
    pub terms: usize,
    pub skills: usize,
    pub conflicts: usize,
    pub files_read: usize,
    pub spans_extracted: usize,
    pub duration_seconds: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolReadDto {
    pub tool: String,
    pub last_read_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityDto {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub source: String,
    pub at: String,
    pub tone: &'static str,
}

// --------------------------------------------------------------- mapping --

pub fn kind_str(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Rule => "rule",
        ObjectKind::Process => "process",
        ObjectKind::Term => "term",
        ObjectKind::Fact => "fact",
        ObjectKind::Skill => "skill",
    }
}

/// The interface's vocabulary, which is the owner's vocabulary.
pub fn status_str(status: ObjectStatus) -> &'static str {
    match status {
        ObjectStatus::Proposed => "draft",
        ObjectStatus::Approved => "approved",
        ObjectStatus::Conflicted => "conflict",
        ObjectStatus::Superseded => "superseded",
        ObjectStatus::Rejected => "rejected",
    }
}

pub fn subtype_str(subtype: Subtype) -> &'static str {
    match subtype {
        Subtype::Term => "term",
        Subtype::Product => "product",
        Subtype::Policy => "policy",
        Subtype::Reference => "reference",
        Subtype::Template => "template",
    }
}

fn relation_str(kind: RelationType) -> &'static str {
    match kind {
        RelationType::DependsOn => "depends_on",
        RelationType::UsedBy => "used_by",
        RelationType::DerivedFrom => "derived_from",
        RelationType::ConflictsWith => "conflicts_with",
    }
}

pub fn document_kind_str(kind: DocumentKind) -> &'static str {
    // The interface's preview only knows these three shapes; everything else
    // renders as prose, which is what `docx` does there.
    match kind {
        DocumentKind::Xlsx | DocumentKind::Csv => "xlsx",
        DocumentKind::Pdf => "pdf",
        _ => "docx",
    }
}

pub fn evidence_dto(evidence: &Evidence, document: Option<&Document>) -> EvidenceDto {
    EvidenceDto {
        // Stable per span, so React keys do not shuffle between polls.
        //
        // The whole span, not only where it starts. Two claims quoting the
        // same paragraph from different offsets into it share a start
        // byte, and the interface then had two list items under one key —
        // which React resolves by dropping one of them, so the owner is
        // shown one piece of evidence where there are two.
        id: format!(
            "{}-{}-{}",
            evidence.document_id, evidence.start_byte, evidence.end_byte
        ),
        document_id: evidence.document_id.clone(),
        document_name: document
            .map(|d| d.name.clone())
            .unwrap_or_else(|| evidence.document_id.clone()),
        locator: evidence.locator.clone(),
        start_byte: evidence.start_byte,
        end_byte: evidence.end_byte,
        quote: evidence.quote.clone(),
        page: document.and_then(|d| d.block(&evidence.locator)).and_then(|b| b.page),
        verified: true,
    }
}

pub fn relation_dto(relation: &knowlith_core::Relation) -> RelationDto {
    RelationDto {
        kind: relation_str(relation.kind),
        target_id: relation.target_id.clone(),
        target_title: relation.target_label.clone(),
        origin: origin_str(relation.origin),
    }
}

pub fn object_dto(object: &ContextObject, documents: &[Document]) -> ObjectDto {
    let find = |id: &str| documents.iter().find(|d| d.id == id);
    ObjectDto {
        id: object.id.clone(),
        kind: kind_str(object.kind),
        subtype: object.subtype.map(subtype_str),
        title: object.title.clone(),
        body: object.body.clone(),
        path: object.path.clone(),
        evidence: object
            .evidence
            .iter()
            .map(|e| evidence_dto(e, find(&e.document_id)))
            .collect(),
        relations: object.relations.iter().map(relation_dto).collect(),
        confidence: object.confidence.0,
        status: status_str(object.status),
        version: object.version,
        valid_from: object.valid_from.clone(),
        valid_to: object.valid_to.clone(),
        supersedes: object.supersedes.clone(),
        updated_at: object.updated_at.clone(),
        decided_by: object.decided_by.clone(),
        edited_on_approval: object.edited_on_approval,
    }
}

pub fn block_dto(block: &Block) -> SourceBlockDto {
    SourceBlockDto {
        locator: block.locator.clone(),
        heading: block.kind == knowlith_core::BlockKind::Heading,
        text: block.cells.is_none().then(|| block.text.clone()),
        cells: block.cells.clone(),
    }
}

pub fn document_dto(document: &Document) -> SourceDocumentDto {
    SourceDocumentDto {
        name: document.name.clone(),
        kind: document_kind_str(document.kind),
        path: document.path.clone(),
        modified: document.modified.clone(),
        columns: document.columns.clone(),
        blocks: document.blocks.iter().map(block_dto).collect(),
    }
}

pub fn origin_str(origin: RelationOrigin) -> &'static str {
    match origin {
        RelationOrigin::Structural => "structural",
        RelationOrigin::Model => "model",
        RelationOrigin::Manual => "manual",
    }
}

// ----------------------------------------------------------- coverage ----

/// Two documents say something about one subject, and neither states a
/// figure — so the compiler cannot assert that they disagree.
///
/// Before this existed, the newer document simply won and the older one
/// vanished from the screen while remaining, invisibly, as a second quote on
/// the same object. That is a true state of affairs the owner could not see.
/// This panel says only what is verifiable — these documents both cover it,
/// here is each one's sentence, the newer wording is the one in use — and
/// asserts no contradiction, because none can be established.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageDto {
    pub summary: String,
    pub sources: Vec<CoverageSourceDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageSourceDto {
    pub label: String,
    pub modified: String,
    /// The wording this object uses came from here.
    pub current: bool,
    pub evidence: EvidenceDto,
}

// -------------------------------------------------------------- skills ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFieldDto {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub description: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDocDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub markdown: String,
    pub requires: Vec<RelationDto>,
    pub inputs: Vec<SkillFieldDto>,
    pub outputs: Vec<SkillFieldDto>,
    pub affects: Vec<RelationDto>,
    pub evidence: Vec<EvidenceDto>,
    pub status: &'static str,
    /// Who approved it, and nobody when nobody has. The interface used to
    /// print a name that was not on the wire at all — a fixture's — beside
    /// any approved skill. An invented approver is the worst thing this
    /// product can put on a screen, so the field exists and is `None`
    /// rather than being filled in by the page.
    pub decided_by: Option<String>,
    pub version: u32,
    pub updated_at: String,
}

/// Reads a skill's own Markdown back into the shape the interface shows.
///
/// The compiler writes that Markdown under a fixed grammar it controls, so
/// this parse is exact rather than hopeful. Keeping the Markdown as the one
/// stored form is the point: the file under `~/Knowlith/skills/` is the
/// artifact, and everything on screen is a reading of it. Storing the
/// structure separately would create a second copy to keep in agreement.
pub fn skill_fields(markdown: &str, heading: &str) -> Vec<SkillFieldDto> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("**") {
            inside = trimmed == heading;
            continue;
        }
        if !inside {
            continue;
        }
        let Some(item) = trimmed.strip_prefix("- ") else {
            continue;
        };
        let (name, description) = item.split_once(" — ").unwrap_or((item, ""));
        out.push(SkillFieldDto {
            name: name.trim().to_string(),
            kind: "string".into(),
            description: description.trim().to_string(),
        });
    }
    out
}

/// The first paragraph: what the skill is for, before the steps.
pub fn skill_description(markdown: &str) -> String {
    markdown
        .lines()
        .map(str::trim)
        .take_while(|line| !line.starts_with("**"))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SKILL: &str = "Koristi kada klijent traži ponudu.\n\n\
**Before you start**\n\n- klijent — Naziv klijenta.\n- oprema — Što se nudi.\n\n\
**Steps**\n\n1. Otvori cjenik.\n2. Pošalji ponudu.\n\n\
**Result**\n\n- ponuda — Ponuda spremna za slanje.\n";

    #[test]
    fn a_skills_own_markdown_reads_back_into_the_shape_the_screen_shows() {
        assert_eq!(skill_description(SKILL), "Koristi kada klijent traži ponudu.");

        let inputs = skill_fields(SKILL, "**Before you start**");
        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[0].name, "klijent");
        assert_eq!(inputs[0].description, "Naziv klijenta.");

        let outputs = skill_fields(SKILL, "**Result**");
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].name, "ponuda");
    }

    #[test]
    fn the_steps_are_not_mistaken_for_inputs() {
        assert!(skill_fields(SKILL, "**Before you start**").iter().all(|f| f.name != "Otvori cjenik."));
    }
}

/// A folder the owner picked or typed, and what is in it.
///
/// `chosen` is `None` only when the system chooser was closed without a
/// choice, which the interface treats as "nothing happened".
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseDto {
    pub chosen: Option<String>,
    pub name: Option<String>,
    pub inventory: Option<knowlith_extract::inventory::Inventory>,
}

/// What the interface sends to add a folder.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSource {
    pub path: String,
    /// What to call it. The folder's own name when left out.
    pub name: Option<String>,
    /// Which engine reads this folder: `codex`, `claude-code`, `cursor-agent`,
    /// `managed`. When omitted, the company's saved policy engine is used.
    #[serde(default)]
    pub processor: Option<String>,
}

#[derive(Deserialize)]
pub struct PathQuery {
    pub path: String,
}

/// The answer to adding a folder.
///
/// `alreadyKnown` is not an error: pointing at a folder that is already being
/// watched is a reasonable thing to do, and the honest answer is to say so
/// and show the owner the source they already have.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceAddedDto {
    pub id: String,
    pub name: String,
    pub path: String,
    /// False when a walk of this folder was already waiting.
    pub queued: bool,
    pub already_known: bool,
}

/// One thing an AI tool did with this company's knowledge.
///
/// Built from cases and reads rather than from a log: a log would be a
/// second thing to keep in agreement with the lake, and the lake already
/// knows everything this shows.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDto {
    pub id: String,
    /// The application's slug, or null for a client we do not recognise.
    pub app: Option<String>,
    /// What to call it on screen. "Another tool" when we cannot tell.
    pub app_label: String,
    /// The question the agent declared, when it opened a case.
    pub question: Option<String>,
    pub at: String,
    /// What it actually read, by title.
    pub read: Vec<UsedObjectDto>,
    /// What the gateway named as relevant and the agent never opened. The
    /// part no other retrieval system can report, and the reason a case
    /// exists at all.
    pub skipped: Vec<UsedObjectDto>,
    /// True once the agent called `check_coverage`. An open case is an
    /// answer given without checking what was missed.
    pub closed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsedObjectDto {
    pub id: String,
    pub title: String,
    /// `rule` | `process` | `term` | `skill` | `fact`.
    pub kind: String,
}

/// What Knowlith is doing right now, and what it just did.
///
/// One response rather than three, because the panel is one thing on
/// screen and three requests a second racing each other would show a
/// progress bar and a log that disagree about how far along the work is.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkDto {
    /// `reading` | `thinking` | `preparing` | `idle`.
    ///
    /// Derived from what is still queued, never stored. A stored stage is
    /// a second source of truth about the queue, and the day it disagrees
    /// the owner is told the work is finished while it runs.
    pub stage: &'static str,
    /// A sentence for the stage, in the owner's words rather than the
    /// worker's job kinds.
    pub doing: &'static str,
    /// Finished within this burst of work, and the total it belongs to.
    /// Both zero when nothing is happening.
    pub done: usize,
    pub total: usize,
    pub held: Option<HoldDto>,
    /// Newest first. What was read, what came out of it, and what would
    /// not read at all.
    pub lines: Vec<WorkLineDto>,
}

/// Work the owner's own policy is holding back.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldDto {
    /// Plain words: "Knowlith is running on battery".
    pub reason: String,
    pub count: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkLineDto {
    /// What it was about, by name. Never an id: `doc:9982ba86ea32837c`
    /// describes the owner's own file in a vocabulary they cannot read.
    pub subject: String,
    /// What the job did, in the worker's own words.
    pub note: String,
    /// `done` | `failed` | `working`.
    pub state: &'static str,
    pub at: String,
}

#[cfg(test)]
mod evidence_id_tests {
    use super::evidence_dto;
    use knowlith_core::Evidence;

    fn span(start: usize, end: usize) -> Evidence {
        Evidence {
            document_id: "doc:90aafc8bde8e17fe".into(),
            locator: "page 1, ¶16".into(),
            start_byte: start,
            end_byte: end,
            quote: "…".into(),
        }
    }

    /// Two claims quoting the same paragraph from the same offset, one
    /// span nested in the other, shared an id. React then rendered one
    /// row where there were two, so the owner was shown less evidence
    /// than the object actually rests on.
    #[test]
    fn two_spans_from_the_same_offset_are_told_apart() {
        let a = evidence_dto(&span(404, 458), None);
        let b = evidence_dto(&span(404, 439), None);
        assert_ne!(a.id, b.id, "both spans answered to {}", a.id);
    }

    #[test]
    fn the_same_span_is_still_the_same_thing_between_polls() {
        assert_eq!(evidence_dto(&span(404, 458), None).id, evidence_dto(&span(404, 458), None).id);
    }
}
