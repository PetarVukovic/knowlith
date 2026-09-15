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
use knowlith_lake::{Lake, LakeError};
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
