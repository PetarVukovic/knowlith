//! Stage 2: the one place a model is asked anything.
//!
//! The model gets one document and is asked for claims, each carried by the
//! sentence it came from. It is not asked which claim is current, whether two
//! documents agree, or how confident it is — those are decided afterwards by
//! rules a person can read, because a model's answer to any of them is an
//! opinion dressed as a result.
//!
//! The prompt therefore asks for one thing and insists on the quote. Stage 4
//! throws away anything whose quote is not literally in the document, so a
//! model that paraphrases loses the claim rather than smuggling it through.

use knowlith_core::{Document, ObjectKind};
use knowlith_engine::{Engine, Request};
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

/// Asks the engine about one document.
pub fn propose(engine: &dyn Engine, document: &Document) -> Result<Vec<Candidate>> {
    let request = Request::new("candidates", INSTRUCTIONS, &document.text).with_schema(CANDIDATE_SCHEMA);
    let reply = engine.run(&request)?;
    parse(&reply.text)
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

    Ok(envelope
        .candidates
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
        .collect())
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
}
