//! Stage 2: the one place a model is asked anything.
//!
//! The model gets one document — or a small batch of them in one CLI invoke —
//! and is asked for claims, each carried by the sentence it came from. It is
//! not asked which claim is current, whether two documents agree, or how
//! confident it is — those are decided afterwards by rules a person can read,
//! because a model's answer to any of them is an opinion dressed as a result.
//!
//! The prompt therefore asks for one thing and insists on the quote. Stage 4
//! throws away anything whose quote is not literally in the document, so a
//! model that paraphrases loses the claim rather than smuggling it through.
//!
//! Batching exists only to amortise CLI cold-start on large folders. The
//! engine is still the owner's own `codex` / `claude` / `agent` child process;
//! nothing here opens an HTTP API behind their back.

use std::collections::HashMap;
use std::time::Duration;

use knowlith_core::{Document, ObjectKind};
use knowlith_engine::{Engine, EngineUsage, Request};
use serde::{Deserialize, Serialize};

use crate::{CompileError, Result};

pub const INSTRUCTIONS: &str = "\
You are reading one internal document from a small company, to find what the \
company has decided.

Return every claim the document states about how the company operates. For \
each one:

- kind: \"rule\" for a limit, deadline, threshold or approval; \"process\" for \
an ordered procedure; \"term\" for a word this company uses in a particular \
way; \"fact\" for a value that holds until the document changes.
- title: a short noun phrase in the document's own language.
- statement: the claim in one or two plain sentences, in the document's \
language.
- quotes: the sentences from the document that state it, copied EXACTLY, \
character for character, including punctuation and diacritics. Do not \
paraphrase, do not fix typos, do not join two sentences.

Rules:
- Every claim must have at least one quote, and every quote must appear \
verbatim in the document. A claim whose quote you cannot copy exactly must be \
left out.
- Do not infer. If the document does not say it, it is not there.
- Do not include anything about a named individual's pay, health or identity \
numbers.
- If the document states nothing about how the company operates, return an \
empty list.";

pub const BATCH_INSTRUCTIONS: &str = "\
You are reading several internal documents from a small company, to find what \
the company has decided. Each document is wrapped in a <document id=\"…\"> \
fence. Keep every claim inside the document it came from — never mix quotes \
across documents.

For each document, return every claim it states about how the company operates:

- kind: \"rule\" for a limit, deadline, threshold or approval; \"process\" for \
an ordered procedure; \"term\" for a word this company uses in a particular \
way; \"fact\" for a value that holds until the document changes.
- title: a short noun phrase in the document's own language.
- statement: the claim in one or two plain sentences, in the document's \
language.
- quotes: the sentences from THAT document that state it, copied EXACTLY, \
character for character, including punctuation and diacritics. Do not \
paraphrase, do not fix typos, do not join two sentences.

Rules:
- Every claim must have at least one quote, and every quote must appear \
verbatim in the same document. A claim whose quote you cannot copy exactly \
must be left out.
- Do not infer. If the document does not say it, it is not there.
- Do not include anything about a named individual's pay, health or identity \
numbers.
- If a document states nothing about how the company operates, return an \
empty candidates list for it.
- Include every document id you were given, even when its candidates list is \
empty.";

pub const CANDIDATE_SCHEMA: &str = r#"{
  "type": "object",
  "additionalProperties": false,
  "required": ["candidates"],
  "properties": {
    "candidates": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "required": ["kind", "title", "statement", "quotes"],
        "properties": {
          "kind": { "type": "string", "enum": ["rule", "process", "term", "fact"] },
          "title": { "type": "string" },
          "statement": { "type": "string" },
          "subtype": { "type": "string" },
          "quotes": { "type": "array", "items": { "type": "string" }, "minItems": 1 }
        }
      }
    }
  }
}"#;

pub const BATCH_SCHEMA: &str = r#"{
  "type": "object",
  "additionalProperties": false,
  "required": ["documents"],
  "properties": {
    "documents": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "required": ["document_id", "candidates"],
        "properties": {
          "document_id": { "type": "string" },
          "candidates": {
            "type": "array",
            "items": {
              "type": "object",
              "additionalProperties": false,
              "required": ["kind", "title", "statement", "quotes"],
              "properties": {
                "kind": { "type": "string", "enum": ["rule", "process", "term", "fact"] },
                "title": { "type": "string" },
                "statement": { "type": "string" },
                "subtype": { "type": "string" },
                "quotes": { "type": "array", "items": { "type": "string" }, "minItems": 1 }
              }
            }
          }
        }
      }
    }
  }
}"#;

/// Serialisable because the lake stores it between the reading of a
/// document and the settling of the whole set — the two no longer happen in
/// the same process, or even on the same day.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Candidate {
    pub kind: String,
    pub title: String,
    pub statement: String,
    #[serde(default)]
    pub subtype: Option<String>,
    pub quotes: Vec<String>,
}

impl Candidate {
    pub fn object_kind(&self) -> ObjectKind {
        match self.kind.to_ascii_lowercase().as_str() {
            "process" => ObjectKind::Process,
            "term" => ObjectKind::Term,
            "fact" => ObjectKind::Fact,
            _ => ObjectKind::Rule,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Envelope {
    candidates: Vec<Candidate>,
}

#[derive(Debug, Deserialize)]
struct BatchEnvelope {
    documents: Vec<BatchDoc>,
}

#[derive(Debug, Deserialize)]
struct BatchDoc {
    document_id: String,
    #[serde(default)]
    candidates: Vec<Candidate>,
}

/// Asks the engine about one document.
///
/// `company` is whatever the owner wrote about what this firm is — industry,
/// customers, what "success" means here. Without it the model invents the
/// schema of an invoice twenty times. With it, stage 2 can prefer claims that
/// matter for *this* company.
pub fn propose(
    engine: &dyn Engine,
    document: &Document,
    company: Option<&str>,
) -> Result<(Vec<Candidate>, Option<EngineUsage>)> {
    let instructions = with_company(INSTRUCTIONS, company);
    let request = Request::new("candidates", &instructions, &document.text).with_schema(CANDIDATE_SCHEMA);
    let reply = engine.run(&request)?;
    Ok((parse(&reply.text)?, reply.usage))
}

/// Asks the engine about several documents in one CLI invoke.
///
/// Returns a map from document id → candidates. Documents the model omitted
/// come back as an empty list so the worker can still finish their jobs.
pub fn propose_many(
    engine: &dyn Engine,
    documents: &[&Document],
    company: Option<&str>,
) -> Result<(HashMap<String, Vec<Candidate>>, Option<EngineUsage>)> {
    if documents.is_empty() {
        return Ok((HashMap::new(), None));
    }
    if documents.len() == 1 {
        let only = documents[0];
        let (found, usage) = propose(engine, only, company)?;
        return Ok((HashMap::from([(only.id.clone(), found)]), usage));
    }

    let instructions = with_company(BATCH_INSTRUCTIONS, company);
    let mut input = String::new();
    for document in documents {
        input.push_str(&format!(
            "<document id=\"{}\" name=\"{}\">\n{}\n</document>\n\n",
            document.id,
            xml_escape(&document.name),
            document.text
        ));
    }

    // Cold-start dominates; a few more minutes for a packed prompt is still
    // cheaper than N separate CLI processes.
    let timeout = Duration::from_secs(600 + 90 * (documents.len() as u64).saturating_sub(1));
    let request = Request::new("candidates-batch", &instructions, input)
        .with_schema(BATCH_SCHEMA)
        .with_timeout(timeout);
    let reply = engine.run(&request)?;
    Ok((parse_batch(&reply.text, documents)?, reply.usage))
}

fn with_company(base: &str, company: Option<&str>) -> String {
    match company.map(str::trim).filter(|s| !s.is_empty()) {
        Some(profile) => format!(
            "{base}\n\nThis company, in the owner's words:\n{profile}\n\n\
             Prefer claims that matter for how this company actually works. \
             Do not invent a schema the document does not state."
        ),
        None => base.to_string(),
    }
}

fn xml_escape(name: &str) -> String {
    name.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Reads the reply, tolerating the wrapping a CLI puts around JSON.
///
/// Even with a schema attached, a model that has been told to produce JSON
/// will sometimes produce JSON inside a fenced code block with a sentence
/// before it. Refusing that would throw away a correct answer over its
/// packaging; accepting anything at all would let prose through as data. So:
/// find the outermost JSON object, and require it to parse.
pub fn parse(reply: &str) -> Result<Vec<Candidate>> {
    let json = extract_object(reply)
        .ok_or_else(|| CompileError::BadReply(first_line(reply)))?;
    let envelope: Envelope = serde_json::from_str(json)
        .map_err(|e| CompileError::BadReply(format!("{e} in: {}", first_line(json))))?;

    Ok(clean_candidates(envelope.candidates))
}

fn parse_batch(reply: &str, documents: &[&Document]) -> Result<HashMap<String, Vec<Candidate>>> {
    let json = extract_object(reply)
        .ok_or_else(|| CompileError::BadReply(first_line(reply)))?;
    let envelope: BatchEnvelope = serde_json::from_str(json)
        .map_err(|e| CompileError::BadReply(format!("{e} in: {}", first_line(json))))?;

    let by_name: HashMap<&str, &str> = documents
        .iter()
        .map(|d| (d.name.as_str(), d.id.as_str()))
        .collect();
    let known: HashMap<&str, ()> = documents.iter().map(|d| (d.id.as_str(), ())).collect();

    let mut out: HashMap<String, Vec<Candidate>> = documents
        .iter()
        .map(|d| (d.id.clone(), Vec::new()))
        .collect();

    for doc in envelope.documents {
        let id = if known.contains_key(doc.document_id.as_str()) {
            doc.document_id
        } else if let Some(mapped) = by_name.get(doc.document_id.as_str()) {
            (*mapped).to_string()
        } else {
            // Unknown id — drop rather than attach claims to the wrong file.
            continue;
        };
        out.insert(id, clean_candidates(doc.candidates));
    }
    Ok(out)
}

fn clean_candidates(candidates: Vec<Candidate>) -> Vec<Candidate> {
    candidates
        .into_iter()
        .filter(|c| {
            // A claim without a quote cannot be checked, and stage 4 would
            // drop it anyway. Dropping it here keeps the "proposed" count
            // honest.
            !c.quotes.iter().all(|q| q.trim().is_empty()) && !c.title.trim().is_empty()
        })
        .map(|mut c| {
            c.quotes.retain(|q| !q.trim().is_empty());
            c
        })
        .collect()
}

/// The outermost `{...}` in the reply, by brace balance.
pub(crate) fn extract_object(reply: &str) -> Option<&str> {
    let start = reply.find('{')?;
    let bytes = reply.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;

    for (offset, byte) in bytes.iter().enumerate().skip(start) {
        if escaped {
            escaped = false;
            continue;
        }
        match byte {
            b'\\' if in_string => escaped = true,
            b'"' => in_string = !in_string,
            b'{' if !in_string => depth += 1,
            b'}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(&reply[start..=offset]);
                }
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn first_line(s: &str) -> String {
    let line = s.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    if line.chars().count() <= 200 {
        line.to_string()
    } else {
        line.chars().take(200).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knowlith_core::DocumentKind;
    use knowlith_engine::Reply;

    const GOOD: &str = r#"{"candidates":[{"kind":"rule","title":"Popust","statement":"Popust je 5%.","quotes":["Popust od 5%."]}]}"#;

    #[test]
    fn plain_json_parses() {
        let found = parse(GOOD).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].object_kind(), ObjectKind::Rule);
    }

    #[test]
    fn json_wrapped_in_prose_and_a_code_fence_still_parses() {
        let reply = format!("Here is what I found:\n\n```json\n{GOOD}\n```\n\nLet me know if you need more.");
        assert_eq!(parse(&reply).unwrap().len(), 1);
    }

    #[test]
    fn a_brace_inside_a_quoted_string_does_not_end_the_object() {
        let reply = r#"{"candidates":[{"kind":"term","title":"Oznaka","statement":"Koristi se {broj} u predlosku.","quotes":["Koristi se {broj}."]}]}"#;
        assert_eq!(parse(reply).unwrap()[0].statement, "Koristi se {broj} u predlosku.");
    }

    #[test]
    fn prose_instead_of_json_is_refused_with_what_it_said() {
        let err = parse("I could not find any rules in this document.").unwrap_err();
        assert!(format!("{err}").contains("could not find any rules"));
    }

    #[test]
    fn a_claim_with_no_quote_is_dropped_before_it_is_counted() {
        let reply = r#"{"candidates":[{"kind":"rule","title":"Nesto","statement":"Bez izvora.","quotes":[""]}]}"#;
        assert!(parse(reply).unwrap().is_empty());
    }

    #[test]
    fn an_empty_result_is_a_valid_answer() {
        assert!(parse(r#"{"candidates":[]}"#).unwrap().is_empty());
    }

    #[test]
    fn the_schema_forbids_a_claim_without_quotes() {
        assert!(CANDIDATE_SCHEMA.contains("\"minItems\": 1"));
        assert!(CANDIDATE_SCHEMA.contains("\"required\": [\"kind\", \"title\", \"statement\", \"quotes\"]"));
    }

    #[test]
    fn the_prompt_insists_on_verbatim_quotes() {
        assert!(INSTRUCTIONS.contains("EXACTLY"));
        assert!(INSTRUCTIONS.contains("Do not infer"));
    }

    #[test]
    fn a_batch_reply_keeps_claims_on_the_right_document() {
        let a = doc("a", "A.txt", "Rok je 15 dana.");
        let b = doc("b", "B.txt", "Popust je 5%.");
        let reply = r#"{
          "documents": [
            {"document_id":"a","candidates":[{"kind":"rule","title":"Rok","statement":"Rok je 15 dana.","quotes":["Rok je 15 dana."]}]},
            {"document_id":"b","candidates":[{"kind":"rule","title":"Popust","statement":"Popust je 5%.","quotes":["Popust je 5%."]}]}
          ]
        }"#;
        let map = parse_batch(reply, &[&a, &b]).unwrap();
        assert_eq!(map["a"].len(), 1);
        assert_eq!(map["a"][0].title, "Rok");
        assert_eq!(map["b"][0].title, "Popust");
    }

    #[test]
    fn propose_many_of_one_uses_the_single_document_path() {
        struct Once;
        impl Engine for Once {
            fn name(&self) -> &str {
                "once"
            }
            fn run(&self, request: &Request) -> knowlith_engine::Result<Reply> {
                assert_eq!(request.stage, "candidates");
                Ok(Reply::new("once", GOOD))
            }
        }
        let document = doc("a", "A.txt", "Popust od 5%.");
        let (map, usage) = propose_many(&Once, &[&document], None).unwrap();
        assert_eq!(map["a"].len(), 1);
        assert!(usage.is_none());
    }

    fn doc(id: &str, name: &str, text: &str) -> Document {
        Document {
            id: id.into(),
            path: format!("/tmp/{name}"),
            name: name.into(),
            kind: DocumentKind::Text,
            byte_len: text.len() as u64,
            sha256: id.into(),
            text: text.into(),
            text_sha256: id.into(),
            verbatim: true,
            modified: "2026-01-01T00:00:00Z".into(),
            columns: None,
            blocks: vec![],
        }
    }
}
