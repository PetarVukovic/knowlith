//! The durable queue.
//!
//! Work outlives the process that started it. A laptop closing mid-scan, a
//! provider that stops answering, a daemon that is killed — none of them
//! should cost the owner the two thousand files already read.
//!
//! Three choices carry that:
//!
//! * **A lease, not a flag.** A worker claims a job until a timestamp. If it
//!   dies, the lease expires and the job returns to the queue on its own. No
//!   janitor process, no jobs stuck in `in_progress` forever.
//! * **An idempotency key.** Delivery is at-least-once, so a retry can and
//!   will re-run work. The key makes the second run recognise itself.
//! * **Transport failure is not failure.** No network means wait and try
//!   again with a longer gap; a refused claim means a person has to look. The
//!   two are recorded differently because they ask for different things.

use chrono::{Duration, Utc};
use rusqlite::{OptionalExtension, params};

use crate::{Lake, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Queued,
    Leased,
    Done,
    /// Refused for a reason a person has to resolve.
    Failed,
    /// Retried past the limit. Kept, never deleted, so the run report can
    /// say what never got read.
    Dead,
}

#[derive(Debug, Clone)]
pub struct NewJob {
    pub kind: String,
    pub payload: String,
    /// Derived from the inputs, not from a counter: `hash(file, stage,
    /// engine_version)`. The same work always produces the same key.
    pub idempotency_key: String,
    /// Lower runs first; [`PRIORITY_NORMAL`] unless there is a reason.
    ///
    /// This exists because the hourly re-check of stored quotes, queued
    /// before a document the owner had just added, was handed out first. The
    /// re-check is worth doing and it is never what somebody is waiting for.
    pub priority: i64,
}

/// What the owner is waiting for.
pub const PRIORITY_NORMAL: i64 = 0;
/// Housekeeping: correct to do, never urgent.
pub const PRIORITY_BACKGROUND: i64 = 10;

#[derive(Debug, Clone)]
pub struct Job {
    pub id: i64,
    pub kind: String,
    pub payload: String,
    pub attempts: i64,
}

/// One line of work, as the owner should read it.
///
/// `subject` is whatever the job's payload identified — a document id, a
/// source id, or nothing at all for the jobs that act on the whole lake.
/// Turning that into a name is the server's job, because only the server
/// knows what the owner calls it.
#[derive(Debug, Clone)]
pub struct WorkLine {
    pub kind: String,
    pub state: String,
    pub subject: Option<String>,
    pub note: Option<String>,
    pub error: Option<String>,
    pub at: Option<String>,
    pub attempts: i64,
}

/// What a job's payload was about, if it was about one thing.
///
/// Read leniently. A payload shape that changes must cost a missing name on
/// one line of a log, never a failed query — the panel exists to report
/// trouble, and it would be a poor one if a surprise silenced it.
fn subject_of(payload: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(payload).ok()?;
    for key in ["document_id", "source_id", "id", "root"] {
        if let Some(found) = value.get(key).and_then(|v| v.as_str()) {
            return Some(found.to_string());
        }
    }
    None
}

/// How long a worker holds a claim before it has to renew.
const LEASE_SECONDS: i64 = 60;
/// After this many attempts a job stops being retried and starts being
/// reported.
const MAX_ATTEMPTS: i64 = 6;

impl Lake {
    /// Adds a job, or does nothing if that exact work is already queued.
    pub fn enqueue(&self, job: &NewJob) -> Result<bool> {
        let changed = self.conn.execute(
            "INSERT OR IGNORE INTO jobs
               (kind, payload_json, idempotency_key, state, priority, run_after, created_at)
             VALUES (?1, ?2, ?3, 'queued', ?5, ?4, ?4)",
            params![
                job.kind,
                job.payload,
                job.idempotency_key,
                Utc::now().to_rfc3339(),
                job.priority
            ],
        )?;
        Ok(changed > 0)
    }

    /// Claims the next job that is due, for `LEASE_SECONDS`.
    ///
    /// A job whose lease has run out is due again, which is how work returns
    /// after a crash without anything having to notice the crash.
    pub fn lease(&self) -> Result<Option<Job>> {
        self.lease_filtered(None)
    }

    /// Claims the next due job whose `kind` is one of `kinds`.
    ///
    /// Used by dual-track workers: the I/O track only walks folders; the AI
    /// track only talks to a CLI. Without a filter, one long rescan would
    /// starve every compile on a single-threaded loop.
    pub fn lease_kinds(&self, kinds: &[&str]) -> Result<Option<Job>> {
        if kinds.is_empty() {
            return self.lease();
        }
        self.lease_filtered(Some(kinds))
    }

    /// Claims another job of exactly `kind`, or `None` when that queue is dry.
    ///
    /// Compile workers call this after the first `compile_document` lease to
    /// fill a multi-document CLI batch without pulling a settle mid-pack.
    pub fn lease_kind(&self, kind: &str) -> Result<Option<Job>> {
        self.lease_filtered(Some(&[kind]))
    }

    fn lease_filtered(&self, kinds: Option<&[&str]>) -> Result<Option<Job>> {
        let at = Utc::now();
        let now = at.to_rfc3339();
        let until = (at + Duration::seconds(LEASE_SECONDS)).to_rfc3339();

        // Kind filters are tiny (track sets), so the IN list is built with
        // bound parameters rather than string-concatenated values.
        let claimed: Option<(i64, String, String, i64)> = match kinds {
            None => self
                .conn
                .query_row(
                    "UPDATE jobs SET state = 'leased', lease_until = ?2, attempts = attempts + 1
                     WHERE id = (
                         SELECT id FROM jobs
                         WHERE run_after <= ?1
                           AND (state = 'queued' OR (state = 'leased' AND lease_until <= ?1))
                         ORDER BY priority, run_after LIMIT 1
                     )
                     RETURNING id, kind, payload_json, attempts",
                    params![now, until],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()?,
            Some(kinds) => {
                let placeholders = (0..kinds.len())
                    .map(|i| format!("?{}", i + 3))
                    .collect::<Vec<_>>()
                    .join(", ");
                let sql = format!(
                    "UPDATE jobs SET state = 'leased', lease_until = ?2, attempts = attempts + 1
                     WHERE id = (
                         SELECT id FROM jobs
                         WHERE run_after <= ?1
                           AND (state = 'queued' OR (state = 'leased' AND lease_until <= ?1))
                           AND kind IN ({placeholders})
                         ORDER BY priority, run_after LIMIT 1
                     )
                     RETURNING id, kind, payload_json, attempts"
                );
                let mut values: Vec<rusqlite::types::Value> =
                    Vec::with_capacity(2 + kinds.len());
                values.push(now.clone().into());
                values.push(until.clone().into());
                for kind in kinds {
                    values.push((*kind).to_string().into());
                }
                self.conn
                    .query_row(
                        &sql,
                        rusqlite::params_from_iter(values),
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                    )
                    .optional()?
            }
        };

        Ok(claimed.map(|(id, kind, payload, attempts)| Job {
            id,
            kind,
            payload,
            attempts,
        }))
    }

    /// Renews every lease in `job_ids` (multi-doc CLI batches hold several).
    pub fn heartbeat_many(&self, job_ids: &[i64]) -> Result<()> {
        for id in job_ids {
            self.heartbeat(*id)?;
        }
        Ok(())
    }

    /// Extends a claim on work that is still running.
    pub fn heartbeat(&self, job_id: i64) -> Result<()> {
        let until = (Utc::now() + Duration::seconds(LEASE_SECONDS)).to_rfc3339();
        self.conn.execute(
            "UPDATE jobs SET lease_until = ?2 WHERE id = ?1 AND state = 'leased'",
            params![job_id, until],
        )?;
        Ok(())
    }

    /// Marks a job done, and keeps what it did.
    ///
    /// `note` is the sentence the worker hands back — "14 claims · 2 not
    /// read". It used to go to the command line and nowhere else, which
    /// left an owner watching the interface with a queue that emptied and
    /// no account of what came out of it.
    pub fn finish(&self, job_id: i64, note: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs SET state = 'done', finished_at = ?2, lease_until = NULL, note = ?3
             WHERE id = ?1",
            params![job_id, Utc::now().to_rfc3339(), note],
        )?;
        Ok(())
    }

    /// The connection went away. Wait longer and try again — this is not a
    /// failure, it is a network.
    ///
    /// The delay doubles with each attempt and carries a deterministic
    /// scatter derived from the job id, so a thousand jobs queued by the same
    /// outage do not all wake up in the same second.
    pub fn defer(&self, job_id: i64, attempts: i64, error: &str) -> Result<JobState> {
        if attempts >= MAX_ATTEMPTS {
            self.conn.execute(
                "UPDATE jobs SET state = 'dead', last_error = ?2, lease_until = NULL, finished_at = ?3
                 WHERE id = ?1",
                params![job_id, error, Utc::now().to_rfc3339()],
            )?;
            return Ok(JobState::Dead);
        }

        let base = 2i64.saturating_pow(attempts.clamp(0, 10) as u32) * 5;
        let jitter = job_id % base.max(1);
        let run_after = (Utc::now() + Duration::seconds(base + jitter)).to_rfc3339();

        self.conn.execute(
            "UPDATE jobs SET state = 'queued', run_after = ?2, last_error = ?3, lease_until = NULL
             WHERE id = ?1",
            params![job_id, run_after, error],
        )?;
        Ok(JobState::Queued)
    }

    /// The work itself was refused — a span that does not verify, a file that
    /// is a scan. Retrying changes nothing, so it stops here and is reported.
    pub fn fail(&self, job_id: i64, error: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs SET state = 'failed', last_error = ?2, lease_until = NULL, finished_at = ?3
             WHERE id = ?1",
            params![job_id, error, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// Puts a leased job back down without counting it as an attempt.
    ///
    /// Held is not failed and it is not deferred. The work is fine, the
    /// moment is wrong — the owner is on battery, or asked to be asked —
    /// and it must not age towards `dead` while it waits, which is exactly
    /// what repeated deferral would do to a laptop left unplugged overnight.
    pub fn hold(&self, job_id: i64, reason: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs
             SET state = 'held', last_error = ?2, lease_until = NULL,
                 attempts = max(attempts - 1, 0)
             WHERE id = ?1",
            params![job_id, reason],
        )?;
        Ok(())
    }

    /// Lets every held job run. What the owner presses when they plug in or
    /// say go.
    pub fn release_held(&self) -> Result<usize> {
        let released = self.conn.execute(
            "UPDATE jobs SET state = 'queued', run_after = ?1, last_error = NULL
             WHERE state = 'held'",
            params![Utc::now().to_rfc3339()],
        )?;
        Ok(released)
    }

    /// Held jobs, with why, so the interface can say which button helps.
    pub fn held(&self) -> Result<Vec<(String, String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT kind, coalesce(last_error, 'held'), count(*)
             FROM jobs WHERE state = 'held' GROUP BY kind, last_error",
        )?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// How much of one kind of work is still outstanding.
    pub fn pending_of_kind(&self, kind: &str) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT count(*) FROM jobs WHERE kind = ?1 AND state IN ('queued', 'leased', 'held')",
            params![kind],
            |r| r.get(0),
        )?)
    }

    /// The most recent work, newest first.
    ///
    /// Held jobs come too, and they carry no `finished_at` — a job waiting
    /// for the owner to plug the laptop in is the single most useful line
    /// on the panel, and ordering it away because it has no end time is
    /// how it would go missing.
    pub fn recent_work(&self, limit: usize) -> Result<Vec<WorkLine>> {
        let mut stmt = self.conn.prepare(
            "SELECT kind, state, payload_json, note, last_error, finished_at, created_at, attempts
               FROM jobs
              WHERE state IN ('done', 'failed', 'dead', 'held', 'leased')
              ORDER BY coalesce(finished_at, created_at) DESC
              LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit as i64], |r| {
                let payload: String = r.get(2)?;
                let finished: Option<String> = r.get(5)?;
                let created: String = r.get(6)?;
                Ok(WorkLine {
                    kind: r.get(0)?,
                    state: r.get(1)?,
                    subject: subject_of(&payload),
                    note: r.get(3)?,
                    error: r.get(4)?,
                    at: Some(finished.unwrap_or(created)),
                    attempts: r.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// How many jobs have finished since the current burst of work began.
    ///
    /// A burst begins when the oldest job still outstanding was queued.
    /// That definition needs no extra bookkeeping and gets the important
    /// case right: adding a folder this afternoon starts a bar at zero
    /// rather than at whatever nine hundred jobs from last week would make
    /// it. With nothing outstanding there is no burst, and the answer is
    /// nought.
    pub fn finished_this_burst(&self) -> Result<i64> {
        let began: Option<String> = self
            .conn
            .query_row(
                "SELECT min(created_at) FROM jobs WHERE state IN ('queued', 'leased', 'held')",
                [],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let Some(began) = began else {
            return Ok(0);
        };
        Ok(self.conn.query_row(
            "SELECT count(*) FROM jobs
              WHERE state IN ('done', 'failed', 'dead') AND finished_at >= ?1",
            params![began],
            |r| r.get(0),
        )?)
    }

    /// How much is outstanding, by kind, so the interface can say which
    /// stage the work is in without a stored field that could disagree
    /// with the queue.
    pub fn pending_by_kind(&self) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT kind, count(*) FROM jobs
              WHERE state IN ('queued', 'leased', 'held')
              GROUP BY kind",
        )?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn job_counts(&self) -> Result<Vec<(String, i64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT state, count(*) FROM jobs GROUP BY state ORDER BY state")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    #[cfg(test)]
    pub(crate) fn force_lease_expiry(&self, job_id: i64) -> Result<()> {
        let past = (Utc::now() - Duration::seconds(600)).to_rfc3339();
        self.conn.execute(
            "UPDATE jobs SET lease_until = ?2 WHERE id = ?1",
            params![job_id, past],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(key: &str) -> NewJob {
        NewJob {
            kind: "extract".into(),
            payload: "{}".into(),
            idempotency_key: key.into(),
            priority: PRIORITY_NORMAL,
        }
    }

    #[test]
    fn the_same_work_is_only_queued_once() {
        let lake = Lake::in_memory().unwrap();
        assert!(lake.enqueue(&job("a")).unwrap());
        assert!(!lake.enqueue(&job("a")).unwrap());
    }

    #[test]
    fn a_leased_job_is_not_handed_out_twice() {
        let lake = Lake::in_memory().unwrap();
        lake.enqueue(&job("a")).unwrap();
        assert!(lake.lease().unwrap().is_some());
        assert!(lake.lease().unwrap().is_none());
    }

    #[test]
    fn two_queued_jobs_can_be_leased_by_two_workers() {
        let lake = Lake::in_memory().unwrap();
        lake.enqueue(&job("a")).unwrap();
        lake.enqueue(&job("b")).unwrap();
        let first = lake.lease().unwrap().unwrap();
        let second = lake.lease().unwrap().unwrap();
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn lease_kinds_skips_jobs_outside_the_track() {
        let lake = Lake::in_memory().unwrap();
        lake.enqueue(&NewJob {
            kind: "rescan".into(),
            payload: "{}".into(),
            idempotency_key: "rescan:1".into(),
            priority: 0,
        })
        .unwrap();
        lake.enqueue(&NewJob {
            kind: "compile_document".into(),
            payload: "{}".into(),
            idempotency_key: "compile:1".into(),
            priority: 0,
        })
        .unwrap();
        let ai = lake
            .lease_kinds(&["compile_document", "settle"])
            .unwrap()
            .expect("compile must be claimable");
        assert_eq!(ai.kind, "compile_document");
        assert!(lake.lease_kinds(&["compile_document", "settle"]).unwrap().is_none());
        let io = lake.lease_kinds(&["rescan"]).unwrap().expect("rescan waits for io track");
        assert_eq!(io.kind, "rescan");
    }

    #[test]
    fn work_returns_by_itself_when_the_worker_dies() {
        let lake = Lake::in_memory().unwrap();
        lake.enqueue(&job("a")).unwrap();
        let first = lake.lease().unwrap().unwrap();

        // The worker is gone; nothing tells the lake so.
        lake.force_lease_expiry(first.id).unwrap();

        let again = lake.lease().unwrap().expect("an expired lease must free the job");
        assert_eq!(again.id, first.id);
        assert_eq!(again.attempts, 2);
    }

    #[test]
    fn a_lost_connection_is_retried_and_a_refusal_is_not() {
        let lake = Lake::in_memory().unwrap();
        lake.enqueue(&job("a")).unwrap();
        let j = lake.lease().unwrap().unwrap();
        assert_eq!(lake.defer(j.id, j.attempts, "connection reset").unwrap(), JobState::Queued);

        lake.enqueue(&job("b")).unwrap();
        let k = lake.lease().unwrap().unwrap();
        lake.fail(k.id, "no text layer").unwrap();
        let counts = lake.job_counts().unwrap();
        assert!(counts.iter().any(|(state, n)| state == "failed" && *n == 1));
    }

    #[test]
    fn retrying_forever_is_not_an_option() {
        let lake = Lake::in_memory().unwrap();
        lake.enqueue(&job("a")).unwrap();
        let j = lake.lease().unwrap().unwrap();
        assert_eq!(lake.defer(j.id, MAX_ATTEMPTS, "still offline").unwrap(), JobState::Dead);
    }
}
