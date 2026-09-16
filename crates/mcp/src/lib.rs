//! The gateway: this company's approved knowledge, served to AI tools.
//!
//! One process per client, spawned by the client, speaking JSON-RPC over
//! stdio. No port, no token, no network. The trust boundary is the user
//! account on this machine, which is a boundary the operating system already
//! enforces and we cannot get wrong.
//!
//! It opens its own connection to the lake — a third, next to the interface
//! and the background worker. WAL is what makes that safe, and not sharing a
//! mutex with the interface is what keeps a long agent conversation from
//! freezing the review screen.
//!
//! Read-only, with one exception that proves the rule: `propose_change`
//! writes, and what it writes is `proposed`. An agent has no shorter path to
//! becoming company knowledge than the compiler has.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use knowlith_desktop::icon::company_icon;
use knowlith_lake::Lake;
use serde_json::{Value, json};

pub mod gate;
pub mod prompts;
pub mod resources;
pub mod rpc;
pub mod tools;

use rpc::{Incoming, Writer, code, log};

/// The revisions of the protocol this server understands.
///
/// Listed newest first. A client that asks for one of these gets it; a
/// client that asks for anything else is answered with the newest, which is
/// what the specification requires and is also the only behaviour that lets
/// a two-year-old client keep working.
const SUPPORTED: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// How often the lake is checked for approvals made elsewhere.
///
/// The owner approves a rule in the Knowlith window while a conversation is
/// open in Claude. Without this, that conversation keeps the old list until
/// the application is restarted, and the owner concludes the approval did
/// not work.
const WATCH_INTERVAL: Duration = Duration::from_secs(4);

pub struct Options {
    pub db: PathBuf,
    pub company: String,
}

/// What this conversation is, for as long as it lasts.
///
/// One process serves one client, so this is the whole of the per-session
/// state. It exists because `clientInfo` arrives once, in `initialize`, and
/// every read after that has to be attributed to it — without this the
/// owner's screen can say that eight things were served but never which
/// application asked.
#[derive(Default)]
struct Session {
    /// The slug of a recognised application, or `None` for a client that
    /// sent no name or one we do not know.
    app: Option<String>,
    /// Exactly what the client called itself, kept for the log so an
    /// unrecognised client can be added by name rather than guessed at.
    client: Option<String>,
}

/// Runs until the client closes stdin.
pub fn serve(options: Options) -> anyhow::Result<()> {
    let writer = Writer::stdout();
    let mut reader = rpc::Reader::new(std::io::stdin());
    run(options, writer, &mut reader)
}

/// The loop itself, with its streams supplied — which is what makes it
/// testable without a client.
pub fn run<R: std::io::Read>(
    options: Options,
    writer: Writer,
    reader: &mut rpc::Reader<R>,
) -> anyhow::Result<()> {
    if let Some(parent) = options.db.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut lake = Lake::open(&options.db).map_err(|e| {
        // The client will show this as a failed server. Saying which file
        // could not be opened is the difference between a five-minute fix
        // and a support conversation.
        log(&format!("could not open {}: {e}", options.db.display()));
        anyhow::anyhow!("could not open the lake at {}: {e}", options.db.display())
    })?;

    let icon = serde_json::to_value(company_icon(&options.company))?;
    let mut session = Session::default();
    let stop = Arc::new(AtomicBool::new(false));
    let watcher = spawn_watcher(&options.db, writer.clone(), Arc::clone(&stop));

    log(&format!(
        "serving {} from {}",
        options.company,
        options.db.display()
    ));

    let result = loop {
        let line = match reader.next_line() {
            Ok(Some(line)) => line,
            // Stdin closed: the client is gone and so are we. This is the
            // normal way a session ends, not a failure.
            Ok(None) => break Ok(()),
            Err(e) => break Err(anyhow::anyhow!("could not read from the client: {e}")),
        };

        let message = match rpc::parse(&line) {
            Ok(message) => message,
            Err((code, reason)) => {
                if writer.error(None, code, &reason).is_err() {
                    break Ok(());
                }
                continue;
            }
        };

        if handle(&mut lake, &options, &icon, &writer, &mut session, message).is_err() {
            // A write failure means the pipe is closed.
            break Ok(());
        }
    };

    stop.store(true, Ordering::Relaxed);
    if let Some(handle) = watcher {
        let _ = handle.join();
    }
    result
}

fn handle(
    lake: &mut Lake,
    options: &Options,
    icon: &Value,
    writer: &Writer,
    session: &mut Session,
    message: Incoming,
) -> std::io::Result<()> {
    // A notification is never answered, whatever it says. Answering one is
    // a protocol violation that some clients treat as fatal.
    if message.is_notification() {
        return Ok(());
    }
    let id = message.id.clone().unwrap_or(Value::Null);

    // A panic in one tool must not take the session with it. The client
    // would show "server crashed" and the owner would have no idea which of
    // their documents caused it, so it comes back as a refusal naming the
    // tool instead.
    let answered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        dispatch(lake, options, icon, session, &message)
    }));

    match answered {
        Ok(Ok(result)) => writer.result(&id, result),
        Ok(Err((code, reason))) => writer.error(Some(&id), code, &reason),
        Err(_) => {
            log(&format!("{} panicked", message.method));
            writer.error(
                Some(&id),
                code::INTERNAL_ERROR,
                &format!(
                    "{} failed inside Knowlith. Nothing was changed. The details are in the log next to your lake.",
                    message.method
                ),
            )
        }
    }
}

fn dispatch(
    lake: &mut Lake,
    options: &Options,
    icon: &Value,
    session: &mut Session,
    message: &Incoming,
) -> Result<Value, (i64, String)> {
    match message.method.as_str() {
        "initialize" => Ok(initialize(lake, options, session, message)),
        "ping" => Ok(json!({})),
        "logging/setLevel" => Ok(json!({})),

        "tools/list" => Ok(json!({ "tools": tools::catalogue(icon) })),
        "tools/call" => call_tool(lake, options, session, message),

        "prompts/list" => Ok(json!({ "prompts": prompts::list(lake, icon) })),
        "prompts/get" => {
            let name = message.required("name").map_err(invalid)?;
            let arguments = message.param("arguments").cloned().unwrap_or_else(|| json!({}));
            match prompts::get(lake, &name, &arguments, session.app.as_deref()) {
                Some((description, messages)) => {
                    Ok(json!({ "description": description, "messages": messages }))
                }
                None => Err((
                    code::INVALID_PARAMS,
                    format!("There is no prompt called \"{name}\"."),
                )),
            }
        }

        "resources/list" => Ok(json!({ "resources": resources::list(lake, &options.company, icon) })),
        "resources/templates/list" => Ok(json!({ "resourceTemplates": resources::templates() })),
        "resources/read" => {
            let uri = message.required("uri").map_err(invalid)?;
            match resources::read(lake, &options.company, &uri) {
                Some(contents) => Ok(json!({ "contents": contents })),
                None => Err((
                    code::INVALID_PARAMS,
                    format!("{uri} is not something this company serves."),
                )),
            }
        }

        other => Err((
            code::METHOD_NOT_FOUND,
            format!("this server does not implement {other}"),
        )),
    }
}

fn invalid(reason: String) -> (i64, String) {
    (code::INVALID_PARAMS, reason)
}

fn call_tool(
    lake: &mut Lake,
    options: &Options,
    session: &Session,
    message: &Incoming,
) -> Result<Value, (i64, String)> {
    let name = message.required("name").map_err(invalid)?;
    let arguments = message
        .param("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    if !tools::names().contains(&name.as_str()) {
        return Err((
            code::INVALID_PARAMS,
            format!("there is no tool called {name}"),
        ));
    }

    let outcome = tools::call(lake, &options.company, &name, &arguments, session.app.as_deref());

    let mut content = vec![json!({ "type": "text", "text": outcome.text })];
    content.extend(outcome.links);

    // A tool that refuses reports it inside a successful result. A protocol
    // error would be handled by the client and never reach the model, which
    // is the one participant that could do something about it.
    Ok(json!({
        "content": content,
        "structuredContent": outcome.structured,
        "isError": outcome.is_error,
    }))
}

fn initialize(lake: &Lake, options: &Options, session: &mut Session, message: &Incoming) -> Value {
    // Who is asking, for every read this session records from here on.
    // Unrecognised clients are logged by the name they gave rather than
    // folded into one of ours, so adding one later is a one-line change
    // made from evidence instead of a guess.
    session.client = message
        .param("clientInfo")
        .and_then(|info| info.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string);
    session.app = session
        .client
        .as_deref()
        .and_then(knowlith_desktop::App::from_client_name)
        .map(|app| app.slug().to_string());
    match (&session.app, &session.client) {
        (Some(app), _) => log(&format!("client is {app}")),
        (None, Some(name)) => log(&format!("client calls itself \"{name}\", which is not one we know")),
        (None, None) => log("client sent no name"),
    }

    let asked = message
        .param("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or("");
    let version = if SUPPORTED.contains(&asked) {
        asked
    } else {
        SUPPORTED[0]
    };

    json!({
        "protocolVersion": version,
        "capabilities": {
            // `listChanged` is not decoration here: the owner approves
            // things in another window while a conversation is open, and
            // this is how that conversation finds out.
            "tools": { "listChanged": true },
            "prompts": { "listChanged": true },
            "resources": { "listChanged": true, "subscribe": false },
        },
        "serverInfo": {
            // Same string Claude's connectors list shows for a local MCP
            // entry — the company name, not the product. Title carries the
            // fuller label for hosts that surface both.
            "name": knowlith_desktop::server_key(&options.company),
            "title": format!("{} — company knowledge", options.company),
            "version": env!("CARGO_PKG_VERSION"),
        },
        "instructions": instructions(lake, &options.company),
    })
}

/// What the model is told about this server before it uses it.
///
/// An MCP server the agent does not know when to reach for is a server that
/// gets used once, by accident. This is short on purpose — it competes for
/// attention with the user's own prompt — and it says the three things that
/// change behaviour: prefer this over guessing, take figures from the table,
/// and finish by checking what you missed.
fn instructions(lake: &Lake, company: &str) -> String {
    let profile = lake
        .setting("company_profile")
        .ok()
        .flatten()
        .unwrap_or_default();
    let about = match profile.trim() {
        "" => String::new(),
        text => format!("\n\nWhat this company is: {text}"),
    };

    let counts = lake.objects().map(|objects| {
        let approved = objects.iter().filter(|o| gate::is_servable(o)).count();
        let skills = objects
            .iter()
            .filter(|o| o.kind == knowlith_core::ObjectKind::Skill && gate::is_servable(o))
            .count();
        (approved, skills)
    });

    let scale = match counts {
        Ok((0, _)) => format!("{company} has not approved anything yet."),
        Ok((approved, 0)) => format!("{company} has approved {approved} rules, processes and terms."),
        Ok((approved, skills)) => format!(
            "{company} has approved {approved} rules, processes and terms, and {skills} procedures for you to run."
        ),
        Err(_) => format!("{company}'s knowledge is here."),
    };

    format!(
        "This server is {company}'s own knowledge, approved by its owner. {scale}{about}\n\n\
         Use it whenever a question touches how this company works — its prices, terms, procedures, \
         vocabulary or policies — instead of answering from general knowledge or from raw files. \
         What it returns is more authoritative than anything in the repository or the conversation.\n\n\
         Three rules:\n\
         - Start a real piece of work with get_relevant_context, which lists everything this company \
           has decided that touches it. Finish with check_coverage, which names what you never read. \
           Do not claim you checked the company's rules before it comes back clean.\n\
         - Take every figure from lookup_value, which reads the row out of the company's own table. \
           Never take a price from a sentence, never round one, and never interpolate between two.\n\
         - When something comes back as an open question, say it is open. Do not decide it. \
           An answer the owner has not settled is theirs to settle, not yours."
    )
}

/// Watches the lake for approvals made in another window.
///
/// Its own connection, because a SQLite connection belongs to one thread and
/// because a poll that had to wait for the request loop would only notice an
/// approval when the agent happened to ask something.
fn spawn_watcher(
    db: &std::path::Path,
    writer: Writer,
    stop: Arc<AtomicBool>,
) -> Option<std::thread::JoinHandle<()>> {
    let db = db.to_path_buf();
    let lake = match Lake::open(&db) {
        Ok(lake) => lake,
        Err(e) => {
            // Not fatal. Without the watcher the gateway still answers
            // everything correctly; it just will not announce changes.
            log(&format!("no change watcher: {e}"));
            return None;
        }
    };

    Some(std::thread::spawn(move || {
        let mut previous = lake.servable_revision().unwrap_or_default();
        while !stop.load(Ordering::Relaxed) {
            // Slept in short steps so shutdown is immediate. A four-second
            // sleep would hold the process open for four seconds after the
            // client went away.
            for _ in 0..(WATCH_INTERVAL.as_millis() / 200) {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(200));
            }

            let current = match lake.servable_revision() {
                Ok(revision) => revision,
                Err(_) => continue,
            };
            if current == previous {
                continue;
            }
            previous = current;

            log("the owner changed something — telling the client");
            for method in [
                "notifications/tools/list_changed",
                "notifications/prompts/list_changed",
                "notifications/resources/list_changed",
            ] {
                if writer.notify(method, json!({})).is_err() {
                    return;
                }
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_protocol_version_is_echoed_back() {
        let lake = Lake::in_memory().unwrap();
        let options = Options {
            db: PathBuf::new(),
            company: "Termoval".into(),
        };
        let message = rpc::parse(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        )
        .unwrap();
        let result = initialize(&lake, &options, &mut Session::default(), &message);
        assert_eq!(result["protocolVersion"], "2025-06-18");
    }

    #[test]
    fn an_unknown_protocol_version_gets_our_newest() {
        let lake = Lake::in_memory().unwrap();
        let options = Options {
            db: PathBuf::new(),
            company: "Termoval".into(),
        };
        let message = rpc::parse(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#,
        )
        .unwrap();
        assert_eq!(initialize(&lake, &options, &mut Session::default(), &message)["protocolVersion"], SUPPORTED[0]);
    }

    #[test]
    fn the_instructions_say_what_changes_behaviour() {
        let lake = Lake::in_memory().unwrap();
        let text = instructions(&lake, "Termoval d.o.o.");
        assert!(text.contains("lookup_value"));
        assert!(text.contains("check_coverage"));
        assert!(text.contains("open question"));
        assert!(text.contains("Termoval d.o.o."));
    }

    #[test]
    fn the_instructions_include_what_the_company_is() {
        let lake = Lake::in_memory().unwrap();
        lake.set_setting("company_profile", "HVAC installer for Croatian SMBs.")
            .unwrap();
        let text = instructions(&lake, "Termoval");
        assert!(text.contains("What this company is: HVAC installer for Croatian SMBs."));
    }

    #[test]
    fn an_empty_company_is_described_honestly() {
        let lake = Lake::in_memory().unwrap();
        assert!(instructions(&lake, "Termoval").contains("has not approved anything yet"));
    }

    #[test]
    fn change_notifications_cover_all_three_lists() {
        // Guards against adding a capability and forgetting to announce it:
        // a skill approved while a conversation is open changes the prompt
        // list, not just the tool list.
        let capabilities = {
            let lake = Lake::in_memory().unwrap();
            let options = Options {
                db: PathBuf::new(),
                company: "Termoval".into(),
            };
            let message =
                rpc::parse(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#).unwrap();
            initialize(&lake, &options, &mut Session::default(), &message)["capabilities"].clone()
        };
        for surface in ["tools", "prompts", "resources"] {
            assert_eq!(capabilities[surface]["listChanged"], json!(true), "{surface}");
        }
    }
}
