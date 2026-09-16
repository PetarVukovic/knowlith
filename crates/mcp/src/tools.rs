//! The eleven things an AI tool can ask this company.
//!
//! Two decisions run through all of them.
//!
//! **A figure is read from a row, never recalled from prose.** `lookup_value`
//! reads cells out of the spreadsheet the owner maintains. Everything else
//! returns sentences. Retrieved by similarity a price comes back *close*,
//! and close is the one thing a price may never be.
//!
//! **Every answer carries where it came from.** Not as a courtesy — as the
//! mechanism. An agent that has the document name, the locator and the exact
//! quote can cite; an agent that has a paragraph of company knowledge with
//! no provenance can only assert. The whole product is the difference.

use std::collections::BTreeSet;

use knowlith_core::{ContextObject, ObjectKind, ObjectStatus};
use knowlith_graph::Graph;
use knowlith_lake::Lake;
use serde_json::{Value, json};

use crate::gate;

/// How many objects a single answer may carry.
///
/// Not a performance limit. An agent given forty rules reads the first
/// three and treats the rest as background, which is worse than being given
/// eight and told there are more.
const MAX_RESULTS: usize = 8;
/// How much of a source passage travels with an answer.
const QUOTE_MARGIN: usize = 240;

/// What a tool produced.
pub struct Outcome {
    /// What the model reads.
    pub text: String,
    /// The same answer as data, for a client that validates against the
    /// output schema.
    pub structured: Value,
    /// Pointers to sources, so the agent can fetch a full document without
    /// it being pasted into every reply.
    pub links: Vec<Value>,
    /// A refusal the model should read and act on — never a transport
    /// failure, which is a protocol error instead.
    pub is_error: bool,
}

impl Outcome {
    fn new(text: impl Into<String>, structured: Value) -> Self {
        Self {
            text: text.into(),
            structured,
            links: Vec::new(),
            is_error: false,
        }
    }

    fn refused(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            structured: json!({ "refused": true }),
            links: Vec::new(),
            is_error: true,
        }
    }

    fn with_links(mut self, links: Vec<Value>) -> Self {
        self.links = links;
        self
    }
}

/// The catalogue, as `tools/list` returns it.
///
/// `readOnlyHint` is the field that decides whether this is usable. A client
/// that cannot tell a read from a write asks the owner to approve every
/// call, and an eight-step question becomes eight dialogs — at which point
/// nobody uses it twice.
pub fn catalogue(icon: &Value) -> Vec<Value> {
    let read = |idempotent: bool| {
        json!({
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": idempotent,
            // Everything here is the company's own approved knowledge. There
            // is no wider world behind these tools, and saying so lets a
            // client reason about what a call can possibly touch.
            "openWorldHint": false,
        })
    };

    vec![
        tool(
            "get_relevant_context",
            "Start here",
            "Given what you are about to do, lists every part of this company's knowledge that touches it — and returns a case id to pass to the other tools. Returns titles, not answers: read the ones you need with get_context. Finish with check_coverage, which will tell you what you did not look at.",
            json!({
                "type": "object",
                "properties": {
                    "question": {
                        "type": "string",
                        "description": "What you are working on, in a sentence. 'Prepare a quote for installing three split units in Zagreb.'"
                    }
                },
                "required": ["question"]
            }),
            Some(json!({
                "type": "object",
                "properties": {
                    "caseId": { "type": "string" },
                    "areas": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "title": { "type": "string" },
                                "kind": { "type": "string" },
                                "why": { "type": "string" }
                            },
                            "required": ["id", "title", "kind", "why"]
                        }
                    },
                    "openQuestions": { "type": "array", "items": { "type": "object" } },
                    "note": { "type": "string" }
                },
                "required": ["caseId", "areas", "note"]
            })),
            read(false),
            icon,
        ),
        tool(
            "search_context",
            "Search the company",
            "Finds approved rules, processes and terms by meaning of the words in them. Returns the text, the document it came from and the exact quote. Never returns anything the owner has not approved.",
            json!({
                "type": "object",
                "properties": {
                    "question": { "type": "string", "description": "What you want to know." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 8, "default": 5 },
                    "caseId": { "type": "string", "description": "From get_relevant_context, so coverage can be checked." }
                },
                "required": ["question"]
            }),
            Some(results_schema()),
            read(true),
            icon,
        ),
        tool(
            "get_context",
            "Read one thing in full",
            "The full text of one rule, process, term or fact, with its sources, what it rests on and what would break if it changed.",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "An id from get_relevant_context or search_context." },
                    "caseId": { "type": "string" }
                },
                "required": ["id"]
            }),
            Some(results_schema()),
            read(true),
            icon,
        ),
        tool(
            "lookup_value",
            "Read an exact figure",
            "Reads a figure out of the company's own price list or table, by the row it is actually written in. Use this for every number. Do not take a price from a sentence, and never interpolate between two you have seen.",
            json!({
                "type": "object",
                "properties": {
                    "what": { "type": "string", "description": "The product, service or line item. 'montaža multi split 3 jedinice'" },
                    "caseId": { "type": "string" }
                },
                "required": ["what"]
            }),
            Some(json!({
                "type": "object",
                "properties": {
                    "rows": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "document": { "type": "string" },
                                "locator": { "type": "string" },
                                "columns": { "type": "array", "items": { "type": "string" } },
                                "cells": { "type": "array", "items": { "type": "string" } }
                            },
                            "required": ["document", "locator", "cells"]
                        }
                    },
                    "found": { "type": "integer" }
                },
                "required": ["rows", "found"]
            })),
            read(true),
            icon,
        ),
        tool(
            "get_source_evidence",
            "Show the original passage",
            "The passage in the owner's own document that a rule was drawn from, word for word, with a little of what surrounds it. Use it when you want to quote rather than paraphrase.",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "The rule, process, term or fact whose source you want." },
                    "caseId": { "type": "string" }
                },
                "required": ["id"]
            }),
            None,
            read(true),
            icon,
        ),
        tool(
            "get_process",
            "How this company does something",
            "The approved procedures — how a quote is made, how a complaint is handled — in the order the company actually does them.",
            json!({
                "type": "object",
                "properties": {
                    "topic": { "type": "string", "description": "Leave empty for all of them." },
                    "caseId": { "type": "string" }
                }
            }),
            Some(results_schema()),
            read(true),
            icon,
        ),
        tool(
            "get_skill",
            "A procedure written for you to run",
            "A step-by-step procedure the owner approved for an AI tool to carry out, with the figures it needs already in it. Call with no name to see what exists.",
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "caseId": { "type": "string" }
                }
            }),
            None,
            read(true),
            icon,
        ),
        tool(
            "what_breaks_if",
            "Consequences of a change",
            "Everything in the company's knowledge that rests on one rule, so a change can be talked about before it is made. This is the question a folder of documents cannot answer.",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "caseId": { "type": "string" }
                },
                "required": ["id"]
            }),
            None,
            read(true),
            icon,
        ),
        tool(
            "list_pending",
            "What the owner has not decided",
            "Subjects where two documents disagree, or where nothing has been approved yet. You get the subject, never either answer — so you can say the question is open instead of guessing at it.",
            json!({ "type": "object", "properties": {} }),
            None,
            read(true),
            icon,
        ),
        tool(
            "check_coverage",
            "Finish here",
            "Closes a case and reports what you never looked at. Do not claim you have checked the company's rules until this comes back clean.",
            json!({
                "type": "object",
                "properties": {
                    "caseId": { "type": "string" },
                    "summary": { "type": "string", "description": "One line on what you concluded, for the owner to read later." }
                },
                "required": ["caseId"]
            }),
            Some(json!({
                "type": "object",
                "properties": {
                    "complete": { "type": "boolean" },
                    "read": { "type": "array", "items": { "type": "string" } },
                    "missed": { "type": "array", "items": { "type": "object" } },
                    "openQuestions": { "type": "array", "items": { "type": "object" } },
                    "verdict": { "type": "string" }
                },
                "required": ["complete", "verdict"]
            })),
            read(false),
            icon,
        ),
        tool(
            "propose_change",
            "Suggest something for the owner",
            "Puts a suggestion in the owner's review queue. It is never knowledge until they approve it, and it is refused unless the quote you give appears word for word in a document Knowlith has already read.",
            json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "The subject, as the company would say it." },
                    "body": { "type": "string", "description": "What you are proposing, in the owner's language." },
                    "documentId": { "type": "string", "description": "From the source of something you read." },
                    "quote": { "type": "string", "description": "The sentence in that document that supports this, word for word." }
                },
                "required": ["title", "body", "documentId", "quote"]
            }),
            None,
            json!({
                "readOnlyHint": false,
                "destructiveHint": false,
                "idempotentHint": true,
                "openWorldHint": false,
            }),
            icon,
        ),
    ]
}

fn tool(
    name: &str,
    title: &str,
    description: &str,
    input: Value,
    output: Option<Value>,
    annotations: Value,
    icon: &Value,
) -> Value {
    let mut entry = json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": input,
        "annotations": annotations,
        // The company's own mark, so an owner scrolling a list of tools sees
        // their company rather than a vendor.
        "icons": [icon],
    });
    if let Some(schema) = output {
        entry["outputSchema"] = schema;
    }
    entry
}

fn results_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "results": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "title": { "type": "string" },
                        "kind": { "type": "string" },
                        "text": { "type": "string" },
                        "restsOn": { "type": "array", "items": { "type": "string" } },
                        "stale": { "type": "boolean" },
                        "source": {
                            "type": "object",
                            "properties": {
                                "document": { "type": "string" },
                                "locator": { "type": "string" },
                                "quote": { "type": "string" }
                            }
                        }
                    },
                    "required": ["id", "title", "kind", "text"]
                }
            },
            "found": { "type": "integer" }
        },
        "required": ["results", "found"]
    })
}

/// The tool names, for a client that wants to check one exists.
pub fn names() -> Vec<&'static str> {
    vec![
        "get_relevant_context",
        "search_context",
        "get_context",
        "lookup_value",
        "get_source_evidence",
        "get_process",
        "get_skill",
        "what_breaks_if",
        "list_pending",
        "check_coverage",
        "propose_change",
    ]
}

// ------------------------------------------------------------------ calls --

pub fn call(
    lake: &mut Lake,
    company: &str,
    name: &str,
    arguments: &Value,
    app: Option<&str>,
) -> Outcome {
    let case = arguments.get("caseId").and_then(Value::as_str);

    match name {
        "get_relevant_context" => match text_argument(arguments, "question") {
            Ok(question) => relevant(lake, &question, app),
            Err(outcome) => outcome,
        },
        "search_context" => match text_argument(arguments, "question") {
            Ok(question) => {
                let limit = arguments
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(5)
                    .clamp(1, MAX_RESULTS as u64) as usize;
                search(lake, &question, limit, case, app)
            }
            Err(outcome) => outcome,
        },
        "get_context" => match text_argument(arguments, "id") {
            Ok(id) => one(lake, &id, case, app),
            Err(outcome) => outcome,
        },
        "lookup_value" => match text_argument(arguments, "what") {
            Ok(what) => lookup(lake, &what, case, app),
            Err(outcome) => outcome,
        },
        "get_source_evidence" => match text_argument(arguments, "id") {
            Ok(id) => evidence(lake, &id, case, app),
            Err(outcome) => outcome,
        },
        "get_process" => processes(lake, arguments.get("topic").and_then(Value::as_str), case, app),
        "get_skill" => skill(lake, arguments.get("name").and_then(Value::as_str), case, app),
        "what_breaks_if" => match text_argument(arguments, "id") {
            Ok(id) => breaks(lake, &id),
            Err(outcome) => outcome,
        },
        "list_pending" => pending(lake, company),
        "check_coverage" => match text_argument(arguments, "caseId") {
            Ok(id) => coverage(lake, &id, arguments.get("summary").and_then(Value::as_str)),
            Err(outcome) => outcome,
        },
        "propose_change" => propose(lake, arguments),
        other => Outcome::refused(format!("There is no tool called {other}.")),
    }
}

fn text_argument(arguments: &Value, name: &str) -> Result<String, Outcome> {
    match arguments.get(name).and_then(Value::as_str) {
        Some(value) if !value.trim().is_empty() => Ok(value.trim().to_string()),
        _ => Err(Outcome::refused(format!("\"{name}\" is required."))),
    }
}

// ------------------------------------------------------- get_relevant_context --

fn relevant(lake: &mut Lake, question: &str, app: Option<&str>) -> Outcome {
    let objects = match lake.objects() {
        Ok(objects) => objects,
        Err(e) => return Outcome::refused(format!("The lake could not be read: {e}")),
    };

    if objects.is_empty() {
        return Outcome::new(
            "This company has not put anything into Knowlith yet. There is nothing to check against, so say so rather than answering from general knowledge.",
            json!({ "caseId": Value::Null, "areas": [], "openQuestions": [], "note": "empty" }),
        );
    }

    let matched = lake.search_objects(question, 16).unwrap_or_default();
    let servable: Vec<&ContextObject> = objects.iter().filter(|o| gate::is_servable(o)).collect();

    // What the words point at.
    let mut areas: Vec<(String, String, String, String)> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for id in &matched {
        if let Some(object) = servable.iter().find(|o| &o.id == id) {
            if seen.insert(object.id.clone()) {
                areas.push((
                    object.id.clone(),
                    object.title.clone(),
                    kind_word(object.kind).to_string(),
                    "matches what you asked".to_string(),
                ));
            }
        }
    }

    // What those rest on. A discount without its turnover threshold is not a
    // discount, it is half of one, and an agent that was never shown the
    // threshold cannot know it was missing.
    let graph = Graph::build(&lake.edges().unwrap_or_default());
    let direct: Vec<String> = areas.iter().map(|(id, ..)| id.clone()).collect();
    for id in &direct {
        for foundation in graph.foundations(id).into_iter().filter(|f| f.distance <= 2) {
            if let Some(object) = servable.iter().find(|o| o.id == foundation.id) {
                if seen.insert(object.id.clone()) {
                    let under = areas
                        .iter()
                        .find(|(candidate, ..)| candidate == id)
                        .map(|(_, title, ..)| title.clone())
                        .unwrap_or_else(|| id.clone());
                    areas.push((
                        object.id.clone(),
                        object.title.clone(),
                        kind_word(object.kind).to_string(),
                        format!("\"{under}\" rests on this"),
                    ));
                }
            }
        }
    }

    // Subjects the owner has not settled. Named, never answered.
    let open: Vec<Value> = gate::unsettled(&objects)
        .into_iter()
        .filter(|(object, _)| {
            matched.contains(&object.id) || overlaps(question, &object.title)
        })
        .map(|(object, why)| json!({ "id": object.id, "subject": object.title, "why": why }))
        .collect();

    let stale: Vec<String> = areas
        .iter()
        .filter(|(id, ..)| lake.stale_since(id).ok().flatten().is_some())
        .map(|(_, title, ..)| title.clone())
        .collect();

    let ids: Vec<String> = areas.iter().map(|(id, ..)| id.clone()).collect();
    let case_id = match lake.open_case(question, &ids, app) {
        Ok(id) => id,
        Err(e) => return Outcome::refused(format!("The case could not be opened: {e}")),
    };

    let area_values: Vec<Value> = areas
        .iter()
        .map(|(id, title, kind, why)| json!({ "id": id, "title": title, "kind": kind, "why": why }))
        .collect();

    let mut text = String::new();
    if areas.is_empty() {
        text.push_str("Nothing in this company's approved knowledge touches that.\n");
    } else {
        text.push_str(&format!(
            "{} things this company has decided touch that. Read the ones you need with get_context, then call check_coverage.\n\n",
            areas.len()
        ));
        for (id, title, kind, why) in &areas {
            text.push_str(&format!("- {title} ({kind}) — {why}\n  id: {id}\n"));
        }
    }
    if !open.is_empty() {
        text.push_str("\nOpen questions — the owner has not decided these. Say so; do not answer them:\n");
        for value in &open {
            text.push_str(&format!(
                "- {}\n",
                value.get("subject").and_then(Value::as_str).unwrap_or("")
            ));
        }
    }
    if !stale.is_empty() {
        text.push_str("\nCheck before relying on: ");
        text.push_str(&stale.join(", "));
        text.push('\n');
    }
    text.push_str(&format!("\ncase id: {case_id}"));

    Outcome::new(
        text,
        json!({
            "caseId": case_id,
            "areas": area_values,
            "openQuestions": open,
            "stale": stale,
            "note": "These are titles, not answers. Read what you need, then call check_coverage with this case id."
        }),
    )
}

/// Whether a question and a title share a content word.
///
/// Crude on purpose. It decides only whether to *mention* that a subject is
/// unsettled, and mentioning one too many is a much smaller harm than
/// letting an agent answer a question the owner is still thinking about.
fn overlaps(question: &str, title: &str) -> bool {
    let words = |text: &str| -> BTreeSet<String> {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|word| word.chars().count() >= 4)
            .map(fold)
            .collect()
    };
    !words(question).is_disjoint(&words(title))
}

/// Lowercase with Croatian diacritics folded, truncated to a stem.
///
/// `plaćanja` and `plaćanje` are the same word to anyone reading them and
/// two different strings to a computer. Five characters is where Croatian
/// inflection has not usually started yet.
fn fold(word: &str) -> String {
    word.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'č' | 'ć' => 'c',
            'ž' => 'z',
            'š' => 's',
            'đ' => 'd',
            other => other,
        })
        .take(5)
        .collect()
}

// ------------------------------------------------------------ search / read --

fn search(
    lake: &mut Lake,
    question: &str,
    limit: usize,
    case: Option<&str>,
    app: Option<&str>,
) -> Outcome {
    let ids = lake.search_objects(question, limit * 3).unwrap_or_default();
    let objects = match lake.objects() {
        Ok(objects) => objects,
        Err(e) => return Outcome::refused(format!("The lake could not be read: {e}")),
    };

    let found: Vec<ContextObject> = ids
        .iter()
        .filter_map(|id| objects.iter().find(|o| &o.id == id).cloned())
        .filter(gate::is_servable)
        .take(limit)
        .collect();

    if found.is_empty() {
        let unsettled = gate::unsettled(&objects);
        let near: Vec<&str> = unsettled
            .iter()
            .filter(|(object, _)| overlaps(question, &object.title))
            .map(|(object, _)| object.title.as_str())
            .collect();

        let mut text = String::from(
            "This company has not approved anything about that. Say so plainly — that answer is useful, because it tells the owner what is missing.",
        );
        if !near.is_empty() {
            text.push_str(&format!(
                "\n\nThere is an unsettled question nearby, which the owner has not decided: {}.",
                near.join(", ")
            ));
        }
        return Outcome::new(text, json!({ "results": [], "found": 0 }));
    }

    answer(lake, found, case, "search_context", app)
}

fn one(lake: &mut Lake, id: &str, case: Option<&str>, app: Option<&str>) -> Outcome {
    let object = match lake.object(id) {
        Ok(Some(object)) => object,
        Ok(None) => return Outcome::refused(format!("There is nothing here with the id {id}.")),
        Err(e) => return Outcome::refused(format!("The lake could not be read: {e}")),
    };

    match gate::access(&object) {
        gate::Access::Full => answer(lake, vec![object], case, "get_context", app),
        gate::Access::NameOnly(why) => Outcome::new(
            format!(
                "\"{}\" is not settled: {why}. Tell whoever asked that this is an open question at this company, and do not answer it yourself.",
                object.title
            ),
            json!({ "results": [], "found": 0, "openQuestion": object.title }),
        ),
        gate::Access::Withheld => Outcome::refused(format!(
            "\"{}\" is not something this company serves — it was replaced or refused.",
            object.title
        )),
    }
}

fn processes(
    lake: &mut Lake,
    topic: Option<&str>,
    case: Option<&str>,
    app: Option<&str>,
) -> Outcome {
    let objects = match lake.objects() {
        Ok(objects) => objects,
        Err(e) => return Outcome::refused(format!("The lake could not be read: {e}")),
    };

    let mut found: Vec<ContextObject> = objects
        .into_iter()
        .filter(|o| o.kind == ObjectKind::Process && gate::is_servable(o))
        .collect();

    if let Some(topic) = topic.filter(|t| !t.trim().is_empty()) {
        found.retain(|o| overlaps(topic, &o.title) || overlaps(topic, &o.body));
    }
    found.truncate(MAX_RESULTS);

    if found.is_empty() {
        return Outcome::new(
            "This company has no approved procedures yet.",
            json!({ "results": [], "found": 0 }),
        );
    }
    answer(lake, found, case, "get_process", app)
}

/// Turns objects into the shape every reading tool returns.
fn answer(
    lake: &mut Lake,
    objects: Vec<ContextObject>,
    case: Option<&str>,
    tool: &str,
    app: Option<&str>,
) -> Outcome {
    let edges = lake.edges().unwrap_or_default();
    let graph = Graph::build(&edges);

    let mut text = String::new();
    let mut results = Vec::new();
    let mut links = Vec::new();

    for object in &objects {
        let stale = lake.stale_since(&object.id).ok().flatten();
        let rests_on: Vec<String> = graph
            .foundations(&object.id)
            .into_iter()
            .filter(|f| f.distance == 1)
            .map(|f| f.id)
            .collect();

        let source = object.evidence.first().map(|span| {
            let document = lake
                .document(&span.document_id)
                .map(|d| d.name)
                .unwrap_or_else(|_| span.document_id.clone());
            json!({
                "document": document,
                "locator": span.locator,
                "quote": span.quote,
            })
        });

        text.push_str(&format!("## {}\n\n{}\n\n", object.title, object.body.trim()));
        if let Some(source) = &source {
            text.push_str(&format!(
                "Source: {} — {}\n> {}\n\n",
                source["document"].as_str().unwrap_or(""),
                source["locator"].as_str().unwrap_or(""),
                source["quote"].as_str().unwrap_or("")
            ));
        }
        if let Some(warning) = gate::stale_warning(stale.as_deref()) {
            text.push_str(&format!("{warning}\n\n"));
        }
        if !rests_on.is_empty() {
            text.push_str(&format!(
                "Rests on: {}. Read those too before you rely on this.\n\n",
                rests_on.join(", ")
            ));
        }

        results.push(json!({
            "id": object.id,
            "title": object.title,
            "kind": kind_word(object.kind),
            "text": object.body,
            "restsOn": rests_on,
            "stale": stale.is_some(),
            "confidence": object.confidence.phrase(),
            "source": source,
        }));

        for span in &object.evidence {
            links.push(json!({
                "type": "resource_link",
                "uri": format!("knowlith://document/{}", span.document_id),
                "name": span.locator,
                "description": format!("The passage \"{}\" was drawn from.", object.title),
                "mimeType": "text/plain",
            }));
        }

        let _ = lake.record_case_read(&object.id, tool, case, app);
    }

    let found = results.len();
    Outcome::new(text.trim_end().to_string(), json!({ "results": results, "found": found }))
        .with_links(links)
}

// -------------------------------------------------------------- lookup_value --

fn lookup(lake: &mut Lake, what: &str, case: Option<&str>, app: Option<&str>) -> Outcome {
    let rows = match lake.rows_matching(what, 6) {
        Ok(rows) => rows,
        Err(e) => return Outcome::refused(format!("The tables could not be read: {e}")),
    };

    if rows.is_empty() {
        return Outcome::new(
            format!(
                "There is no row for \"{what}\" in anything this company has given Knowlith. Do not estimate one — say it is not in the price list and let the owner add it."
            ),
            json!({ "rows": [], "found": 0 }),
        );
    }

    // Where the rows come from more than one file, the agent has to be told
    // which file is newer — otherwise it quotes last year's price with the
    // same confidence as this year's, and both are equally "in the company's
    // own table".
    let files: BTreeSet<&str> = rows.iter().map(|row| row.document_name.as_str()).collect();
    let mut text = if files.len() > 1 {
        String::from(
            "Read from the company's own tables, cell by cell. More than one file has a row for this, newest file first — check with the owner which one is current before quoting an older one:\n\n",
        )
    } else {
        String::from("Read from the company's own table, cell by cell:\n\n")
    };
    let mut values = Vec::new();
    let mut links = Vec::new();

    for row in &rows {
        let where_at = match (&row.sheet, row.row) {
            (Some(sheet), Some(number)) => format!("{sheet}, row {number}"),
            _ => row.locator.clone(),
        };
        let day = row.modified.split('T').next().unwrap_or(&row.modified);
        text.push_str(&format!("{} ({day}) — {}\n", row.document_name, where_at));

        if row.columns.len() == row.cells.len() && !row.columns.is_empty() {
            for (column, cell) in row.columns.iter().zip(&row.cells) {
                text.push_str(&format!("  {column}: {cell}\n"));
            }
        } else {
            text.push_str(&format!("  {}\n", row.text));
        }
        text.push('\n');

        values.push(json!({
            "document": row.document_name,
            "documentId": row.document_id,
            "modified": row.modified,
            "locator": row.locator,
            "sheet": row.sheet,
            "row": row.row,
            "columns": row.columns,
            "cells": row.cells,
        }));
        links.push(json!({
            "type": "resource_link",
            "uri": format!("knowlith://document/{}", row.document_id),
            "name": row.document_name,
            "mimeType": "text/plain",
        }));
    }

    text.push_str("These are the figures as written. Use them exactly; do not round them and do not add anything between two of them.");
    if files.len() > 1 {
        text.push_str(&format!(
            "\n\nThey came from {} different files. If two of them disagree, say so rather than choosing.",
            files.len()
        ));
    }
    let _ = lake.record_case_read("table", "lookup_value", case, app);

    let found = values.len();
    Outcome::new(text, json!({ "rows": values, "found": found })).with_links(links)
}

// -------------------------------------------------------------- the source --

fn evidence(lake: &mut Lake, id: &str, case: Option<&str>, app: Option<&str>) -> Outcome {
    let object = match lake.object(id) {
        Ok(Some(object)) => object,
        Ok(None) => return Outcome::refused(format!("There is nothing here with the id {id}.")),
        Err(e) => return Outcome::refused(format!("The lake could not be read: {e}")),
    };
    if !gate::is_servable(&object) {
        return Outcome::refused(format!(
            "\"{}\" is not approved, so its source is not served either.",
            object.title
        ));
    }

    let mut text = format!("What the company's own documents say behind \"{}\":\n\n", object.title);
    let mut passages = Vec::new();

    for span in &object.evidence {
        match lake.passage(&span.document_id, span.start_byte, span.end_byte, QUOTE_MARGIN) {
            Ok(Some((document, passage))) => {
                text.push_str(&format!("{} — {}\n\n{}\n\n", document.name, span.locator, passage.trim()));
                passages.push(json!({
                    "document": document.name,
                    "documentId": document.id,
                    "locator": span.locator,
                    "quote": span.quote,
                    "passage": passage,
                    // Whether the offsets address the owner's file itself or
                    // a rendition of it. For a PDF they never do, and an
                    // agent telling someone to look at "byte 4120 of the
                    // PDF" would be sending them nowhere.
                    "verbatimFile": document.verbatim,
                }));
            }
            _ => {
                text.push_str(&format!(
                    "{} — the document behind this is no longer readable. Do not quote it.\n\n",
                    span.locator
                ));
            }
        }
    }

    let _ = lake.record_case_read(&object.id, "get_source_evidence", case, app);
    Outcome::new(text.trim_end().to_string(), json!({ "passages": passages }))
}

// ------------------------------------------------------------------ skills --

fn skill(
    lake: &mut Lake,
    name: Option<&str>,
    case: Option<&str>,
    app: Option<&str>,
) -> Outcome {
    let objects = match lake.objects() {
        Ok(objects) => objects,
        Err(e) => return Outcome::refused(format!("The lake could not be read: {e}")),
    };
    let skills: Vec<ContextObject> = objects
        .into_iter()
        .filter(|o| o.kind == ObjectKind::Skill && gate::is_servable(o))
        .collect();

    if skills.is_empty() {
        return Outcome::new(
            "This company has no approved procedures for you to run yet. Skills are built from processes the owner has approved, so there will be none until they approve one.",
            json!({ "skills": [] }),
        );
    }

    match name.filter(|n| !n.trim().is_empty()) {
        None => {
            let listing: Vec<Value> = skills
                .iter()
                .map(|s| json!({ "name": s.title, "id": s.id }))
                .collect();
            let text = skills
                .iter()
                .map(|s| format!("- {}", s.title))
                .collect::<Vec<_>>()
                .join("\n");
            Outcome::new(
                format!("Procedures this company approved for you to run:\n\n{text}\n\nAsk for one by name."),
                json!({ "skills": listing }),
            )
        }
        Some(wanted) => {
            let found = skills
                .iter()
                .find(|s| s.title.eq_ignore_ascii_case(wanted) || s.id == wanted)
                .or_else(|| skills.iter().find(|s| overlaps(wanted, &s.title)));

            match found {
                Some(found) => {
                    let _ = lake.record_case_read(&found.id, "get_skill", case, app);
                    Outcome::new(
                        found.body.clone(),
                        json!({ "name": found.title, "id": found.id, "markdown": found.body }),
                    )
                }
                None => Outcome::refused(format!(
                    "There is no approved procedure called \"{wanted}\". Call get_skill with no name to see what there is."
                )),
            }
        }
    }
}

// ------------------------------------------------------------- the graph --

fn breaks(lake: &mut Lake, id: &str) -> Outcome {
    let objects = match lake.objects() {
        Ok(objects) => objects,
        Err(e) => return Outcome::refused(format!("The lake could not be read: {e}")),
    };
    let Some(subject) = objects.iter().find(|o| o.id == id) else {
        return Outcome::refused(format!("There is nothing here with the id {id}."));
    };

    let graph = Graph::build(&lake.edges().unwrap_or_default());
    let affected = graph.impact(id);

    if affected.is_empty() {
        return Outcome::new(
            format!("Nothing else rests on \"{}\". Changing it affects only itself.", subject.title),
            json!({ "subject": subject.title, "affected": [], "found": 0 }),
        );
    }

    let mut text = format!(
        "Changing \"{}\" reaches {} other thing{}:\n\n",
        subject.title,
        affected.len(),
        if affected.len() == 1 { "" } else { "s" }
    );
    let mut values = Vec::new();

    for hit in affected.iter().take(12) {
        let title = objects
            .iter()
            .find(|o| o.id == hit.id)
            .map(|o| o.title.clone())
            .unwrap_or_else(|| hit.id.clone());
        let how = if hit.distance == 1 {
            "directly".to_string()
        } else {
            format!("through {}", hit.distance - 1)
        };
        text.push_str(&format!("- {title} ({how})\n"));
        values.push(json!({ "id": hit.id, "title": title, "distance": hit.distance }));
    }

    let found = values.len();
    Outcome::new(text, json!({ "subject": subject.title, "affected": values, "found": found }))
}

// ----------------------------------------------------------------- pending --

fn pending(lake: &mut Lake, company: &str) -> Outcome {
    let objects = match lake.objects() {
        Ok(objects) => objects,
        Err(e) => return Outcome::refused(format!("The lake could not be read: {e}")),
    };

    let open = gate::unsettled(&objects);
    let stale: Vec<&ContextObject> = objects
        .iter()
        .filter(|o| {
            o.status == ObjectStatus::Approved
                && lake.stale_since(&o.id).ok().flatten().is_some()
        })
        .collect();

    if open.is_empty() && stale.is_empty() {
        return Outcome::new(
            format!("Everything {company} has put into Knowlith is settled."),
            json!({ "openQuestions": [], "needsChecking": [] }),
        );
    }

    let mut text = String::new();
    if !open.is_empty() {
        text.push_str("Subjects the owner has not decided. You know the question, not the answer — say the question is open:\n\n");
        for (object, why) in &open {
            text.push_str(&format!("- {} ({why})\n", object.title));
        }
    }
    if !stale.is_empty() {
        text.push_str("\nApproved, but something underneath moved since:\n\n");
        for object in &stale {
            text.push_str(&format!("- {}\n", object.title));
        }
    }

    Outcome::new(
        text.trim_end().to_string(),
        json!({
            "openQuestions": open.iter().map(|(o, why)| json!({ "subject": o.title, "why": why })).collect::<Vec<_>>(),
            "needsChecking": stale.iter().map(|o| json!({ "subject": o.title })).collect::<Vec<_>>(),
        }),
    )
}

// ---------------------------------------------------------------- coverage --

fn coverage(lake: &mut Lake, case_id: &str, summary: Option<&str>) -> Outcome {
    let case = match lake.case(case_id) {
        Ok(Some(case)) => case,
        Ok(None) => {
            return Outcome::refused(format!(
                "There is no case {case_id}. Start with get_relevant_context, which gives you one."
            ));
        }
        Err(e) => return Outcome::refused(format!("The lake could not be read: {e}")),
    };

    let read: BTreeSet<String> = lake
        .case_reads(case_id)
        .unwrap_or_default()
        .into_iter()
        .collect();
    let objects = lake.objects().unwrap_or_default();
    let title_of = |id: &str| -> String {
        objects
            .iter()
            .find(|o| o.id == id)
            .map(|o| o.title.clone())
            .unwrap_or_else(|| id.to_string())
    };

    let missed: Vec<&String> = case.areas.iter().filter(|id| !read.contains(*id)).collect();
    let complete = missed.is_empty();

    let mut text = String::new();
    if complete {
        text.push_str(&format!(
            "You looked at all {} things this company has decided about that.\n",
            case.areas.len()
        ));
    } else {
        text.push_str(&format!(
            "You read {} of {}. You did not look at:\n\n",
            case.areas.len() - missed.len(),
            case.areas.len()
        ));
        for id in &missed {
            text.push_str(&format!("- {} (id: {id})\n", title_of(id)));
        }
        text.push_str("\nEither read them, or say in your answer that you did not check them. Do not claim to have checked the company's rules.\n");
    }

    let _ = lake.close_case(case_id, summary);

    Outcome::new(
        text,
        json!({
            "complete": complete,
            "read": read.iter().collect::<Vec<_>>(),
            "missed": missed.iter().map(|id| json!({ "id": id, "title": title_of(id) })).collect::<Vec<_>>(),
            "verdict": if complete { "covered" } else { "incomplete" },
        }),
    )
}

// ---------------------------------------------------------- propose_change --

fn propose(lake: &mut Lake, arguments: &Value) -> Outcome {
    let (title, body, document_id, quote) = match (
        text_argument(arguments, "title"),
        text_argument(arguments, "body"),
        text_argument(arguments, "documentId"),
        text_argument(arguments, "quote"),
    ) {
        (Ok(title), Ok(body), Ok(document), Ok(quote)) => (title, body, document, quote),
        (Err(e), ..) | (_, Err(e), ..) | (.., Err(e), _) | (.., Err(e)) => return e,
    };

    let document = match lake.document(&document_id) {
        Ok(document) => document,
        Err(_) => {
            return Outcome::refused(format!(
                "Knowlith has not read a document {document_id}. A suggestion has to point at something the owner actually gave it."
            ));
        }
    };

    // The same gate the compiler goes through. An agent gets no easier path
    // to becoming company knowledge than the machine that reads the folder:
    // the quote is located in the real text or the suggestion is refused.
    let Some((start, end)) = knowlith_core::locate(&document, &quote) else {
        return Outcome::refused(format!(
            "That sentence does not appear in {} — or it appears more than once, which is the same problem. A suggestion has to quote the document word for word.",
            document.name
        ));
    };

    let id = format!("fact:agent.{}", slug(&title));
    let now = chrono::Utc::now().to_rfc3339();
    let object = ContextObject {
        id: id.clone(),
        kind: ObjectKind::Fact,
        subtype: None,
        title: title.clone(),
        body,
        // The one status an agent may write. There is no second path to
        // becoming knowledge: this lands in the same queue as everything the
        // compiler produced, and the owner decides.
        status: ObjectStatus::Proposed,
        confidence: knowlith_core::Confidence(0.5),
        version: 1,
        valid_from: now.clone(),
        valid_to: None,
        supersedes: None,
        decided_by: None,
        edited_on_approval: false,
        evidence: vec![knowlith_core::Evidence {
            document_id: document.id.clone(),
            locator: document
                .block_at(start)
                .map(|b| b.locator.clone())
                .unwrap_or_else(|| "quoted".to_string()),
            start_byte: start,
            end_byte: end,
            quote: quote.clone(),
        }],
        relations: Vec::new(),
        path: format!("facts/{}.md", slug(&title)),
        updated_at: now,
    };

    match lake.put_object(&object) {
        Ok(()) => Outcome::new(
            format!(
                "Noted for the owner to review: \"{title}\", quoting {}. It is not company knowledge until they approve it, so do not use it as one in this conversation.",
                document.name
            ),
            json!({ "id": id, "status": "proposed" }),
        ),
        Err(e) => Outcome::refused(format!("The suggestion was refused: {e}")),
    }
}

fn slug(title: &str) -> String {
    let folded: String = title
        .chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'č' | 'ć' => 'c',
            'ž' => 'z',
            'š' => 's',
            'đ' => 'd',
            c if c.is_alphanumeric() => c,
            _ => '-',
        })
        .collect();
    folded
        .split('-')
        .filter(|part| !part.is_empty())
        .take(5)
        .collect::<Vec<_>>()
        .join("-")
}

pub fn kind_word(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Rule => "rule",
        ObjectKind::Process => "process",
        ObjectKind::Term => "term",
        ObjectKind::Fact => "fact",
        ObjectKind::Skill => "skill",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_in_the_catalogue_can_be_called() {
        let icon = json!({ "src": "data:,", "mimeType": "image/svg+xml" });
        let listed: Vec<String> = catalogue(&icon)
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        let dispatched: Vec<String> = names().into_iter().map(str::to_string).collect();
        assert_eq!(listed, dispatched, "the catalogue and the dispatcher disagree");
    }

    #[test]
    fn only_proposing_a_change_is_a_write() {
        let icon = json!({ "src": "data:,", "mimeType": "image/svg+xml" });
        for entry in catalogue(&icon) {
            let name = entry["name"].as_str().unwrap();
            let read_only = entry["annotations"]["readOnlyHint"].as_bool().unwrap();
            assert_eq!(
                read_only,
                name != "propose_change",
                "{name} is marked wrongly"
            );
        }
    }

    #[test]
    fn nothing_claims_to_reach_outside_the_company() {
        let icon = json!({ "src": "data:,", "mimeType": "image/svg+xml" });
        for entry in catalogue(&icon) {
            assert_eq!(entry["annotations"]["openWorldHint"], json!(false));
        }
    }

    #[test]
    fn every_tool_carries_the_company_mark() {
        let icon = json!({ "src": "data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=", "mimeType": "image/svg+xml" });
        for entry in catalogue(&icon) {
            assert_eq!(entry["icons"][0]["src"], icon["src"]);
        }
    }

    #[test]
    fn croatian_inflection_does_not_hide_an_overlap() {
        assert!(overlaps("koji je rok plaćanja", "Rok plaćanje i avans"));
        assert!(overlaps("popusti za stalne kupce", "Popust za stalnog kupca"));
        assert!(!overlaps("cijena montaže", "Radno vrijeme"));
    }

    #[test]
    fn a_title_becomes_a_readable_id() {
        assert_eq!(slug("Rok plaćanja i avans"), "rok-placanja-i-avans");
        assert_eq!(slug("Popust 5% za stalne kupce"), "popust-5-za-stalne-kupce");
    }
}
