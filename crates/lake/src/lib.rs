//! The Context Lake: one SQLite file the company owns.
//!
//! Two rules hold this crate together.
//!
//! The first is that **evidence cannot be stored unless it verifies**. There
//! is no `insert_evidence_unchecked`, no flag to skip the check, and the
//! verification is recorded against the rendition hash it ran on. That makes
//! the evidence gate a property of the storage layer rather than a discipline
//! the callers are asked to keep.
//!
//! The second is that **approval is one transaction**. Snapshotting the old
//! version, publishing the new one, rewriting the graph edges and marking the
//! dependents stale either all happen or none do. This is what "the graph
//! updates itself when something changes" actually means here — not a feature
//! of the database, a property of the write path.

mod export;
mod jobs;
mod merge;
mod migrate;
mod policy;
mod serve;

pub use export::{export_approved, ExportReport};

use std::path::Path;

use chrono::Utc;
use knowlith_core::document::BlockKind;
use knowlith_core::object::{RelationOrigin, RelationType};
use knowlith_core::{
    Block, ContextObject, Document, DocumentKind, Evidence, ObjectKind, ObjectStatus, Rejection,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

pub use jobs::{Job, JobState, NewJob, PRIORITY_BACKGROUND, PRIORITY_NORMAL};
pub use merge::MergeHint;
pub use migrate::SCHEMA_VERSION;
pub use policy::{
    DEFAULT_COMPILE_BATCH, DEFAULT_COMPILE_WORKERS, DEFAULT_LARGE_SCAN, Held, MAX_COMPILE_BATCH,
    MAX_COMPILE_WORKERS, Policy, Processing,
};
pub use serve::{Case, Row};

const SCHEMA: &str = include_str!("schema.sql");

/// How long a writer waits for another connection to finish before giving up.
///
/// Three processes share this file: the interface, the background worker and
/// the gateway an AI tool spawns. Without a timeout, a write that lands
/// during someone else's commit fails instantly with `SQLITE_BUSY`, and the
/// owner sees "could not save" for a lock that was gone a millisecond later.
const BUSY_TIMEOUT_MS: u32 = 5_000;

#[derive(Debug, thiserror::Error)]
pub enum LakeError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("no document {0} in the lake")]
    UnknownDocument(String),
    #[error("no object {0} in the lake")]
    UnknownObject(String),
    /// The whole point of the crate. Carries the reason so the review screen
    /// can say which span failed and why.
    #[error("evidence for {object_id} was refused: {rejection:?}")]
    EvidenceRefused {
        object_id: String,
        rejection: Box<Rejection>,
    },
    #[error("an approved object must carry at least one verified source span")]
    NoEvidence,
    #[error("an approved object must say something — the text was empty")]
    NoBody,
}

type Result<T> = std::result::Result<T, LakeError>;

/// Creates the folders above the lake so that only the owner can enter them.
///
/// Only the folders that did not exist are touched. `KNOWLITH_HOME` may point
/// anywhere, and tightening a folder somebody else chose — `/tmp`, a shared
/// drive — would lock other people out of their own files.
fn create_private_dirs(db: &Path) {
    let Some(parent) = db.parent() else { return };
    let mut missing = Vec::new();
    let mut cursor = Some(parent);
    while let Some(dir) = cursor {
        if dir.as_os_str().is_empty() || dir.exists() {
            break;
        }
        missing.push(dir);
        cursor = dir.parent();
    }
    for dir in missing.into_iter().rev() {
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        let _ = builder.create(dir);
    }
}

/// Makes the lake, and the WAL beside it, readable by the owner alone.
///
/// The token was written at 0600 from the start; the file holding the full
/// text of every document the company owns was created with the default
/// umask and came out world-readable. On a shared machine that is every
/// contract and price list, open to every account. SQLite gives `-wal` and
/// `-shm` the mode of the main file when it creates them, so tightening the
/// database first is enough for a fresh lake; the two extra names cover a
/// lake that already existed.
fn keep_private(db: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let private = std::fs::Permissions::from_mode(0o600);
        for suffix in ["", "-wal", "-shm"] {
            let mut name = db.as_os_str().to_owned();
            name.push(suffix);
            let path = Path::new(&name);
            if path.exists() {
                let _ = std::fs::set_permissions(path, private.clone());
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = db;
    }
}

pub struct Lake {
    conn: Connection,
}

/// One edge of the knowledge graph.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub from_id: String,
    pub to_id: String,
    pub kind: RelationType,
    pub origin: RelationOrigin,
    pub why: Option<String>,
    pub confidence: Option<f32>,
}

impl Lake {
    /// Opens or creates the lake at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        create_private_dirs(path);
        let conn = Connection::open(path)?;
        keep_private(path);
        Self::prepare(conn)
    }

    /// An in-memory lake, for tests.
    pub fn in_memory() -> Result<Self> {
        Self::prepare(Connection::open_in_memory()?)
    }

    fn prepare(conn: Connection) -> Result<Self> {
        // WAL so a long compile run does not block the UI reading the same
        // file; FKs so a deleted source really takes its documents with it.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(std::time::Duration::from_millis(BUSY_TIMEOUT_MS as u64))?;
        conn.execute_batch(SCHEMA)?;
        migrate::run(&conn)?;

        Ok(Self { conn })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    // ------------------------------------------------------------ sources --

    pub fn put_source(&self, id: &str, name: &str, root: &str, kind: &str, processor: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sources (id, name, root, kind, processor, status, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6)
             ON CONFLICT(id) DO UPDATE SET name = ?2, root = ?3, processor = ?5",
            params![id, name, root, kind, processor, now()],
        )?;
        Ok(())
    }

    pub fn mark_scanned(&self, source_id: &str) -> Result<()> {
        // A walk that was already queued when the owner pressed Pause still
        // finishes; finishing must not quietly un-pause the folder.
        self.conn.execute(
            "UPDATE sources SET last_scan = ?2, last_error = NULL,
                    status = CASE WHEN status = 'paused' THEN 'paused' ELSE 'active' END
             WHERE id = ?1",
            params![source_id, now()],
        )?;
        Ok(())
    }

    /// Pauses or resumes the reading of one folder. `false` when there is no
    /// such source. A paused source is skipped by [`Lake::scan_targets`], so
    /// this is the row the Sources screen's "Paused" badge rests on — for a
    /// while the badge was set in the browser alone and the worker kept
    /// reading.
    pub fn set_source_paused(&self, source_id: &str, paused: bool) -> Result<bool> {
        let status = if paused { "paused" } else { "active" };
        let changed = self.conn.execute(
            "UPDATE sources SET status = ?2 WHERE id = ?1",
            params![source_id, status],
        )?;
        Ok(changed > 0)
    }

    /// Stops reading a folder for good, without touching what was read.
    ///
    /// The row is kept rather than deleted: `documents` and `evidence`
    /// cascade from `sources`, so a DELETE would pull the quotes out from
    /// under every approved object — and an object with no quote is the one
    /// thing this product refuses to show. A removed source is skipped by
    /// [`Lake::scan_targets`] and hidden from the Sources screen; its name
    /// still resolves in the Activity log. Adding the same folder again
    /// revives it.
    pub fn remove_source(&self, source_id: &str) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE sources SET status = 'removed' WHERE id = ?1",
            params![source_id],
        )?;
        Ok(changed > 0)
    }

    /// Which engine each source was added with, by source id.
    pub fn source_processors(&self) -> Result<std::collections::HashMap<String, String>> {
        let mut stmt = self.conn.prepare_cached("SELECT id, processor FROM sources")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    // ---------------------------------------------------------- documents --

    /// Stores a document and its blocks, replacing any earlier read of the
    /// same content.
    pub fn put_document(&mut self, source_id: &str, doc: &Document) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO documents
               (id, source_id, path, name, kind, byte_len, sha256, text, text_sha256,
                verbatim, modified, columns_json, read_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
               path = ?3, name = ?4, text = ?8, text_sha256 = ?9,
               verbatim = ?10, modified = ?11, columns_json = ?12, read_at = ?13",
            params![
                doc.id,
                source_id,
                doc.path,
                doc.name,
                kind_str(doc.kind),
                doc.byte_len as i64,
                doc.sha256,
                doc.text,
                doc.text_sha256,
                doc.verbatim as i64,
                doc.modified,
                doc.columns.as_ref().map(|c| serde_json::to_string(c)).transpose()?,
                now(),
            ],
        )?;

        tx.execute("DELETE FROM blocks WHERE document_id = ?1", params![doc.id])?;
        tx.execute("DELETE FROM blocks_fts WHERE document_id = ?1", params![doc.id])?;

        for (ordinal, block) in doc.blocks.iter().enumerate() {
            tx.execute(
                "INSERT INTO blocks
                   (document_id, ordinal, locator, kind, text, start_byte, end_byte, page, sheet, row, cells_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    doc.id,
                    ordinal as i64,
                    block.locator,
                    block_kind_str(block.kind),
                    block.text,
                    block.start_byte as i64,
                    block.end_byte as i64,
                    block.page,
                    block.sheet,
                    block.row,
                    block.cells.as_ref().map(|c| serde_json::to_string(c)).transpose()?,
                ],
            )?;
            tx.execute(
                "INSERT INTO blocks_fts (text, document_id, locator) VALUES (?1, ?2, ?3)",
                params![block.text, doc.id, block.locator],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// True when this exact content is already stored, so a rescan can skip
    /// the parse entirely.
    pub fn has_content(&self, sha256: &str) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT count(*) FROM documents WHERE sha256 = ?1",
            params![sha256],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    pub fn document(&self, id: &str) -> Result<Document> {
        let mut doc = self
            .conn
            .query_row(
                "SELECT id, path, name, kind, byte_len, sha256, text, text_sha256, verbatim, modified, columns_json
                 FROM documents WHERE id = ?1",
                params![id],
                |r| {
                    Ok(Document {
                        id: r.get(0)?,
                        path: r.get(1)?,
                        name: r.get(2)?,
                        kind: kind_from(&r.get::<_, String>(3)?),
                        byte_len: r.get::<_, i64>(4)? as u64,
                        sha256: r.get(5)?,
                        text: r.get(6)?,
                        text_sha256: r.get(7)?,
                        verbatim: r.get::<_, i64>(8)? != 0,
                        modified: r.get(9)?,
                        columns: r
                            .get::<_, Option<String>>(10)?
                            .and_then(|s| serde_json::from_str(&s).ok()),
                        blocks: Vec::new(),
                    })
                },
            )
            .optional()?
            .ok_or_else(|| LakeError::UnknownDocument(id.to_string()))?;

        let mut stmt = self.conn.prepare(
            "SELECT locator, kind, text, start_byte, end_byte, page, sheet, row, cells_json
             FROM blocks WHERE document_id = ?1 ORDER BY ordinal",
        )?;
        doc.blocks = stmt
            .query_map(params![id], |r| {
                Ok(Block {
                    locator: r.get(0)?,
                    kind: block_kind_from(&r.get::<_, String>(1)?),
                    text: r.get(2)?,
                    start_byte: r.get::<_, i64>(3)? as usize,
                    end_byte: r.get::<_, i64>(4)? as usize,
                    page: r.get(5)?,
                    sheet: r.get(6)?,
                    row: r.get(7)?,
                    cells: r
                        .get::<_, Option<String>>(8)?
                        .and_then(|s| serde_json::from_str(&s).ok()),
                })
            })?
            .collect::<std::result::Result<_, _>>()?;

        Ok(doc)
    }

    pub fn document_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT count(*) FROM documents", [], |r| r.get(0))?)
    }

    pub fn block_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT count(*) FROM blocks", [], |r| r.get(0))?)
    }

    /// Full-text search over blocks, diacritics-insensitive.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<(String, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT document_id, locator, text FROM blocks_fts
             WHERE blocks_fts MATCH ?1 ORDER BY rank LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![query, limit as i64], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    // ------------------------------------------------------------ objects --

    /// Writes an object together with its evidence.
    ///
    /// Every span is checked against the stored document before anything is
    /// committed. One bad span refuses the whole object: a rule that is half
    /// traceable is not a rule, it is a guess with a footnote.
    pub fn put_object(&mut self, object: &ContextObject) -> Result<()> {
        let tx = self.conn.transaction()?;
        Self::write_object(&tx, object)?;
        tx.commit()?;
        Ok(())
    }

    fn write_object(tx: &Transaction<'_>, object: &ContextObject) -> Result<()> {
        // Re-drafting must not demote an approved skill back to Proposed —
        // that is what made the Skills list show one green and one orange
        // copy of the same procedure.
        let status = {
            let existing: Option<String> = tx
                .query_row(
                    "SELECT status FROM objects WHERE id = ?1",
                    params![object.id],
                    |r| r.get(0),
                )
                .optional()?;
            match (existing.as_deref(), object.kind, object.status) {
                (Some("approved"), ObjectKind::Skill, ObjectStatus::Proposed) => "approved",
                _ => status_str(object.status),
            }
        };

        tx.execute(
            "INSERT INTO objects
               (id, kind, subtype, title, body, status, confidence, version,
                valid_from, valid_to, supersedes, decided_by, edited_on_approval, path, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(id) DO UPDATE SET
               title = ?4, body = ?5, status = ?6, confidence = ?7, version = ?8,
               valid_from = ?9, valid_to = ?10, supersedes = ?11, decided_by = ?12,
               edited_on_approval = ?13, updated_at = ?15",
            params![
                object.id,
                format!("{:?}", object.kind).to_lowercase(),
                object.subtype.map(|s| format!("{s:?}").to_lowercase()),
                object.title,
                object.body,
                status,
                object.confidence.0 as f64,
                object.version as i64,
                object.valid_from,
                object.valid_to,
                object.supersedes,
                object.decided_by,
                object.edited_on_approval as i64,
                object.path,
                object.updated_at,
            ],
        )?;

        tx.execute("DELETE FROM evidence WHERE object_id = ?1", params![object.id])?;
        for span in &object.evidence {
            Self::write_evidence(tx, &object.id, span)?;
        }

        // The search index is maintained here rather than by a trigger so
        // that it lives and dies inside the same transaction as the object:
        // an approval that rolls back must not leave the gateway able to
        // find a rule that no longer exists.
        tx.execute("DELETE FROM objects_fts WHERE object_id = ?1", params![object.id])?;
        tx.execute(
            "INSERT INTO objects_fts (title, body, object_id) VALUES (?1, ?2, ?3)",
            params![object.title, object.body, object.id],
        )?;

        // Only the edges this compile produced. A recompile regenerates
        // structural edges from the text, and must not throw away what the
        // engine proposed or what a person drew by hand — those were
        // decisions, and a rescan is not a reason to undo one.
        tx.execute(
            "DELETE FROM relations WHERE from_id = ?1 AND origin = 'structural'",
            params![object.id],
        )?;
        for relation in &object.relations {
            let confidence = relation
                .edge_confidence
                .or_else(|| Some(edge_confidence_for(relation.origin)));
            tx.execute(
                "INSERT OR REPLACE INTO relations (from_id, to_id, type, origin, created_at, why, confidence)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    object.id,
                    relation.target_id,
                    relation_str(relation.kind),
                    origin_str(relation.origin),
                    now(),
                    relation.why,
                    confidence,
                ],
            )?;
        }

        Ok(())
    }

    /// The gate. Loads the cited document, runs the mechanical check, and
    /// stores the span only if it passes.
    fn write_evidence(tx: &Transaction<'_>, object_id: &str, span: &Evidence) -> Result<()> {
        let (text, text_sha256): (String, String) = tx
            .query_row(
                "SELECT text, text_sha256 FROM documents WHERE id = ?1",
                params![span.document_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| LakeError::UnknownDocument(span.document_id.clone()))?;

        let probe = Document {
            id: span.document_id.clone(),
            path: String::new(),
            name: String::new(),
            kind: DocumentKind::Text,
            byte_len: text.len() as u64,
            sha256: String::new(),
            text,
            text_sha256: text_sha256.clone(),
            verbatim: false,
            modified: String::new(),
            columns: None,
            blocks: Vec::new(),
        };

        knowlith_core::verify(&probe, span).map_err(|rejection| LakeError::EvidenceRefused {
            object_id: object_id.to_string(),
            rejection: Box::new(rejection),
        })?;

        tx.execute(
            "INSERT OR REPLACE INTO evidence
               (object_id, document_id, locator, start_byte, end_byte, quote, verified_at, verified_against)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                object_id,
                span.document_id,
                span.locator,
                span.start_byte as i64,
                span.end_byte as i64,
                span.quote,
                now(),
                text_sha256,
            ],
        )?;
        Ok(())
    }

    pub fn object_ids(&self, status: Option<&str>) -> Result<Vec<String>> {
        let mut stmt = match status {
            Some(_) => self
                .conn
                .prepare("SELECT id FROM objects WHERE status = ?1 ORDER BY id")?,
            None => self.conn.prepare("SELECT id FROM objects ORDER BY id")?,
        };
        let rows: Vec<String> = match status {
            Some(s) => stmt
                .query_map(params![s], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<_, _>>()?,
            None => stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<_, _>>()?,
        };
        Ok(rows)
    }

    /// Every object, with its evidence and outgoing edges attached.
    ///
    /// Read in three queries rather than one join, because an object with
    /// four spans and three relations would otherwise come back twelve times
    /// and have to be folded back together.
    pub fn objects(&self) -> Result<Vec<ContextObject>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, subtype, title, body, status, confidence, version,
                    valid_from, valid_to, supersedes, decided_by, edited_on_approval, path, updated_at
             FROM objects ORDER BY id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ContextObject {
                    id: r.get(0)?,
                    kind: object_kind_from(&r.get::<_, String>(1)?),
                    subtype: r.get::<_, Option<String>>(2)?.as_deref().and_then(subtype_from),
                    title: r.get(3)?,
                    body: r.get(4)?,
                    status: object_status_from(&r.get::<_, String>(5)?),
                    confidence: knowlith_core::Confidence(r.get::<_, f64>(6)? as f32),
                    version: r.get::<_, i64>(7)? as u32,
                    valid_from: r.get(8)?,
                    valid_to: r.get(9)?,
                    supersedes: r.get(10)?,
                    decided_by: r.get(11)?,
                    edited_on_approval: r.get::<_, i64>(12)? != 0,
                    path: r.get(13)?,
                    updated_at: r.get(14)?,
                    evidence: Vec::new(),
                    relations: Vec::new(),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let edges = self.edges()?;
        let titles: std::collections::HashMap<&str, &str> =
            rows.iter().map(|o| (o.id.as_str(), o.title.as_str())).collect();

        let mut out = Vec::with_capacity(rows.len());
        for mut object in rows.iter().cloned() {
            object.evidence = self.evidence_of(&object.id)?;
            object.relations = edges
                .iter()
                .filter(|e| e.from_id == object.id)
                .map(|e| knowlith_core::Relation {
                    target_id: e.to_id.clone(),
                    // The label the owner sees, when the target is something
                    // we know about; the id is a poor substitute but better
                    // than an empty row.
                    target_label: titles.get(e.to_id.as_str()).map(|t| t.to_string()).unwrap_or_else(|| e.to_id.clone()),
                    kind: e.kind,
                    origin: e.origin,
                    why: e.why.clone(),
                    edge_confidence: e.confidence,
                })
                .collect();
            out.push(object);
        }
        Ok(out)
    }

    /// The objects that changed under something already approved.
    pub fn stale_ids(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM objects WHERE stale_since IS NOT NULL ORDER BY id")?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Id → file name for every document, and nothing else.
    ///
    /// `documents()` carries the full text and every block of every file,
    /// which `/api/work` was loading once a second only to turn a job's
    /// subject into a name. On a folder of thousands of files that is the
    /// whole company's text through memory sixty times a minute.
    pub fn document_names(&self) -> Result<std::collections::HashMap<String, String>> {
        let mut stmt = self.conn.prepare_cached("SELECT id, name FROM documents")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    pub fn documents(&self) -> Result<Vec<Document>> {
        let ids: Vec<String> = {
            let mut stmt = self.conn.prepare("SELECT id FROM documents ORDER BY name")?;
            stmt.query_map([], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<_, _>>()?
        };
        ids.iter().map(|id| self.document(id)).collect()
    }

    /// Documents that belong to one source folder.
    pub fn documents_for_source(&self, source_id: &str) -> Result<Vec<Document>> {
        let ids: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id FROM documents WHERE source_id = ?1 ORDER BY name")?;
            stmt.query_map(params![source_id], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<_, _>>()?
        };
        ids.iter().map(|id| self.document(id)).collect()
    }

    /// Proposed objects whose evidence points into this source.
    pub fn proposed_count_for_source(&self, source_id: &str) -> Result<usize> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT o.id)
             FROM objects o
             JOIN evidence e ON e.object_id = o.id
             JOIN documents d ON d.id = e.document_id
             WHERE d.source_id = ?1 AND o.status = 'proposed'",
            params![source_id],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }

    pub fn conflict_count_for_source(&self, source_id: &str) -> Result<usize> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT o.id)
             FROM objects o
             JOIN evidence e ON e.object_id = o.id
             JOIN documents d ON d.id = e.document_id
             WHERE d.source_id = ?1 AND o.status = 'conflicted'",
            params![source_id],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }

    pub fn sources(&self) -> Result<Vec<(String, String, String, String, String, Option<String>)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, root, kind, status, last_scan FROM sources ORDER BY name")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn reject(&self, object_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE objects SET status = 'rejected', updated_at = ?2 WHERE id = ?1",
            params![object_id, now()],
        )?;
        Ok(())
    }

    pub fn evidence_of(&self, object_id: &str) -> Result<Vec<Evidence>> {
        let mut stmt = self.conn.prepare(
            "SELECT document_id, locator, start_byte, end_byte, quote
             FROM evidence WHERE object_id = ?1 ORDER BY id",
        )?;
        let rows = stmt
            .query_map(params![object_id], |r| {
                Ok(Evidence {
                    document_id: r.get(0)?,
                    locator: r.get(1)?,
                    start_byte: r.get::<_, i64>(2)? as usize,
                    end_byte: r.get::<_, i64>(3)? as usize,
                    quote: r.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Re-runs the mechanical check over everything stored, and reports what
    /// no longer holds.
    ///
    /// This is what makes `verified` a claim the product can keep. A parser
    /// change or an edited source file moves the rendition out from under a
    /// span, and without this the badge would be a memory of a check rather
    /// than a check.
    pub fn recheck_evidence(&self) -> Result<Vec<(String, Rejection)>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.object_id, e.document_id, e.locator, e.start_byte, e.end_byte, e.quote
             FROM evidence e ORDER BY e.object_id",
        )?;
        let spans = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    Evidence {
                        document_id: r.get(1)?,
                        locator: r.get(2)?,
                        start_byte: r.get::<_, i64>(3)? as usize,
                        end_byte: r.get::<_, i64>(4)? as usize,
                        quote: r.get(5)?,
                    },
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut broken = Vec::new();
        for (object_id, span) in spans {
            let doc = match self.document(&span.document_id) {
                Ok(d) => d,
                Err(_) => {
                    broken.push((
                        object_id,
                        Rejection::UnknownDocument {
                            document_id: span.document_id.clone(),
                        },
                    ));
                    continue;
                }
            };
            if let Err(rejection) = knowlith_core::verify(&doc, &span) {
                broken.push((object_id, rejection));
            }
        }
        Ok(broken)
    }

    // -------------------------------------------------------------- graph --

    pub fn edges(&self) -> Result<Vec<Edge>> {
        let mut stmt = self.conn.prepare(
            "SELECT from_id, to_id, type, origin, why, confidence FROM relations",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Edge {
                    from_id: r.get(0)?,
                    to_id: r.get(1)?,
                    kind: relation_from(&r.get::<_, String>(2)?),
                    origin: origin_from(&r.get::<_, String>(3)?),
                    why: r.get(4)?,
                    confidence: r.get(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Adds one edge, leaving everything else alone.
    ///
    /// Separate from [`Lake::put_object`] because an edge and an object have
    /// different lifetimes here: a model-proposed dependency outlives the
    /// compile run that the object was last written by.
    pub fn put_relation(
        &self,
        from_id: &str,
        to_id: &str,
        kind: RelationType,
        origin: RelationOrigin,
    ) -> Result<bool> {
        self.put_relation_detail(from_id, to_id, kind, origin, None, None)
    }

    /// Writes one edge, updating `why` and `confidence` when the edge already exists.
    pub fn put_relation_detail(
        &self,
        from_id: &str,
        to_id: &str,
        kind: RelationType,
        origin: RelationOrigin,
        why: Option<&str>,
        confidence: Option<f32>,
    ) -> Result<bool> {
        let confidence = confidence.or_else(|| Some(edge_confidence_for(origin)));
        let changed = self.conn.execute(
            "INSERT INTO relations (from_id, to_id, type, origin, created_at, why, confidence)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(from_id, to_id, type) DO UPDATE SET
                why = COALESCE(excluded.why, relations.why),
                confidence = COALESCE(excluded.confidence, relations.confidence)",
            params![
                from_id,
                to_id,
                relation_str(kind),
                origin_str(origin),
                now(),
                why,
                confidence,
            ],
        )?;
        Ok(changed > 0)
    }

    /// Removes a dependency and its mirror, in one go.
    ///
    /// The owner sees one line saying "this rests on that" and removes one
    /// thing; leaving the `used_by` twin behind would make the impact query
    /// keep reporting a dependency the interface no longer shows.
    pub fn remove_relation(&self, from_id: &str, to_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM relations
             WHERE (from_id = ?1 AND to_id = ?2) OR (from_id = ?2 AND to_id = ?1)",
            params![from_id, to_id],
        )?;
        Ok(())
    }

    /// Approves a proposed object, and settles everything that follows from
    /// it in the same transaction.
    ///
    /// Returns the ids that were marked stale — the objects that now sit on a
    /// changed foundation and should be looked at. Nothing is silently
    /// rewritten: a dependent's text is the owner's to change, and this only
    /// says that it needs their attention.
    pub fn approve(&mut self, object_id: &str, body: &str, decided_by: &str, edited: bool) -> Result<Vec<String>> {
        let tx = self.conn.transaction()?;

        let (version, old_body, old_status, valid_from): (i64, String, String, String) = tx
            .query_row(
                "SELECT version, body, status, valid_from FROM objects WHERE id = ?1",
                params![object_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?
            .ok_or_else(|| LakeError::UnknownObject(object_id.to_string()))?;

        let spans: i64 = tx.query_row(
            "SELECT count(*) FROM evidence WHERE object_id = ?1",
            params![object_id],
            |r| r.get(0),
        )?;
        if spans == 0 {
            return Err(LakeError::NoEvidence);
        }

        // An approved object with no text is not a decision, it is a heading.
        // The gateway would serve it as a title with nothing underneath, and
        // an agent reading that has been told a rule exists and not what it
        // says — which is worse than not being told at all.
        if body.trim().is_empty() {
            return Err(LakeError::NoBody);
        }

        let at = now();

        // The version that was in effect until now is kept, not overwritten.
        tx.execute(
            "INSERT OR REPLACE INTO object_versions
               (object_id, version, body, status, valid_from, valid_to, decided_by, snapshot_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![object_id, version, old_body, old_status, valid_from, at, decided_by, at],
        )?;

        tx.execute(
            "UPDATE objects
             SET body = ?2, status = 'approved', version = version + 1, valid_from = ?3,
                 valid_to = NULL, decided_by = ?4, edited_on_approval = ?5,
                 updated_at = ?3, stale_since = NULL
             WHERE id = ?1",
            params![object_id, body, at, decided_by, edited as i64],
        )?;

        // The owner edited the wording; the index has to say what the rule
        // now says, or an agent searching for the approved text does not
        // find the approved object.
        tx.execute("DELETE FROM objects_fts WHERE object_id = ?1", params![object_id])?;
        tx.execute(
            "INSERT INTO objects_fts (title, body, object_id)
             SELECT title, ?2, id FROM objects WHERE id = ?1",
            params![object_id, body],
        )?;

        // Everything that uses this now rests on something that moved. This
        // is the propagation step, and it runs here rather than in a
        // background job so it can never disagree with the approval.
        let affected = dependents(&tx, object_id)?;
        for id in &affected {
            tx.execute(
                "UPDATE objects SET stale_since = ?2 WHERE id = ?1 AND stale_since IS NULL",
                params![id, at],
            )?;
        }

        tx.commit()?;
        Ok(affected)
    }

    /// Owner rewrote a live claim. It leaves the served set until they
    /// approve again. Evidence stays — the quote still has to hold.
    pub fn suggest_edit(&mut self, object_id: &str, body: &str) -> Result<()> {
        if body.trim().is_empty() {
            return Err(LakeError::NoBody);
        }
        let spans: i64 = self.conn.query_row(
            "SELECT count(*) FROM evidence WHERE object_id = ?1",
            params![object_id],
            |r| r.get(0),
        )?;
        if spans == 0 {
            return Err(LakeError::NoEvidence);
        }
        let at = now();
        let n = self.conn.execute(
            "UPDATE objects
             SET body = ?2, status = 'proposed', decided_by = NULL, edited_on_approval = 0,
                 updated_at = ?3, stale_since = NULL
             WHERE id = ?1",
            params![object_id, body, at],
        )?;
        if n == 0 {
            return Err(LakeError::UnknownObject(object_id.to_string()));
        }
        self.conn
            .execute("DELETE FROM objects_fts WHERE object_id = ?1", params![object_id])?;
        self.conn.execute(
            "INSERT INTO objects_fts (title, body, object_id)
             SELECT title, ?2, id FROM objects WHERE id = ?1",
            params![object_id, body],
        )?;
        Ok(())
    }

    /// Records that a tool actually read an object.
    pub fn record_read(&self, object_id: &str, tool: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO tool_reads (object_id, tool, read_at) VALUES (?1, ?2, ?3)",
            params![object_id, tool, now()],
        )?;
        Ok(())
    }

    /// The tools that read an object, most recent first.
    pub fn reads_of(&self, object_id: &str) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT tool, max(read_at) FROM tool_reads WHERE object_id = ?1
             GROUP BY tool ORDER BY 2 DESC",
        )?;
        let rows = stmt
            .query_map(params![object_id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    // --------------------------------------------------------- candidates --

    /// Replaces everything the engine said about one document.
    ///
    /// Stored as opaque JSON so this crate does not have to know the
    /// compiler's shape: the lake's job is that the answer survives a
    /// restart, not that it understands it.
    pub fn put_candidates(&mut self, document_id: &str, json: &[String]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM candidates WHERE document_id = ?1", params![document_id])?;
        for (ordinal, one) in json.iter().enumerate() {
            tx.execute(
                "INSERT INTO candidates (document_id, ordinal, json, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![document_id, ordinal as i64, one, now()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Everything the engine has ever said, per document, in a stable order.
    pub fn candidates(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT document_id, json FROM candidates ORDER BY document_id, ordinal")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn candidate_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT count(*) FROM candidates", [], |r| r.get(0))?)
    }

    // ------------------------------------------------------------- worker --

    /// Everything the background worker needs to know about a source, in one
    /// row: where to walk, which engine the owner chose for it, and when it
    /// was last read.
    pub fn scan_targets(&self) -> Result<Vec<ScanTarget>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, root, processor, last_scan FROM sources
             WHERE status NOT IN ('paused', 'removed') ORDER BY id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ScanTarget {
                    id: r.get(0)?,
                    root: r.get(1)?,
                    processor: r.get(2)?,
                    last_scan: r.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Records that a scan could not be done, where the owner will see it.
    pub fn mark_source_error(&self, source_id: &str, error: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sources SET status = 'error', last_error = ?2 WHERE id = ?1",
            params![source_id, error],
        )?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, value],
        )?;
        Ok(())
    }

    /// Marks an object as resting on something that has moved.
    ///
    /// The worker uses this after a recheck: a quote that no longer matches
    /// its source does not silently disappear, and it does not silently keep
    /// being served as verified either.
    pub fn mark_stale(&self, object_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE objects SET stale_since = ?2 WHERE id = ?1 AND stale_since IS NULL",
            params![object_id, now()],
        )?;
        Ok(())
    }
}

/// One folder the worker is responsible for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanTarget {
    pub id: String,
    pub root: String,
    /// `codex` | `claude-code` | `managed`, as the owner chose it.
    pub processor: String,
    pub last_scan: Option<String>,
}

/// Everything reachable from `object_id` by following `used_by`, transitively.
///
/// Written as a recursive CTE rather than a loop in Rust so it stays inside
/// the approval transaction and sees exactly the rows that transaction sees.
fn dependents(tx: &Transaction<'_>, object_id: &str) -> Result<Vec<String>> {
    let mut stmt = tx.prepare(
        "WITH RECURSIVE downstream(id) AS (
             SELECT to_id FROM relations WHERE from_id = ?1 AND type = 'used_by'
             UNION
             SELECT r.to_id FROM relations r
             JOIN downstream d ON r.from_id = d.id
             WHERE r.type = 'used_by'
         )
         SELECT id FROM downstream WHERE id <> ?1 ORDER BY id",
    )?;
    let rows = stmt
        .query_map(params![object_id], |r| r.get(0))?
        .collect::<std::result::Result<Vec<String>, _>>()?;
    Ok(rows)
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn kind_str(kind: DocumentKind) -> &'static str {
    match kind {
        DocumentKind::Markdown => "markdown",
        DocumentKind::Text => "text",
        DocumentKind::Csv => "csv",
        DocumentKind::Xlsx => "xlsx",
        DocumentKind::Docx => "docx",
        DocumentKind::Pdf => "pdf",
    }
}

fn kind_from(s: &str) -> DocumentKind {
    match s {
        "markdown" => DocumentKind::Markdown,
        "csv" => DocumentKind::Csv,
        "xlsx" => DocumentKind::Xlsx,
        "docx" => DocumentKind::Docx,
        "pdf" => DocumentKind::Pdf,
        _ => DocumentKind::Text,
    }
}

fn block_kind_str(kind: BlockKind) -> &'static str {
    match kind {
        BlockKind::Heading => "heading",
        BlockKind::Paragraph => "paragraph",
        BlockKind::ListItem => "list_item",
        BlockKind::TableRow => "table_row",
    }
}

fn block_kind_from(s: &str) -> BlockKind {
    match s {
        "heading" => BlockKind::Heading,
        "list_item" => BlockKind::ListItem,
        "table_row" => BlockKind::TableRow,
        _ => BlockKind::Paragraph,
    }
}

fn object_kind_from(s: &str) -> knowlith_core::ObjectKind {
    use knowlith_core::ObjectKind::*;
    match s {
        "process" => Process,
        "term" => Term,
        "fact" => Fact,
        "skill" => Skill,
        _ => Rule,
    }
}

fn object_status_from(s: &str) -> knowlith_core::ObjectStatus {
    use knowlith_core::ObjectStatus::*;
    match s {
        "approved" => Approved,
        "conflicted" => Conflicted,
        "superseded" => Superseded,
        "rejected" => Rejected,
        _ => Proposed,
    }
}

fn subtype_from(s: &str) -> Option<knowlith_core::Subtype> {
    use knowlith_core::Subtype::*;
    match s {
        "term" => Some(Term),
        "product" => Some(Product),
        "policy" => Some(Policy),
        "reference" => Some(Reference),
        "template" => Some(Template),
        _ => None,
    }
}

fn status_str(status: knowlith_core::ObjectStatus) -> &'static str {
    use knowlith_core::ObjectStatus::*;
    match status {
        Proposed => "proposed",
        Approved => "approved",
        Conflicted => "conflicted",
        Superseded => "superseded",
        Rejected => "rejected",
    }
}

/// Default trust for graph expansion, by how the edge was created.
fn edge_confidence_for(origin: RelationOrigin) -> f32 {
    match origin {
        RelationOrigin::Structural | RelationOrigin::Manual => 1.0,
        RelationOrigin::Model => 0.75,
    }
}

fn relation_str(kind: RelationType) -> &'static str {
    match kind {
        RelationType::DependsOn => "depends_on",
        RelationType::UsedBy => "used_by",
        RelationType::DerivedFrom => "derived_from",
        RelationType::ConflictsWith => "conflicts_with",
    }
}

fn relation_from(s: &str) -> RelationType {
    match s {
        "used_by" => RelationType::UsedBy,
        "derived_from" => RelationType::DerivedFrom,
        "conflicts_with" => RelationType::ConflictsWith,
        _ => RelationType::DependsOn,
    }
}

fn origin_str(origin: RelationOrigin) -> &'static str {
    match origin {
        RelationOrigin::Structural => "structural",
        RelationOrigin::Model => "model",
        RelationOrigin::Manual => "manual",
    }
}

fn origin_from(s: &str) -> RelationOrigin {
    match s {
        "model" => RelationOrigin::Model,
        "manual" => RelationOrigin::Manual,
        _ => RelationOrigin::Structural,
    }
}
