//! A whole conversation, driven the way a client drives it.
//!
//! These are the tests that would have caught every protocol mistake worth
//! catching: they write real JSON-RPC lines into the server and read real
//! lines back. Nothing here reaches inside the crate.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use knowlith_core::{
    Block, BlockKind, Confidence, ContextObject, Document, DocumentKind, Evidence, ObjectKind,
    ObjectStatus, document_id, sha256_hex,
};
use knowlith_lake::Lake;
use knowlith_mcp::{Options, rpc};
use serde_json::{Value, json};

/// A lake with one document and three objects: an approved rule, the
/// threshold it rests on, and a subject the owner has not settled.
fn lake_with_a_company(path: &Path) -> Lake {
    let text = "Popust za stalne kupce iznosi 5%.\nStalan kupac je onaj s prometom preko 20.000 EUR godišnje.\nRok plaćanja je 30 dana.\n";
    let sha = sha256_hex(text.as_bytes());
    let document = Document {
        id: document_id(&sha),
        path: "/tmp/uvjeti.md".into(),
        name: "Uvjeti prodaje.md".into(),
        kind: DocumentKind::Markdown,
        byte_len: text.len() as u64,
        sha256: sha.clone(),
        text: text.to_string(),
        text_sha256: sha,
        verbatim: true,
        modified: "2026-01-01T00:00:00Z".into(),
        columns: None,
        blocks: text
            .lines()
            .enumerate()
            .map(|(n, line)| {
                let start = text.find(line).unwrap();
                Block {
                    locator: format!("§{}", n + 1),
                    kind: BlockKind::Paragraph,
                    text: line.to_string(),
                    start_byte: start,
                    end_byte: start + line.len(),
                    page: None,
                    sheet: None,
                    row: None,
                    cells: None,
                }
            })
            .collect(),
    };

    let mut lake = Lake::open(path).unwrap();
    lake.put_source("s1", "Uvjeti", "/tmp", "folder", "codex").unwrap();
    lake.put_document("s1", &document).unwrap();

    let span = |quote: &str, locator: &str| {
        let start = document.text.find(quote).unwrap();
        Evidence {
            document_id: document.id.clone(),
            locator: locator.into(),
            start_byte: start,
            end_byte: start + quote.len(),
            quote: quote.into(),
        }
    };

    let object = |id: &str, title: &str, body: &str, status, evidence: Vec<Evidence>| ContextObject {
        id: id.into(),
        kind: ObjectKind::Rule,
        subtype: None,
        title: title.into(),
        body: body.into(),
        status,
        confidence: Confidence(0.9),
        version: 1,
        valid_from: "2026-01-01T00:00:00Z".into(),
        valid_to: None,
        supersedes: None,
        decided_by: Some("ana".into()),
        edited_on_approval: false,
        evidence,
        relations: Vec::new(),
        path: format!("rules/{id}.md"),
        updated_at: "2026-01-01T00:00:00Z".into(),
    };

    lake.put_object(&object(
        "rule:threshold",
        "Tko je stalan kupac",
        "Kupac s godišnjim prometom preko 20.000 EUR.",
        ObjectStatus::Approved,
        vec![span("Stalan kupac je onaj s prometom preko 20.000 EUR godišnje.", "§2")],
    ))
    .unwrap();

    let mut discount = object(
        "rule:discount",
        "Popust za stalne kupce",
        "Stalnim kupcima odobrava se 5% popusta.",
        ObjectStatus::Approved,
        vec![span("Popust za stalne kupce iznosi 5%.", "§1")],
    );
    discount.relations = vec![knowlith_core::Relation {
        target_id: "rule:threshold".into(),
        target_label: "Tko je stalan kupac".into(),
        kind: knowlith_core::RelationType::DependsOn,
        origin: knowlith_core::RelationOrigin::Model,
    }];
    lake.put_object(&discount).unwrap();

    lake.put_object(&object(
        "rule:terms",
        "Rok plaćanja",
        "Dva dokumenta se ne slažu.",
        ObjectStatus::Conflicted,
        vec![span("Rok plaćanja je 30 dana.", "§3")],
    ))
    .unwrap();

    lake
}

/// Runs a scripted conversation and returns every message the server wrote.
fn converse(db: &Path, requests: &[Value]) -> Vec<Value> {
    let input: String = requests
        .iter()
        .map(|r| format!("{r}\n"))
        .collect::<Vec<_>>()
        .concat();

    let sink = Recorder::default();
    let writer = rpc::Writer::to(Box::new(sink.clone()));
    let mut reader = rpc::Reader::new(input.as_bytes());

    knowlith_mcp::run(
        Options {
            db: db.to_path_buf(),
            company: "Termoval d.o.o.".into(),
        },
        writer,
        &mut reader,
    )
    .unwrap();

    sink.messages()
}

fn call(name: &str, id: i64, arguments: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": name, "arguments": arguments },
    })
}

fn temp_db(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "knowlith-mcp-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("lake.sqlite")
}

#[test]
fn a_client_can_initialize_and_list_everything() {
    let db = temp_db("handshake");
    drop(lake_with_a_company(&db));

    let out = converse(
        &db,
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":3,"method":"prompts/list"}),
            json!({"jsonrpc":"2.0","id":4,"method":"resources/list"}),
        ],
    );

    // The notification was not answered.
    assert_eq!(out.len(), 4, "a notification was answered: {out:#?}");

    assert_eq!(out[0]["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(out[0]["result"]["serverInfo"]["name"], "knowlith");
    assert!(out[0]["result"]["instructions"].as_str().unwrap().contains("Termoval"));

    let tools = out[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 11);
    assert!(tools.iter().all(|t| t["icons"][0]["src"].is_string()));

    assert!(!out[2]["result"]["prompts"].as_array().unwrap().is_empty());
    assert_eq!(out[3]["result"]["resources"][0]["uri"], "knowlith://company");
}

#[test]
fn an_approved_rule_comes_back_with_its_source_and_its_foundation() {
    let db = temp_db("read");
    drop(lake_with_a_company(&db));

    let out = converse(&db, &[call("get_context", 1, json!({ "id": "rule:discount" }))]);
    let result = &out[0]["result"];

    assert_eq!(result["isError"], json!(false));
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("5% popusta"));
    assert!(text.contains("Uvjeti prodaje.md"), "no document name: {text}");
    assert!(text.contains("Popust za stalne kupce iznosi 5%."), "no quote: {text}");
    assert!(text.contains("Rests on"), "the threshold was not named: {text}");

    let structured = &result["structuredContent"]["results"][0];
    assert_eq!(structured["id"], "rule:discount");
    assert_eq!(structured["restsOn"][0], "rule:threshold");
    assert_eq!(structured["stale"], json!(false));

    // The source travels as a link, not as a pasted document.
    assert_eq!(result["content"][1]["type"], "resource_link");
    assert!(result["content"][1]["uri"].as_str().unwrap().starts_with("knowlith://document/"));
}

#[test]
fn an_unsettled_subject_is_named_and_never_answered() {
    let db = temp_db("conflict");
    drop(lake_with_a_company(&db));

    let out = converse(
        &db,
        &[
            call("get_context", 1, json!({ "id": "rule:terms" })),
            call("list_pending", 2, json!({})),
        ],
    );

    let text = out[0]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Rok plaćanja"), "the subject was hidden: {text}");
    assert!(
        !text.contains("30 dana"),
        "the unsettled answer leaked: {text}"
    );
    assert!(text.contains("open question"));

    let pending = out[1]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(pending.contains("Rok plaćanja"));
    assert!(!pending.contains("30 dana"));
}

#[test]
fn coverage_reports_what_the_agent_never_looked_at() {
    let db = temp_db("coverage");
    drop(lake_with_a_company(&db));

    let opened = converse(&db, &[call("get_relevant_context", 1, json!({ "question": "kakav popust odobravamo stalnim kupcima" }))]);
    let case_id = opened[0]["result"]["structuredContent"]["caseId"]
        .as_str()
        .unwrap()
        .to_string();
    let areas = opened[0]["result"]["structuredContent"]["areas"]
        .as_array()
        .unwrap();
    assert!(
        areas.iter().any(|a| a["id"] == "rule:threshold"),
        "the foundation was not listed: {areas:#?}"
    );
    // The map gives titles, not answers, or the agent stops at the map.
    assert!(!opened[0]["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("5% popusta"));

    // Read only one of the two, then close.
    let closed = converse(
        &db,
        &[
            call("get_context", 1, json!({ "id": "rule:discount", "caseId": case_id })),
            call("check_coverage", 2, json!({ "caseId": case_id })),
        ],
    );

    let verdict = &closed[1]["result"]["structuredContent"];
    assert_eq!(verdict["complete"], json!(false));
    assert_eq!(verdict["missed"][0]["id"], "rule:threshold");
    let text = closed[1]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Tko je stalan kupac"), "{text}");
    assert!(text.contains("did not look at"));
}

#[test]
fn coverage_is_clean_when_everything_was_read() {
    let db = temp_db("covered");
    drop(lake_with_a_company(&db));

    let opened = converse(&db, &[call("get_relevant_context", 1, json!({ "question": "popust stalnim kupcima" }))]);
    let case_id = opened[0]["result"]["structuredContent"]["caseId"]
        .as_str()
        .unwrap()
        .to_string();

    let out = converse(
        &db,
        &[
            call("get_context", 1, json!({ "id": "rule:discount", "caseId": case_id })),
            call("get_context", 2, json!({ "id": "rule:threshold", "caseId": case_id })),
            call("check_coverage", 3, json!({ "caseId": case_id, "summary": "5% uz prag 20.000 EUR" })),
        ],
    );

    assert_eq!(out[2]["result"]["structuredContent"]["complete"], json!(true));
}

#[test]
fn a_suggestion_without_a_real_quote_is_refused() {
    let db = temp_db("propose");
    let lake = lake_with_a_company(&db);
    let document = lake.documents().unwrap().first().unwrap().id.clone();
    drop(lake);

    let out = converse(
        &db,
        &[
            call(
                "propose_change",
                1,
                json!({
                    "title": "Izmišljeno pravilo",
                    "body": "Popust je 15%.",
                    "documentId": document,
                    "quote": "Popust za stalne kupce iznosi 15%."
                }),
            ),
            call(
                "propose_change",
                2,
                json!({
                    "title": "Rok plaćanja",
                    "body": "Rok je 30 dana.",
                    "documentId": document,
                    "quote": "Rok plaćanja je 30 dana."
                }),
            ),
        ],
    );

    assert_eq!(out[0]["result"]["isError"], json!(true), "an invented quote was accepted");
    assert_eq!(out[1]["result"]["isError"], json!(false));
    assert_eq!(out[1]["result"]["structuredContent"]["status"], "proposed");

    // And it landed in the owner's queue rather than in the served set.
    let lake = Lake::open(&db).unwrap();
    let written = lake.object("fact:agent.rok-placanja").unwrap().unwrap();
    assert_eq!(written.status, ObjectStatus::Proposed);
    assert_eq!(written.evidence.len(), 1);
}

#[test]
fn an_empty_lake_tells_the_agent_to_say_so() {
    let db = temp_db("empty");
    let out = converse(
        &db,
        &[
            call("search_context", 1, json!({ "question": "rok plaćanja" })),
            call("lookup_value", 2, json!({ "what": "montaža" })),
        ],
    );

    for message in &out {
        let text = message["result"]["content"][0]["text"].as_str().unwrap();
        assert!(
            text.to_lowercase().contains("not") || text.to_lowercase().contains("no row"),
            "{text}"
        );
    }
    assert!(
        out[1]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("do not estimate"),
        "a missing price must forbid inventing one"
    );
}

#[test]
fn a_broken_line_does_not_end_the_session() {
    let db = temp_db("broken");
    drop(lake_with_a_company(&db));

    let input = "{not json\n{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\n";
    let sink = Recorder::default();
    let writer = rpc::Writer::to(Box::new(sink.clone()));
    let mut reader = rpc::Reader::new(input.as_bytes());
    knowlith_mcp::run(
        Options { db, company: "Termoval".into() },
        writer,
        &mut reader,
    )
    .unwrap();

    let out = sink.messages();
    assert_eq!(out[0]["error"]["code"], -32700);
    assert_eq!(out[1]["result"], json!({}), "the session carried on");
}

#[test]
fn an_unknown_method_is_an_error_and_an_unknown_tool_is_a_refusal() {
    let db = temp_db("unknown");
    drop(lake_with_a_company(&db));

    let out = converse(
        &db,
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"resources/subscribe","params":{"uri":"x"}}),
            call("delete_everything", 2, json!({})),
        ],
    );

    // A method we do not implement is a protocol error, handled by the client.
    assert_eq!(out[0]["error"]["code"], -32601);
    // A tool that does not exist is a parameter error on a call we do
    // implement, which is a different thing and reported differently.
    assert_eq!(out[1]["error"]["code"], -32602);
}

#[test]
fn a_skill_is_offered_as_a_prompt_the_owner_can_type() {
    let db = temp_db("prompt");
    let mut lake = lake_with_a_company(&db);
    let document = lake.documents().unwrap().first().unwrap().id.clone();
    let text = lake.document(&document).unwrap().text;
    let quote = "Popust za stalne kupce iznosi 5%.";
    let start = text.find(quote).unwrap();

    lake.put_object(&ContextObject {
        id: "skill:odobravanje-popusta".into(),
        kind: ObjectKind::Skill,
        subtype: None,
        title: "Odobravanje popusta".into(),
        body: "# Odobravanje popusta\n\nProvjeri promet kupca, pa primijeni 5%.".into(),
        status: ObjectStatus::Approved,
        confidence: Confidence(0.8),
        version: 1,
        valid_from: "2026-01-01T00:00:00Z".into(),
        valid_to: None,
        supersedes: None,
        decided_by: Some("ana".into()),
        edited_on_approval: false,
        evidence: vec![Evidence {
            document_id: document,
            locator: "§1".into(),
            start_byte: start,
            end_byte: start + quote.len(),
            quote: quote.into(),
        }],
        relations: Vec::new(),
        path: "skills/odobravanje-popusta.md".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
    })
    .unwrap();
    drop(lake);

    let out = converse(
        &db,
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"prompts/list"}),
            json!({"jsonrpc":"2.0","id":2,"method":"prompts/get","params":{"name":"odobravanje-popusta","arguments":{"situation":"ACME, promet 30.000 EUR"}}}),
            call("get_skill", 3, json!({ "name": "Odobravanje popusta" })),
        ],
    );

    let names: Vec<&str> = out[0]["result"]["prompts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"odobravanje-popusta"),
        "the skill is not a slash command: {names:?}"
    );

    let message = out[1]["result"]["messages"][0]["content"]["text"].as_str().unwrap();
    assert!(message.contains("Provjeri promet kupca"));
    assert!(message.contains("ACME"), "the situation was dropped");
    assert!(message.contains("check_coverage"));

    assert!(out[2]["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Provjeri promet kupca"));
}

#[test]
fn the_company_card_can_be_read_as_a_resource() {
    let db = temp_db("card");
    drop(lake_with_a_company(&db));

    let out = converse(
        &db,
        &[json!({"jsonrpc":"2.0","id":1,"method":"resources/read","params":{"uri":"knowlith://company"}})],
    );

    let card = out[0]["result"]["contents"][0]["text"].as_str().unwrap();
    assert!(card.contains("Termoval d.o.o."));
    // A rule that rests on a threshold is not a standing rule, so it must
    // not be on the card without it.
    assert!(!card.contains("Popust za stalne kupce —"), "{card}");
    assert!(card.contains("Rok plaćanja"), "the open question is named: {card}");
}

/// Collects the lines the server writes, the way a client would read them.
#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<u8>>>);

impl Recorder {
    fn messages(&self) -> Vec<Value> {
        String::from_utf8(self.0.lock().unwrap().clone())
            .unwrap()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                serde_json::from_str(line)
                    .unwrap_or_else(|e| panic!("the server wrote a line that is not JSON: {line}\n{e}"))
            })
            .collect()
    }
}

impl Write for Recorder {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
