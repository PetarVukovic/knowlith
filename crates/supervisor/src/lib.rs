//! Build supervisor: synthesise company knowledge from a whole folder, then
//! produce an owner confirmation quiz.

mod chunk;
mod cluster;
mod session;

use std::collections::{BTreeSet, HashMap};
#[cfg(test)]
use std::collections::BTreeMap;

use chrono::Utc;
use knowlith_core::{
    locate, Confidence, ContextObject, Evidence, ObjectKind, ObjectStatus, Subtype,
};
use knowlith_engine::{Engine, Request};

use knowlith_lake::{EntityRow, Lake, QuizEvidence, QuizQuestion};
#[cfg(test)]
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

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    Engine(#[from] knowlith_engine::EngineError),
    #[error("{0}")]
    Invalid(String),
}

impl From<String> for RunError {
    fn from(message: String) -> Self { Self::Invalid(message) }
}

impl RunError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Engine(error) if error.is_retryable())
    }
}

pub fn run(lake: &mut Lake, engine: &dyn Engine, session_id: &str) -> Result<RunReport, RunError> {
    run_for_job(lake, engine, session_id, None)
}

/// Keep the durable lease alive while each bounded CLI request is in flight.
pub fn run_for_job(lake: &mut Lake, engine: &dyn Engine, session_id: &str, job_id: Option<i64>) -> Result<RunReport, RunError> {
    let result = run_inner(lake, engine, session_id, job_id);
    if let Err(error) = &result {
        let phase = match error {
            RunError::Engine(knowlith_engine::EngineError::Unavailable(_)) => "held",
            error if error.is_retryable() => "retrying",
            _ => "failed",
        };
        let _ = lake.set_build_phase(phase);
        let _ = lake.finish_supervisor_session(session_id, "interrupted");
    }
    result
}

fn run_inner(lake: &mut Lake, engine: &dyn Engine, session_id: &str, job_id: Option<i64>) -> Result<RunReport, RunError> {
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
        return Err("no documents to supervise".to_string().into());
    }

    let names: HashMap<String, String> = documents
        .iter()
        .map(|d| (d.id.clone(), d.name.clone()))
        .collect();

    for community in cluster::communities(&documents) {
        lake.put_community(&community).map_err(|e| e.to_string())?;
    }
    let communities = lake.communities().map_err(|e| e.to_string())?;

    let mut parsed = Output { entities: vec![], canonical: vec![], quiz: vec![] };
    let batches = build_batches(&documents, &company)?;
    let total = batches.len();
    for (index, input) in batches.into_iter().enumerate() {
        // The key includes all model-visible context. A new session can reuse
        // finished work without inheriting an ever-growing conversation.
        let request = Request::new("supervisor", INSTRUCTIONS, input).with_schema(OUTPUT_SCHEMA);
        let key = format!("supervisor:v2:{}:{}", engine.name(), knowlith_engine::cassette_key(&request));
        let cached = lake.setting(&key).map_err(|e| e.to_string())?;
        let (reply, batch) = if let Some(reply) = cached.filter(|s| parse_json(s).is_ok()) {
            let batch = parse_json(&reply)?;
            (reply, batch)
        } else {
            let reply = std::thread::scope(|scope| {
                let handle = scope.spawn(|| engine.run(&request));
                while !handle.is_finished() {
                    if let Some(id) = job_id { let _ = lake.heartbeat(id); }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                handle.join().unwrap_or_else(|_| Err(knowlith_engine::EngineError::Transport("The AI reader stopped unexpectedly.".into())))
            })?.text;
            let batch = parse_json(&reply)?;
            lake.set_setting(&key, &reply).map_err(|e| e.to_string())?;
            (reply, batch)
        };
        lake.append_supervisor_turn(session_id, "checkpoint", &format!("{key}: {} bytes", reply.len()))
            .map_err(|e| e.to_string())?;
        lake.set_setting("build_progress", &serde_json::json!({"done": index + 1, "total": total}).to_string())
            .map_err(|e| e.to_string())?;
        parsed.entities.extend(batch.entities);
        parsed.canonical.extend(batch.canonical);
        parsed.quiz.extend(batch.quiz);
    }
    let now = Utc::now().to_rfc3339();

    let existing_entities = lake.entities().map_err(|e| e.to_string())?;
    let mut eligible = BTreeSet::new();
    for entity in &parsed.entities {
        if existing_entities.iter().any(|e| e.id == entity.id && e.status != "proposed") { continue; }
        if entity.links.is_empty() || entity.links.iter().any(|l| !names.contains_key(&l.document_id)) { continue; }
        eligible.insert(entity.id.clone());
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
        // Model-provided identifiers must never overwrite an owner's decision.
        if lake.object(&item.id).map_err(|e| e.to_string())?.is_some_and(|o| o.status != ObjectStatus::Proposed) { continue; }
        if let Some(doc) = documents.iter().find(|d| d.id == item.document_id) {
            if let Ok(object) = object_from(item, doc, &now) {
                if lake.put_object(&object).is_ok() {
                    canonical += 1;
                    eligible.insert(item.id.clone());
                }
            }
        }
    }

    let quiz_id = format!("quiz:{session_id}");
    let mut seen_questions = BTreeSet::new();
    let questions: Vec<QuizQuestion> = parsed
        .quiz
        .iter()
        .filter(|q| {
            !q.quote.trim().is_empty()
                && documents.iter().any(|d| d.id == q.document_id && locate(d, &q.quote).is_some())
                && q.proposed_object_id.as_ref().is_none_or(|id| eligible.contains(id))
                && seen_questions.insert((q.document_id.clone(), q.question.clone()))
        })
        .enumerate()
        .map(|(index, q)| QuizQuestion {
            id: format!("{session_id}:{index}:{}", q.id),
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

/// Bounded requests retain every byte, including the end of long documents.
fn build_batches(documents: &[knowlith_core::Document], company: &str) -> Result<Vec<String>, String> {
    const LIMIT: usize = 48_000;
    if company.len() > 8_000 { return Err("Company profile must be at most 8,000 bytes.".into()); }
    let prefix = format!("Company profile:\n{company}\nDocuments (episodes):\n");
    let mut batches = Vec::new();
    let mut current = prefix.clone();
    let mut sorted: Vec<_> = documents.iter().collect();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    for doc in sorted {
        if doc.id.len() + doc.name.len() > 4_000 { return Err("Document metadata is too long.".into()); }
        for piece in chunk::chunk_text(&doc.text, 24_000) {
            let entry = format!("\n<document id={:?} name={:?}>\n{piece}\n</document>\n", doc.id, doc.name);
            if current.len() + entry.len() > LIMIT {
                batches.push(current);
                current = prefix.clone();
            }
            current.push_str(&entry);
        }
    }
    if current.len() > prefix.len() { batches.push(current); }
    Ok(batches)
}

#[cfg(test)]
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

/// Confirms the quiz and approves only what the owner marked correct.
pub fn confirm_quiz(lake: &mut Lake, quiz_id: &str, approved_object_ids: &[String]) -> Result<(), String> {
    let quiz = lake
        .build_quiz()
        .map_err(|e| e.to_string())?
        .ok_or("no quiz to confirm")?;
    if quiz.id != quiz_id {
        return Err("quiz id mismatch".into());
    }
    let approved: BTreeSet<&str> = approved_object_ids.iter().map(String::as_str).collect();

    if quiz.state == "confirmed" {
        // A daemon that confirmed before entity rows were wired up left
        // `build_quiz.state = confirmed` while entities stayed `proposed`.
        // Re-applying the owner's answer list is idempotent and fixes that lake.
        for id in &approved {
            if id.starts_with("entity:") {
                lake.approve_entity(id).map_err(|e| e.to_string())?;
            }
        }
        return Ok(());
    }
    let all = lake.objects().map_err(|e| e.to_string())?;

    for id in &approved {
        if id.starts_with("entity:") {
            lake.approve_entity(id).map_err(|e| e.to_string())?;
            continue;
        }
        let Some(mut object) = all.iter().find(|o| o.id == *id).cloned() else {
            continue;
        };
        object.status = ObjectStatus::Approved;
        object.decided_by = Some("owner:quiz".into());
        lake.put_object(&object).map_err(|e| e.to_string())?;
    }

    for q in &quiz.questions {
        let Some(id) = q.proposed_object_id.as_deref() else {
            continue;
        };
        if approved.contains(id) {
            continue;
        }
        if id.starts_with("entity:") {
            let _ = lake.reject_entity(id);
            continue;
        }
        let Some(mut object) = all.iter().find(|o| o.id == id).cloned() else {
            continue;
        };
        if object.status == ObjectStatus::Proposed {
            object.status = ObjectStatus::Rejected;
            object.decided_by = Some("owner:quiz".into());
            let _ = lake.put_object(&object);
        }
    }

    lake.confirm_build_quiz(quiz_id).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use knowlith_core::{Document, DocumentKind};

    fn sample_lake(text: String) -> Lake {
        let mut lake = Lake::in_memory().unwrap();
        lake.put_source("s", "Company", "/company", "folder", "codex").unwrap();
        lake.put_document("s", &Document {
            id: "doc:terms".into(), path: "/company/terms.md".into(), name: "terms.md".into(),
            kind: DocumentKind::Markdown, byte_len: text.len() as u64,
            sha256: "source".into(), text_sha256: "rendition".into(), text,
            verbatim: true, modified: "2026-09-17T00:00:00Z".into(), columns: None, blocks: vec![],
        }).unwrap();
        lake
    }

    struct Capture(std::sync::Mutex<Vec<String>>);
    impl Engine for Capture {
        fn name(&self) -> &str { "capture" }
        fn run(&self, request: &knowlith_engine::Request) -> knowlith_engine::Result<knowlith_engine::Reply> {
            self.0.lock().unwrap().push(request.input.clone());
            Ok(knowlith_engine::Reply::new("capture", r#"{"entities":[],"canonical":[],"quiz":[]}"#))
        }
    }

    struct Interrupted(std::sync::atomic::AtomicUsize);
    impl Engine for Interrupted {
        fn name(&self) -> &str { "interrupted" }
        fn run(&self, _: &Request) -> knowlith_engine::Result<knowlith_engine::Reply> {
            let call = self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if call == 1 { return Err(knowlith_engine::EngineError::Transport("rate limited".into())); }
            Ok(knowlith_engine::Reply::new("interrupted", r#"{"entities":[],"canonical":[],"quiz":[]}"#))
        }
    }

    #[test]
    fn an_interrupted_build_resumes_without_repeating_finished_batches() {
        let mut lake = sample_lake(format!("{}END", "A long company document. ".repeat(8000)));
        let engine = Interrupted(std::sync::atomic::AtomicUsize::new(0));
        let error = run(&mut lake, &engine, "resume").unwrap_err();
        assert!(error.is_retryable());
        let expected = build_batches(&lake.documents().unwrap(), "").unwrap().into_iter().collect::<BTreeSet<_>>().len();
        run(&mut lake, &engine, "resume").unwrap();
        assert_eq!(engine.0.load(std::sync::atomic::Ordering::SeqCst), expected + 1);
    }

    #[test]
    fn first_supervisor_request_contains_the_company_documents() {
        let mut lake = sample_lake("Payment is due in 30 days.".into());
        let engine = Capture(std::sync::Mutex::new(vec![]));
        run(&mut lake, &engine, "first").unwrap();
        assert!(engine.0.lock().unwrap()[0].contains("Payment is due in 30 days."));
    }

    #[test]
    fn long_documents_are_covered_without_unbounded_requests() {
        let mut lake = sample_lake(format!("{}\nTAIL_MARKER", "ŽPayment terms.\n\n".repeat(12_000)));
        let engine = Capture(std::sync::Mutex::new(vec![]));
        run(&mut lake, &engine, "large").unwrap();
        let calls = engine.0.lock().unwrap();
        assert!(calls.iter().any(|input| input.contains("TAIL_MARKER")));
        assert!(calls.iter().all(|input| input.len() <= 48_000));
        assert!(calls.len() > 1);
    }

    #[test]
    fn unchanged_documents_reuse_successful_synthesis_across_sessions() {
        let mut lake = sample_lake("Payment terms.".into());
        let engine = Capture(std::sync::Mutex::new(vec![]));
        run(&mut lake, &engine, "first").unwrap();
        run(&mut lake, &engine, "second").unwrap();
        assert_eq!(engine.0.lock().unwrap().len(), 1);
        lake.set_setting("company_profile", "A different business").unwrap();
        run(&mut lake, &engine, "third").unwrap();
        assert_eq!(engine.0.lock().unwrap().len(), 2);
    }

    struct Fixed(&'static str);
    impl Engine for Fixed {
        fn name(&self) -> &str { "fixed" }
        fn run(&self, _: &Request) -> knowlith_engine::Result<knowlith_engine::Reply> {
            Ok(knowlith_engine::Reply::new("fixed", self.0))
        }
    }

    #[test]
    fn invented_quiz_evidence_is_not_presented_for_approval() {
        let mut lake = sample_lake("Payment terms.".into());
        let engine = Fixed(r#"{"entities":[],"canonical":[],"quiz":[{"id":"q1","question":"True?","agent_answer":"Yes","document_id":"doc:terms","quote":"Invented quote","proposed_object_id":"rule:missing"}]}"#);
        run(&mut lake, &engine, "quiz").unwrap();
        assert!(lake.build_quiz().unwrap().unwrap().questions.is_empty());
    }

    #[test]
    fn synthesis_cannot_overwrite_an_owner_approved_rule() {
        let mut lake = sample_lake("Payment terms.".into());
        let engine = Fixed(r#"{"entities":[],"canonical":[{"id":"rule:terms","kind":"rule","title":"Terms","body":"New interpretation","document_id":"doc:terms","quote":"Payment terms."}],"quiz":[]}"#);
        let parsed = parse_json(engine.0).unwrap();
        let doc = lake.documents().unwrap().remove(0);
        let mut approved = object_from(&parsed.canonical[0], &doc, "2026-09-17T00:00:00Z").unwrap();
        approved.status = ObjectStatus::Approved;
        approved.body = "Owner approved wording".into();
        lake.put_object(&approved).unwrap();
        run(&mut lake, &engine, "safe").unwrap();
        let after = lake.object("rule:terms").unwrap().unwrap();
        assert_eq!(after.status, ObjectStatus::Approved);
        assert_eq!(after.body, "Owner approved wording");
    }

    #[test]
    fn confirm_quiz_approves_only_what_the_owner_marked_correct() {
        let dir = std::env::temp_dir().join(format!(
            "knowlith-quiz-confirm-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut lake = Lake::open(&dir.join("lake.sqlite")).unwrap();

        lake.open_supervisor_session("build:1", "test").unwrap();

        lake.put_entity(&EntityRow {
            id: "entity:a".into(),
            title: "A".into(),
            kind: "party".into(),
            summary: "Issuer".into(),
            status: "proposed".into(),
        })
        .unwrap();
        lake.put_entity(&EntityRow {
            id: "entity:b".into(),
            title: "B".into(),
            kind: "party".into(),
            summary: "Customer".into(),
            status: "proposed".into(),
        })
        .unwrap();

        let quiz = knowlith_lake::BuildQuiz {
            id: "quiz:test".into(),
            session_id: "build:1".into(),
            state: "pending".into(),
            questions: vec![
                QuizQuestion {
                    id: "q1".into(),
                    question: "Who?".into(),
                    agent_answer: "A".into(),
                    evidence: vec![],
                    proposed_object_id: Some("entity:a".into()),
                },
                QuizQuestion {
                    id: "q2".into(),
                    question: "Whom?".into(),
                    agent_answer: "B".into(),
                    evidence: vec![],
                    proposed_object_id: Some("entity:b".into()),
                },
            ],
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        lake.put_build_quiz(&quiz).unwrap();

        confirm_quiz(&mut lake, "quiz:test", &["entity:a".into()]).unwrap();

        let entities = lake.entities().unwrap();
        let a = entities.iter().find(|e| e.id == "entity:a").unwrap();
        let b = entities.iter().find(|e| e.id == "entity:b").unwrap();
        assert_eq!(a.status, "approved");
        assert_eq!(b.status, "rejected");
        assert_eq!(lake.build_quiz().unwrap().unwrap().state, "confirmed");
    }

    #[test]
    fn confirm_quiz_repairs_entities_after_an_already_confirmed_quiz() {
        let dir = std::env::temp_dir().join(format!(
            "knowlith-quiz-repair-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut lake = Lake::open(&dir.join("lake.sqlite")).unwrap();
        lake.open_supervisor_session("build:1", "test").unwrap();
        lake.put_entity(&EntityRow {
            id: "entity:a".into(),
            title: "A".into(),
            kind: "party".into(),
            summary: "Issuer".into(),
            status: "proposed".into(),
        })
        .unwrap();
        lake.put_build_quiz(&knowlith_lake::BuildQuiz {
            id: "quiz:test".into(),
            session_id: "build:1".into(),
            state: "confirmed".into(),
            questions: vec![QuizQuestion {
                id: "q1".into(),
                question: "Who?".into(),
                agent_answer: "A".into(),
                evidence: vec![],
                proposed_object_id: Some("entity:a".into()),
            }],
            created_at: "2026-01-01T00:00:00Z".into(),
        })
        .unwrap();

        confirm_quiz(&mut lake, "quiz:test", &["entity:a".into()]).unwrap();

        let entity = lake.entities().unwrap().into_iter().next().unwrap();
        assert_eq!(entity.status, "approved");
    }

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
