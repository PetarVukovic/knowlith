//! Turning documents into company knowledge.
//!
//! Four stages, not ten. Each one that involves a model multiplies its own
//! error into everything after it — at 95% per stage, ten stages land near
//! 60%, which is worse than useless because it is wrong in a way that looks
//! right. So exactly one stage here asks a model anything:
//!
//! 1. **Structural** — pick the blocks worth asking about. Deterministic.
//! 2. **Candidates** — a model reads one document and proposes claims, each
//!    with the sentence it came from. The only stage with an engine in it.
//! 3. **Consolidation** — group claims about the same subject across
//!    documents, pick the current one, mark the rest superseded, and record
//!    disagreements as conflicts. Deterministic.
//! 4. **Validation** — find each quote in the document, check the span
//!    mechanically, and keep only what survives. Deterministic.
//!
//! The model is therefore never trusted with *which* claim is current, only
//! with noticing that a claim is there. Recency, authority and contradiction
//! are decided by rules a person can read.

mod candidates;
mod consolidate;
mod relations;
mod similar;
mod skills;
mod structural;

use std::collections::HashMap;

use knowlith_core::object::{Relation, RelationOrigin, RelationType, Subtype};
use knowlith_core::{Confidence, ContextObject, Document, Evidence, ObjectKind, ObjectStatus};
use knowlith_engine::{Engine, EngineError, EngineUsage};

pub use candidates::{Candidate, CANDIDATE_SCHEMA, INSTRUCTIONS, propose_many};
pub use consolidate::{Conflict, Group};
pub use relations::{ProposedEdge, RELATION_INSTRUCTIONS, RELATION_SCHEMA, RelationRun, edge_pair, propose as propose_relations};
pub use similar::{Hint, HintKind, hints};
pub use skills::{Draft, Field, SKILL_INSTRUCTIONS, SKILL_SCHEMA, SkillRun, draft_all, skill_id};
pub use structural::{Skip, classify, worth_reading};

#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error("the engine replied with something that is not the requested JSON: {0}")]
    BadReply(String),
    /// The reply parsed and was still unusable — a drafted skill that states
    /// a number no approved rule states, for instance. Retrying produces the
    /// same thing, so this is reported rather than queued again.
    #[error("{0}")]
    Refused(String),
}

type Result<T> = std::result::Result<T, CompileError>;

/// Why a proposed claim did not become knowledge.
///
/// Kept per claim rather than counted, because "41 rejected" tells an owner
/// nothing and "the model quoted a sentence this document does not contain"
/// tells them the compiler is working.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dropped {
    pub document: String,
    pub title: String,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct Compilation {
    pub objects: Vec<ContextObject>,
    pub conflicts: Vec<Conflict>,
    pub dropped: Vec<Dropped>,
    /// Documents the engine was actually asked about.
    pub documents_read: usize,
    pub candidates_proposed: usize,
}

/// Runs the whole pipeline over a set of documents.
///
/// Each document is compiled independently, so one unreadable file costs that
/// file and nothing else — a scan of two thousand documents must not be an
/// all-or-nothing transaction.
pub fn compile(engine: &dyn Engine, documents: &[Document]) -> Result<Compilation> {
    let mut out = Compilation::default();
    let mut proposals: Vec<(String, Candidate)> = Vec::new();

    for document in documents {
        let mut read = Reading::default();
        read_one(engine, document, &mut read, None)?;
        out.documents_read += read.documents_read;
        out.candidates_proposed += read.candidates.len();
        out.dropped.extend(read.dropped);
        for candidate in read.candidates {
            proposals.push((document.id.clone(), candidate));
        }
    }

    let settled = settle(&proposals, documents);
    out.objects = settled.objects;
    out.conflicts = settled.conflicts;
    out.dropped.extend(settled.dropped);
    Ok(out)
}

/// What one document's reading produced. Stages 1 and 2 only.
#[derive(Debug, Default, Clone)]
pub struct Reading {
    pub candidates: Vec<Candidate>,
    pub dropped: Vec<Dropped>,
    /// 1 unless the document was held back by stage 1.
    pub documents_read: usize,
}

/// Stages 1 and 2: the expensive half, and the only half with a model in it.
///
/// Separated from the rest because it is the half that must not be repeated.
/// A document whose text has not moved has already been read; consolidating
/// it against a folder that has grown costs nothing and has to happen again.
pub fn read_one(
    engine: &dyn Engine,
    document: &Document,
    out: &mut Reading,
    company: Option<&str>,
) -> Result<()> {
    // Stage 1. A skipped document is reported, not silently dropped: the
    // owner needs to know that their payroll sheet was held back and that
    // last year's terms were left to this year's.
    if let Some(skip) = classify(document) {
        out.dropped.push(Dropped {
            document: document.name.clone(),
            title: String::new(),
            reason: skip.reason().to_string(),
        });
        return Ok(());
    }
    out.documents_read += 1;

    // Stage 2.
    match candidates::propose(engine, document, company) {
        Ok((found, _)) => out.candidates.extend(found),
        Err(CompileError::Engine(e)) if !e.is_retryable() => out.dropped.push(Dropped {
            document: document.name.clone(),
            title: String::new(),
            reason: format!("{e}"),
        }),
        // A transport failure is the queue's problem, not this document's.
        Err(e) => return Err(e),
    }
    Ok(())
}

/// Stages 1 and 2 over several documents in one CLI invoke.
///
/// Structural skips stay free. Everything that needs a model shares one
/// child process so a folder of hundreds of files is not hundreds of cold
/// starts. Settle still waits for the whole compile queue to drain.
#[derive(Debug, Default, Clone)]
pub struct BatchRead {
    pub readings: HashMap<String, Reading>,
    pub usage: Option<EngineUsage>,
}

pub fn read_many(
    engine: &dyn Engine,
    documents: &[&Document],
    company: Option<&str>,
) -> Result<BatchRead> {
    let mut out: HashMap<String, Reading> = HashMap::new();
    let mut need_model: Vec<&Document> = Vec::new();

    for document in documents {
        let mut reading = Reading::default();
        if let Some(skip) = classify(document) {
            reading.dropped.push(Dropped {
                document: document.name.clone(),
                title: String::new(),
                reason: skip.reason().to_string(),
            });
            out.insert(document.id.clone(), reading);
            continue;
        }
        need_model.push(*document);
        out.insert(document.id.clone(), reading);
    }

    if need_model.is_empty() {
        return Ok(BatchRead {
            readings: out,
            usage: None,
        });
    }

    match candidates::propose_many(engine, &need_model, company) {
        Ok((by_id, usage)) => {
            for document in need_model {
                let reading = out.entry(document.id.clone()).or_default();
                reading.documents_read = 1;
                if let Some(found) = by_id.get(&document.id) {
                    reading.candidates = found.clone();
                }
            }
            Ok(BatchRead {
                readings: out,
                usage,
            })
        }
        Err(CompileError::Engine(e)) if !e.is_retryable() => {
            for document in need_model {
                let reading = out.entry(document.id.clone()).or_default();
                reading.dropped.push(Dropped {
                    document: document.name.clone(),
                    title: String::new(),
                    reason: format!("{e}"),
                });
            }
            Ok(BatchRead {
                readings: out,
                usage: None,
            })
        }
        Err(e) => Err(e),
    }
}

/// Stages 3 and 4: the deterministic half, over the whole set at once.
///
/// This is where "which claim is current", "which document supersedes
/// which" and "where do two documents disagree" are decided, and every one
/// of those is a property of the set. Running it per document — which is
/// what a background worker naturally wants to do — answers all three
/// questions with a sample size of one and finds no conflicts at all.
pub fn settle(proposals: &[(String, Candidate)], documents: &[Document]) -> Compilation {
    let by_id: HashMap<&str, &Document> = documents.iter().map(|d| (d.id.as_str(), d)).collect();
    let (groups, conflicts) = consolidate::group(proposals, &by_id);

    let mut out = Compilation {
        conflicts,
        candidates_proposed: proposals.len(),
        ..Compilation::default()
    };

    for group in groups {
        match validate(&group, &by_id) {
            Ok(object) => out.objects.push(object),
            Err(reason) => out.dropped.push(Dropped {
                document: group.document_name.clone(),
                title: group.candidate.title.clone(),
                reason,
            }),
        }
    }

    out.objects.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Stage 4: the quote has to be findable, once, in the document it claims.
///
/// This is where a confident, fluent, entirely invented rule stops. The check
/// is the same one `knowlith-lake` runs on write; running it here as well
/// means the review queue never shows a claim that could not be stored.
fn validate(group: &Group, documents: &HashMap<&str, &Document>) -> std::result::Result<ContextObject, String> {
    let document = documents
        .get(group.document_id.as_str())
        .ok_or_else(|| "the document it came from is no longer in the lake".to_string())?;

    let mut evidence = Vec::new();
    for quote in &group.quotes {
        let Some((start, end)) = knowlith_core::locate(document, quote) else {
            return Err(format!(
                "the quoted sentence is not in {}, or appears more than once: \"{}\"",
                document.name,
                short(quote)
            ));
        };
        let locator = document
            .block_at(start)
            .map(|b| b.locator.clone())
            .unwrap_or_else(|| "unknown".into());
        evidence.push(Evidence {
            document_id: document.id.clone(),
            locator,
            start_byte: start,
            end_byte: end,
            quote: quote.clone(),
        });
    }

    // A conflicted object must carry the losing document's quote as well.
    // Without it the review screen had nothing honest to put beside the
    // current one and fell back to duplicating the winner.
    for (document_id, quote) in &group.disagreeing {
        let Some(document) = documents.get(document_id.as_str()) else {
            continue;
        };
        let Some((start, end)) = knowlith_core::locate(document, quote) else {
            continue;
        };
        let locator = document
            .block_at(start)
            .map(|b| b.locator.clone())
            .unwrap_or_else(|| "unknown".into());
        evidence.push(Evidence {
            document_id: document_id.clone(),
            locator,
            start_byte: start,
            end_byte: end,
            quote: quote.clone(),
        });
    }

    if evidence.is_empty() {
        return Err("no source sentence was given".into());
    }

    let kind = group.candidate.object_kind();

    // A price is a row to look up, not a decision to approve.
    //
    // Left alone, a model reading a price list produces one "fact" per row,
    // and a 380-row spreadsheet becomes 380 things a person has to approve.
    // Worse, it makes a number something a model recalls rather than
    // something the daemon reads: a price retrieved by similarity can come
    // back close, a price read from the row comes back right or not at all.
    // The sentences *around* the table — "all prices exclude VAT" — are
    // decisions and stay.
    if kind == ObjectKind::Fact
        && evidence.iter().all(|e| {
            document
                .block(&e.locator)
                .is_some_and(|b| b.kind == knowlith_core::BlockKind::TableRow)
        })
    {
        return Err(format!(
            "is a row in {}, and is served by looking the row up rather than by approving it",
            document.name
        ));
    }
    Ok(ContextObject {
        id: group.id.clone(),
        kind,
        subtype: subtype_for(kind, &group.candidate),
        title: group.candidate.title.clone(),
        body: group.candidate.statement.clone(),
        status: if group.conflicted {
            ObjectStatus::Conflicted
        } else {
            ObjectStatus::Proposed
        },
        confidence: confidence_of(group),
        version: 1,
        valid_from: document.modified.clone(),
        valid_to: None,
        supersedes: group.supersedes.clone(),
        decided_by: None,
        edited_on_approval: false,
        evidence,
        relations: group
            .uses
            .iter()
            .map(|target| Relation {
                target_id: target.clone(),
                target_label: target.clone(),
                kind: RelationType::DependsOn,
                origin: RelationOrigin::Structural,
                why: None,
                edge_confidence: None,
            })
            .collect(),
        path: path_for(kind, &group.id),
        updated_at: document.modified.clone(),
    })
}

/// Confidence from what can be counted, never from what the model says about
/// itself.
///
/// A model asked how sure it is answers "high" and means nothing by it. These
/// three signals are checkable: how many documents say it, whether anything
/// contradicts it, and whether it was stated or inferred.
fn confidence_of(group: &Group) -> Confidence {
    // A claim that survived stage 4 was quoted verbatim from a document that
    // exists. That is already "stated" — starting lower made every single
    // finding read as "worth a closer look", which tells the owner nothing
    // and trains them to ignore the label.
    let mut score: f32 = 0.68;
    // Agreement across documents is the strongest signal available without a
    // model, so it carries the most weight.
    score += 0.12 * (group.agreeing_documents.saturating_sub(1) as f32).min(2.0);
    if group.quotes.len() > 1 {
        score += 0.04;
    }
    if group.conflicted {
        score -= 0.35;
    }
    Confidence(score.clamp(0.05, 0.98))
}

fn subtype_for(kind: ObjectKind, candidate: &Candidate) -> Option<Subtype> {
    match kind {
        ObjectKind::Term => Some(Subtype::Term),
        ObjectKind::Fact => Some(Subtype::Reference),
        ObjectKind::Rule => Some(Subtype::Policy),
        ObjectKind::Process | ObjectKind::Skill => None,
    }
    .or_else(|| candidate.subtype.as_deref().and_then(parse_subtype))
}

fn parse_subtype(raw: &str) -> Option<Subtype> {
    match raw.to_ascii_lowercase().as_str() {
        "term" => Some(Subtype::Term),
        "product" => Some(Subtype::Product),
        "policy" => Some(Subtype::Policy),
        "reference" => Some(Subtype::Reference),
        "template" => Some(Subtype::Template),
        _ => None,
    }
}

fn path_for(kind: ObjectKind, id: &str) -> String {
    let slug = id.split_once(':').map(|(_, rest)| rest).unwrap_or(id);
    let folder = match kind {
        ObjectKind::Rule => "rules",
        ObjectKind::Process => "processes",
        ObjectKind::Term => "terms",
        ObjectKind::Fact => "facts",
        ObjectKind::Skill => "skills",
    };
    format!("{folder}/{}.md", slug.replace('.', "/"))
}

fn short(s: &str) -> String {
    if s.chars().count() <= 60 {
        return s.to_string();
    }
    s.chars().take(60).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;
    use knowlith_engine::{Reply, Request};

    struct Scripted(&'static str);

    impl Engine for Scripted {
        fn name(&self) -> &str {
            "Scripted"
        }
        fn run(&self, _request: &Request) -> knowlith_engine::Result<Reply> {
            Ok(Reply::new("Scripted", self.0))
        }
    }

    fn document(name: &str, text: &str, modified: &str) -> Document {
        knowlith_extract::extract_bytes(std::path::Path::new(name), text.as_bytes(), modified).unwrap()
    }

    const UVJETI: &str = "# Uvjeti prodaje\n\nPopust od 5% odobrava se stalnim kupcima.\n\nRok plaćanja je 15 dana od izdavanja računa.\n\nPonuda vrijedi 14 dana od datuma izdavanja.\n";

    #[test]
    fn a_spreadsheet_row_does_not_become_something_to_approve() {
        let sheet = document(
            "/Cjenik.csv",
            "Sifra,Stavka,Cijena\nKL-002,Daikin FTXM35R,892\n",
            "2026-01-01T00:00:00Z",
        );
        let engine = Scripted(
            r#"{"candidates":[{"kind":"fact","title":"Cijena Daikin FTXM35R","statement":"892 EUR.","quotes":["KL-002 | Daikin FTXM35R | 892"]}]}"#,
        );

        let out = compile(&engine, &[sheet]).unwrap();
        assert!(out.objects.is_empty(), "a price is looked up, not approved");
        assert!(out.dropped[0].reason.contains("looking the row up"));
    }

    #[test]
    fn a_sentence_beside_a_table_is_still_a_decision() {
        let sheet = document(
            "/Cjenik.md",
            "# Cjenik 2026\n\nSve cijene u ovom cjeniku iskazane su bez PDV-a.\n",
            "2026-01-01T00:00:00Z",
        );
        let engine = Scripted(
            r#"{"candidates":[{"kind":"fact","title":"Cijene bez PDV-a","statement":"Cijene su bez PDV-a.","quotes":["Sve cijene u ovom cjeniku iskazane su bez PDV-a."]}]}"#,
        );
        assert_eq!(compile(&engine, &[sheet]).unwrap().objects.len(), 1);
    }

    #[test]
    fn a_held_back_document_says_why() {
        let payroll = document(
            "/Zaposlenici-place-2026.md",
            "# Place\n\nAna Kovac, voditelj prodaje, bruto 2450 EUR mjesecno.\n",
            "2026-01-01T00:00:00Z",
        );
        let out = compile(&Scripted(r#"{"candidates":[]}"#), &[payroll]).unwrap();
        assert_eq!(out.documents_read, 0, "it must never reach an engine");
        assert_eq!(out.dropped.len(), 1);
        assert!(out.dropped[0].reason.contains("personal"));
    }

    #[test]
    fn a_quoted_claim_becomes_an_object_with_a_real_span() {
        let doc = document("/u.md", UVJETI, "2026-01-01T00:00:00Z");
        let engine = Scripted(
            r#"{"candidates":[{"kind":"rule","title":"Odobravanje popusta","statement":"Popust je 5%.","quotes":["Popust od 5% odobrava se stalnim kupcima."]}]}"#,
        );

        let out = compile(&engine, &[doc.clone()]).unwrap();
        assert_eq!(out.objects.len(), 1);
        let object = &out.objects[0];
        assert_eq!(object.kind, ObjectKind::Rule);
        assert_eq!(object.status, ObjectStatus::Proposed);
        assert_eq!(object.evidence.len(), 1);
        assert_eq!(
            &doc.text[object.evidence[0].start_byte..object.evidence[0].end_byte],
            "Popust od 5% odobrava se stalnim kupcima."
        );
    }

    #[test]
    fn an_invented_quote_never_reaches_the_review_queue() {
        let doc = document("/u.md", UVJETI, "2026-01-01T00:00:00Z");
        let engine = Scripted(
            r#"{"candidates":[{"kind":"rule","title":"Popust","statement":"Popust je 8%.","quotes":["Popust od 8% odobrava se stalnim kupcima."]}]}"#,
        );

        let out = compile(&engine, &[doc]).unwrap();
        assert!(out.objects.is_empty(), "a fluent invention is still an invention");
        assert_eq!(out.dropped.len(), 1);
        assert!(out.dropped[0].reason.contains("not in"));
    }

    #[test]
    fn the_newer_document_wins_and_the_disagreement_is_recorded() {
        let old = document(
            "/uvjeti-2023.md",
            "# Uvjeti prodaje 2023\n\nPopust od 8% odobrava se stalnim kupcima.\n\nRok placanja je 30 dana.\n",
            "2025-01-01T00:00:00Z",
        );
        let new = document("/novo.md", UVJETI, "2026-01-01T00:00:00Z");

        // One reply for both documents: each names the sentence in its own.
        struct PerDocument;
        impl Engine for PerDocument {
            fn name(&self) -> &str {
                "PerDocument"
            }
            fn run(&self, request: &Request) -> knowlith_engine::Result<Reply> {
                let quote = if request.input.contains("8%") {
                    "Popust od 8% odobrava se stalnim kupcima."
                } else {
                    "Popust od 5% odobrava se stalnim kupcima."
                };
                Ok(Reply::new(
                    "PerDocument",
                    format!(
                        r#"{{"candidates":[{{"kind":"rule","title":"Odobravanje popusta","statement":"{quote}","quotes":["{quote}"]}}]}}"#
                    ),
                ))
            }
        }

        let out = compile(&PerDocument, &[old, new]).unwrap();
        assert_eq!(out.conflicts.len(), 1, "two documents disagree about one subject");
        assert_eq!(out.objects.len(), 1, "one canonical object, not two");
        assert!(out.objects[0].body.contains("5%"), "the newer document is the current one");
        assert_eq!(out.objects[0].status, ObjectStatus::Conflicted);
        assert!(out.objects[0].supersedes.is_some());
        let docs: std::collections::HashSet<_> = out.objects[0]
            .evidence
            .iter()
            .map(|e| e.document_id.as_str())
            .collect();
        assert_eq!(docs.len(), 2, "both documents' quotes must be on the object");
    }

    #[test]
    fn a_verbatim_quote_from_one_document_reads_as_stated() {
        let single = Group {
            agreeing_documents: 1,
            conflicted: false,
            ..Group::for_test()
        };
        assert_eq!(
            confidence_of(&single).phrase(),
            "Stated, in one place",
            "a sentence copied out of a real document is not merely implied"
        );
    }

    #[test]
    fn three_documents_saying_the_same_thing_read_as_clearly_stated() {
        let agreed = Group {
            agreeing_documents: 3,
            conflicted: false,
            ..Group::for_test()
        };
        assert_eq!(confidence_of(&agreed).phrase(), "Clearly stated");
    }

    #[test]
    fn agreement_across_documents_raises_confidence_and_a_conflict_lowers_it() {
        let single = Group {
            agreeing_documents: 1,
            conflicted: false,
            ..Group::for_test()
        };
        let agreed = Group {
            agreeing_documents: 3,
            conflicted: false,
            ..Group::for_test()
        };
        let disputed = Group {
            agreeing_documents: 2,
            conflicted: true,
            ..Group::for_test()
        };
        assert!(confidence_of(&agreed).0 > confidence_of(&single).0);
        assert!(confidence_of(&disputed).0 < confidence_of(&single).0);
    }

    #[test]
    fn canonical_paths_are_derived_from_the_id() {
        assert_eq!(path_for(ObjectKind::Rule, "rule:sales.discount"), "rules/sales/discount.md");
        assert_eq!(path_for(ObjectKind::Term, "term:bez-pdv"), "terms/bez-pdv.md");
    }
}
