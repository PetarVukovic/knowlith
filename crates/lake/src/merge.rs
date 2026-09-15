//! Two objects that may be one.
//!
//! The compiler groups claims that cite the same sentence, which is an
//! identity rather than a guess. What it cannot do is notice that two
//! documents worded one discount rule differently and never share a
//! sentence — grouping by title turns those into two entries, and on a real
//! twelve-file folder that was most of the duplication.
//!
//! Closing that gap by lowering the bar for merging would mean a similarity
//! score deciding that two of the company's rules are one rule. A wrong
//! merge deletes a rule quietly, and quiet deletion is the failure this
//! product can least afford.
//!
//! So nothing here merges anything. It records a **question**, the owner
//! answers it once, and the answer is kept: a dismissed pair is never
//! offered again.

use rusqlite::params;

use crate::{Lake, LakeError, Result, now};

/// A pair worth asking about, with both sides resolved for display.
#[derive(Debug, Clone, PartialEq)]
pub struct MergeHint {
    pub left_id: String,
    pub left_title: String,
    pub left_body: String,
    pub right_id: String,
    pub right_title: String,
    pub right_body: String,
    /// `duplicate` or `disagreement`.
    pub kind: String,
    pub score: f32,
}

impl Lake {
    /// Records a pair, unless the owner has already answered about it.
    ///
    /// `INSERT OR IGNORE` is the whole of that guarantee: once a row exists
    /// in any state, a later scan cannot reset it to `open`.
    pub fn put_merge_hint(&self, left_id: &str, right_id: &str, kind: &str, score: f32) -> Result<bool> {
        // Ordered, so the same pair found in the other direction is the same
        // row rather than a second question about the same two objects.
        let (a, b) = if left_id <= right_id {
            (left_id, right_id)
        } else {
            (right_id, left_id)
        };
        let changed = self.conn.execute(
            "INSERT OR IGNORE INTO merge_hints (left_id, right_id, kind, score, state, created_at)
             VALUES (?1, ?2, ?5, ?3, 'open', ?4)",
            params![a, b, score as f64, now(), kind],
        )?;
        Ok(changed > 0)
    }

    /// The pairs still waiting for an answer, strongest first.
    ///
    /// A hint whose objects are gone, rejected or already superseded is not
    /// returned — the question stopped making sense and asking it anyway
    /// would be asking about something the owner cannot see.
    pub fn open_merge_hints(&self) -> Result<Vec<MergeHint>> {
        let mut stmt = self.conn.prepare(
            "SELECT h.left_id, l.title, l.body, h.right_id, r.title, r.body, h.kind, h.score
             FROM merge_hints h
             JOIN objects l ON l.id = h.left_id
             JOIN objects r ON r.id = h.right_id
             WHERE h.state = 'open'
               AND l.status IN ('proposed', 'conflicted', 'approved')
               AND r.status IN ('proposed', 'conflicted', 'approved')
             ORDER BY h.kind, h.score DESC, h.left_id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(MergeHint {
                    left_id: r.get(0)?,
                    left_title: r.get(1)?,
                    left_body: r.get(2)?,
                    right_id: r.get(3)?,
                    right_title: r.get(4)?,
                    right_body: r.get(5)?,
                    kind: r.get(6)?,
                    score: r.get::<_, f64>(7)? as f32,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn dismiss_merge_hint(&self, left_id: &str, right_id: &str) -> Result<()> {
        let (a, b) = order(left_id, right_id);
        self.conn.execute(
            "UPDATE merge_hints SET state = 'dismissed', decided_at = ?3
             WHERE left_id = ?1 AND right_id = ?2",
            params![a, b, now()],
        )?;
        Ok(())
    }

    /// Folds `drop_id` into `keep_id`, in one transaction.
    ///
    /// Four things move together or not at all: the evidence, the edges that
    /// pointed at the dropped object, its status, and the hint that asked
    /// the question. Doing any of them separately leaves the graph naming an
    /// object the owner can no longer reach.
    ///
    /// The dropped object is superseded, never deleted. "What did we have on
    /// file in March" has to keep having an answer, and a merge the owner
    /// regrets has to leave something to go back to.
    pub fn merge_objects(&mut self, keep_id: &str, drop_id: &str) -> Result<()> {
        if keep_id == drop_id {
            return Err(LakeError::UnknownObject(drop_id.to_string()));
        }
        let spans = self.evidence_of(drop_id)?;
        let at = now();
        let tx = self.conn.transaction()?;

        let exists: i64 = tx.query_row(
            "SELECT count(*) FROM objects WHERE id IN (?1, ?2)",
            params![keep_id, drop_id],
            |r| r.get(0),
        )?;
        if exists != 2 {
            return Err(LakeError::UnknownObject(format!("{keep_id} or {drop_id}")));
        }

        // The evidence goes through the same gate it did the first time.
        // Re-checking spans we already stored looks redundant until the
        // rendition underneath one of them has moved, in which case copying
        // the row would launder a stale quote into a live object.
        for span in &spans {
            Self::write_evidence(&tx, keep_id, span)?;
        }

        // Anything that named the dropped object now names the kept one.
        tx.execute(
            "UPDATE OR IGNORE relations SET to_id = ?1 WHERE to_id = ?2",
            params![keep_id, drop_id],
        )?;
        // An edge that could not be moved because the kept object already
        // had it is a duplicate, not a survivor.
        tx.execute("DELETE FROM relations WHERE to_id = ?1", params![drop_id])?;
        tx.execute("DELETE FROM relations WHERE from_id = to_id", [])?;

        tx.execute(
            "UPDATE objects SET status = 'superseded', valid_to = ?2, updated_at = ?2 WHERE id = ?1",
            params![drop_id, at],
        )?;
        tx.execute(
            "UPDATE objects SET supersedes = ?2, updated_at = ?3 WHERE id = ?1",
            params![keep_id, drop_id, at],
        )?;

        let (a, b) = order(keep_id, drop_id);
        tx.execute(
            "UPDATE merge_hints SET state = 'merged', decided_at = ?3
             WHERE left_id = ?1 AND right_id = ?2",
            params![a, b, at],
        )?;

        tx.commit()?;
        Ok(())
    }

    pub fn merge_hint_counts(&self) -> Result<Vec<(String, i64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT state, count(*) FROM merge_hints GROUP BY state ORDER BY state")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

fn order<'a>(left: &'a str, right: &'a str) -> (&'a str, &'a str) {
    if left <= right { (left, right) } else { (right, left) }
}
