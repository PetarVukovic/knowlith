//! Build supervisor storage: entities, sessions, and the owner confirmation quiz.

use chrono::Utc;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::{Lake, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRow {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub summary: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuizQuestion {
    pub id: String,
    pub question: String,
    pub agent_answer: String,
    pub evidence: Vec<QuizEvidence>,
    pub proposed_object_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuizEvidence {
    pub document_id: String,
    pub document_name: String,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildQuiz {
    pub id: String,
    pub session_id: String,
    pub state: String,
    pub questions: Vec<QuizQuestion>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityRow {
    pub id: String,
    pub label: String,
    pub document_ids: Vec<String>,
}

impl Lake {
    pub fn build_phase(&self) -> Result<Option<String>> {
        self.setting("build_phase")
    }

    pub fn set_build_phase(&self, phase: &str) -> Result<()> {
        self.set_setting("build_phase", phase)
    }

    pub fn put_entity(&self, row: &EntityRow) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO entities (id, title, kind, summary, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(id) DO UPDATE SET
               title = excluded.title,
               kind = excluded.kind,
               summary = excluded.summary,
               status = excluded.status,
               updated_at = excluded.updated_at",
            rusqlite::params![row.id, row.title, row.kind, row.summary, row.status, now],
        )?;
        Ok(())
    }

    pub fn link_episode_entity(&self, document_id: &str, entity_id: &str, role: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT OR IGNORE INTO episode_entities (document_id, entity_id, role, linked_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![document_id, entity_id, role, now],
        )?;
        Ok(())
    }

    pub fn entities(&self) -> Result<Vec<EntityRow>> {
        let conn = &self.conn;
        let mut stmt = conn.prepare(
            "SELECT id, title, kind, summary, status FROM entities ORDER BY title",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(EntityRow {
                id: r.get(0)?,
                title: r.get(1)?,
                kind: r.get(2)?,
                summary: r.get(3)?,
                status: r.get(4)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn put_community(&self, row: &CommunityRow) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let docs = serde_json::to_string(&row.document_ids)?;
        self.conn.execute(
            "INSERT INTO communities (id, label, document_ids_json, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
               label = excluded.label,
               document_ids_json = excluded.document_ids_json",
            rusqlite::params![row.id, row.label, docs, now],
        )?;
        Ok(())
    }

    pub fn communities(&self) -> Result<Vec<CommunityRow>> {
        let conn = &self.conn;
        let mut stmt =
            conn.prepare("SELECT id, label, document_ids_json FROM communities ORDER BY label")?;
        let rows = stmt.query_map([], |r| {
            let docs: String = r.get(2)?;
            Ok(CommunityRow {
                id: r.get(0)?,
                label: r.get(1)?,
                document_ids: serde_json::from_str(&docs).unwrap_or_default(),
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn open_supervisor_session(&self, id: &str, engine: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO supervisor_sessions (id, state, engine, turn_count, started_at)
             VALUES (?1, 'active', ?2, 0, ?3)
             ON CONFLICT(id) DO UPDATE SET state = 'active', engine = excluded.engine",
            rusqlite::params![id, engine, now],
        )?;
        Ok(())
    }

    pub fn append_supervisor_turn(&self, session_id: &str, role: &str, content: &str) -> Result<()> {
        let conn = &self.conn;
        let ordinal: i64 = conn.query_row(
            "SELECT COALESCE(MAX(ordinal), -1) + 1 FROM supervisor_turns WHERE session_id = ?1",
            rusqlite::params![session_id],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT INTO supervisor_turns (session_id, ordinal, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![session_id, ordinal, role, content, Utc::now().to_rfc3339()],
        )?;
        conn.execute(
            "UPDATE supervisor_sessions SET turn_count = turn_count + 1 WHERE id = ?1",
            rusqlite::params![session_id],
        )?;
        Ok(())
    }

    pub fn supervisor_turns(&self, session_id: &str) -> Result<Vec<(String, String)>> {
        let conn = &self.conn;
        let mut stmt = conn.prepare(
            "SELECT role, content FROM supervisor_turns
             WHERE session_id = ?1 ORDER BY ordinal",
        )?;
        let rows = stmt.query_map(rusqlite::params![session_id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn finish_supervisor_session(&self, id: &str, state: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE supervisor_sessions SET state = ?2, finished_at = ?3 WHERE id = ?1",
            rusqlite::params![id, state, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn put_build_quiz(&self, quiz: &BuildQuiz) -> Result<()> {
        let json = serde_json::to_string(&quiz.questions)?;
        self.conn.execute(
            "INSERT INTO build_quiz (id, session_id, state, questions_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
               state = excluded.state,
               questions_json = excluded.questions_json",
            rusqlite::params![
                quiz.id,
                quiz.session_id,
                quiz.state,
                json,
                quiz.created_at
            ],
        )?;
        Ok(())
    }

    pub fn build_quiz(&self) -> Result<Option<BuildQuiz>> {
        let conn = &self.conn;
        let row = conn
            .query_row(
                "SELECT id, session_id, state, questions_json, created_at
                 FROM build_quiz ORDER BY created_at DESC LIMIT 1",
                [],
                |r| {
                    let questions_json: String = r.get(3)?;
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        questions_json,
                        r.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?;
        Ok(row.map(|(id, session_id, state, questions_json, created_at)| BuildQuiz {
            id,
            session_id,
            state,
            questions: serde_json::from_str(&questions_json).unwrap_or_default(),
            created_at,
        }))
    }

    pub fn confirm_build_quiz(&self, quiz_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE build_quiz SET state = 'confirmed', confirmed_at = ?2 WHERE id = ?1",
            rusqlite::params![quiz_id, Utc::now().to_rfc3339()],
        )?;
        self.set_build_phase("complete")?;
        Ok(())
    }

    /// Marks an entity row as owner-confirmed from the build quiz.
    pub fn approve_entity(&self, id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE entities SET status = 'approved', updated_at = ?2 WHERE id = ?1",
            rusqlite::params![id, now],
        )?;
        Ok(())
    }

    pub fn reject_entity(&self, id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE entities SET status = 'rejected', updated_at = ?2 WHERE id = ?1",
            rusqlite::params![id, now],
        )?;
        Ok(())
    }
}
