//! What the storage layer guarantees, tested against a real SQLite file.
//!
//! These are the claims the product makes out loud — that a quote is checked
//! rather than trusted, that approving updates what depends on it, that older
//! versions survive — so they are tested at the boundary rather than through
//! the internals that happen to implement them today.

use knowlith_core::object::{Relation, RelationOrigin, RelationType, Subtype};
use knowlith_core::{Confidence, ContextObject, Evidence, ObjectKind, ObjectStatus};
use knowlith_extract::extract_bytes;
use knowlith_graph::Graph;
use knowlith_lake::{Lake, LakeError, NewJob, PRIORITY_NORMAL};
use std::path::Path;

const UVJETI: &str = "\
# Uvjeti prodaje

Popust od 5% odobrava se stalnim kupcima.

Rok plaćanja je 15 dana od izdavanja računa.
";

fn lake_with_document() -> (Lake, knowlith_core::Document) {
    let mut lake = Lake::in_memory().unwrap();
    lake.put_source("src-1", "Prodaja", "/tmp/prodaja", "folder", "codex")
        .unwrap();
    let doc = extract_bytes(
        Path::new("/tmp/prodaja/uvjeti.md"),
        UVJETI.as_bytes(),
        "2026-01-15T09:00:00Z",
    )
    .unwrap();
    lake.put_document("src-1", &doc).unwrap();
    (lake, doc)
}

fn object(id: &str, kind: ObjectKind, evidence: Vec<Evidence>, relations: Vec<Relation>) -> ContextObject {
    ContextObject {
        id: id.into(),
        kind,
        subtype: Some(Subtype::Policy),
        title: id.into(),
        body: "Popust od 5% odobrava se stalnim kupcima.".into(),
        status: ObjectStatus::Proposed,
        confidence: Confidence(0.9),
        version: 1,
        valid_from: "2026-01-15T00:00:00Z".into(),
        valid_to: None,
        supersedes: None,
        decided_by: None,
        edited_on_approval: false,
        evidence,
        relations,
        path: format!("{id}.md"),
        updated_at: "2026-01-15T09:00:00Z".into(),
    }
}

fn span(doc: &knowlith_core::Document, quote: &str) -> Evidence {
    let (start, end) = knowlith_core::locate(doc, quote).expect("the fixture must contain this quote once");
    let locator = doc
        .block_at(start)
        .map(|b| b.locator.clone())
        .unwrap_or_default();
    Evidence {
        document_id: doc.id.clone(),
        locator,
        start_byte: start,
        end_byte: end,
        quote: quote.into(),
    }
}

#[test]
fn a_document_survives_a_round_trip_intact() {
    let (lake, doc) = lake_with_document();
    let back = lake.document(&doc.id).unwrap();
    assert_eq!(back.text, doc.text);
    assert_eq!(back.blocks, doc.blocks);
    assert!(back.verbatim, "markdown is stored as the file, byte for byte");
}

#[test]
fn a_quote_the_document_does_not_contain_cannot_be_stored() {
    let (mut lake, doc) = lake_with_document();

    // A plausible sentence, cited at a span that holds something else. This
    // is the failure the whole product is built to make impossible.
    let forged = Evidence {
        document_id: doc.id.clone(),
        locator: "§1 ¶1".into(),
        start_byte: 18,
        end_byte: 58,
        quote: "Popust od 8% odobrava se stalnim kupcima.".into(),
    };

    let err = lake
        .put_object(&object("rule:discount", ObjectKind::Rule, vec![forged], vec![]))
        .unwrap_err();
    assert!(matches!(err, LakeError::EvidenceRefused { .. }));

    assert!(
        lake.object_ids(None).unwrap().is_empty(),
        "a refused span must take the whole object with it"
    );
}

#[test]
fn a_real_quote_is_stored_and_reads_back() {
    let (mut lake, doc) = lake_with_document();
    let good = span(&doc, "Popust od 5% odobrava se stalnim kupcima.");
    lake.put_object(&object("rule:discount", ObjectKind::Rule, vec![good.clone()], vec![]))
        .unwrap();

    let stored = lake.evidence_of("rule:discount").unwrap();
    assert_eq!(stored, vec![good]);
}

#[test]
fn nothing_is_approved_without_a_source_span() {
    let (mut lake, _doc) = lake_with_document();
    lake.put_object(&object("rule:bare", ObjectKind::Rule, vec![], vec![]))
        .unwrap();

    let err = lake.approve("rule:bare", "whatever", "Ana", false).unwrap_err();
    assert!(matches!(err, LakeError::NoEvidence));
}

#[test]
fn approving_marks_everything_downstream_as_needing_attention() {
    let (mut lake, doc) = lake_with_document();
    let quote = span(&doc, "Popust od 5% odobrava se stalnim kupcima.");

    lake.put_object(&object(
        "rule:discount",
        ObjectKind::Rule,
        vec![quote.clone()],
        vec![Relation {
            target_id: "process:quote".into(),
            target_label: "Izrada ponude".into(),
            kind: RelationType::UsedBy,
            origin: RelationOrigin::Structural,
        }],
    ))
    .unwrap();

    lake.put_object(&object(
        "process:quote",
        ObjectKind::Process,
        vec![quote.clone()],
        vec![Relation {
            target_id: "skill:offer".into(),
            target_label: "Izradi ponudu".into(),
            kind: RelationType::UsedBy,
            origin: RelationOrigin::Structural,
        }],
    ))
    .unwrap();

    lake.put_object(&object("skill:offer", ObjectKind::Process, vec![quote], vec![]))
        .unwrap();

    let affected = lake
        .approve("rule:discount", "Popust od 10% odobrava se stalnim kupcima.", "Ana Kovač", true)
        .unwrap();

    assert_eq!(
        affected,
        ["process:quote", "skill:offer"],
        "propagation follows the chain, not just the first hop"
    );
}

#[test]
fn the_previous_version_is_kept() {
    let (mut lake, doc) = lake_with_document();
    let quote = span(&doc, "Rok plaćanja je 15 dana od izdavanja računa.");
    lake.put_object(&object("rule:terms", ObjectKind::Rule, vec![quote], vec![]))
        .unwrap();

    lake.approve("rule:terms", "Rok plaćanja je 30 dana.", "Ana", false)
        .unwrap();

    let conn = lake.connection();
    let (version, body): (i64, String) = conn
        .query_row(
            "SELECT version, body FROM object_versions WHERE object_id = 'rule:terms'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(version, 1);
    assert!(body.contains("5%"), "the superseded text stays readable");

    let current: i64 = conn
        .query_row("SELECT version FROM objects WHERE id = 'rule:terms'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(current, 2);
}

#[test]
fn the_graph_in_memory_agrees_with_the_graph_on_disk() {
    let (mut lake, doc) = lake_with_document();
    let quote = span(&doc, "Popust od 5% odobrava se stalnim kupcima.");

    lake.put_object(&object(
        "fact:price",
        ObjectKind::Fact,
        vec![quote.clone()],
        vec![Relation {
            target_id: "rule:discount".into(),
            target_label: "Popust".into(),
            kind: RelationType::UsedBy,
            origin: RelationOrigin::Structural,
        }],
    ))
    .unwrap();
    lake.put_object(&object(
        "rule:discount",
        ObjectKind::Rule,
        vec![quote],
        vec![Relation {
            target_id: "process:quote".into(),
            target_label: "Ponuda".into(),
            kind: RelationType::UsedBy,
            origin: RelationOrigin::Structural,
        }],
    ))
    .unwrap();

    let graph = Graph::build(&lake.edges().unwrap());
    let impact = graph.impact("fact:price");
    let hits: Vec<&str> = impact.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(hits, ["rule:discount", "process:quote"]);
}

#[test]
fn an_edited_source_file_invalidates_the_check_it_already_passed() {
    let (mut lake, doc) = lake_with_document();
    let quote = span(&doc, "Rok plaćanja je 15 dana od izdavanja računa.");
    lake.put_object(&object("rule:terms", ObjectKind::Rule, vec![quote], vec![]))
        .unwrap();
    assert!(lake.recheck_evidence().unwrap().is_empty());

    // Somebody edits the document. The stored span now covers other bytes.
    let edited = UVJETI.replace("Popust od 5%", "Popust");
    let changed = extract_bytes(
        Path::new("/tmp/prodaja/uvjeti.md"),
        edited.as_bytes(),
        "2026-02-01T09:00:00Z",
    )
    .unwrap();
    lake.connection()
        .execute(
            "UPDATE documents SET text = ?2, text_sha256 = ?3 WHERE id = ?1",
            rusqlite::params![doc.id, changed.text, changed.text_sha256],
        )
        .unwrap();

    let broken = lake.recheck_evidence().unwrap();
    assert_eq!(broken.len(), 1, "a moved span must stop counting as verified");
    assert_eq!(broken[0].0, "rule:terms");
}

#[test]
fn search_ignores_diacritics() {
    let (lake, _doc) = lake_with_document();
    let hits = lake.search("placanja", 5).unwrap();
    assert!(
        hits.iter().any(|(_, _, text)| text.contains("plaćanja")),
        "an owner typing without diacritics still finds their own document"
    );
}

#[test]
fn reading_the_same_file_twice_stores_it_once() {
    let (mut lake, doc) = lake_with_document();
    let again = extract_bytes(
        Path::new("/somewhere/else/kopija.md"),
        UVJETI.as_bytes(),
        "2026-03-01T09:00:00Z",
    )
    .unwrap();
    assert_eq!(again.id, doc.id);
    lake.put_document("src-1", &again).unwrap();
    assert_eq!(lake.document_count().unwrap(), 1);
}

#[test]
fn only_recorded_reads_are_reported() {
    let (lake, _doc) = lake_with_document();
    assert!(lake.reads_of("rule:discount").unwrap().is_empty());

    lake.record_read("rule:discount", "Claude Desktop").unwrap();
    lake.record_read("rule:discount", "Codex CLI").unwrap();
    lake.record_read("rule:discount", "Claude Desktop").unwrap();

    let reads = lake.reads_of("rule:discount").unwrap();
    assert_eq!(reads.len(), 2, "a tool that read twice is still one tool");
}

// ---------------------------------------------------------------- merging --

/// A merge is the one operation that makes a rule disappear from view, so
/// these test the four things that have to move together and the two that
/// must not happen at all.

#[test]
fn a_merge_moves_the_evidence_and_leaves_the_dropped_version_readable() {
    let (mut lake, doc) = lake_with_document();

    let keep = object(
        "rule:popust-stalni",
        ObjectKind::Rule,
        vec![span(&doc, "Popust od 5% odobrava se stalnim kupcima.")],
        vec![],
    );
    let drop = object(
        "rule:redovni-popust",
        ObjectKind::Rule,
        vec![span(&doc, "Rok plaćanja je 15 dana od izdavanja računa.")],
        vec![],
    );
    lake.put_object(&keep).unwrap();
    lake.put_object(&drop).unwrap();

    lake.merge_objects("rule:popust-stalni", "rule:redovni-popust").unwrap();

    let objects = lake.objects().unwrap();
    let kept = objects.iter().find(|o| o.id == "rule:popust-stalni").unwrap();
    assert_eq!(kept.evidence.len(), 2, "the dropped wording's quote survives on the kept object");
    assert_eq!(kept.supersedes.as_deref(), Some("rule:redovni-popust"));

    // Not deleted. "What did we have on file in March" keeps having an answer,
    // and a merge the owner regrets has something to go back to.
    let dropped = objects.iter().find(|o| o.id == "rule:redovni-popust").unwrap();
    assert_eq!(dropped.status, ObjectStatus::Superseded);
    assert!(dropped.valid_to.is_some());
}

#[test]
fn a_merge_rewires_what_pointed_at_the_dropped_object() {
    let (mut lake, doc) = lake_with_document();

    lake.put_object(&object(
        "rule:popust-stalni",
        ObjectKind::Rule,
        vec![span(&doc, "Popust od 5% odobrava se stalnim kupcima.")],
        vec![],
    ))
    .unwrap();
    lake.put_object(&object(
        "rule:redovni-popust",
        ObjectKind::Rule,
        vec![span(&doc, "Rok plaćanja je 15 dana od izdavanja računa.")],
        vec![],
    ))
    .unwrap();
    lake.put_object(&object(
        "process:izrada-ponude",
        ObjectKind::Process,
        vec![span(&doc, "Popust od 5% odobrava se stalnim kupcima.")],
        vec![Relation {
            target_id: "rule:redovni-popust".into(),
            target_label: "Redovni popust".into(),
            kind: RelationType::DependsOn,
            origin: RelationOrigin::Structural,
        }],
    ))
    .unwrap();

    lake.merge_objects("rule:popust-stalni", "rule:redovni-popust").unwrap();

    let edges = lake.edges().unwrap();
    assert!(
        edges.iter().any(|e| e.from_id == "process:izrada-ponude" && e.to_id == "rule:popust-stalni"),
        "the process must now depend on the rule that survived: {edges:?}"
    );
    assert!(
        !edges.iter().any(|e| e.to_id == "rule:redovni-popust"),
        "nothing may still point at the merged-away object: {edges:?}"
    );
}

#[test]
fn an_answered_hint_is_never_asked_again() {
    let (mut lake, doc) = lake_with_document();
    lake.put_object(&object("rule:a", ObjectKind::Rule, vec![span(&doc, "Popust od 5% odobrava se stalnim kupcima.")], vec![]))
        .unwrap();
    lake.put_object(&object("rule:b", ObjectKind::Rule, vec![span(&doc, "Rok plaćanja je 15 dana od izdavanja računa.")], vec![]))
        .unwrap();

    assert!(lake.put_merge_hint("rule:a", "rule:b", "duplicate", 0.8).unwrap());
    assert_eq!(lake.open_merge_hints().unwrap().len(), 1);

    lake.dismiss_merge_hint("rule:a", "rule:b").unwrap();
    assert!(lake.open_merge_hints().unwrap().is_empty());

    // A later scan finds the same pair, in the other order. The owner has
    // already answered; being asked again after every rescan is how a review
    // queue becomes noise.
    assert!(!lake.put_merge_hint("rule:b", "rule:a", "duplicate", 0.8).unwrap());
    assert!(lake.open_merge_hints().unwrap().is_empty());
}

#[test]
fn merging_something_into_itself_is_refused() {
    let (mut lake, doc) = lake_with_document();
    lake.put_object(&object("rule:a", ObjectKind::Rule, vec![span(&doc, "Popust od 5% odobrava se stalnim kupcima.")], vec![]))
        .unwrap();
    assert!(matches!(
        lake.merge_objects("rule:a", "rule:a"),
        Err(LakeError::UnknownObject(_))
    ));
}

#[test]
fn a_hint_whose_object_was_merged_away_stops_being_asked() {
    let (mut lake, doc) = lake_with_document();
    lake.put_object(&object("rule:a", ObjectKind::Rule, vec![span(&doc, "Popust od 5% odobrava se stalnim kupcima.")], vec![]))
        .unwrap();
    lake.put_object(&object("rule:b", ObjectKind::Rule, vec![span(&doc, "Rok plaćanja je 15 dana od izdavanja računa.")], vec![]))
        .unwrap();
    lake.put_merge_hint("rule:a", "rule:b", "duplicate", 0.8).unwrap();

    lake.merge_objects("rule:a", "rule:b").unwrap();
    assert!(lake.open_merge_hints().unwrap().is_empty());
    let counts = lake.merge_hint_counts().unwrap();
    assert!(counts.iter().any(|(state, n)| state == "merged" && *n == 1));
}

/// Approving with no text was accepted, and the gateway then served a
/// heading with nothing under it — a rule an agent is told exists but not
/// what it says. Found by approving through the API with the wrong field
/// name, which is exactly how it would happen in the wild.
#[test]
fn an_approval_with_no_text_is_refused() {
    let (mut lake, document) = lake_with_document();
    let quote = "Popust od 5% odobrava se stalnim kupcima.";
    lake.put_object(&object(
        "rule:test",
        ObjectKind::Rule,
        vec![span(&document, quote)],
        Vec::new(),
    ))
    .unwrap();

    let empty = lake.approve("rule:test", "   \n\t ", "ana", false);
    assert!(
        matches!(empty, Err(LakeError::NoBody)),
        "an empty body was accepted: {empty:?}"
    );

    // The object is untouched: still proposed, still the owner's to decide.
    let stored = lake.object("rule:test").unwrap().unwrap();
    assert_eq!(stored.status, ObjectStatus::Proposed);

    // And a real body still goes through.
    lake.approve("rule:test", "Popust je 5%.", "ana", false).unwrap();
    let stored = lake.object("rule:test").unwrap().unwrap();
    assert_eq!(stored.status, ObjectStatus::Approved);
    assert_eq!(stored.body, "Popust je 5%.");
}

// ------------------------------------------------------- the work panel --

/// The sentence a worker hands back is the only account an owner gets of
/// work that went right. It used to go to the command line and nowhere
/// else, so the interface showed a queue emptying and nothing else.
#[test]
fn what_a_job_did_is_kept_not_only_printed() {
    let lake = Lake::in_memory().unwrap();
    lake.enqueue(&NewJob {
        kind: "compile_document".into(),
        payload: r#"{"document_id":"doc:abc"}"#.into(),
        idempotency_key: "one".into(),
        priority: PRIORITY_NORMAL,
    })
    .unwrap();

    let job = lake.lease().unwrap().expect("a job");
    lake.finish(job.id, "Cjenik 2026.xlsx: 14 claims · 2 not read").unwrap();

    let work = lake.recent_work(10).unwrap();
    assert_eq!(work.len(), 1);
    assert_eq!(work[0].note.as_deref(), Some("Cjenik 2026.xlsx: 14 claims · 2 not read"));
    assert_eq!(work[0].subject.as_deref(), Some("doc:abc"));
    assert_eq!(work[0].state, "done");
}

/// A document that would not read is the one line on the panel that asks
/// the owner for something, so it must not vanish when the queue drains.
#[test]
fn a_failure_outlives_the_queue_it_was_in() {
    let lake = Lake::in_memory().unwrap();
    lake.enqueue(&NewJob {
        kind: "compile_document".into(),
        payload: r#"{"document_id":"doc:locked"}"#.into(),
        idempotency_key: "locked".into(),
        priority: PRIORITY_NORMAL,
    })
    .unwrap();
    let job = lake.lease().unwrap().expect("a job");
    lake.fail(job.id, "the file is password protected").unwrap();

    // Nothing is outstanding any more.
    assert!(lake.lease().unwrap().is_none());
    assert_eq!(lake.pending_by_kind().unwrap(), vec![]);

    let work = lake.recent_work(10).unwrap();
    assert_eq!(work.len(), 1, "the failure was forgotten with the queue");
    assert_eq!(work[0].state, "failed");
    assert_eq!(work[0].error.as_deref(), Some("the file is password protected"));
}

/// The bar must start at nought when the owner adds a folder, not at
/// whatever last week's nine hundred finished jobs would make it.
#[test]
fn progress_counts_this_burst_and_not_the_whole_history() {
    let lake = Lake::in_memory().unwrap();

    // Yesterday's work, finished and done with.
    lake.enqueue(&NewJob {
        kind: "compile_document".into(),
        payload: "{}".into(),
        idempotency_key: "old".into(),
        priority: PRIORITY_NORMAL,
    })
    .unwrap();
    let old = lake.lease().unwrap().expect("a job");
    lake.finish(old.id, "done long ago").unwrap();

    // With nothing outstanding there is no burst to be part of.
    assert_eq!(lake.finished_this_burst().unwrap(), 0);

    // Now the owner adds a folder: two jobs, one of which finishes.
    for key in ["new-1", "new-2"] {
        lake.enqueue(&NewJob {
            kind: "compile_document".into(),
            payload: "{}".into(),
            idempotency_key: key.into(),
            priority: PRIORITY_NORMAL,
        })
        .unwrap();
    }
    let first = lake.lease().unwrap().expect("a job");
    lake.finish(first.id, "one of two").unwrap();

    assert_eq!(
        lake.finished_this_burst().unwrap(),
        1,
        "yesterday's job was counted into today's progress"
    );
    assert_eq!(
        lake.pending_by_kind().unwrap(),
        vec![("compile_document".to_string(), 1)]
    );
}

/// The stage shown on screen is read off the queue, so it has to be
/// possible to tell the kinds apart.
#[test]
fn outstanding_work_is_reported_per_kind() {
    let lake = Lake::in_memory().unwrap();
    for (kind, key) in [("rescan", "r"), ("compile_document", "c1"), ("compile_document", "c2")] {
        lake.enqueue(&NewJob {
            kind: kind.into(),
            payload: "{}".into(),
            idempotency_key: key.into(),
            priority: PRIORITY_NORMAL,
        })
        .unwrap();
    }

    let mut pending = lake.pending_by_kind().unwrap();
    pending.sort();
    assert_eq!(
        pending,
        vec![
            ("compile_document".to_string(), 2),
            ("rescan".to_string(), 1),
        ]
    );
}

/// A payload whose shape nobody anticipated must cost one missing name,
/// never a failed query — the panel exists to report trouble.
#[test]
fn an_unreadable_payload_does_not_take_the_panel_down() {
    let lake = Lake::in_memory().unwrap();
    lake.enqueue(&NewJob {
        kind: "settle".into(),
        payload: "not json at all".into(),
        idempotency_key: "odd".into(),
        priority: PRIORITY_NORMAL,
    })
    .unwrap();
    let job = lake.lease().unwrap().expect("a job");
    lake.finish(job.id, "51 kept").unwrap();

    let work = lake.recent_work(10).unwrap();
    assert_eq!(work.len(), 1);
    assert_eq!(work[0].subject, None);
    assert_eq!(work[0].note.as_deref(), Some("51 kept"));
}
