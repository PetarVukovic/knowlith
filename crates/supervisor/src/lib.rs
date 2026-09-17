//! Build supervisor: synthesise company knowledge from a whole folder, then
//! produce an owner confirmation quiz.

mod chunk;
mod cluster;
mod session;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::Utc;
use knowlith_core::{
    locate, Confidence, ContextObject, Evidence, ObjectKind, ObjectStatus, Subtype,
};
use knowlith_engine::Engine;
use knowlith_lake::{CommunityRow, EntityRow, Lake, QuizEvidence, QuizQuestion};
use regex::Regex;
use serde::Deserialize;

pub use session::Session;

#[derive(Debug, Clone)]
pub struct RunReport {
    pub session_id: String,
    pub entities: usize,
    pub canonical: usize,
    pub quiz_questions: usize,
    pub communities: usize,
}

const INSTRUCTIONS: &str = "\
You are the Knowlith build supervisor for one company.

Your job is NOT to list every fact from every invoice or document separately.
Your job IS to understand how this company works:
- recurring parties (customers, suppliers, issuers)
- rules that repeat (payment terms, billing format, approval)
- terms the company uses

Rules:
- Every canonical claim must include a verbatim quote from a document id below.
- Invoice numbers (INV-xxx) are document instances, not company rules.
- Prefer a few canonical rules/terms over dozens of duplicate facts.
- Return JSON only, matching the schema.";

const OUTPUT_SCHEMA: &str = r#"{
  "type": "object",
  "additionalProperties": false,
  "required": ["entities", "canonical", "quiz"],
  "properties": {
    "entities": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "required": ["id", "title", "kind", "summary", "links"],
        "properties": {
          "id": { "type": "string" },
          "title": { "type": "string" },
          "kind": { "type": "string" },
          "summary": { "type": "string" },
          "links": {
            "type": "array",
            "items": {
              "type": "object",
              "additionalProperties": false,
              "required": ["document_id", "role"],
              "properties": {
                "document_id": { "type": "string" },
                "role": { "type": "string" }
              }
            }
          }
        }
      }
    },
    "canonical": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "required": ["id", "kind", "title", "body", "document_id", "quote"],
        "properties": {
          "id": { "type": "string" },
          "kind": { "type": "string" },
          "title": { "type": "string" },
          "body": { "type": "string" },
          "document_id": { "type": "string" },
          "quote": { "type": "string" }
        }
      }
    },
    "quiz": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "required": ["id", "question", "agent_answer", "document_id", "quote"],
        "properties": {
          "id": { "type": "string" },
          "question": { "type": "string" },
          "agent_answer": { "type": "string" },
          "document_id": { "type": "string" },
          "quote": { "type": "string" },
          "proposed_object_id": { "type": "string" }
        }
      }
    }
  }
}"#;

#[derive(Debug, Deserialize)]
struct Output {
    entities: Vec<EntityOut>,
    canonical: Vec<CanonicalOut>,
    quiz: Vec<QuizOut>,
}

#[derive(Debug, Deserialize)]
struct EntityOut {
    id: String,
    title: String,
    kind: String,
    summary: String,
    links: Vec<EntityLinkOut>,
}

#[derive(Debug, Deserialize)]
struct EntityLinkOut {
    document_id: String,
    role: String,
}

#[derive(Debug, Deserialize)]
struct CanonicalOut {
    id: String,
    kind: String,
    title: String,
    body: String,
    document_id: String,
    quote: String,
}

#[derive(Debug, Deserialize)]
struct QuizOut {
    id: String,
    question: String,
    agent_answer: String,
    document_id: String,
    quote: String,
    proposed_object_id: Option<String>,
}

/// Runs the full build supervisor pass over everything in the lake.
pub fn run(lake: &mut Lake, engine: &dyn Engine, session_id: &str) -> Result<RunReport, String> {
    lake.set_build_phase("active").map_err(|e| e.to_string())?;
    lake.open_supervisor_session(session_id, engine.name())
        .map_err(|e| e.to_string())?;

    let company = lake
        .setting("company_profile")
        .ok()
        .flatten()
        .unwrap_or_default();
    let documents = lake.documents().map_err(|e| e.to_string())?;
    if documents.is_empty() {
        return Err("no documents to supervise".into());
    }

    let names: HashMap<String, String> = documents
        .iter()
        .map(|d| (d.id.clone(), d.name.clone()))
        .collect();

    for community in cluster::communities(&documents) {
        lake.put_community(&community).map_err(|e| e.to_string())?;
    }
    let communities = lake.communities().map_err(|e| e.to_string())?;

    let mut session = Session::load(lake, session_id)?;
    let corpus = build_corpus(&documents, &communities, &company)?;
    session.record(lake, "user", &corpus)?;

    let reply = session.ask(
        engine,
        "Synthesise entities, canonical company knowledge, and a confirmation quiz.",
        INSTRUCTIONS,
        Some(OUTPUT_SCHEMA),
    )?;
    session.record(lake, "assistant", &reply)?;

    let parsed: Output = parse_json(&reply)?;
    let now = Utc::now().to_rfc3339();

    for entity in &parsed.entities {
        lake.put_entity(&EntityRow {
            id: entity.id.clone(),
            title: entity.title.clone(),
            kind: entity.kind.clone(),
            summary: entity.summary.clone(),
            status: "proposed".into(),
        })
        .map_err(|e| e.to_string())?;
        for link in &entity.links {
            let _ = lake.link_episode_entity(&link.document_id, &entity.id, &link.role);
        }
    }

    let mut canonical = 0usize;
    for item in &parsed.canonical {
        if let Some(doc) = documents.iter().find(|d| d.id == item.document_id) {
            if let Ok(object) = object_from(item, doc, &now) {
                if lake.put_object(&object).is_ok() {
                    canonical += 1;
                }
            }
        }
    }

    let quiz_id = format!("quiz:{session_id}");
    let questions: Vec<QuizQuestion> = parsed
        .quiz
        .iter()
        .map(|q| QuizQuestion {
            id: q.id.clone(),
            question: q.question.clone(),
            agent_answer: q.agent_answer.clone(),
            evidence: vec![QuizEvidence {
                document_id: q.document_id.clone(),
                document_name: names
                    .get(&q.document_id)
                    .cloned()
                    .unwrap_or_else(|| q.document_id.clone()),
                quote: q.quote.clone(),
            }],
            proposed_object_id: q.proposed_object_id.clone(),
        })
        .collect();

    lake.put_build_quiz(&knowlith_lake::BuildQuiz {
        id: quiz_id,
        session_id: session_id.to_string(),
        state: "pending".into(),
        questions: questions.clone(),
        created_at: now.clone(),
    })
    .map_err(|e| e.to_string())?;

    lake.set_build_phase("quiz_pending")
        .map_err(|e| e.to_string())?;
    lake.finish_supervisor_session(session_id, "complete")
        .map_err(|e| e.to_string())?;

    Ok(RunReport {
        session_id: session_id.to_string(),
        entities: parsed.entities.len(),
        canonical,
        quiz_questions: questions.len(),
        communities: communities.len(),
    })
}

fn build_corpus(
    documents: &[knowlith_core::Document],
    communities: &[CommunityRow],
    company: &str,
) -> Result<String, String> {
    let mut out = String::new();
    if !company.trim().is_empty() {
        out.push_str("Company profile:\n");
        out.push_str(company.trim());
        out.push_str("\n\n");
    }
    out.push_str("Communities:\n");
    for c in communities {
        out.push_str(&format!("- {} ({} documents)\n", c.label, c.document_ids.len()));
    }
    out.push_str("\nDocuments (episodes):\n");
    for doc in documents {
        out.push_str(&format!("\n<document id=\"{}\" name=\"{}\">\n", doc.id, doc.name));
        let chunks = chunk::chunk_text(&doc.text, 1800);
        for (i, piece) in chunks.iter().enumerate().take(6) {
            out.push_str(&format!("<!-- chunk {i} -->\n{piece}\n"));
        }
        if chunks.len() > 6 {
            out.push_str(&format!("<!-- {} more chunks omitted -->\n", chunks.len() - 6));
        }
        out.push_str("</document>\n");
    }

    let hints = deterministic_entities(documents);
    if !hints.is_empty() {
        out.push_str("\nDeterministic entity hints (verify before using):\n");
        for (name, count) in hints {
            out.push_str(&format!("- {name} (seen in {count} documents)\n"));
        }
    }
    Ok(out)
}

fn deterministic_entities(documents: &[knowlith_core::Document]) -> BTreeMap<String, usize> {
    let billed = Regex::new(r"(?i)billed to[:\s]+([A-Za-z0-9][A-Za-z0-9 .&-]{1,40})").unwrap();
    let issued = Regex::new(r"(?i)issued by[:\s]+([A-Za-z0-9][A-Za-z0-9 .&-]{1,40})").unwrap();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for doc in documents {
        for cap in billed.captures_iter(&doc.text).chain(issued.captures_iter(&doc.text)) {
            if let Some(m) = cap.get(1) {
                // Stop at the first sentence boundary — invoice lines often run on.
                let name = m
                    .as_str()
                    .trim()
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if name.len() >= 3 {
                    *counts.entry(name).or_default() += 1;
                }
            }
        }
    }
    counts
}

fn object_from(
    item: &CanonicalOut,
    doc: &knowlith_core::Document,
    now: &str,
) -> Result<ContextObject, String> {
    let kind = match item.kind.to_lowercase().as_str() {
        "rule" => ObjectKind::Rule,
        "process" => ObjectKind::Process,
        "term" => ObjectKind::Term,
        _ => ObjectKind::Fact,
    };
    let (start, end) = locate(doc, &item.quote)
        .ok_or_else(|| format!("quote not in document: {}", item.quote))?;
    let slug = item.id.trim_start_matches("entity:").trim_start_matches("rule:");
    Ok(ContextObject {
        id: item.id.clone(),
        kind,
        subtype: Some(Subtype::Policy),
        title: item.title.clone(),
        body: item.body.clone(),
        status: ObjectStatus::Proposed,
        confidence: Confidence(0.85),
        version: 1,
        valid_from: now[..10].to_string(),
        valid_to: None,
        supersedes: None,
        decided_by: None,
        edited_on_approval: false,
        evidence: vec![Evidence {
            document_id: doc.id.clone(),
            locator: doc.name.clone(),
            start_byte: start,
            end_byte: end,
            quote: item.quote.clone(),
        }],
        relations: vec![],
        path: format!("supervisor/{slug}.md"),
        updated_at: now.to_string(),
    })
}

fn parse_json(text: &str) -> Result<Output, String> {
    let trimmed = text.trim();
    if let Ok(v) = serde_json::from_str::<Output>(trimmed) {
        return Ok(v);
    }
    let start = trimmed.find('{').ok_or("supervisor returned no JSON")?;
    let end = trimmed.rfind('}').ok_or("supervisor returned no JSON")?;
    serde_json::from_str(&trimmed[start..=end]).map_err(|e| format!("invalid supervisor JSON: {e}"))
}

/// Confirms the quiz and approves linked canonical objects.
pub fn confirm_quiz(lake: &mut Lake, quiz_id: &str, approved_object_ids: &[String]) -> Result<(), String> {
    let quiz = lake
        .build_quiz()
        .map_err(|e| e.to_string())?
        .ok_or("no quiz to confirm")?;
    if quiz.id != quiz_id {
        return Err("quiz id mismatch".into());
    }
    let mut ids: BTreeSet<String> = approved_object_ids.iter().cloned().collect();
    for q in &quiz.questions {
        if let Some(id) = &q.proposed_object_id {
            ids.insert(id.clone());
        }
    }
    let all = lake.objects().map_err(|e| e.to_string())?;
    for id in ids {
        let Some(mut object) = all.iter().find(|o| o.id == id).cloned() else {
            continue;
        };
        object.status = ObjectStatus::Approved;
        object.decided_by = Some("owner:quiz".into());
        let _ = lake.put_object(&object);
    }
    lake.confirm_build_quiz(quiz_id).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use knowlith_core::{Document, DocumentKind};

    #[test]
    fn deterministic_entities_find_repeated_parties() {
        let doc = Document {
            id: "doc:test".into(),
            path: "/inv.md".into(),
            name: "inv.md".into(),
            kind: DocumentKind::Markdown,
            byte_len: 0,
            sha256: "abc".into(),
            text: "Invoice INV-010. Billed to Formify. Issued by Bethel AI.".into(),
            text_sha256: "def".into(),
            verbatim: true,
            modified: "2026-01-01T00:00:00Z".into(),
            columns: None,
            blocks: vec![],
        };
        let found = deterministic_entities(&[doc]);
        assert!(found.contains_key("Formify"));
        assert!(found.contains_key("Bethel AI"));
    }
}
