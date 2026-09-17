//! Writing a skill from knowledge the owner has already approved.
//!
//! A skill is the one object an agent *executes* rather than reads, which
//! makes it the one place a hallucinated number does real damage: a rule that
//! is wrong gets argued with, a procedure that is wrong gets followed.
//!
//! So this stage is built the opposite way round from stage 2. Stage 2 reads
//! a document and is checked against that document. This stage never sees a
//! document at all. Its input is a set of objects the owner has **already
//! approved**, and its job is to arrange them into something an agent can
//! run. It may rephrase, order and structure; it may not introduce a fact.
//!
//! That restriction is enforced mechanically rather than asked for politely:
//!
//! * **Every figure in the draft must already exist in the approved
//!   sources.** A draft that invents "within 5 working days" where no
//!   approved object says five is refused whole. Numbers are where a made-up
//!   procedure does its damage, and numbers are exactly what a machine can
//!   check.
//! * **The evidence is inherited, never created.** A skill's spans are the
//!   spans of the objects it rests on, so it passes the lake's evidence gate
//!   for the same reason they did, and clicking a quote still lands in a real
//!   file.
//! * **Confidence is the weakest link.** A procedure is no more certain than
//!   the least certain rule inside it, so the minimum is taken rather than
//!   the average — an average lets four solid rules carry one shaky one.

use std::collections::{BTreeSet, HashMap};

use knowlith_core::object::{Relation, RelationOrigin, RelationType};
use knowlith_core::{Confidence, ContextObject, Evidence, ObjectKind, ObjectStatus};
use knowlith_engine::{Engine, Request};
use serde::Deserialize;

use crate::consolidate::figures;
use crate::{CompileError, Result};

pub const SKILL_INSTRUCTIONS: &str = "\
You are turning a small company's own approved knowledge into one skill: a \
procedure another AI assistant can follow on their behalf.

You are given a process the company has approved, and the rules, terms and \
facts it rests on. Write a skill that carries that process out.

- name: a short imperative name, in the language of the source material.
- description: one sentence saying when this skill should be used.
- inputs: what the assistant must be told before it can start. Name, type \
and a short description each.
- outputs: what the assistant produces.
- steps: the ordered steps. Each step is one instruction, in plain language, \
in the source material's language.

Rules, and they are absolute:
- Use ONLY what the supplied material states. You are arranging knowledge \
that already exists, not adding any.
- Never introduce a number, a deadline, a percentage, a price or an amount \
that is not already in the supplied material. A draft containing one will be \
thrown away whole.
- Where the material gives a limit, say the limit and say which rule it comes \
from.
- Where the material does not settle something the procedure needs, add a \
step that says to ask the owner. Do not decide it yourself.
- Do not mention documents, files or where the knowledge was stored.";

pub const SKILL_SCHEMA: &str = r#"{
  "type": "object",
  "additionalProperties": false,
  "required": ["name", "description", "inputs", "outputs", "steps"],
  "properties": {
    "name": { "type": "string" },
    "description": { "type": "string" },
    "inputs": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "required": ["name", "type", "description"],
        "properties": {
          "name": { "type": "string" },
          "type": { "type": "string" },
          "description": { "type": "string" }
        }
      }
    },
    "outputs": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "required": ["name", "type", "description"],
        "properties": {
          "name": { "type": "string" },
          "type": { "type": "string" },
          "description": { "type": "string" }
        }
      }
    },
    "steps": { "type": "array", "items": { "type": "string" }, "minItems": 2 }
  }
}"#;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Draft {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub inputs: Vec<Field>,
    #[serde(default)]
    pub outputs: Vec<Field>,
    pub steps: Vec<String>,
}

/// What a skill was built from, and what it cost.
#[derive(Debug, Default)]
pub struct SkillRun {
    pub skills: Vec<ContextObject>,
    pub dropped: Vec<crate::Dropped>,
    pub processes_considered: usize,
}

/// Drafts one skill for every approved process that has something to rest on.
///
/// Only approved objects are eligible on either side. A skill built on a
/// draft rule would be a procedure the owner never signed off, wearing the
/// authority of one they did.
pub fn draft_all(engine: &dyn Engine, objects: &[ContextObject]) -> Result<SkillRun> {
    let approved: Vec<&ContextObject> = objects
        .iter()
        .filter(|o| o.status == ObjectStatus::Approved)
        .collect();
    let by_id: HashMap<&str, &ContextObject> = approved.iter().map(|o| (o.id.as_str(), *o)).collect();

    let mut out = SkillRun::default();

    for process in approved.iter().filter(|o| o.kind == ObjectKind::Process) {
        out.processes_considered += 1;

        let skill_id = skill_id(&process.id);
        // An already-approved skill is the owner's procedure. Re-drafting
        // would flip it back to Proposed and look like a duplicate draft
        // next to the process it came from.
        if objects.iter().any(|o| {
            o.id == skill_id
                && o.kind == ObjectKind::Skill
                && o.status == ObjectStatus::Approved
        }) {
            continue;
        }

        let sources = foundations(process, &by_id);

        match draft_one(engine, process, &sources) {
            Ok(object) => out.skills.push(object),
            Err(CompileError::Engine(e)) if !e.is_retryable() => out.dropped.push(crate::Dropped {
                document: skill_id,
                title: process.title.clone(),
                reason: format!("{e}"),
            }),
            Err(CompileError::Refused(reason)) => out.dropped.push(crate::Dropped {
                document: skill_id,
                title: process.title.clone(),
                reason,
            }),
            // A transport failure belongs to the queue, not to this process.
            Err(e) => return Err(e),
        }
    }

    out.skills.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

/// The approved objects a process rests on: what it names, plus itself.
fn foundations<'a>(
    process: &'a ContextObject,
    by_id: &HashMap<&str, &'a ContextObject>,
) -> Vec<&'a ContextObject> {
    let mut out = vec![process];
    for relation in &process.relations {
        if relation.kind != RelationType::DependsOn {
            continue;
        }
        if let Some(target) = by_id.get(relation.target_id.as_str()) {
            out.push(target);
        }
    }
    out
}

/// One skill, drafted and then checked.
pub fn draft_one(
    engine: &dyn Engine,
    process: &ContextObject,
    sources: &[&ContextObject],
) -> Result<ContextObject> {
    let request = Request::new("skill", SKILL_INSTRUCTIONS, brief(process, sources))
        .with_schema(SKILL_SCHEMA);
    let reply = engine.run(&request)?;
    let draft = parse(&reply.text)?;
    build(process, sources, &draft)
}

/// What the engine is shown: the process, then everything it rests on.
///
/// Titles and bodies only. The engine never sees a file path, a document
/// name or a byte offset, because a skill that mentions where knowledge was
/// stored is a skill that breaks when the folder is reorganised.
pub fn brief(process: &ContextObject, sources: &[&ContextObject]) -> String {
    let mut out = String::with_capacity(1024);
    out.push_str("# The process\n\n");
    out.push_str(&process.title);
    out.push_str("\n\n");
    out.push_str(&process.body);
    out.push_str("\n\n# What it rests on\n");
    for source in sources.iter().filter(|s| s.id != process.id) {
        out.push_str("\n## ");
        out.push_str(&source.title);
        out.push('\n');
        out.push_str(&source.body);
        out.push('\n');
    }
    out
}

pub fn parse(reply: &str) -> Result<Draft> {
    let json = crate::candidates::extract_object(reply)
        .ok_or_else(|| CompileError::BadReply(crate::candidates::first_line(reply)))?;
    let draft: Draft = serde_json::from_str(json)
        .map_err(|e| CompileError::BadReply(format!("{e} in: {}", crate::candidates::first_line(json))))?;
    if draft.name.trim().is_empty() || draft.steps.iter().all(|s| s.trim().is_empty()) {
        return Err(CompileError::Refused("the draft has no name or no steps".into()));
    }
    Ok(draft)
}

/// The gate: a drafted skill may rearrange approved knowledge, never add to it.
fn build(
    process: &ContextObject,
    sources: &[&ContextObject],
    draft: &Draft,
) -> Result<ContextObject> {
    let body = markdown(draft);

    // Every figure the draft states has to exist in the approved material.
    // This is the whole difference between a skill and a plausible-sounding
    // procedure, and it is checked rather than requested.
    //
    // The check runs over what the engine wrote, not over the assembled
    // Markdown. The step numbering this module adds is a figure by any
    // string test and belongs to no rule, so checking the rendered body
    // refuses every skill ever drafted — the gate has to see the claim, not
    // the formatting we wrapped around it.
    let written = draft
        .steps
        .iter()
        .cloned()
        .chain(std::iter::once(draft.description.clone()))
        .chain(draft.inputs.iter().map(|f| f.description.clone()))
        .chain(draft.outputs.iter().map(|f| f.description.clone()))
        .collect::<Vec<_>>()
        .join("\n");

    let allowed: BTreeSet<String> = sources
        .iter()
        .flat_map(|source| {
            figures(&source.body)
                .into_iter()
                .chain(figures(&source.title))
        })
        .collect();
    let invented: Vec<String> = figures(&written)
        .into_iter()
        .filter(|figure| !allowed.contains(figure))
        .collect();
    if !invented.is_empty() {
        return Err(CompileError::Refused(format!(
            "the draft states {} which no approved rule does",
            invented.join(", ")
        )));
    }

    // Inherited, never created: the spans already verified against their
    // documents, so the skill passes the lake's gate for the same reason the
    // rules underneath it did.
    let mut evidence: Vec<Evidence> = Vec::new();
    for source in sources {
        for span in &source.evidence {
            if !evidence
                .iter()
                .any(|e| e.document_id == span.document_id && e.start_byte == span.start_byte)
            {
                evidence.push(span.clone());
            }
        }
    }
    if evidence.is_empty() {
        return Err(CompileError::Refused(
            "the process it was built from carries no source quote".into(),
        ));
    }

    // The weakest link, not the average. An average lets four solid rules
    // carry one shaky one into a procedure somebody will follow.
    let confidence = sources
        .iter()
        .map(|s| s.confidence.0)
        .fold(f32::INFINITY, f32::min);

    let id = skill_id(&process.id);
    Ok(ContextObject {
        id: id.clone(),
        kind: ObjectKind::Skill,
        subtype: None,
        title: draft.name.trim().to_string(),
        body,
        status: ObjectStatus::Proposed,
        confidence: Confidence(confidence.clamp(0.05, 0.98)),
        version: 1,
        valid_from: process.valid_from.clone(),
        valid_to: None,
        supersedes: None,
        decided_by: None,
        edited_on_approval: false,
        evidence,
        relations: sources
            .iter()
            .map(|source| Relation {
                target_id: source.id.clone(),
                target_label: source.title.clone(),
                kind: RelationType::DependsOn,
                // Structural: the edge is the set of objects we handed the
                // engine, which we chose, not something it proposed.
                origin: RelationOrigin::Structural,
                why: None,
                edge_confidence: None,
            })
            .collect(),
        path: format!("skills/{}.md", id.trim_start_matches("skill:").replace('.', "/")),
        updated_at: process.updated_at.clone(),
    })
}

/// `process:izrada-ponude` → `skill:izrada-ponude`, so a skill and the
/// process it came from are recognisably the same subject.
pub fn skill_id(process_id: &str) -> String {
    let rest = process_id.split_once(':').map(|(_, r)| r).unwrap_or(process_id);
    format!("skill:{rest}")
}

/// The SKILL.md body, assembled here rather than asked for.
///
/// The engine returns structure; the Markdown around it is ours. A model
/// asked for formatted Markdown spends its attention on formatting, and the
/// one thing that must not vary between two skills is their shape.
fn markdown(draft: &Draft) -> String {
    let mut out = String::with_capacity(512);
    out.push_str(draft.description.trim());
    out.push_str("\n\n");

    if !draft.inputs.is_empty() {
        out.push_str("**Before you start**\n\n");
        for field in &draft.inputs {
            out.push_str(&format!("- {} — {}\n", field.name.trim(), field.description.trim()));
        }
        out.push('\n');
    }

    out.push_str("**Steps**\n\n");
    for (index, step) in draft.steps.iter().filter(|s| !s.trim().is_empty()).enumerate() {
        out.push_str(&format!("{}. {}\n", index + 1, step.trim()));
    }

    if !draft.outputs.is_empty() {
        out.push_str("\n**Result**\n\n");
        for field in &draft.outputs {
            out.push_str(&format!("- {} — {}\n", field.name.trim(), field.description.trim()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use knowlith_core::Evidence;
    use knowlith_engine::{EngineError, Reply};

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

    struct Silent;

    impl Engine for Silent {
        fn name(&self) -> &str {
            "Silent"
        }
        fn run(&self, _request: &Request) -> knowlith_engine::Result<Reply> {
            Err(EngineError::Unavailable("codex is not signed in".into()))
        }
    }

    fn object(id: &str, kind: ObjectKind, title: &str, body: &str, approved: bool) -> ContextObject {
        ContextObject {
            id: id.into(),
            kind,
            subtype: None,
            title: title.into(),
            body: body.into(),
            status: if approved { ObjectStatus::Approved } else { ObjectStatus::Proposed },
            confidence: Confidence(0.8),
            version: 1,
            valid_from: "2026-01-01T00:00:00Z".into(),
            valid_to: None,
            supersedes: None,
            decided_by: Some("You".into()),
            edited_on_approval: false,
            evidence: vec![Evidence {
                document_id: "doc:abc".into(),
                locator: "§1 ¶1".into(),
                start_byte: 0,
                end_byte: 10,
                quote: "Rok plaćanja je 15 dana.".into(),
            }],
            relations: Vec::new(),
            path: "x.md".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    fn process_with_rule() -> Vec<ContextObject> {
        let mut process = object(
            "process:izrada-ponude",
            ObjectKind::Process,
            "Izrada ponude",
            "Ponuda se izrađuje iz cjenika i šalje klijentu.",
            true,
        );
        process.relations.push(Relation {
            target_id: "rule:rok-placanja".into(),
            target_label: "Rok plaćanja".into(),
            kind: RelationType::DependsOn,
            origin: RelationOrigin::Structural,
            why: None,
            edge_confidence: None,
        });
        vec![
            process,
            object(
                "rule:rok-placanja",
                ObjectKind::Rule,
                "Rok plaćanja",
                "Rok plaćanja je 15 dana od izdavanja računa.",
                true,
            ),
        ]
    }

    const GOOD: &str = r#"{"name":"Izradi ponudu","description":"Koristi kada klijent traži ponudu.",
      "inputs":[{"name":"klijent","type":"string","description":"Naziv klijenta."}],
      "outputs":[{"name":"ponuda","type":"document","description":"Ponuda spremna za slanje."}],
      "steps":["Otvori cjenik i pronađi stavke.","Upiši rok plaćanja od 15 dana.","Pošalji ponudu klijentu."]}"#;

    #[test]
    fn a_skill_is_built_from_approved_objects_and_inherits_their_spans() {
        let objects = process_with_rule();
        let run = draft_all(&Scripted(GOOD), &objects).unwrap();
        assert_eq!(run.skills.len(), 1);

        let skill = &run.skills[0];
        assert_eq!(skill.id, "skill:izrada-ponude");
        assert_eq!(skill.kind, ObjectKind::Skill);
        assert_eq!(skill.status, ObjectStatus::Proposed);
        assert!(!skill.evidence.is_empty(), "a skill must carry the spans it rests on");
        assert!(skill.body.contains("1. Otvori cjenik"));
    }

    /// The single most important test in this module.
    #[test]
    fn a_drafted_skill_that_invents_a_number_is_thrown_away_whole() {
        let invented = r#"{"name":"Izradi ponudu","description":"Koristi kada klijent traži ponudu.",
          "inputs":[],"outputs":[],
          "steps":["Upiši rok plaćanja od 30 dana.","Pošalji ponudu."]}"#;
        let objects = process_with_rule();
        let run = draft_all(&Scripted(invented), &objects).unwrap();

        assert!(run.skills.is_empty(), "an invented deadline must not become a procedure");
        assert_eq!(run.dropped.len(), 1);
        assert!(run.dropped[0].reason.contains("30dana"), "{}", run.dropped[0].reason);
    }

    #[test]
    fn a_number_that_is_already_approved_passes() {
        let objects = process_with_rule();
        let run = draft_all(&Scripted(GOOD), &objects).unwrap();
        assert_eq!(run.skills.len(), 1, "15 dana is stated by an approved rule");
    }

    #[test]
    fn a_process_the_owner_has_not_approved_gets_no_skill() {
        let mut objects = process_with_rule();
        objects[0].status = ObjectStatus::Proposed;
        let run = draft_all(&Scripted(GOOD), &objects).unwrap();
        assert!(run.skills.is_empty());
        assert_eq!(run.processes_considered, 0);
    }

    #[test]
    fn an_unavailable_engine_is_reported_per_process_and_does_not_stop_the_run() {
        let objects = process_with_rule();
        let run = draft_all(&Silent, &objects).unwrap();
        assert!(run.skills.is_empty());
        assert!(run.dropped[0].reason.contains("signed in"));
    }

    #[test]
    fn the_confidence_of_a_skill_is_its_weakest_source() {
        let mut objects = process_with_rule();
        objects[1].confidence = Confidence(0.44);
        let run = draft_all(&Scripted(GOOD), &objects).unwrap();
        assert!((run.skills[0].confidence.0 - 0.44).abs() < 1e-6);
    }

    #[test]
    fn the_engine_is_never_shown_a_file_name_or_an_offset() {
        let objects = process_with_rule();
        let sources: Vec<&ContextObject> = objects.iter().collect();
        let text = brief(&objects[0], &sources);
        assert!(!text.contains("doc:abc"));
        assert!(!text.contains("§1"));
        assert!(text.contains("Rok plaćanja je 15 dana"));
    }

    #[test]
    fn the_prompt_forbids_inventing_a_figure_and_the_schema_demands_steps() {
        assert!(SKILL_INSTRUCTIONS.contains("Never introduce a number"));
        assert!(SKILL_SCHEMA.contains("\"minItems\": 2"));
    }
}
