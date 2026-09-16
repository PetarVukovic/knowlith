//! What the gateway reads, and what it records about having read it.
//!
//! Everything in here is shaped by one difference between answering a person
//! and answering an agent. A person who gets an incomplete answer notices:
//! they know they asked about a quote and got only the price. An agent does
//! not notice. It writes a confident paragraph around whatever it was given,
//! and the gap becomes a sentence the owner has to catch in review.
//!
//! So the lake lets a caller declare what it is working on, records what it
//! actually read against that declaration, and can be asked afterwards which
//! of the relevant things were never looked at. That is the `cases` table,
//! and it is the reason this module is not just three `SELECT`s.

use chrono::Utc;
use knowlith_core::{ContextObject, Document};
use rusqlite::{OptionalExtension, params};

use crate::{Lake, Result, now};

/// One piece of work an agent declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    pub id: String,
    pub question: String,
    /// The object ids the gateway said were relevant when the case opened.
    pub areas: Vec<String>,
    pub opened_at: String,
    pub closed_at: Option<String>,
    pub summary: Option<String>,
}

/// One row of a spreadsheet, addressed well enough to quote.
///
/// A figure is read from here or it is not given. The whole reason prices
/// were kept out of the review queue is that a model recalling a number
/// returns something *close*, and close is the one thing a price may never
/// be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub document_id: String,
    pub document_name: String,
    pub locator: String,
    pub sheet: Option<String>,
    pub row: Option<u32>,
    pub text: String,
    pub cells: Vec<String>,
    pub columns: Vec<String>,
}

impl Lake {
    // ------------------------------------------------------------ objects --

    /// One object with its evidence and edges, or `None`.
    pub fn object(&self, id: &str) -> Result<Option<ContextObject>> {
        Ok(self.objects()?.into_iter().find(|o| o.id == id))
    }

    /// When an object's foundation moved after it was approved.
    pub fn stale_since(&self, id: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT stale_since FROM objects WHERE id = ?1",
                params![id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }

    /// Object ids matching a question, best first.
    ///
    /// Ranked by SQLite's own `bm25`, which is worth saying plainly: there is
    /// no embedding here and no similarity threshold to tune. The set being
    /// searched is a few hundred short paragraphs the owner approved, and on
    /// that set word matching with folded diacritics beats a vector index
    /// that would also have to be kept in sync with the approvals.
    pub fn search_objects(&self, question: &str, limit: usize) -> Result<Vec<String>> {
        let Some(query) = fts_query(question) else {
            return Ok(Vec::new());
        };
        let mut stmt = self.conn.prepare(
            "SELECT object_id FROM objects_fts
             WHERE objects_fts MATCH ?1
             ORDER BY bm25(objects_fts, 4.0, 1.0)
             LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![query, limit as i64], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Spreadsheet and table rows matching a question, best first.
    pub fn rows_matching(&self, question: &str, limit: usize) -> Result<Vec<Row>> {
        let Some(query) = fts_query(question) else {
            return Ok(Vec::new());
        };

        let mut stmt = self.conn.prepare(
            "SELECT b.document_id, d.name, b.locator, b.sheet, b.row, b.text,
                    b.cells_json, d.columns_json
             FROM blocks_fts f
             JOIN blocks b ON b.document_id = f.document_id AND b.locator = f.locator
             JOIN documents d ON d.id = b.document_id
             WHERE blocks_fts MATCH ?1 AND b.kind = 'table_row'
             ORDER BY rank
             LIMIT ?2",
        )?;

        let rows = stmt
            .query_map(params![query, limit as i64], |r| {
                let cells: Option<String> = r.get(6)?;
                let columns: Option<String> = r.get(7)?;
                Ok(Row {
                    document_id: r.get(0)?,
                    document_name: r.get(1)?,
                    locator: r.get(2)?,
                    sheet: r.get(3)?,
                    row: r.get::<_, Option<i64>>(4)?.map(|n| n as u32),
                    text: r.get(5)?,
                    cells: cells
                        .and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok())
                        .unwrap_or_default(),
                    columns: columns
                        .and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok())
                        .unwrap_or_default(),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// The passage a citation points at, with a little of what surrounds it.
    ///
    /// `before` and `after` are byte budgets, trimmed back to the nearest
    /// character boundary — a span that splits a `č` in half would come back
    /// as a decoding error rather than as context.
    pub fn passage(&self, document_id: &str, start: usize, end: usize, margin: usize) -> Result<Option<(Document, String)>> {
        let document = match self.document(document_id) {
            Ok(document) => document,
            Err(crate::LakeError::UnknownDocument(_)) => return Ok(None),
            Err(other) => return Err(other),
        };

        let text = &document.text;
        if start >= end || end > text.len() {
            return Ok(None);
        }

        let from = floor_boundary(text, start.saturating_sub(margin));
        let to = ceil_boundary(text, (end + margin).min(text.len()));
        let passage = text[from..to].to_string();
        Ok(Some((document, passage)))
    }

    // -------------------------------------------------------------- cases --

    /// Opens a case and returns its id.
    pub fn open_case(&self, question: &str, areas: &[String]) -> Result<String> {
        let opened = Utc::now();
        // Readable in a log and unique enough for a machine that is not
        // serving a thousand agents a second. A random id would be harder to
        // match up with what the owner sees in the interface.
        let id = format!(
            "case:{}-{:04x}",
            opened.format("%Y%m%dT%H%M%S"),
            (areas.len() as u32).wrapping_mul(2654435761) ^ (opened.timestamp_subsec_nanos() & 0xffff)
        );
        self.conn.execute(
            "INSERT INTO cases (id, question, areas_json, opened_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, question, serde_json::to_string(areas)?, now()],
        )?;
        Ok(id)
    }

    pub fn case(&self, id: &str) -> Result<Option<Case>> {
        let found = self
            .conn
            .query_row(
                "SELECT id, question, areas_json, opened_at, closed_at, summary
                 FROM cases WHERE id = ?1",
                params![id],
                |r| {
                    let areas: String = r.get(2)?;
                    Ok(Case {
                        id: r.get(0)?,
                        question: r.get(1)?,
                        areas: serde_json::from_str(&areas).unwrap_or_default(),
                        opened_at: r.get(3)?,
                        closed_at: r.get(4)?,
                        summary: r.get(5)?,
                    })
                },
            )
            .optional()?;
        Ok(found)
    }

    /// Records a serve, against a case when the caller declared one.
    pub fn record_case_read(&self, object_id: &str, tool: &str, case_id: Option<&str>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO tool_reads (object_id, tool, read_at, case_id) VALUES (?1, ?2, ?3, ?4)",
            params![object_id, tool, now(), case_id],
        )?;
        Ok(())
    }

    /// The distinct objects actually read under a case.
    pub fn case_reads(&self, case_id: &str) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT object_id FROM tool_reads WHERE case_id = ?1")?;
        let rows = stmt
            .query_map(params![case_id], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn close_case(&self, case_id: &str, summary: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE cases SET closed_at = ?2, summary = ?3 WHERE id = ?1 AND closed_at IS NULL",
            params![case_id, now(), summary],
        )?;
        Ok(())
    }

    /// Cases still open, oldest first.
    ///
    /// An agent that crashed mid-conversation leaves one here. They are not
    /// cleaned up automatically: an abandoned case is a real thing that
    /// happened, and the owner seeing "three questions were asked and never
    /// finished" is information, not clutter.
    pub fn open_cases(&self, limit: usize) -> Result<Vec<Case>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, question, areas_json, opened_at, closed_at, summary
             FROM cases WHERE closed_at IS NULL ORDER BY opened_at DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit as i64], |r| {
                let areas: String = r.get(2)?;
                Ok(Case {
                    id: r.get(0)?,
                    question: r.get(1)?,
                    areas: serde_json::from_str(&areas).unwrap_or_default(),
                    opened_at: r.get(3)?,
                    closed_at: r.get(4)?,
                    summary: r.get(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// How many times anything has been served, and to which tools.
    pub fn read_summary(&self) -> Result<Vec<(String, i64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT tool, count(*) FROM tool_reads GROUP BY tool ORDER BY 2 DESC")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// A number that changes whenever anything servable changes.
    ///
    /// The gateway polls this to decide whether to tell its client that the
    /// tool and prompt lists moved. Cheaper than watching the file, and
    /// correct across the three processes that write here — a file watcher
    /// would fire on every WAL checkpoint and miss nothing but say nothing
    /// useful either.
    pub fn servable_revision(&self) -> Result<String> {
        let (count, latest): (i64, Option<String>) = self.conn.query_row(
            "SELECT count(*), max(updated_at) FROM objects WHERE status = 'approved'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(format!("{count}:{}", latest.unwrap_or_default()))
    }
}

/// Turns a person's question into something FTS5 will accept.
///
/// This is not cosmetic. FTS5 treats `:`, `"`, `*`, `-`, `NEAR` and `OR` as
/// syntax, so the literal question "rok plaćanja: 30 dana?" is a syntax
/// error, and a syntax error here reaches the agent as "the company has no
/// knowledge about that" — the most damaging wrong answer this system can
/// give. Every token is therefore quoted and the whole thing is an `OR`,
/// because an agent's question carries words the documents will not have.
fn fts_query(question: &str) -> Option<String> {
    let terms: Vec<String> = question
        .split(|c: char| !c.is_alphanumeric())
        .filter(|term| term.chars().count() >= 2)
        .take(24)
        .map(|term| format!("\"{}\"", term.replace('"', "")))
        .collect();

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" OR "))
    }
}

fn floor_boundary(text: &str, mut index: usize) -> usize {
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn ceil_boundary(text: &str, mut index: usize) -> usize {
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn punctuation_in_a_question_is_not_a_syntax_error() {
        let query = fts_query("rok plaćanja: 30 dana?").unwrap();
        assert_eq!(query, "\"rok\" OR \"plaćanja\" OR \"30\" OR \"dana\"");
    }

    #[test]
    fn fts_operators_typed_by_a_person_are_treated_as_words() {
        // "OR", "NEAR" and a bare `*` would each change the meaning of the
        // query if they were passed through.
        let query = fts_query("popust OR rabat NEAR* cijena").unwrap();
        assert!(query.contains("\"OR\""));
        assert!(query.contains("\"NEAR\""));
        assert!(!query.contains('*'));
    }

    #[test]
    fn a_question_with_no_words_matches_nothing_rather_than_everything() {
        assert!(fts_query("?! ...").is_none());
        assert!(fts_query("").is_none());
    }

    #[test]
    fn a_quote_cannot_escape_the_quoting() {
        let query = fts_query("cijena \"montaže\"").unwrap();
        assert_eq!(query, "\"cijena\" OR \"montaže\"");
    }

    #[test]
    fn boundaries_never_split_a_character() {
        let text = "cijena je 5.000 kuna po krugu";
        assert_eq!(floor_boundary(text, 0), 0);
        assert_eq!(ceil_boundary(text, text.len()), text.len());

        // 'ć' occupies two bytes, so byte 4 is inside it.
        let croatian = "plaćanje";
        assert!(!croatian.is_char_boundary(4));
        assert_eq!(floor_boundary(croatian, 4), 3);
        assert_eq!(ceil_boundary(croatian, 4), 5);
    }
}
