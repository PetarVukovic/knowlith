//! `knowlith` — the daemon's command line, and for now the whole of it.
//!
//! Everything here runs against a real folder and a real SQLite file. There
//! is no model in this binary: it reads, it stores, it answers questions
//! about what it stored. That is deliberate — the part of the product that
//! has to be trustworthy is the part that can be checked without one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use knowlith_engine::{Breaker, CliEngine, Engine, Flavour, ManagedEngine, RecordingEngine, ReplayEngine, Request, detect};
use knowlith_extract::{ExtractError, extract_file, is_noise};
use knowlith_compiler::Compilation;
use knowlith_graph::Graph;
use knowlith_lake::Lake;
use knowlith_server::AppState;
use knowlith_worker::Worker;
use walkdir::WalkDir;

#[derive(Parser)]
#[command(name = "knowlith", version, about = "Your company knowledge, compiled for every AI.")]
struct Cli {
    /// The Context Lake. Defaults to ~/Knowlith/data/lake.sqlite.
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read a folder into the lake.
    Scan {
        folder: PathBuf,
        /// Name this source, so several folders stay apart.
        #[arg(long, default_value = "default")]
        source: String,
        /// Count and classify without storing anything.
        #[arg(long)]
        dry_run: bool,
    },
    /// Search the stored text. Diacritics do not matter.
    Search {
        query: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Print a document, block by block, with its offsets.
    Show { document_id: String },
    /// What breaks if this object changes.
    Impact { object_id: String },
    /// What this object rests on.
    Foundations { object_id: String },
    /// Re-check every stored quote, and report anything the graph got wrong.
    Doctor,
    /// What is in the lake.
    Status,
    /// Which AI tools this machine can run.
    Engines,
    /// Turn the documents in the lake into rules, processes and terms.
    Compile {
        #[arg(long, default_value = "claude")]
        engine: String,
        /// Write every engine reply into this folder, so the run can be
        /// replayed later without a provider.
        #[arg(long)]
        record: Option<PathBuf>,
        /// Replay a folder of recorded replies instead of calling anything.
        #[arg(long)]
        replay: Option<PathBuf>,
        /// Compile without storing, to see what it would produce.
        #[arg(long)]
        dry_run: bool,
    },
    /// Serve the local HTTP API the interface talks to.
    Serve {
        #[arg(long, default_value_t = 7717)]
        port: u16,
        #[arg(long, default_value = "Termoval d.o.o.")]
        company: String,
        /// Which engine the background worker uses.
        #[arg(long, default_value = "claude")]
        engine: String,
        /// Replay recorded replies instead of calling an engine. The whole
        /// loop then runs with no provider and no cost.
        #[arg(long)]
        replay: Option<PathBuf>,
        /// Serve without the background worker. Then nothing happens on its
        /// own, which is worth being explicit about.
        #[arg(long)]
        no_worker: bool,
    },
    /// Drain the work queue: rescan folders, compile what moved, recheck quotes.
    ///
    /// This is what makes "you can close this window — it keeps going" true.
    /// Stopping it is safe at any moment: an unfinished job's lease expires
    /// and the next start picks it up.
    Work {
        #[arg(long, default_value = "claude")]
        engine: String,
        #[arg(long)]
        replay: Option<PathBuf>,
        /// Run until the queue is empty, then stop.
        #[arg(long)]
        once: bool,
    },
    /// Draft a skill for every approved process.
    ///
    /// Skills are the one thing an AI tool executes rather than reads, so
    /// they are built only from knowledge the owner has already approved,
    /// and a draft that states a figure no approved rule states is refused.
    Skills {
        #[arg(long, default_value = "claude")]
        engine: String,
        #[arg(long)]
        record: Option<PathBuf>,
        #[arg(long)]
        replay: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Work out which objects depend on which.
    ///
    /// Structural detection only sees one object's title quoted inside
    /// another's text, which on a real folder finds almost nothing. This
    /// asks the engine, once, about the whole set. Every edge it proposes is
    /// marked as the engine's rather than as structure, and can be removed.
    Relate {
        #[arg(long, default_value = "claude")]
        engine: String,
        #[arg(long)]
        record: Option<PathBuf>,
        #[arg(long)]
        replay: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Objects that may be one thing written twice.
    Merges,
    /// Fold the second object into the first, keeping both readable.
    Merge { keep: String, drop: String },
    /// Say no to a pair, once and for all.
    Dismiss { keep: String, drop: String },
    /// Send one document to an engine and print what comes back.
    ///
    /// This is the child-process path end to end, and the quickest way to
    /// find out whether an owner's CLI is installed, signed in and answering.
    Ask {
        /// What the engine should do.
        instructions: String,
        /// A file to read and send along with the instructions.
        #[arg(long)]
        document: Option<PathBuf>,
        #[arg(long, default_value = "codex")]
        engine: String,
        #[arg(long, default_value_t = 120)]
        timeout: u64,
    },
    /// Serve this company's approved knowledge to an AI tool over stdio.
    ///
    /// Not run by hand. An AI application starts this, speaks JSON-RPC on
    /// stdin and stdout, and stops it when it closes. Everything a person
    /// would want to read goes to stderr, because a single stray line on
    /// stdout ends the session.
    Mcp {
        #[arg(long, default_value = "Termoval d.o.o.")]
        company: String,
    },
    /// Hand Knowlith to the AI applications on this machine.
    Connect {
        /// `claude-desktop`, `claude-code` or `codex`. All of them when omitted.
        app: Option<String>,
        /// Show what would be written, and write nothing.
        #[arg(long)]
        dry_run: bool,
        /// Also open the application, restarting it if it needs that.
        #[arg(long)]
        open: bool,
        /// Also write the standing instruction that tells the agent when to
        /// reach for the company.
        #[arg(long, default_value_t = true)]
        guidance: bool,
        #[arg(long, default_value = "Termoval d.o.o.")]
        company: String,
    },
    /// Take Knowlith back out of an application's settings.
    Disconnect {
        app: Option<String>,
    },
    /// Which AI applications can see this company, and which cannot.
    Tools,
    /// Build the Claude Desktop extension, so it can be installed with a
    /// double-click instead of by editing a configuration file.
    Bundle {
        #[arg(long, default_value = "Termoval d.o.o.")]
        company: String,
        /// Open it, which is what shows Claude Desktop's install screen.
        #[arg(long)]
        install: bool,
    },
    /// Keep the background service running when the window is closed.
    Autostart {
        #[command(subcommand)]
        what: AutostartCommand,
    },
}

#[derive(Subcommand)]
enum AutostartCommand {
    /// Register with this machine's login service and start now.
    On {
        #[arg(long, default_value_t = 7717)]
        port: u16,
    },
    /// Remove the registration and stop.
    Off,
    /// Whether it is registered, and whether it is answering.
    Show,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let db = cli.db.unwrap_or_else(default_db);
    if let Some(parent) = db.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    let mut lake = Lake::open(&db).with_context(|| format!("could not open {}", db.display()))?;

    match cli.command {
        Command::Scan { folder, source, dry_run } => scan(&mut lake, &folder, &source, dry_run),
        Command::Search { query, limit } => search(&lake, &query, limit),
        Command::Show { document_id } => show(&lake, &document_id),
        Command::Impact { object_id } => impact(&lake, &object_id, false),
        Command::Foundations { object_id } => impact(&lake, &object_id, true),
        Command::Doctor => doctor(&lake),
        Command::Status => status(&lake, &db),
        Command::Engines => engines(),
        Command::Ask {
            instructions,
            document,
            engine,
            timeout,
        } => ask(&instructions, document.as_deref(), &engine, timeout),
        Command::Compile {
            engine,
            record,
            replay,
            dry_run,
        } => run_compile(&mut lake, &engine, record.as_deref(), replay.as_deref(), dry_run),
        Command::Serve {
            port,
            company,
            engine,
            replay,
            no_worker,
        } => serve(lake, db, port, &company, &engine, replay.as_deref(), no_worker),
        Command::Work { engine, replay, once } => work(db, &engine, replay.as_deref(), once),
        Command::Skills {
            engine,
            record,
            replay,
            dry_run,
        } => run_skills(&mut lake, &engine, record.as_deref(), replay.as_deref(), dry_run),
        Command::Relate {
            engine,
            record,
            replay,
            dry_run,
        } => relate(&lake, &engine, record.as_deref(), replay.as_deref(), dry_run),
        Command::Merges => merges(&lake),
        Command::Merge { keep, drop } => {
            lake.merge_objects(&keep, &drop)?;
            println!("{drop} folded into {keep}; both stay readable");
            Ok(())
        }
        Command::Dismiss { keep, drop } => {
            lake.dismiss_merge_hint(&keep, &drop)?;
            println!("noted — that pair will not be offered again");
            Ok(())
        }
        Command::Mcp { company } => {
            drop(lake);
            knowlith_mcp::serve(knowlith_mcp::Options { db, company })
        }
        Command::Connect {
            app,
            dry_run,
            open,
            guidance,
            company,
        } => connect(app.as_deref(), dry_run, open, guidance, &company),
        Command::Disconnect { app } => disconnect(app.as_deref()),
        Command::Tools => tools_status(),
        Command::Bundle { company, install } => bundle(&lake, &company, install),
        Command::Autostart { what } => autostart(what),
    }
}

fn pick_engine(name: &str) -> Result<Box<dyn Engine>> {
    Ok(match name {
        "codex" => Box::new(Breaker::new(CliEngine::new(Flavour::Codex))),
        "claude" | "claude-code" => Box::new(Breaker::new(CliEngine::new(Flavour::ClaudeCode))),
        "managed" => Box::new(ManagedEngine),
        other => anyhow::bail!("unknown engine \"{other}\". Try codex, claude or managed."),
    })
}

fn run_compile(
    lake: &mut Lake,
    engine_name: &str,
    record: Option<&Path>,
    replay: Option<&Path>,
    dry_run: bool,
) -> Result<()> {
    let documents = lake.documents()?;
    if documents.is_empty() {
        println!("nothing to compile — run `knowlith scan <folder>` first");
        return Ok(());
    }

    let engine: Box<dyn Engine> = match (replay, record) {
        // Replaying beats recording: asking for both means "re-record only
        // what is missing", and that is not what either flag says.
        (Some(dir), _) => Box::new(ReplayEngine::new(dir)),
        (None, Some(dir)) => Box::new(RecordingEngine::new(
            pick_engine(engine_name)?,
            dir.to_path_buf(),
        )),
        (None, None) => pick_engine(engine_name)?,
    };

    println!("reading {} documents with {}…", documents.len(), engine.name());

    // Read each document once, keep what the engine said, then settle the
    // whole set. Identical to what the background worker does, and stored
    // the same way, so the two cannot produce different lakes from the same
    // folder.
    let mut proposals: Vec<(String, knowlith_compiler::Candidate)> = Vec::new();
    let mut out = Compilation::default();
    for document in &documents {
        let mut read = knowlith_compiler::Reading::default();
        knowlith_compiler::read_one(engine.as_ref(), document, &mut read)?;
        out.documents_read += read.documents_read;
        out.dropped.extend(read.dropped);

        if !dry_run {
            let json: Vec<String> = read
                .candidates
                .iter()
                .map(serde_json::to_string)
                .collect::<std::result::Result<_, _>>()?;
            lake.put_candidates(&document.id, &json)?;
        }
        for candidate in read.candidates {
            proposals.push((document.id.clone(), candidate));
        }
    }

    let settled = knowlith_compiler::settle(&proposals, &documents);
    out.candidates_proposed = settled.candidates_proposed;
    out.objects = settled.objects;
    out.conflicts = settled.conflicts;
    out.dropped.extend(settled.dropped);

    report(&out);

    if dry_run {
        println!("\n  nothing was stored (--dry-run)");
        return Ok(());
    }

    // The gate runs again on write. An object that passed stage 4 and fails
    // here means the two checks disagree, which is worth shouting about.
    let mut stored = 0;
    for object in &out.objects {
        match lake.put_object(object) {
            Ok(()) => stored += 1,
            Err(e) => println!("  could not store {}: {e}", object.id),
        }
    }
    // The same step the worker takes after a compile. Running it in one
    // path and not the other would make `knowlith compile` and the
    // background worker produce different lakes from the same folder.
    let all = lake.objects()?;
    let mut asked = 0;
    for hint in knowlith_compiler::hints(&all) {
        if lake.put_merge_hint(&hint.keep_id, &hint.drop_id, hint_kind(hint.kind), hint.score)? {
            asked += 1;
        }
    }

    println!("\n  {stored} objects in the lake, waiting for review");
    if asked > 0 {
        println!("  {asked} pairs may be one thing written twice — `knowlith merges`");
    }
    Ok(())
}

fn report(out: &Compilation) {
    println!(
        "  {} documents read · {} claims proposed · {} kept · {} dropped",
        out.documents_read,
        out.candidates_proposed,
        out.objects.len(),
        out.dropped.len()
    );

    if !out.conflicts.is_empty() {
        println!("\n  {} conflicts:", out.conflicts.len());
        for conflict in &out.conflicts {
            println!("    {}", conflict.subject);
            for side in &conflict.sides {
                println!(
                    "      {} {:<28} {}",
                    if side.current { "→" } else { " " },
                    side.value,
                    side.document_name
                );
            }
        }
    }

    if !out.objects.is_empty() {
        println!("\n  found:");
        for object in &out.objects {
            println!(
                "    {:<44} {} spans · {:.2}",
                object.id,
                object.evidence.len(),
                object.confidence.0
            );
        }
    }

    if !out.dropped.is_empty() {
        println!("\n  not kept:");
        for dropped in &out.dropped {
            let what = if dropped.title.is_empty() {
                dropped.document.clone()
            } else {
                format!("{} ({})", dropped.title, dropped.document)
            };
            println!("    {what} — {}", dropped.reason);
        }
    }
}

/// Serves the interface, and unless told not to, works in the background
/// while it does.
///
/// The worker gets its own connection to the same SQLite file rather than
/// sharing the server's. A compile run holds a model for minutes, and behind
/// one mutex that would freeze every screen; WAL exists for exactly this.
fn serve(
    lake: Lake,
    db: PathBuf,
    port: u16,
    company: &str,
    engine_name: &str,
    replay: Option<&Path>,
    no_worker: bool,
) -> Result<()> {
    let stop = Arc::new(AtomicBool::new(false));

    let worker = if no_worker {
        println!("the background worker is off — nothing will happen on its own");
        None
    } else {
        let engine = worker_engine(engine_name, replay)?;
        let mut worker = Worker::open(&db, engine)?;
        let flag = Arc::clone(&stop);
        println!("working in the background with {}", worker_label(engine_name, replay));
        Some(std::thread::spawn(move || {
            if let Err(e) = worker.run(flag) {
                eprintln!("the background worker stopped: {e}");
            }
        }))
    };

    let state = AppState::new(lake, company);
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let result = runtime.block_on(knowlith_server::serve(state, port));

    stop.store(true, Ordering::Relaxed);
    if let Some(handle) = worker {
        let _ = handle.join();
    }
    result
}

/// Runs the queue down, either once or until stopped.
fn work(db: PathBuf, engine_name: &str, replay: Option<&Path>, once: bool) -> Result<()> {
    let engine = worker_engine(engine_name, replay)?;
    let mut worker = Worker::open(&db, engine)?;
    println!("working with {}", worker_label(engine_name, replay));

    if !once {
        // Stopping this is safe at any moment. There is no goodbye: the
        // process ends, the lease on whatever it was doing expires, and the
        // next start picks the job up with one more attempt against it.
        return Ok(worker.run(Arc::new(AtomicBool::new(false)))?);
    }

    let stop = AtomicBool::new(false);
    let mut did = 0;
    loop {
        let tick = worker.tick(&stop)?;
        if tick.scheduled > 0 {
            println!("  {} more queued", tick.scheduled);
        }
        match (&tick.ran, &tick.outcome) {
            (Some(kind), Some(note)) => {
                did += 1;
                println!("  {kind}: {note}");
            }
            _ => break,
        }
    }
    println!("\n  {did} jobs done, queue empty");
    Ok(())
}

fn worker_engine(name: &str, replay: Option<&Path>) -> Result<Arc<dyn Engine>> {
    Ok(match replay {
        Some(dir) => Arc::new(ReplayEngine::new(dir)),
        None => Arc::from(pick_engine(name)?),
    })
}

fn worker_label(name: &str, replay: Option<&Path>) -> String {
    match replay {
        Some(dir) => format!("recorded replies from {}", dir.display()),
        None => name.to_string(),
    }
}

/// Drafts skills from what the owner has approved.
fn run_skills(
    lake: &mut Lake,
    engine_name: &str,
    record: Option<&Path>,
    replay: Option<&Path>,
    dry_run: bool,
) -> Result<()> {
    let objects = lake.objects()?;
    let approved = objects
        .iter()
        .filter(|o| o.status == knowlith_core::ObjectStatus::Approved && o.kind == knowlith_core::ObjectKind::Process)
        .count();
    if approved == 0 {
        println!(
            "no approved process to build a skill from yet — approve one in the review queue first"
        );
        return Ok(());
    }

    let engine: Box<dyn Engine> = match (replay, record) {
        (Some(dir), _) => Box::new(ReplayEngine::new(dir)),
        (None, Some(dir)) => Box::new(RecordingEngine::new(pick_engine(engine_name)?, dir.to_path_buf())),
        (None, None) => pick_engine(engine_name)?,
    };

    println!("drafting skills from {approved} approved processes with {}…", engine.name());
    let run = knowlith_compiler::draft_all(engine.as_ref(), &objects)?;

    for skill in &run.skills {
        println!(
            "  {:<40} rests on {} objects · {:.2}",
            skill.id,
            skill.relations.len(),
            skill.confidence.0
        );
    }
    for dropped in &run.dropped {
        println!("  {} — {}", dropped.title, dropped.reason);
    }

    if dry_run {
        println!("\n  nothing was stored (--dry-run)");
        return Ok(());
    }

    let mut stored = 0;
    for skill in &run.skills {
        match lake.put_object(skill) {
            Ok(()) => stored += 1,
            Err(e) => println!("  could not store {}: {e}", skill.id),
        }
    }
    println!("\n  {stored} skills waiting for review");
    Ok(())
}

/// The word stored in the lake for a hint's kind.
fn hint_kind(kind: knowlith_compiler::HintKind) -> &'static str {
    match kind {
        knowlith_compiler::HintKind::Duplicate => "duplicate",
        knowlith_compiler::HintKind::Disagreement => "disagreement",
    }
}

/// Proposes the dependency edges a string match cannot find.
fn relate(
    lake: &Lake,
    engine_name: &str,
    record: Option<&Path>,
    replay: Option<&Path>,
    dry_run: bool,
) -> Result<()> {
    let objects = lake.objects()?;
    if objects.len() < 2 {
        println!("there is nothing to relate yet — run `knowlith compile` first");
        return Ok(());
    }

    let engine: Box<dyn Engine> = match (replay, record) {
        (Some(dir), _) => Box::new(ReplayEngine::new(dir)),
        (None, Some(dir)) => Box::new(RecordingEngine::new(pick_engine(engine_name)?, dir.to_path_buf())),
        (None, None) => pick_engine(engine_name)?,
    };

    println!("looking at {} objects with {}…", objects.len(), engine.name());
    let run = knowlith_compiler::propose_relations(engine.as_ref(), &objects)?;

    let titles: std::collections::HashMap<&str, &str> =
        objects.iter().map(|o| (o.id.as_str(), o.title.as_str())).collect();
    for edge in &run.edges {
        println!(
            "  {} → {}",
            titles.get(edge.from.as_str()).unwrap_or(&edge.from.as_str()),
            titles.get(edge.to.as_str()).unwrap_or(&edge.to.as_str())
        );
        println!("      {}", edge.why);
    }
    for dropped in &run.dropped {
        println!("  not used — {}", dropped.reason);
    }

    if dry_run {
        println!("\n  nothing was stored (--dry-run)");
        return Ok(());
    }

    let mut added = 0;
    for edge in &run.edges {
        for (from, to, kind, origin) in knowlith_compiler::edge_pair(edge) {
            if lake.put_relation(&from, &to, kind, origin)? {
                added += 1;
            }
        }
    }
    println!("\n  {added} edges in the graph, all marked as suggested");
    Ok(())
}

fn merges(lake: &Lake) -> Result<()> {
    let hints = lake.open_merge_hints()?;
    if hints.is_empty() {
        println!("nothing looks like a duplicate");
        return Ok(());
    }
    println!("{} pairs need one answer from you:\n", hints.len());
    for hint in &hints {
        let label = if hint.kind == "disagreement" {
            "DISAGREE"
        } else {
            "same?   "
        };
        println!("  {label} {:.0}%  {}", hint.score * 100.0, hint.left_title);
        println!("        {}", hint.left_id);
        println!("        {}", hint.right_title);
        println!("        {}\n", hint.right_id);
    }
    println!("knowlith merge <keep> <drop>    folds the second into the first");
    println!("knowlith dismiss <keep> <drop>  says they are different, once and for all");
    Ok(())
}

fn engines() -> Result<()> {
    for found in detect() {
        match (&found.path, &found.version) {
            (Some(path), Some(version)) => {
                println!("{:<14} {version}", found.label);
                println!("               {path}");
            }
            (Some(path), None) => {
                println!("{:<14} installed, but did not answer --version", found.label);
                println!("               {path}");
            }
            (None, _) => println!("{:<14} not installed", found.label),
        }
    }
    println!("{:<14} not available in this build", "Managed");
    Ok(())
}

fn ask(instructions: &str, document: Option<&Path>, engine: &str, timeout: u64) -> Result<()> {
    let text = match document {
        Some(path) => {
            // Through the extractor, not through `read_to_string`: a PDF or a
            // spreadsheet has to arrive as the same rendition the evidence
            // gate will later check spans against.
            let doc = extract_file(path)
                .with_context(|| format!("could not read {}", path.display()))?;
            eprintln!(
                "{} · {} blocks · {}",
                doc.name,
                doc.blocks.len(),
                human_bytes(doc.byte_len)
            );
            doc.text
        }
        None => String::new(),
    };

    // The breaker is here even for one request, so this command behaves the
    // way the worker will rather than being a separate path that works.
    let engine: Box<dyn Engine> = match engine {
        "codex" => Box::new(Breaker::new(CliEngine::new(Flavour::Codex))),
        "claude" | "claude-code" => Box::new(Breaker::new(CliEngine::new(Flavour::ClaudeCode))),
        "managed" => Box::new(ManagedEngine),
        other => anyhow::bail!("unknown engine \"{other}\". Try codex, claude or managed."),
    };

    let request = Request::new("ask", instructions, text)
        .with_timeout(std::time::Duration::from_secs(timeout));

    match engine.run(&request) {
        Ok(reply) => {
            eprintln!("— {} —", reply.engine);
            println!("{}", reply.text);
            Ok(())
        }
        Err(e) => {
            // The distinction the queue acts on, said out loud.
            eprintln!(
                "{e}\n{}",
                if e.is_retryable() {
                    "This would be retried automatically."
                } else {
                    "This would stop and wait for you."
                }
            );
            std::process::exit(1);
        }
    }
}

/// The lake lives beside the knowledge, in a folder the owner can open,
/// back up and take with them.
///
/// Resolved through `knowlith-desktop` rather than from `HOME`, which does
/// not exist on Windows and is set to a private root by Git Bash.
fn default_db() -> PathBuf {
    knowlith_desktop::paths::lake_db()
}

fn scan(lake: &mut Lake, folder: &Path, source: &str, dry_run: bool) -> Result<()> {
    let folder = folder
        .canonicalize()
        .with_context(|| format!("no folder at {}", folder.display()))?;

    if !dry_run {
        lake.put_source(
            source,
            folder.file_name().and_then(|n| n.to_str()).unwrap_or(source),
            &folder.to_string_lossy(),
            "folder",
            "codex",
        )?;
    }

    let mut read = 0usize;
    let mut blocks = 0usize;
    let mut unchanged = 0usize;
    let mut bytes = 0u64;
    // Reasons, not a count: "412 files skipped" tells the owner nothing they
    // can act on, and "132 are scans with no text layer" tells them exactly
    // what to fix.
    let mut skipped: BTreeMap<String, usize> = BTreeMap::new();

    for entry in WalkDir::new(&folder).follow_links(false).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy();
        if is_noise(&name) {
            *skipped.entry("working copies and system files".into()).or_default() += 1;
            continue;
        }

        match extract_file(path) {
            Ok(doc) => {
                let known = !dry_run && lake.has_content(&doc.sha256)?;
                bytes += doc.byte_len;
                blocks += doc.blocks.len();
                read += 1;
                if !dry_run {
                    lake.put_document(source, &doc)?;
                    // Reading a folder is not the same as understanding it.
                    // Queueing the work here is what lets the owner add a
                    // source and walk away: `knowlith work` picks it up, and
                    // so does the worker inside `knowlith serve`.
                    if known {
                        unchanged += 1;
                    } else {
                        knowlith_worker::enqueue_compile(lake, &doc)?;
                    }
                }
            }
            Err(ExtractError::UnsupportedType) => {
                *skipped.entry("file types Knowlith does not read".into()).or_default() += 1;
            }
            Err(ExtractError::NoText) => {
                *skipped
                    .entry("documents with no text, usually scans".into())
                    .or_default() += 1;
            }
            Err(ExtractError::Empty) => {
                *skipped.entry("empty files".into()).or_default() += 1;
            }
            Err(e) => {
                *skipped.entry(format!("could not be read: {e}")).or_default() += 1;
            }
        }
    }

    if !dry_run {
        lake.mark_scanned(source)?;
    }

    println!("{}", folder.display());
    println!("  {read} documents read · {blocks} blocks · {}", human_bytes(bytes));
    if unchanged > 0 {
        println!("  {unchanged} unchanged since the last scan");
    }
    for (reason, count) in &skipped {
        println!("  {count} skipped — {reason}");
    }
    if dry_run {
        println!("\n  nothing was stored (--dry-run)");
    } else {
        // Recorded, so the background worker does not immediately walk the
        // same folder again on the theory that nobody ever has.
        lake.mark_scanned(source)?;
        println!("\n  {} documents in the lake", lake.document_count()?);
        let waiting = lake.job_counts()?;
        let queued: i64 = waiting.iter().filter(|(s, _)| s == "queued").map(|(_, n)| n).sum();
        if queued > 0 {
            println!("  {queued} queued for reading — `knowlith work` or `knowlith serve`");
        }
    }
    Ok(())
}

fn search(lake: &Lake, query: &str, limit: usize) -> Result<()> {
    let hits = lake.search(query, limit)?;
    if hits.is_empty() {
        println!("nothing matches \"{query}\"");
        return Ok(());
    }
    for (document_id, locator, text) in hits {
        let name = lake
            .document(&document_id)
            .map(|d| d.name)
            .unwrap_or(document_id);
        println!("{name} · {locator}");
        println!("  {}", truncate(&text, 160));
    }
    Ok(())
}

fn show(lake: &Lake, document_id: &str) -> Result<()> {
    let doc = lake.document(document_id)?;
    println!("{}", doc.path);
    println!(
        "  {:?} · {} · {} blocks · offsets are {}",
        doc.kind,
        human_bytes(doc.byte_len),
        doc.blocks.len(),
        if doc.verbatim {
            "into the file itself"
        } else {
            "into the extracted text"
        }
    );
    if let Some(columns) = &doc.columns {
        println!("  columns: {}", columns.join(" | "));
    }
    println!();
    for block in &doc.blocks {
        println!(
            "  {:<18} {:>6}–{:<6} {}",
            block.locator,
            block.start_byte,
            block.end_byte,
            truncate(&block.text, 90)
        );
    }
    Ok(())
}

fn impact(lake: &Lake, object_id: &str, backwards: bool) -> Result<()> {
    let graph = Graph::build(&lake.edges()?);
    let hits = if backwards {
        graph.foundations(object_id)
    } else {
        graph.impact(object_id)
    };

    if hits.is_empty() {
        println!(
            "{object_id} — {}",
            if backwards {
                "stands on its own"
            } else {
                "nothing else uses this yet"
            }
        );
        return Ok(());
    }

    println!(
        "{object_id} — {} {}",
        hits.len(),
        if backwards { "it rests on" } else { "would be affected" }
    );
    for hit in hits {
        println!("  {} hop{}  {}", hit.distance, if hit.distance == 1 { " " } else { "s" }, hit.id);
        println!("           via {}", hit.path.join(" → "));
    }
    Ok(())
}

fn doctor(lake: &Lake) -> Result<()> {
    let broken = lake.recheck_evidence()?;
    if broken.is_empty() {
        println!("evidence: every stored quote still matches its document");
    } else {
        println!("evidence: {} spans no longer match", broken.len());
        for (object_id, rejection) in &broken {
            println!("  {object_id} — {rejection:?}");
        }
    }

    let graph = Graph::build(&lake.edges()?);
    let cycles = graph.cycles();
    if cycles.is_empty() {
        println!("graph: {} objects, {} edges, no cycles", graph.len(), graph.edge_count());
    } else {
        println!("graph: {} groups of objects justify each other", cycles.len());
        for group in cycles {
            println!("  {}", group.join(" ↔ "));
        }
    }

    for (state, count) in lake.job_counts()? {
        println!("jobs: {count} {state}");
    }
    Ok(())
}

fn status(lake: &Lake, db: &Path) -> Result<()> {
    println!("{}", db.display());
    println!("  {} documents · {} blocks", lake.document_count()?, lake.block_count()?);
    let approved = lake.object_ids(Some("approved"))?.len();
    let proposed = lake.object_ids(Some("proposed"))?.len();
    println!("  {approved} approved · {proposed} waiting for review");
    let graph = Graph::build(&lake.edges()?);
    println!("  graph: {} objects, {} edges", graph.len(), graph.edge_count());
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let cut: String = flat.chars().take(max).collect();
    format!("{cut}…")
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

// ------------------------------------------------------- AI applications --

/// Hands Knowlith to one application, or to every one that is installed.
///
/// Reports per application rather than as one success or failure: a machine
/// with Claude Desktop and no Codex is the normal case, and "connected 1 of
/// 3" reads as a failure when it is the right answer.
fn connect(
    app: Option<&str>,
    dry_run: bool,
    open: bool,
    guidance: bool,
    company: &str,
) -> Result<()> {
    let targets = chosen(app)?;

    for target in targets {
        let status = knowlith_desktop::status(target);
        if !status.installed {
            println!("{} — not on this machine", target.label());
            continue;
        }

        if dry_run {
            println!("{} — would write into {}", target.label(), status.config_path.unwrap_or_default());
            println!("{}", indent(&knowlith_desktop::connect::manual_instructions(target)));
            continue;
        }

        match knowlith_desktop::connect(target) {
            Ok(now) => {
                println!("{} — connected", target.label());
                println!("  {}", now.config_path.unwrap_or_default());
                println!("  {}", target.refresh_hint());
            }
            Err(e) => {
                println!("{} — not connected: {e}", target.label());
                println!("{}", indent(&knowlith_desktop::connect::manual_instructions(target)));
                continue;
            }
        }

        if guidance {
            let guide = match target {
                knowlith_desktop::App::Codex => Some(knowlith_desktop::Guide::Codex),
                knowlith_desktop::App::ClaudeCode => Some(knowlith_desktop::Guide::ClaudeCode),
                // Claude Desktop has no standing-instructions file; what it
                // reads is the server's own `instructions`, which it gets
                // anyway.
                knowlith_desktop::App::ClaudeDesktop => None,
            };
            if let Some(guide) = guide {
                match knowlith_desktop::guidance::write(guide, company) {
                    Ok(path) => println!("  told {} when to use it — {}", guide.label(), path.display()),
                    Err(e) => println!("  could not write the standing instruction: {e}"),
                }
            }
        }

        if open {
            let outcome = knowlith_desktop::open_or_restart(target);
            println!("  {}", outcome.message(target));
        }
    }
    Ok(())
}

fn disconnect(app: Option<&str>) -> Result<()> {
    for target in chosen(app)? {
        match knowlith_desktop::disconnect(target) {
            Ok(_) => println!("{} — removed", target.label()),
            Err(e) => println!("{} — could not remove: {e}", target.label()),
        }
    }
    Ok(())
}

/// What every application on this machine currently says about Knowlith.
fn tools_status() -> Result<()> {
    for status in knowlith_desktop::status_all() {
        let state = if !status.installed {
            "not installed".to_string()
        } else if status.stale_command.is_some() {
            "needs attention".to_string()
        } else if status.connected {
            "connected".to_string()
        } else {
            "not connected".to_string()
        };
        println!("{} — {state}", status.label);

        if let Some(stale) = &status.stale_command {
            println!("  it is pointing at {stale}, which is not this Knowlith");
            println!("  run `knowlith connect {}` to repair it", status.slug);
        }
        if let Some(path) = &status.config_path {
            println!("  {path}");
        }
        if status.installed && !status.connected {
            println!("  run `knowlith connect {}`", status.slug);
        }
    }
    Ok(())
}

fn bundle(lake: &Lake, company: &str, install: bool) -> Result<()> {
    // The manifest lists what the extension can do, so Claude Desktop's
    // install screen shows the tools by name instead of a count. The list
    // comes from the gateway itself, which is the only way it cannot drift.
    let icon = serde_json::to_value(knowlith_desktop::company_icon(company))?;
    let tools = knowlith_mcp::tools::catalogue(&icon)
        .into_iter()
        .map(|entry| {
            (
                entry["name"].as_str().unwrap_or_default().to_string(),
                entry["description"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    let prompts = knowlith_mcp::prompts::list(lake, &icon)
        .into_iter()
        .map(|entry| {
            (
                entry["name"].as_str().unwrap_or_default().to_string(),
                entry["description"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();

    let built = knowlith_desktop::bundle::build(&knowlith_desktop::bundle::Contents {
        company,
        tools,
        prompts,
    })?;

    println!("{}", built.path);
    println!("  {:.1} MB · version {}", built.bytes as f64 / 1_048_576.0, built.version);

    if install {
        knowlith_desktop::bundle::reveal(&built);
        println!("  opened — Claude Desktop will show what it installs before it does");
    } else {
        println!("  open it, or run again with --install, to let Claude Desktop install it");
    }
    Ok(())
}

fn autostart(what: AutostartCommand) -> Result<()> {
    match what {
        AutostartCommand::On { port } => {
            let state = knowlith_desktop::autostart::enable(port)?;
            println!("the background service starts with this machine");
            if let Some(location) = state.location {
                println!("  {location}");
            }
            println!(
                "  {}",
                if state.running {
                    "it is running now"
                } else {
                    "it is not answering yet — give it a moment"
                }
            );
        }
        AutostartCommand::Off => {
            let state = knowlith_desktop::autostart::disable()?;
            println!("removed — nothing starts on its own any more");
            if state.running {
                println!("  something is still answering on the port; it will stop when it exits");
            }
        }
        AutostartCommand::Show => {
            let state = knowlith_desktop::autostart::status();
            println!(
                "{}",
                if state.enabled {
                    "registered to start with this machine"
                } else {
                    "not registered — the window has to be open for anything to happen"
                }
            );
            if let Some(location) = state.location {
                println!("  {location}");
            }
            println!(
                "  {}",
                if state.running { "running now" } else { "not running" }
            );
        }
    }
    Ok(())
}

fn chosen(app: Option<&str>) -> Result<Vec<knowlith_desktop::App>> {
    match app {
        None => Ok(knowlith_desktop::App::ALL.to_vec()),
        Some(name) => match knowlith_desktop::App::parse(name) {
            Some(app) => Ok(vec![app]),
            None => anyhow::bail!(
                "unknown application \"{name}\". Try claude-desktop, claude-code or codex."
            ),
        },
    }
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
