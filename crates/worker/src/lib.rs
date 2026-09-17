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
//! **Two tracks, a few CLI workers.** Folder walks must not wait behind a
//! model, and a large company folder must not wait behind a single CLI
//! cold-start. One I/O track walks and hashes; N AI tracks (default 2) each
//! own one CLI child at a time and may pack several documents into that
//! child. More workers mostly hit the provider's rate limit, not this Mac's
//! cores — so the count is capped and owned by policy.
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
use std::thread::JoinHandle;
use std::time::Duration;

use chrono::{DateTime, Utc};
use knowlith_core::Document;
use knowlith_engine::{Engine, EngineError};
use knowlith_extract::{ExtractError, extract_file, is_noise};
use knowlith_lake::{Lake, NewJob, PRIORITY_BACKGROUND, PRIORITY_NORMAL, ScanTarget};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

mod watch;

pub const KIND_RESCAN: &str = "rescan";
pub const KIND_COMPILE: &str = "compile_document";
pub const KIND_SKILLS: &str = "draft_skills";
pub const KIND_RECHECK: &str = "recheck";
pub const KIND_RELATE: &str = "relate";
pub const KIND_SETTLE: &str = "settle";
pub const KIND_SUPERVISE: &str = "supervise";

const IO_KINDS: &[&str] = &[KIND_RESCAN, KIND_RECHECK];
const AI_KINDS: &[&str] = &[KIND_COMPILE, KIND_SETTLE, KIND_RELATE, KIND_SUPERVISE, KIND_SKILLS];

/// Which slice of the queue this worker drains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Track {
    /// Everything — `work --once` and tests.
    All,
    /// Folder walks and quote rechecks. Never calls a model.
    Io,
    /// Compile / settle / relate / skills.
    Ai,
}

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
    track: Track,
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
            track: Track::All,
            rescan_hours,
            power: None,
        }
    }

    pub fn track(mut self, track: Track) -> Self {
        self.track = track;
        self
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
    pub fn run(&mut self, stop: Arc<AtomicBool>) -> Result<()> {
        while !stop.load(Ordering::Relaxed) {
            let tick = self.tick(&stop)?;
            if tick.idle() {
                sleep_until(&stop, IDLE);
            }
        }
        Ok(())
    }

    /// One turn: schedule what is due, then run one job — or a compile batch.
    pub fn tick(&mut self, stop: &AtomicBool) -> Result<Tick> {
        let mut tick = Tick {
            scheduled: self.schedule_due()?,
            ..Tick::default()
        };

        let jobs = self.claim_work()?;
        if jobs.is_empty() {
            return Ok(tick);
        }
        tick.ran = Some(jobs[0].kind.clone());

        // Expensive work is the owner's to allow. A job that needs a model
        // is put back down rather than deferred, because deferring ages it
        // towards `dead` and a laptop left unplugged overnight would wake
        // up having thrown its own queue away.
        if needs_a_model(&jobs[0].kind) {
            if let Err(held) = self.lake.policy().may_run_ai(self.on_battery()) {
                for job in &jobs {
                    self.lake.hold(job.id, held.as_str())?;
                }
                tick.outcome = Some(format!("held: {}", held.reason()));
                return Ok(tick);
            }
        }

        let outcome = self.dispatch_jobs(&jobs, stop);
        match outcome {
            Ok(notes) => {
                let mut summary = Vec::new();
                for (job, note) in jobs.iter().zip(notes.into_iter()) {
                    // Kept, not only printed. This sentence is the whole of
                    // what the owner is shown about work that went right.
                    self.lake.finish(job.id, &note)?;
                    if !note.is_empty() {
                        summary.push(note);
                    }
                }
                tick.outcome = Some(if summary.len() <= 1 {
                    summary.pop().unwrap_or_default()
                } else {
                    format!("{} docs · {}", summary.len(), summary.join(" · "))
                });
            }
            // A network is not a failure. Wait longer, try again, say nothing
            // to the owner until it has stopped being temporary.
            Err(Failure::Transport(reason)) => {
                let mut last = String::new();
                for job in &jobs {
                    let state = self.lake.defer(job.id, job.attempts, &reason)?;
                    last = format!("deferred ({state:?}): {reason}");
                }
                tick.outcome = Some(last);
            }
            // Retrying changes nothing, so it stops here and is reported.
            Err(Failure::Refused(reason)) => {
                for job in &jobs {
                    self.lake.fail(job.id, &reason)?;
                }
                tick.outcome = Some(format!("failed: {reason}"));
            }
        }
        Ok(tick)
    }

    /// Claims the next job for this track, packing compile jobs into a batch.
    fn claim_work(&mut self) -> Result<Vec<knowlith_lake::Job>> {
        let first = match self.track {
            Track::All => self.lake.lease()?,
            Track::Io => self.lake.lease_kinds(IO_KINDS)?,
            Track::Ai => self.lake.lease_kinds(AI_KINDS)?,
        };
        let Some(first) = first else {
            return Ok(Vec::new());
        };

        if first.kind != KIND_COMPILE {
            return Ok(vec![first]);
        }

        let want = self.lake.policy().compile_batch_capped();
        let mut jobs = vec![first];
        while jobs.len() < want {
            match self.lake.lease_kind(KIND_COMPILE)? {
                Some(next) => jobs.push(next),
                None => break,
            }
        }
        Ok(jobs)
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
        if try_enqueue_relate_if_due(&self.lake)? {
            queued += 1;
        }

        // Build supervisor: after reading (and relating, unless deferred).
        // When relate is deferred until after the build quiz, do not wait for
        // a relate job that has not been queued yet.
        let relate_done = self.lake.pending_of_kind(KIND_RELATE)? == 0;
        if self.lake.pending_of_kind(KIND_COMPILE)? == 0
            && self.lake.pending_of_kind(KIND_SETTLE)? == 0
            && relate_done
        {
            let docs = self.lake.documents().map(|d| d.len()).unwrap_or(0);
            let supervised_at: i64 = self
                .lake
                .setting("supervised_at")?
                .and_then(|v| v.parse().ok())
                .unwrap_or(-1);
            if docs > 0 && docs as i64 != supervised_at && enqueue_supervise(&self.lake, docs)? {
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

    fn dispatch_jobs(
        &mut self,
        jobs: &[knowlith_lake::Job],
        stop: &AtomicBool,
    ) -> std::result::Result<Vec<String>, Failure> {
        if jobs.is_empty() {
            return Ok(Vec::new());
        }
        if jobs[0].kind == KIND_COMPILE {
            return self.compile_documents(jobs, stop);
        }
        let job = &jobs[0];
        let note = match job.kind.as_str() {
            KIND_RESCAN => self.rescan(job)?,
            KIND_SKILLS => self.draft_skills(job, stop)?,
            KIND_RECHECK => self.recheck(job)?,
            KIND_RELATE => self.relate(job, stop)?,
            KIND_SUPERVISE => self.supervise(job, stop)?,
            KIND_SETTLE => self.settle(job)?,
            other => {
                return Err(Failure::Refused(format!(
                    "no worker knows how to do \"{other}\""
                )));
            }
        };
        Ok(vec![note])
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
        let mut settling = 0usize;
        let mut secrets = 0usize;

        for entry in WalkDir::new(&root).follow_links(false).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            if knowlith_extract::is_secret(&name) {
                secrets += 1;
                continue;
            }
            if is_noise(&name) {
                continue;
            }
            walked += 1;
            if walked % 25 == 0 {
                let _ = self.lake.heartbeat(job.id);
            }

            // Write-settle: a file whose mtime is still moving was often
            // mid-copy from a NAS. Extracting it produces a half document
            // and a content hash that will "change" again on the next pass.
            if still_being_written(entry.path()) {
                settling += 1;
                continue;
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
        // What changed this pass — so Sources can show "3 files changed"
        // instead of a global inbox count stamped on every folder.
        let digest = serde_json::json!({
            "at": Utc::now().to_rfc3339(),
            "walked": walked,
            "changed": changed,
            "unchanged": unchanged,
            "unreadable": unreadable,
            "settling": settling,
            "secrets": secrets,
        });
        let _ = self.lake.set_setting(
            &format!("source_digest:{}", payload.source_id),
            &digest.to_string(),
        );

        let _ = self.lake.reconcile_missing_files(&payload.source_id);

        let mut note = format!(
            "{} · {changed} changed · {unchanged} unchanged · {unreadable} not readable",
            plural(walked, "file walked", "files walked")
        );
        if settling > 0 {
            note.push_str(&format!(
                " · {}",
                plural(settling, "file still being written", "files still being written")
            ));
        }
        if secrets > 0 {
            note.push_str(&format!(
                " · {}",
                plural(secrets, "secret skipped", "secrets skipped")
            ));
        }
        Ok(note)
    }

    fn queue_compile(&mut self, document: &Document) -> std::result::Result<(), Failure> {
        enqueue_compile(&self.lake, document)
            .map(|_| ())
            .map_err(|e| Failure::Refused(e.to_string()))
    }

    // ------------------------------------------------------------ compile --

    /// Stages 1 and 2 for one or more documents in a single CLI invoke.
    ///
    /// Each job stays individually leased and finished so the work panel and
    /// idempotency keys keep talking about documents, not opaque batches.
    fn compile_documents(
        &mut self,
        jobs: &[knowlith_lake::Job],
        stop: &AtomicBool,
    ) -> std::result::Result<Vec<String>, Failure> {
        let mut loaded: Vec<(i64, Document)> = Vec::with_capacity(jobs.len());
        for job in jobs {
            let payload: DocumentPayload = serde_json::from_str(&job.payload)
                .map_err(|e| Failure::Refused(format!("this job has no document in it: {e}")))?;
            let document = self.lake.document(&payload.document_id).map_err(|e| {
                Failure::Refused(format!("{} is not in the lake: {e}", payload.document_id))
            })?;
            loaded.push((job.id, document));
        }

        let company = self.lake.setting("company_profile").ok().flatten();
        let engine = Arc::clone(&self.engine);
        let docs_for_engine: Vec<Document> = loaded.iter().map(|(_, d)| d.clone()).collect();
        let job_ids: Vec<i64> = loaded.iter().map(|(id, _)| *id).collect();

        // Stages 1 and 2 only. What the engine says about these documents is
        // stored as-is; deciding which claim is current happens once, over
        // everything, in `settle`.
        let readings = self.with_heartbeat_many(&job_ids, stop, move || {
            let refs: Vec<&Document> = docs_for_engine.iter().collect();
            knowlith_compiler::read_many(&*engine, &refs, company.as_deref())
        })??;

        let mut notes = Vec::with_capacity(loaded.len());
        for (_, document) in &loaded {
            let read = readings.get(&document.id).cloned().unwrap_or_default();
            let json: Vec<String> = read
                .candidates
                .iter()
                .map(serde_json::to_string)
                .collect::<std::result::Result<_, _>>()
                .map_err(|e| Failure::Refused(e.to_string()))?;
            let found = json.len();
            self.lake
                .put_candidates(&document.id, &json)
                .map_err(|e| Failure::Refused(e.to_string()))?;
            notes.push(format!(
                "{}: {found} claims · {} not read",
                document.name,
                read.dropped.len()
            ));
        }
        Ok(notes)
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

    // -------------------------------------------------------- supervisor --

    fn supervise(
        &mut self,
        job: &knowlith_lake::Job,
        stop: &AtomicBool,
    ) -> std::result::Result<String, Failure> {
        let docs = self.lake.documents().map_err(|e| Failure::Refused(e.to_string()))?;
        let session_id = format!("build:{}", job.id);
        let _ = self.lake.heartbeat(job.id);
        if stop.load(Ordering::Relaxed) {
            return Ok("stopped before supervisor started".into());
        }
        let report = knowlith_supervisor::run(&mut self.lake, &*self.engine, &session_id)
            .map_err(Failure::Refused)?;
        let _ = self
            .lake
            .set_setting("supervised_at", &docs.len().to_string());

        Ok(format!(
            "{} entities · {} canonical · {} quiz questions · {} communities",
            report.entities, report.canonical, report.quiz_questions, report.communities
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
            for stored in knowlith_compiler::edge_pair(edge) {
                if self
                    .lake
                    .put_relation_detail(
                        &stored.from,
                        &stored.to,
                        stored.kind,
                        stored.origin,
                        stored.why.as_deref(),
                        stored.confidence,
                    )
                    .unwrap_or(false)
                {
                    added += 1;
                }
            }
        }
        // Recorded so the pass does not repeat until the set changes again.
        // Written after the edges land, so a failed run is retried rather
        // than marked done.
        let count = self.lake.object_ids(None).map(|ids| ids.len()).unwrap_or(0);
        let fingerprint = self.lake.knowledge_fingerprint().unwrap_or_default();
        let _ = self.lake.set_setting("related_at", &count.to_string());
        let _ = self.lake.set_setting("related_fingerprint", &fingerprint);

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
        // Nothing to say when nothing moved. This job runs every hour for
        // the life of the installation, and reported each time it would
        // be the only thing on an idle machine's work panel — telling the
        // owner, hourly, that nothing happened.
        if marked == 0 {
            return Ok(String::new());
        }
        Ok(format!("{marked} objects now rest on something that moved"))
    }

    // -------------------------------------------------------------- plumbing --

    /// Runs `work` on its own thread, renewing every lease until it finishes.
    fn with_heartbeat_many<T, F>(
        &mut self,
        job_ids: &[i64],
        stop: &AtomicBool,
        work: F,
    ) -> std::result::Result<T, Failure>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let handle = std::thread::spawn(work);
        while !handle.is_finished() {
            let _ = self.lake.heartbeat_many(job_ids);
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
        handle
            .join()
            .map_err(|_| Failure::Refused("the worker thread stopped without answering".into()))
    }

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
        self.with_heartbeat_many(&[job_id], stop, work)
    }

    pub fn lake(&self) -> &Lake {
        &self.lake
    }

    pub fn lake_mut(&mut self) -> &mut Lake {
        &mut self.lake
    }
}

/// Starts the I/O track plus N AI compile workers against one lake file.
///
/// Each thread opens its own lake connection. Callers own `stop` and join
/// the handles on shutdown.
pub fn spawn_pool(
    db: &Path,
    engine: Arc<dyn Engine>,
    stop: Arc<AtomicBool>,
) -> Result<Vec<JoinHandle<()>>> {
    let workers = {
        let lake = Lake::open(db)?;
        lake.policy().compile_workers_capped()
    };

    let mut handles = Vec::with_capacity(workers + 1);

    let io_db = db.to_path_buf();
    let io_engine = Arc::clone(&engine);
    let io_stop = Arc::clone(&stop);
    handles.push(std::thread::spawn(move || {
        match Worker::open(&io_db, io_engine) {
            Ok(worker) => {
                let mut worker = worker.track(Track::Io);
                if let Err(e) = worker.run(io_stop) {
                    eprintln!("the I/O worker stopped: {e}");
                }
            }
            Err(e) => eprintln!("the I/O worker could not open the lake: {e}"),
        }
    }));

    for index in 0..workers {
        let ai_db = db.to_path_buf();
        let ai_engine = Arc::clone(&engine);
        let ai_stop = Arc::clone(&stop);
        handles.push(std::thread::spawn(move || {
            match Worker::open(&ai_db, ai_engine) {
                Ok(worker) => {
                    let mut worker = worker.track(Track::Ai);
                    if let Err(e) = worker.run(ai_stop) {
                        eprintln!("AI worker {index} stopped: {e}");
                    }
                }
                Err(e) => eprintln!("AI worker {index} could not open the lake: {e}"),
            }
        }));
    }

    handles.push(watch::spawn(db.to_path_buf(), stop));

    Ok(handles)
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
fn enqueue_relating(lake: &Lake) -> Result<bool> {
    let fingerprint = lake.knowledge_fingerprint().unwrap_or_default();
    Ok(lake.enqueue(&NewJob {
        kind: KIND_RELATE.into(),
        payload: "{}".into(),
        idempotency_key: format!("{KIND_RELATE}:{fingerprint}"),
        priority: PRIORITY_NORMAL,
    })?)
}

/// Queues relate when compile has settled and policy allows it.
pub fn try_enqueue_relate_if_due(lake: &Lake) -> Result<bool> {
    if lake.pending_of_kind(KIND_COMPILE)? > 0 || lake.pending_of_kind(KIND_SETTLE)? > 0 {
        return Ok(false);
    }
    if !lake.may_enqueue_relate()? {
        return Ok(false);
    }
    let objects = lake.object_ids(None)?.len();
    if objects <= 1 {
        return Ok(false);
    }
    let fingerprint = lake.knowledge_fingerprint().unwrap_or_default();
    let related_fingerprint = lake.setting("related_fingerprint")?.unwrap_or_default();
    if fingerprint == related_fingerprint {
        return Ok(false);
    }
    enqueue_relating(lake)
}

pub fn enqueue_supervise(lake: &Lake, document_count: usize) -> Result<bool> {
    Ok(lake.enqueue(&NewJob {
        kind: KIND_SUPERVISE.into(),
        payload: "{}".into(),
        idempotency_key: format!("{KIND_SUPERVISE}:{document_count}"),
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
    matches!(kind, KIND_COMPILE | KIND_SKILLS | KIND_RELATE | KIND_SUPERVISE)
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    #[test]
    fn only_the_three_that_call_an_engine_are_governed() {
        assert!(needs_a_model(KIND_COMPILE));
        assert!(needs_a_model(KIND_SKILLS));
        assert!(needs_a_model(KIND_RELATE));
        assert!(needs_a_model(KIND_SUPERVISE));

        // These cost nothing and must keep running on battery, or a laptop
        // on a train stops noticing that its own files changed.
        assert!(!needs_a_model(KIND_RESCAN));
        assert!(!needs_a_model(KIND_SETTLE));
        assert!(!needs_a_model(KIND_RECHECK));
    }
}

/// How long after the last write we wait before trusting the bytes.
///
/// Renfield-style write-settle. Separate from the compiler's `settle` job.
pub(crate) const WRITE_SETTLE_SECS: u64 = 3;

/// True when the file's mtime is so recent it may still be mid-copy.
fn still_being_written(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    match modified.elapsed() {
        Ok(elapsed) => elapsed.as_secs() < WRITE_SETTLE_SECS,
        // Clock skew / future mtime — wait rather than read half a file.
        Err(_) => true,
    }
}

/// `1 file walked`, not `1 files walked`.
///
/// The owner reads these sentences on the dashboard, where a folder with
/// one document in it is an ordinary case rather than an edge one.
fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

#[cfg(test)]
mod plural_tests {
    use super::plural;

    #[test]
    fn one_is_not_reported_in_the_plural() {
        assert_eq!(plural(1, "file walked", "files walked"), "1 file walked");
        assert_eq!(plural(0, "file walked", "files walked"), "0 files walked");
        assert_eq!(plural(2, "file walked", "files walked"), "2 files walked");
    }
}
