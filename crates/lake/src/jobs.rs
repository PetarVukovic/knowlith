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
        let at = Utc::now();
        let now = at.to_rfc3339();
        let until = (at + Duration::seconds(LEASE_SECONDS)).to_rfc3339();

        let claimed: Option<(i64, String, String, i64)> = self
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
            .optional()?;

        Ok(claimed.map(|(id, kind, payload, attempts)| Job {
            id,
            kind,
            payload,
            attempts,
        }))
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

    pub fn finish(&self, job_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs SET state = 'done', finished_at = ?2, lease_until = NULL WHERE id = ?1",
            params![job_id, Utc::now().to_rfc3339()],
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

    /// How much of one kind of work is still outstanding.
    pub fn pending_of_kind(&self, kind: &str) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT count(*) FROM jobs WHERE kind = ?1 AND state IN ('queued', 'leased')",
            params![kind],
            |r| r.get(0),
        )?)
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
