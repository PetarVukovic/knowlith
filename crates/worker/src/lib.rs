//! The loop that drains the queue.
//!
//! Until this existed, everything in Knowlith happened because a person
//! typed a command. The queue was built, tested and empty, the interface
//! said "you can close this window — it keeps going", and nothing kept
//! going. This crate is what makes that sentence true.
//!
//! Four decisions shape it.
//!
//! **It owns its own connection to the lake.** Not the server's, behind the
//! server's mutex. A compile run holds a model for minutes, and holding the
//! write lock for that long would freeze every screen in the interface.
//! SQLite in WAL mode is built for exactly this: two connections to one
//! file, one reading while the other writes.
//!
//! **One job at a time.** A second concurrent Codex process on one Mac
//! doubles what the owner pays and finishes no sooner, because the bottleneck
//! is somebody else's rate limit rather than this machine's cores.
//!
//! **A lease is renewed, not assumed.** The engine call runs on its own
//! thread so the loop can keep the lease alive while it waits. Without that,
//! every job that takes longer than sixty seconds is silently handed to a
//! second worker that does not exist, and then re-run.
//!
//! **Shutdown drops the job rather than corrupting it.** There is no
//! goodbye: the process stops, the lease expires, and the next start picks
//! the job up with `attempts` one higher. That is what the lease was for.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use chrono::{DateTime, Utc};
use knowlith_core::Document;
use knowlith_engine::{Engine, EngineError};
use knowlith_extract::{ExtractError, extract_file, is_noise};
use knowlith_lake::{Lake, NewJob, PRIORITY_BACKGROUND, PRIORITY_NORMAL, ScanTarget};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

pub const KIND_RESCAN: &str = "rescan";
pub const KIND_COMPILE: &str = "compile_document";
pub const KIND_SKILLS: &str = "draft_skills";
pub const KIND_RECHECK: &str = "recheck";
pub const KIND_RELATE: &str = "relate";
pub const KIND_SETTLE: &str = "settle";

/// How often the loop renews a lease it is still using.
const HEARTBEAT: Duration = Duration::from_secs(5);
/// How long the loop waits when the queue is empty.
const IDLE: Duration = Duration::from_secs(2);
/// How often the heartbeat loop looks at whether the work has landed.
const POLL: Duration = Duration::from_millis(50);
/// The default gap between scheduled rescans, when the owner has not set one.
const DEFAULT_RESCAN_HOURS: i64 = 6;

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error(transparent)]
    Lake(#[from] knowlith_lake::LakeError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("the worker thread stopped without answering")]
    Panicked,
}

type Result<T> = std::result::Result<T, WorkerError>;

#[derive(Debug, Serialize, Deserialize)]
pub struct SourcePayload {
    pub source_id: String,
    pub root: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct DocumentPayload {
    document_id: String,
}

/// What one turn of the loop did, so `knowlith work --once` can say something
/// and the tests can assert on it rather than on a log line.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Tick {
    pub scheduled: usize,
    pub ran: Option<String>,
    pub outcome: Option<String>,
}

impl Tick {
    pub fn idle(&self) -> bool {
        self.ran.is_none() && self.scheduled == 0
    }
}

pub struct Worker {
    lake: Lake,
    engine: Arc<dyn Engine>,
    /// Kept so a rescan can tell whether a file it walked is the same one the
    /// lake already read.
    rescan_hours: i64,
    /// Set only by tests. Reading the real power state would make every
    /// test in this crate depend on whether the laptop running it happens
    /// to be plugged in, which is the kind of failure nobody reproduces.
    power: Option<bool>,
}

impl Worker {
    pub fn new(lake: Lake, engine: Arc<dyn Engine>) -> Self {
        let rescan_hours = lake
            .setting("rescan_hours")
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_RESCAN_HOURS);
        Self {
            lake,
            engine,
            rescan_hours,
            power: None,
        }
    }

    /// Pins the power state, for tests.
    pub fn on_mains(mut self) -> Self {
        self.power = Some(false);
        self
    }

    /// Pins the power state to battery, for tests.
    pub fn unplugged(mut self) -> Self {
        self.power = Some(true);
        self
    }

    pub fn open(db: &Path, engine: Arc<dyn Engine>) -> Result<Self> {
        Ok(Self::new(Lake::open(db)?, engine))
    }

    /// Runs until `stop` is set.
    ///
    /// Nothing here is async. The work is a child process and a SQLite file,
    /// and wrapping either in a runtime would buy concurrency this
    /// deliberately does not want.
    pub fn run(&mut self, stop: Arc<AtomicBool>) -> Result<()> {
        while !stop.load(Ordering::Relaxed) {
            let tick = self.tick(&stop)?;
            if tick.idle() {
                sleep_until(&stop, IDLE);
            }
        }
        Ok(())
    }

    /// One turn: schedule what is due, then run at most one job.
    pub fn tick(&mut self, stop: &AtomicBool) -> Result<Tick> {
        let mut tick = Tick {
            scheduled: self.schedule_due()?,
            ..Tick::default()
        };

        let Some(job) = self.lake.lease()? else {
            return Ok(tick);
        };
        tick.ran = Some(job.kind.clone());

        // Expensive work is the owner's to allow. A job that needs a model
        // is put back down rather than deferred, because deferring ages it
        // towards `dead` and a laptop left unplugged overnight would wake
        // up having thrown its own queue away.
        if needs_a_model(&job.kind) {
            if let Err(held) = self.lake.policy().may_run_ai(self.on_battery()) {
                self.lake.hold(job.id, held.as_str())?;
                tick.outcome = Some(format!("held: {}", held.reason()));
                return Ok(tick);
            }
        }

        let outcome = self.dispatch(&job, stop);
        match outcome {
            Ok(note) => {
                self.lake.finish(job.id)?;
                tick.outcome = Some(note);
            }
            // A network is not a failure. Wait longer, try again, say nothing
            // to the owner until it has stopped being temporary.
            Err(Failure::Transport(reason)) => {
                let state = self.lake.defer(job.id, job.attempts, &reason)?;
                tick.outcome = Some(format!("deferred ({state:?}): {reason}"));
            }
            // Retrying changes nothing, so it stops here and is reported.
            Err(Failure::Refused(reason)) => {
                self.lake.fail(job.id, &reason)?;
                tick.outcome = Some(format!("failed: {reason}"));
            }
        }
        Ok(tick)
    }

    /// Enqueues a rescan for every source whose turn has come.
    ///
    /// The idempotency key carries the time window rather than the instant,
    /// so a worker that restarts four times inside one window queues one
    /// rescan, not four.
    fn schedule_due(&mut self) -> Result<usize> {
        let now = Utc::now();
        let mut queued = 0;

        for target in self.lake.scan_targets()? {
            if !is_due(&target, now, self.rescan_hours) {
                continue;
            }
            let window = now.timestamp() / (self.rescan_hours.max(1) * 3600);
            let queued_now = self.lake.enqueue(&NewJob {
                kind: KIND_RESCAN.into(),
                payload: serde_json::to_string(&SourcePayload {
                    source_id: target.id.clone(),
                    root: target.root.clone(),
                })?,
                idempotency_key: format!("{KIND_RESCAN}:{}:{window}", target.id),
                priority: PRIORITY_NORMAL,
            })?;
            if queued_now {
                queued += 1;
            }
        }

        // Comparing everything against everything, once the reading has
        // finished. Cheap, deterministic, and redone whenever the set of
        // stored claims changes.
        if self.lake.pending_of_kind(KIND_COMPILE)? == 0 {
            let claims = self.lake.candidate_count()?;
            let settled_at: i64 = self
                .lake
                .setting("settled_at")?
                .and_then(|v| v.parse().ok())
                .unwrap_or(-1);
            if claims > 0 && claims != settled_at {
                let queued_now = self.lake.enqueue(&NewJob {
                    kind: KIND_SETTLE.into(),
                    payload: "{}".into(),
                    idempotency_key: format!("{KIND_SETTLE}:{claims}"),
                    priority: PRIORITY_NORMAL,
                })?;
                if queued_now {
                    queued += 1;
                }
            }
        }

        // The graph pass, once the reading has finished.
        //
        // Asking after every document looked reasonable and was ten model
        // calls for one folder — dependency is a property of the whole set,
        // so a pass over eleven objects is thrown away by the pass over
        // twelve. It waits for the compile queue to drain, and then only
        // runs if the set actually grew since the last time.
        if self.lake.pending_of_kind(KIND_COMPILE)? == 0
            && self.lake.pending_of_kind(KIND_SETTLE)? == 0
        {
            let objects = self.lake.object_ids(None)?.len();
            let related_at: usize = self
                .lake
                .setting("related_at")?
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            if objects > 1 && objects != related_at && enqueue_relating(&self.lake)? {
                queued += 1;
            }
        }

        // Every hour, check that the quotes still say what they said. This
        // is what keeps `verified` a check rather than a memory: a source
        // file edited under an approved rule has to mark that rule stale
        // without anybody noticing the edit.
        if enqueue_recheck(&self.lake)? {
            queued += 1;
        }

        Ok(queued)
    }

    /// Whether this machine is on battery right now.
    ///
    /// Read once per job rather than cached: the interesting moment is
    /// exactly the one where someone unplugs a laptop mid-queue.
    fn on_battery(&self) -> bool {
        self.power
            .unwrap_or_else(|| knowlith_desktop::power::power().on_battery())
    }

    fn dispatch(&mut self, job: &knowlith_lake::Job, stop: &AtomicBool) -> std::result::Result<String, Failure> {
        match job.kind.as_str() {
            KIND_RESCAN => self.rescan(job),
            KIND_COMPILE => self.compile_document(job, stop),
            KIND_SKILLS => self.draft_skills(job, stop),
            KIND_RECHECK => self.recheck(job),
            KIND_RELATE => self.relate(job, stop),
            KIND_SETTLE => self.settle(job),
            other => Err(Failure::Refused(format!("no worker knows how to do \"{other}\""))),
        }
    }

    // ------------------------------------------------------------- rescan --

    /// Walks a source folder and queues a compile for anything that moved.
    ///
    /// The content hash is what decides. A file that was touched, copied or
    /// renamed but whose bytes are identical costs one hash and nothing
    /// else — which is the difference between a rescan that takes a minute
    /// and one that re-reads nine gigabytes every six hours.
    fn rescan(&mut self, job: &knowlith_lake::Job) -> std::result::Result<String, Failure> {
        let payload: SourcePayload = serde_json::from_str(&job.payload)
            .map_err(|e| Failure::Refused(format!("this rescan has no folder in it: {e}")))?;
        let root = PathBuf::from(&payload.root);
        if !root.is_dir() {
            let reason = format!("{} is not there any more", root.display());
            let _ = self.lake.mark_source_error(&payload.source_id, &reason);
            return Err(Failure::Refused(reason));
        }

        let mut walked = 0usize;
        let mut changed = 0usize;
        let mut unchanged = 0usize;
        let mut unreadable = 0usize;

        for entry in WalkDir::new(&root).follow_links(false).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            if is_noise(&entry.file_name().to_string_lossy()) {
                continue;
            }
            walked += 1;
            if walked % 25 == 0 {
                let _ = self.lake.heartbeat(job.id);
            }

            match extract_file(entry.path()) {
                Ok(document) => {
                    let known = self.lake.has_content(&document.sha256).unwrap_or(false);
                    if let Err(e) = self.lake.put_document(&payload.source_id, &document) {
                        return Err(Failure::Refused(format!("could not store {}: {e}", document.name)));
                    }
                    if known {
                        unchanged += 1;
                        continue;
                    }
                    changed += 1;
                    self.queue_compile(&document)?;
                }
                Err(ExtractError::UnsupportedType | ExtractError::NoText | ExtractError::Empty) => {
                    unreadable += 1;
                }
                Err(e) => return Err(Failure::Refused(format!("could not read a file: {e}"))),
            }
        }

        let _ = self.lake.mark_scanned(&payload.source_id);
        Ok(format!(
            "{walked} files walked · {changed} changed · {unchanged} unchanged · {unreadable} not readable"
        ))
    }

    fn queue_compile(&mut self, document: &Document) -> std::result::Result<(), Failure> {
        enqueue_compile(&self.lake, document)
            .map(|_| ())
            .map_err(|e| Failure::Refused(e.to_string()))
    }

    // ------------------------------------------------------------ compile --

    fn compile_document(
        &mut self,
        job: &knowlith_lake::Job,
        stop: &AtomicBool,
    ) -> std::result::Result<String, Failure> {
        let payload: DocumentPayload = serde_json::from_str(&job.payload)
            .map_err(|e| Failure::Refused(format!("this job has no document in it: {e}")))?;
        let document = self
            .lake
            .document(&payload.document_id)
            .map_err(|e| Failure::Refused(format!("{} is not in the lake: {e}", payload.document_id)))?;
        let name = document.name.clone();

        let engine = Arc::clone(&self.engine);
        // Stages 1 and 2 only. What the engine says about this document is
        // stored as-is; deciding which claim is current happens once, over
        // everything, in `settle`.
        let read = self.with_heartbeat(job.id, stop, move || {
            let mut read = knowlith_compiler::Reading::default();
            knowlith_compiler::read_one(&*engine, &document, &mut read).map(|()| read)
        })??;

        let json: Vec<String> = read
            .candidates
            .iter()
            .map(serde_json::to_string)
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| Failure::Refused(e.to_string()))?;
        let found = json.len();

        self.lake
            .put_candidates(&payload.document_id, &json)
            .map_err(|e| Failure::Refused(e.to_string()))?;

        Ok(format!(
            "{name}: {found} claims · {} not read",
            read.dropped.len()
        ))
    }

    // ------------------------------------------------------------- settle --

    /// Stages 3 and 4, over everything the engine has ever said.
    ///
    /// Deterministic, so it can be redone as often as the set changes
    /// without costing anything. That is the point of storing candidates:
    /// a thirteenth document does not make the first twelve need reading
    /// again, but it absolutely makes them need comparing again.
    fn settle(&mut self, job: &knowlith_lake::Job) -> std::result::Result<String, Failure> {
        let _ = self.lake.heartbeat(job.id);

        let documents = self.lake.documents().map_err(|e| Failure::Refused(e.to_string()))?;
        let stored = self.lake.candidates().map_err(|e| Failure::Refused(e.to_string()))?;

        let mut proposals = Vec::with_capacity(stored.len());
        for (document_id, json) in &stored {
            match serde_json::from_str::<knowlith_compiler::Candidate>(json) {
                Ok(candidate) => proposals.push((document_id.clone(), candidate)),
                Err(e) => return Err(Failure::Refused(format!("a stored claim is unreadable: {e}"))),
            }
        }

        let out = knowlith_compiler::settle(&proposals, &documents);

        let mut kept = 0;
        for object in &out.objects {
            // The lake runs the evidence gate again on write. A span that
            // passed stage 4 and fails here means the two checks disagree,
            // which is a bug worth surfacing rather than a document worth
            // dropping.
            match self.lake.put_object(object) {
                Ok(()) => kept += 1,
                Err(e) => return Err(Failure::Refused(format!("{} was refused on write: {e}", object.id))),
            }
        }

        let all = self.lake.objects().map_err(|e| Failure::Refused(e.to_string()))?;
        let mut asked = 0;
        for hint in knowlith_compiler::hints(&all) {
            if self
                .lake
                .put_merge_hint(&hint.keep_id, &hint.drop_id, hint_kind(hint.kind), hint.score)
                .unwrap_or(false)
            {
                asked += 1;
            }
        }

        let _ = self
            .lake
            .candidate_count()
            .and_then(|n| self.lake.set_setting("settled_at", &n.to_string()));

        Ok(format!(
            "{kept} kept · {} dropped · {} conflicts · {asked} pairs to decide",
            out.dropped.len(),
            out.conflicts.len()
        ))
    }

    // ------------------------------------------------------------- skills --

    fn draft_skills(
        &mut self,
        job: &knowlith_lake::Job,
        stop: &AtomicBool,
    ) -> std::result::Result<String, Failure> {
        let objects = self.lake.objects().map_err(|e| Failure::Refused(e.to_string()))?;
        let engine = Arc::clone(&self.engine);

        let run = self.with_heartbeat(job.id, stop, move || {
            knowlith_compiler::draft_all(&*engine, &objects)
        })??;

        let mut stored = 0;
        for skill in &run.skills {
            match self.lake.put_object(skill) {
                Ok(()) => stored += 1,
                Err(e) => return Err(Failure::Refused(format!("{} was refused on write: {e}", skill.id))),
            }
        }
        Ok(format!(
            "{stored} skills drafted from {} approved processes · {} not kept",
            run.processes_considered,
            run.dropped.len()
        ))
    }

    // ---------------------------------------------------------- relations --

    /// Asks the engine which objects depend on which.
    ///
    /// Stored as [`RelationOrigin::Model`], which is what makes them
    /// removable and what makes the interface able to say "suggested"
    /// rather than presenting a guess as structure.
    fn relate(
        &mut self,
        job: &knowlith_lake::Job,
        stop: &AtomicBool,
    ) -> std::result::Result<String, Failure> {
        let objects = self.lake.objects().map_err(|e| Failure::Refused(e.to_string()))?;
        let engine = Arc::clone(&self.engine);

        let run = self.with_heartbeat(job.id, stop, move || {
            knowlith_compiler::propose_relations(&*engine, &objects)
        })??;

        let mut added = 0;
        for edge in &run.edges {
            for (from, to, kind, origin) in knowlith_compiler::edge_pair(edge) {
                if self.lake.put_relation(&from, &to, kind, origin).unwrap_or(false) {
                    added += 1;
                }
            }
        }
        // Recorded so the pass does not repeat until the set changes again.
        // Written after the edges land, so a failed run is retried rather
        // than marked done.
        let _ = self
            .lake
            .set_setting("related_at", &self.lake.object_ids(None).map(|ids| ids.len()).unwrap_or(0).to_string());

        Ok(format!(
            "{added} edges across {} objects · {} proposals not usable",
            run.objects_considered,
            run.dropped.len()
        ))
    }

    // ------------------------------------------------------------ recheck --

    /// Re-runs the mechanical span check over everything stored.
    ///
    /// This is what keeps `verified` a claim rather than a memory. A source
    /// file that was edited under an approved rule marks that rule stale;
    /// the gateway keeps serving it and says so, because silently serving a
    /// stale answer and silently withholding one are both worse.
    fn recheck(&mut self, job: &knowlith_lake::Job) -> std::result::Result<String, Failure> {
        let _ = self.lake.heartbeat(job.id);
        let broken = self
            .lake
            .recheck_evidence()
            .map_err(|e| Failure::Refused(e.to_string()))?;

        let mut marked = 0;
        for (object_id, _) in &broken {
            if self.lake.mark_stale(object_id).is_ok() {
                marked += 1;
            }
        }
        Ok(format!("{marked} objects now rest on something that moved"))
    }

    // -------------------------------------------------------------- plumbing --

    /// Runs `work` on its own thread, renewing the lease until it finishes.
    fn with_heartbeat<T, F>(
        &mut self,
        job_id: i64,
        stop: &AtomicBool,
        work: F,
    ) -> std::result::Result<T, Failure>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let handle = std::thread::spawn(work);
        while !handle.is_finished() {
            let _ = self.lake.heartbeat(job_id);
            // Waited in small steps rather than one long sleep, so a job
            // that finishes in a millisecond is not charged five seconds of
            // latency by the thing watching it.
            //
            // The stop flag is watched but not obeyed here: killing a model
            // mid-answer throws away what the owner already paid for. The
            // process exits when this job ends, and an unfinished one comes
            // back through its expired lease.
            let mut waited = Duration::ZERO;
            while waited < HEARTBEAT && !handle.is_finished() && !stop.load(Ordering::Relaxed) {
                std::thread::sleep(POLL);
                waited += POLL;
            }
            if stop.load(Ordering::Relaxed) && !handle.is_finished() {
                // Keep renewing until it lands; the alternative is a lease
                // that expires while the answer is still being written.
                continue;
            }
        }
        handle.join().map_err(|_| Failure::Refused("the worker thread stopped without answering".into()))
    }

    pub fn lake(&self) -> &Lake {
        &self.lake
    }

    pub fn lake_mut(&mut self) -> &mut Lake {
        &mut self.lake
    }
}

/// The two things that can go wrong, kept apart because they ask for
/// different things: one waits, the other tells somebody.
enum Failure {
    Transport(String),
    Refused(String),
}

impl From<knowlith_compiler::CompileError> for Failure {
    fn from(error: knowlith_compiler::CompileError) -> Self {
        match error {
            knowlith_compiler::CompileError::Engine(e) if e.is_retryable() => {
                Failure::Transport(e.to_string())
            }
            other => Failure::Refused(other.to_string()),
        }
    }
}

impl From<EngineError> for Failure {
    fn from(error: EngineError) -> Self {
        if error.is_retryable() {
            Failure::Transport(error.to_string())
        } else {
            Failure::Refused(error.to_string())
        }
    }
}

/// The word stored in the lake for a hint's kind.
fn hint_kind(kind: knowlith_compiler::HintKind) -> &'static str {
    match kind {
        knowlith_compiler::HintKind::Duplicate => "duplicate",
        knowlith_compiler::HintKind::Disagreement => "disagreement",
    }
}

fn is_due(target: &ScanTarget, now: DateTime<Utc>, hours: i64) -> bool {
    let Some(last) = target.last_scan.as_deref() else {
        // Never scanned. The owner added the folder and expects something to
        // happen without typing a command.
        return true;
    };
    match DateTime::parse_from_rfc3339(last) {
        Ok(at) => now.signed_duration_since(at.with_timezone(&Utc)).num_hours() >= hours,
        // An unparseable timestamp is treated as "we do not know", which
        // costs one scan and fixes the timestamp.
        Err(_) => true,
    }
}

/// Sleeps, but wakes early when asked to stop.
fn sleep_until(stop: &AtomicBool, total: Duration) {
    let step = Duration::from_millis(100);
    let mut slept = Duration::ZERO;
    while slept < total && !stop.load(Ordering::Relaxed) {
        std::thread::sleep(step);
        slept += step;
    }
}

/// Queues the two jobs a freshly added source needs, in the order it needs
/// them: read the folder, then look at what came out.
pub fn enqueue_first_run(lake: &Lake, source_id: &str, root: &str) -> Result<()> {
    lake.enqueue(&NewJob {
        kind: KIND_RESCAN.into(),
        payload: serde_json::to_string(&SourcePayload {
            source_id: source_id.to_string(),
            root: root.to_string(),
        })?,
        idempotency_key: format!("{KIND_RESCAN}:{source_id}:first"),
        priority: PRIORITY_NORMAL,
    })?;
    Ok(())
}

/// Asks for a source folder to be walked.
///
/// This is how a folder the owner just added becomes documents: the interface
/// does not scan, it queues, and the worker that is already running picks the
/// job up. That is the difference between a scan that survives the window
/// being closed and one that does not.
///
/// Keyed on the source and the minute, so an impatient double-click is one
/// walk while a deliberate "scan again" a minute later is a second one. A key
/// without the clock in it would be worse than it sounds: the finished job
/// keeps its key, so the same folder could never be walked on request again.
pub fn enqueue_rescan(lake: &Lake, source_id: &str, root: &str) -> Result<bool> {
    let minute = chrono::Utc::now().timestamp() / 60;
    Ok(lake.enqueue(&NewJob {
        kind: KIND_RESCAN.into(),
        payload: serde_json::to_string(&SourcePayload {
            source_id: source_id.to_string(),
            root: root.to_string(),
        })?,
        idempotency_key: format!("{KIND_RESCAN}:{source_id}:{minute}"),
        priority: PRIORITY_NORMAL,
    })?)
}

/// Queues the reading of one document.
///
/// Keyed on the rendition hash rather than the document id: a document whose
/// text has not moved does not need compiling again, and a document whose
/// text has moved is different work wearing the same id.
pub fn enqueue_compile(lake: &Lake, document: &Document) -> Result<bool> {
    Ok(lake.enqueue(&NewJob {
        kind: KIND_COMPILE.into(),
        payload: serde_json::to_string(&DocumentPayload {
            document_id: document.id.clone(),
        })?,
        idempotency_key: format!("{KIND_COMPILE}:{}", document.text_sha256),
        priority: PRIORITY_NORMAL,
    })?)
}

/// Asks for skills to be drafted from whatever is approved right now.
///
/// Keyed on the approval count so the same request twice in a row is one
/// job, but a new approval makes it new work.
pub fn enqueue_skill_drafting(lake: &Lake) -> Result<bool> {
    let approved = lake.object_ids(Some("approved"))?.len();
    Ok(lake.enqueue(&NewJob {
        kind: KIND_SKILLS.into(),
        payload: "{}".into(),
        idempotency_key: format!("{KIND_SKILLS}:{approved}"),
        priority: PRIORITY_NORMAL,
    })?)
}

/// Asks for the dependency graph to be looked at again.
///
/// Keyed on how many objects exist, so asking twice with nothing new is one
/// job, and a folder that grew is new work.
pub fn enqueue_relating(lake: &Lake) -> Result<bool> {
    let objects = lake.object_ids(None)?.len();
    Ok(lake.enqueue(&NewJob {
        kind: KIND_RELATE.into(),
        payload: "{}".into(),
        idempotency_key: format!("{KIND_RELATE}:{objects}"),
        priority: PRIORITY_NORMAL,
    })?)
}

pub fn enqueue_recheck(lake: &Lake) -> Result<bool> {
    let window = Utc::now().timestamp() / 3600;
    Ok(lake.enqueue(&NewJob {
        kind: KIND_RECHECK.into(),
        payload: "{}".into(),
        idempotency_key: format!("{KIND_RECHECK}:{window}"),
        // Never ahead of a document somebody added a minute ago.
        priority: PRIORITY_BACKGROUND,
    })?)
}

/// Whether a kind of job calls a model.
///
/// The division that the whole policy rests on. Reading a folder, comparing
/// what was read and re-checking stored quotes are arithmetic and run
/// whatever the owner has chosen; the three that ask an engine are the ones
/// that cost money.
pub fn needs_a_model(kind: &str) -> bool {
    matches!(kind, KIND_COMPILE | KIND_SKILLS | KIND_RELATE)
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    #[test]
    fn only_the_three_that_call_an_engine_are_governed() {
        assert!(needs_a_model(KIND_COMPILE));
        assert!(needs_a_model(KIND_SKILLS));
        assert!(needs_a_model(KIND_RELATE));

        // These cost nothing and must keep running on battery, or a laptop
        // on a train stops noticing that its own files changed.
        assert!(!needs_a_model(KIND_RESCAN));
        assert!(!needs_a_model(KIND_SETTLE));
        assert!(!needs_a_model(KIND_RECHECK));
    }
}
