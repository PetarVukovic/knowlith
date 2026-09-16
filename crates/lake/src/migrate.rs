//! Bringing an existing lake up to the current shape.
//!
//! `CREATE TABLE IF NOT EXISTS` covers a lake that does not exist yet and
//! nothing else. A column added to `schema.sql` never appears in a file
//! someone has been using for a month, and the failure is the worst kind:
//! the daemon starts, the interface loads, and one query returns "no such
//! column" at the moment the owner clicks something.
//!
//! So every change to an existing table goes through here. Each step is
//! written to be safe to run twice, because the cheapest way to be sure a
//! migration ran is to run it on every open.

use rusqlite::{Connection, params};

use crate::Result;

/// The shape this build expects. Bumped whenever a step is added, and stored
/// so a lake written by a newer Knowlith can be recognised rather than
/// quietly half-read by an older one.
pub const SCHEMA_VERSION: i64 = 2;

pub fn run(conn: &Connection) -> Result<()> {
    // A read of what is actually there beats a version number: a lake that
    // was created by this build already has the column, and a lake whose
    // version row was lost to a crash still must not be double-migrated.
    if !has_column(conn, "tool_reads", "case_id")? {
        // Which case a serve belonged to. Nullable, because an agent that
        // never opens a case still gets answers — coverage is a service the
        // gateway offers, not a toll it charges.
        conn.execute("ALTER TABLE tool_reads ADD COLUMN case_id TEXT", [])?;
    }

    if !has_index(conn, "idx_tool_reads_case")? {
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tool_reads_case ON tool_reads(case_id)",
            [],
        )?;
    }

    // An older lake has objects but no index over them. Building it here
    // rather than asking the owner to re-compile is the difference between
    // an upgrade and a migration project.
    let indexed: i64 = conn.query_row("SELECT count(*) FROM objects_fts", [], |r| r.get(0))?;
    let stored: i64 = conn.query_row("SELECT count(*) FROM objects", [], |r| r.get(0))?;
    if indexed != stored {
        conn.execute("DELETE FROM objects_fts", [])?;
        conn.execute(
            "INSERT INTO objects_fts (title, body, object_id)
             SELECT title, body, id FROM objects",
            [],
        )?;
    }

    conn.execute("DELETE FROM schema_version", [])?;
    conn.execute(
        "INSERT INTO schema_version (version) VALUES (?1)",
        params![SCHEMA_VERSION],
    )?;
    Ok(())
}

fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn has_index(conn: &Connection, name: &str) -> Result<bool> {
    let found: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
        params![name],
        |r| r.get(0),
    )?;
    Ok(found > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lake as it looked before the gateway existed: `tool_reads` without a
    /// case, no object index. Opening it must not fail and must not lose
    /// anything.
    #[test]
    fn an_older_lake_is_brought_forward() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE tool_reads (id INTEGER PRIMARY KEY, object_id TEXT NOT NULL,
                                      tool TEXT NOT NULL, read_at TEXT NOT NULL);
             CREATE TABLE objects (id TEXT PRIMARY KEY, title TEXT NOT NULL, body TEXT NOT NULL);
             CREATE VIRTUAL TABLE objects_fts USING fts5(title, body, object_id UNINDEXED);
             CREATE TABLE schema_version (version INTEGER NOT NULL);
             INSERT INTO objects (id, title, body) VALUES ('rule:a', 'Rok plaćanja', '30 dana');
             INSERT INTO tool_reads (object_id, tool, read_at) VALUES ('rule:a', 'x', 'then');",
        )
        .unwrap();

        run(&conn).unwrap();

        assert!(has_column(&conn, "tool_reads", "case_id").unwrap());
        let indexed: i64 = conn
            .query_row("SELECT count(*) FROM objects_fts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(indexed, 1, "the existing object was indexed for search");
        let kept: i64 = conn
            .query_row("SELECT count(*) FROM tool_reads", [], |r| r.get(0))
            .unwrap();
        assert_eq!(kept, 1, "history survived the migration");
    }

    #[test]
    fn running_twice_changes_nothing() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE tool_reads (id INTEGER PRIMARY KEY, object_id TEXT NOT NULL,
                                      tool TEXT NOT NULL, read_at TEXT NOT NULL);
             CREATE TABLE objects (id TEXT PRIMARY KEY, title TEXT NOT NULL, body TEXT NOT NULL);
             CREATE VIRTUAL TABLE objects_fts USING fts5(title, body, object_id UNINDEXED);
             CREATE TABLE schema_version (version INTEGER NOT NULL);
             INSERT INTO objects (id, title, body) VALUES ('rule:a', 'A', 'B');",
        )
        .unwrap();

        run(&conn).unwrap();
        run(&conn).unwrap();

        let indexed: i64 = conn
            .query_row("SELECT count(*) FROM objects_fts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(indexed, 1, "the object was not indexed twice");
        let versions: i64 = conn
            .query_row("SELECT count(*) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(versions, 1);
    }
}
