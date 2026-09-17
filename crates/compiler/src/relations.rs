//! Proposing the edges a string match cannot find.
//!
//! The compiler's structural edge detection asks whether one object's title
//! appears inside another's statement. It is cheap, it is right when it
//! fires, and on a real twelve-file folder it found **one edge across
//! fifty-one objects**. A graph with one edge answers no question worth
//! asking: "what breaks if I change the discount rule" comes back empty, and
//! empty reads as "nothing", which is a wrong answer rather than a missing
//! one.
//!
//! A model finds these. `Postupak odobravanja popusta` rests on
//! `Prag prometa kupca` and on `Odobrenje popusta većeg od 10%` without
//! quoting either title, and no amount of string comparison recovers that.
//!
//! What a model must not be is the *authority* on them. An edge has no
//! sentence behind it, so the evidence gate that protects every claim in
//! this product has nothing to check an edge against. The resolution is the
//! one the object model already carries: [`RelationOrigin`]. A proposed edge
//! is stored as `Model`, shown as suggested wherever it appears, and can be
//! removed with one click.
//!
//! Edges are treated less strictly than claims, deliberately. A claim
//! asserts something about the company and a wrong one is a lie. An edge
//! asserts "look at this too" — a wrong one over-warns, a missing one
//! under-warns, and for a question whose whole purpose is "what else should
//! I check", over-warning is the safe direction.

use std::collections::{HashMap, HashSet};

use knowlith_core::object::{RelationOrigin, RelationType};
use knowlith_core::{ContextObject, ObjectStatus};
use knowlith_engine::{Engine, Request};
use serde::Deserialize;

use crate::{CompileError, Result};

pub const RELATION_INSTRUCTIONS: &str = "\
You are given a small company's own rules, processes, terms and facts, each \
with an id.

Say which of them depend on which. One object depends on another when \
changing the second would change what the first means or what somebody \
should do about it.

For each dependency give:
- from: the id of the object that depends
- to: the id of the object it depends on
- why: one short sentence, in the language of the material, saying what the \
dependency is

Rules:
- Use only the ids given. Do not invent one.
- A process depends on the rules that constrain its steps, and on the terms \
its steps use.
- A rule depends on a term it relies on, or on a threshold defined elsewhere.
- Do not link two objects merely because they are about the same area of the \
business. There has to be something that would actually change.
- Do not link an object to itself.
- If two objects say the same thing, that is not a dependency. Leave it out.
- Returning few dependencies is better than returning weak ones.";

pub const RELATION_SCHEMA: &str = r#"{
  "type": "object",
  "additionalProperties": false,
  "required": ["dependencies"],
  "properties": {
    "dependencies": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "required": ["from", "to", "why"],
        "properties": {
          "from": { "type": "string" },
          "to": { "type": "string" },
          "why": { "type": "string" }
        }
      }
    }
  }
}"#;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ProposedEdge {
    pub from: String,
    pub to: String,
    pub why: String,
}

#[derive(Debug, Deserialize)]
struct Envelope {
    dependencies: Vec<ProposedEdge>,
}

#[derive(Debug, Default)]
pub struct RelationRun {
    pub edges: Vec<ProposedEdge>,
    pub dropped: Vec<crate::Dropped>,
    pub objects_considered: usize,
}

/// Asks the engine, once, about the whole set.
///
/// One call rather than one per object: dependency is a property of the set,
/// and an engine shown a single rule can only guess at what else exists. It
/// also keeps this stage's cost flat as the lake grows in the only dimension
/// that matters to an owner — number of model calls, not tokens.
pub fn propose(engine: &dyn Engine, objects: &[ContextObject]) -> Result<RelationRun> {
    let live: Vec<&ContextObject> = objects
        .iter()
        .filter(|o| matches!(o.status, ObjectStatus::Proposed | ObjectStatus::Approved | ObjectStatus::Conflicted))
        .collect();

    let mut out = RelationRun {
        objects_considered: live.len(),
        ..RelationRun::default()
    };
    if live.len() < 2 {
        return Ok(out);
    }

    let request = Request::new("relations", RELATION_INSTRUCTIONS, catalogue(&live))
        .with_schema(RELATION_SCHEMA);
    let reply = engine.run(&request)?;
    let proposed = parse(&reply.text)?;

    out.edges = keep_usable(proposed, &live, &mut out.dropped);
    Ok(out)
}

/// What the engine is shown: id, title, statement. Nothing else.
///
/// No confidence, no evidence, no document name. A model told that one rule
/// is better evidenced than another will use that to decide which depends on
/// which, and evidence quality has nothing to do with dependency.
pub fn catalogue(objects: &[&ContextObject]) -> String {
    let mut out = String::with_capacity(objects.len() * 160);
    for object in objects {
        out.push_str(&object.id);
        out.push_str("\n  ");
        out.push_str(&object.title);
        out.push_str("\n  ");
        out.push_str(object.body.lines().next().unwrap_or(""));
        out.push_str("\n\n");
    }
    out
}

pub fn parse(reply: &str) -> Result<Vec<ProposedEdge>> {
    let json = crate::candidates::extract_object(reply)
        .ok_or_else(|| CompileError::BadReply(crate::candidates::first_line(reply)))?;
    let envelope: Envelope = serde_json::from_str(json)
        .map_err(|e| CompileError::BadReply(format!("{e} in: {}", crate::candidates::first_line(json))))?;
    Ok(envelope.dependencies)
}

/// The gate. Not an evidence check — there is nothing to check an edge
/// against — but every refusal here is mechanical.
fn keep_usable(
    proposed: Vec<ProposedEdge>,
    objects: &[&ContextObject],
    dropped: &mut Vec<crate::Dropped>,
) -> Vec<ProposedEdge> {
    let known: HashMap<&str, &str> = objects.iter().map(|o| (o.id.as_str(), o.title.as_str())).collect();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut kept: Vec<ProposedEdge> = Vec::new();

    for edge in proposed {
        if edge.from == edge.to {
            dropped.push(crate::Dropped {
                document: edge.from.clone(),
                title: String::new(),
                reason: "an object cannot depend on itself".into(),
            });
            continue;
        }
        // An id the model produced rather than copied. This happens, and a
        // dangling edge shows up in the interface as a dependency on
        // something the owner cannot open.
        for id in [&edge.from, &edge.to] {
            if !known.contains_key(id.as_str()) {
                dropped.push(crate::Dropped {
                    document: id.clone(),
                    title: String::new(),
                    reason: format!("there is no object with the id \"{id}\""),
                });
            }
        }
        if !known.contains_key(edge.from.as_str()) || !known.contains_key(edge.to.as_str()) {
            continue;
        }
        if !seen.insert((edge.from.clone(), edge.to.clone())) {
            continue;
        }
        // A cycle makes "what does this rest on" non-terminating and turns
        // the propagation order into nonsense. Dropping the edge that closes
        // the loop keeps everything before it.
        if creates_cycle(&kept, &edge) {
            dropped.push(crate::Dropped {
                document: edge.from.clone(),
                title: known.get(edge.from.as_str()).unwrap_or(&"").to_string(),
                reason: format!("would make {} and {} depend on each other", edge.from, edge.to),
            });
            continue;
        }
        kept.push(edge);
    }

    kept.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.to.cmp(&b.to)));
    kept
}

/// Whether adding `edge` closes a loop, by walking what is already kept.
fn creates_cycle(kept: &[ProposedEdge], edge: &ProposedEdge) -> bool {
    let mut frontier = vec![edge.to.as_str()];
    let mut seen: HashSet<&str> = HashSet::new();
    while let Some(current) = frontier.pop() {
        if current == edge.from {
            return true;
        }
        if !seen.insert(current) {
            continue;
        }
        for next in kept.iter().filter(|e| e.from == current) {
            frontier.push(next.to.as_str());
        }
    }
    false
}

/// The pair of edges one dependency produces.
///
/// `UsedBy` is stored explicitly rather than derived, because the impact
/// query — "what breaks if I change this" — is the one the owner runs, and
/// it must not depend on scanning every row in the other direction.
/// One stored edge, ready for the lake.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredEdge {
    pub from: String,
    pub to: String,
    pub kind: RelationType,
    pub origin: RelationOrigin,
    pub why: Option<String>,
    pub confidence: Option<f32>,
}

pub fn edge_pair(edge: &ProposedEdge) -> [StoredEdge; 2] {
    [
        StoredEdge {
            from: edge.from.clone(),
            to: edge.to.clone(),
            kind: RelationType::DependsOn,
            origin: RelationOrigin::Model,
            why: Some(edge.why.clone()),
            confidence: Some(0.75),
        },
        StoredEdge {
            from: edge.to.clone(),
            to: edge.from.clone(),
            kind: RelationType::UsedBy,
            origin: RelationOrigin::Model,
            why: None,
            confidence: Some(0.75),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use knowlith_core::{Confidence, Evidence, ObjectKind};
    use knowlith_engine::{Reply, Request};

    struct Scripted(&'static str);

    impl Engine for Scripted {
        fn name(&self) -> &str {
            "Scripted"
        }
        fn run(&self, _request: &Request) -> knowlith_engine::Result<Reply> {
            Ok(Reply {
                text: self.0.to_string(),
                engine: "Scripted".into(),
            })
        }
    }

    fn object(id: &str, title: &str, body: &str) -> ContextObject {
        ContextObject {
            id: id.into(),
            kind: ObjectKind::Rule,
            subtype: None,
            title: title.into(),
            body: body.into(),
            status: ObjectStatus::Proposed,
            confidence: Confidence(0.8),
            version: 1,
            valid_from: "2026-01-01T00:00:00Z".into(),
            valid_to: None,
            supersedes: None,
            decided_by: None,
            edited_on_approval: false,
            evidence: vec![Evidence {
                document_id: "doc:a".into(),
                locator: "§1".into(),
                start_byte: 0,
                end_byte: 1,
                quote: "x".into(),
            }],
            relations: Vec::new(),
            path: "x.md".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    fn three() -> Vec<ContextObject> {
        vec![
            object("process:popust", "Postupak odobravanja popusta", "Prodavač provjerava promet kupca."),
            object("rule:prag", "Prag prometa kupca", "Stalni kupac je onaj s prometom iznad 15.000 EUR."),
            object("rule:odobrenje", "Odobrenje popusta većeg od 10%", "Popust veći od 10% odobrava direktor."),
        ]
    }

    #[test]
    fn a_dependency_a_string_match_cannot_see_is_found() {
        let reply = r#"{"dependencies":[
          {"from":"process:popust","to":"rule:prag","why":"Postupak provjerava prag prometa."},
          {"from":"process:popust","to":"rule:odobrenje","why":"Veći popust traži odobrenje."}]}"#;
        let run = propose(&Scripted(reply), &three()).unwrap();
        assert_eq!(run.edges.len(), 2);
        assert!(run.edges.iter().all(|e| e.from == "process:popust"));
    }

    #[test]
    fn an_id_the_model_made_up_never_reaches_the_graph() {
        let reply = r#"{"dependencies":[
          {"from":"process:popust","to":"rule:nepostojece","why":"Izmišljeno."}]}"#;
        let run = propose(&Scripted(reply), &three()).unwrap();
        assert!(run.edges.is_empty());
        assert!(run.dropped[0].reason.contains("no object with the id"));
    }

    #[test]
    fn an_object_cannot_be_made_to_depend_on_itself() {
        let reply = r#"{"dependencies":[
          {"from":"rule:prag","to":"rule:prag","why":"Samo na sebe."}]}"#;
        let run = propose(&Scripted(reply), &three()).unwrap();
        assert!(run.edges.is_empty());
        assert!(run.dropped[0].reason.contains("itself"));
    }

    #[test]
    fn a_cycle_is_broken_at_the_edge_that_closes_it() {
        let reply = r#"{"dependencies":[
          {"from":"process:popust","to":"rule:prag","why":"a"},
          {"from":"rule:prag","to":"rule:odobrenje","why":"b"},
          {"from":"rule:odobrenje","to":"process:popust","why":"c"}]}"#;
        let run = propose(&Scripted(reply), &three()).unwrap();
        assert_eq!(run.edges.len(), 2, "the first two survive, the loop-closer does not");
        assert!(run.dropped[0].reason.contains("depend on each other"));
    }

    #[test]
    fn the_same_edge_twice_is_one_edge() {
        let reply = r#"{"dependencies":[
          {"from":"process:popust","to":"rule:prag","why":"a"},
          {"from":"process:popust","to":"rule:prag","why":"again"}]}"#;
        assert_eq!(propose(&Scripted(reply), &three()).unwrap().edges.len(), 1);
    }

    #[test]
    fn every_proposed_edge_is_marked_as_the_models_and_not_as_structure() {
        let edge = ProposedEdge {
            from: "a".into(),
            to: "b".into(),
            why: "w".into(),
        };
        assert!(edge_pair(&edge).iter().all(|stored| stored.origin == RelationOrigin::Model));
    }

    #[test]
    fn the_engine_is_shown_ids_and_words_but_never_confidence_or_evidence() {
        let objects = three();
        let refs: Vec<&ContextObject> = objects.iter().collect();
        let text = catalogue(&refs);
        assert!(text.contains("process:popust"));
        assert!(!text.contains("0.8"));
        assert!(!text.contains("doc:a"));
    }

    #[test]
    fn the_prompt_refuses_topical_links_and_the_schema_demands_a_reason() {
        assert!(RELATION_INSTRUCTIONS.contains("merely because they are about the same area"));
        assert!(RELATION_SCHEMA.contains("\"required\": [\"from\", \"to\", \"why\"]"));
    }
}
