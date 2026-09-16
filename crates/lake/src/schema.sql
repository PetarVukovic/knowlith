-- The Context Lake.
--
-- One SQLite file under the company's own folder. Three decisions in here
-- cannot be made later without migrating everything, so they are made now
-- even where today's product does not use them: `tenant_id` on every row,
-- `acl_scope` on every document, and a real column for each field the
-- canonical Markdown serialises.

CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER NOT NULL
);

-- ---------------------------------------------------------------- sources --

CREATE TABLE IF NOT EXISTS sources (
    id          TEXT PRIMARY KEY,
    tenant_id   TEXT NOT NULL DEFAULT 'local',
    name        TEXT NOT NULL,
    root        TEXT NOT NULL,
    kind        TEXT NOT NULL,              -- folder | nas
    processor   TEXT NOT NULL,              -- codex | claude-code | managed
    status      TEXT NOT NULL,              -- active | paused | scanning | error
    added_at    TEXT NOT NULL,
    last_scan   TEXT,
    last_error  TEXT
);

-- -------------------------------------------------------------- documents --

-- Identity is the content hash, so moving or renaming a file does not orphan
-- the evidence that points into it. Two copies of the same file in different
-- folders collapse to one row, which is the behaviour the owner expects when
-- the preview told them there were duplicates.
CREATE TABLE IF NOT EXISTS documents (
    id            TEXT PRIMARY KEY,
    tenant_id     TEXT NOT NULL DEFAULT 'local',
    source_id     TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    acl_scope     TEXT NOT NULL DEFAULT 'company',
    path          TEXT NOT NULL,
    name          TEXT NOT NULL,
    kind          TEXT NOT NULL,
    byte_len      INTEGER NOT NULL,
    sha256        TEXT NOT NULL,
    -- The deterministic rendition every offset in `blocks` and `evidence`
    -- refers to. For Markdown, text and CSV it is the file itself.
    text          TEXT NOT NULL,
    text_sha256   TEXT NOT NULL,
    verbatim      INTEGER NOT NULL,
    modified      TEXT NOT NULL,
    columns_json  TEXT,
    read_at       TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_documents_source ON documents(source_id);
CREATE INDEX IF NOT EXISTS idx_documents_sha    ON documents(sha256);
CREATE INDEX IF NOT EXISTS idx_documents_path   ON documents(tenant_id, path);

CREATE TABLE IF NOT EXISTS blocks (
    document_id  TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    ordinal      INTEGER NOT NULL,
    locator      TEXT NOT NULL,
    kind         TEXT NOT NULL,
    text         TEXT NOT NULL,
    start_byte   INTEGER NOT NULL,
    end_byte     INTEGER NOT NULL,
    page         INTEGER,
    sheet        TEXT,
    row          INTEGER,
    cells_json   TEXT,
    PRIMARY KEY (document_id, ordinal)
);

CREATE INDEX IF NOT EXISTS idx_blocks_locator ON blocks(document_id, locator);

-- `remove_diacritics 2` is not optional here: without it "placanje" does not
-- find "plaćanje", and every Croatian document in the folder is half
-- unsearchable.
CREATE VIRTUAL TABLE IF NOT EXISTS blocks_fts USING fts5(
    text,
    document_id UNINDEXED,
    locator UNINDEXED,
    tokenize = "unicode61 remove_diacritics 2"
);

-- ---------------------------------------------------------------- objects --

CREATE TABLE IF NOT EXISTS objects (
    id                  TEXT PRIMARY KEY,
    tenant_id           TEXT NOT NULL DEFAULT 'local',
    kind                TEXT NOT NULL,      -- rule | process | term | fact
    subtype             TEXT,
    title               TEXT NOT NULL,
    body                TEXT NOT NULL,
    status              TEXT NOT NULL,      -- proposed | approved | conflicted | superseded | rejected
    confidence          REAL NOT NULL,
    version             INTEGER NOT NULL,
    valid_from          TEXT NOT NULL,
    valid_to            TEXT,
    supersedes          TEXT,
    decided_by          TEXT,
    edited_on_approval  INTEGER NOT NULL DEFAULT 0,
    path                TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    -- Set when something this object depends on changed after it was
    -- approved. The gateway keeps serving it and says so; silently serving a
    -- stale answer and silently withholding one are both worse.
    stale_since         TEXT
);

CREATE INDEX IF NOT EXISTS idx_objects_status ON objects(status);
CREATE INDEX IF NOT EXISTS idx_objects_kind   ON objects(kind);

-- Older versions are kept, never overwritten, so "what applied in March" has
-- an answer.
CREATE TABLE IF NOT EXISTS object_versions (
    object_id    TEXT NOT NULL,
    version      INTEGER NOT NULL,
    body         TEXT NOT NULL,
    status       TEXT NOT NULL,
    valid_from   TEXT NOT NULL,
    valid_to     TEXT,
    decided_by   TEXT,
    snapshot_at  TEXT NOT NULL,
    PRIMARY KEY (object_id, version)
);

-- Nothing gets in here that did not pass the mechanical span check. The
-- verification is recorded against the rendition hash it was checked on, so a
-- parser change invalidates the check rather than silently inheriting it.
CREATE TABLE IF NOT EXISTS evidence (
    id                INTEGER PRIMARY KEY,
    object_id         TEXT NOT NULL REFERENCES objects(id) ON DELETE CASCADE,
    document_id       TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    locator           TEXT NOT NULL,
    start_byte        INTEGER NOT NULL,
    end_byte          INTEGER NOT NULL,
    quote             TEXT NOT NULL,
    verified_at       TEXT NOT NULL,
    verified_against  TEXT NOT NULL,
    UNIQUE (object_id, document_id, start_byte, end_byte)
);

CREATE INDEX IF NOT EXISTS idx_evidence_object   ON evidence(object_id);
CREATE INDEX IF NOT EXISTS idx_evidence_document ON evidence(document_id);

-- -------------------------------------------------------------- the graph --

-- The knowledge graph is stored here and nowhere else. It is derived from
-- approved objects, rebuilt in memory for traversal, and written in the same
-- transaction as the approval that caused it — so it cannot disagree with the
-- objects it describes.
CREATE TABLE IF NOT EXISTS relations (
    from_id     TEXT NOT NULL,
    to_id       TEXT NOT NULL,
    type        TEXT NOT NULL,   -- depends_on | used_by | derived_from | conflicts_with
    origin      TEXT NOT NULL,   -- structural | model | manual
    created_at  TEXT NOT NULL,
    PRIMARY KEY (from_id, to_id, type)
);

CREATE INDEX IF NOT EXISTS idx_relations_to ON relations(to_id);

-- What the gateway actually served, and when. "Read by 3 AI tools" is a fact
-- recorded here or it is not said at all.
CREATE TABLE IF NOT EXISTS tool_reads (
    id         INTEGER PRIMARY KEY,
    object_id  TEXT NOT NULL,
    tool       TEXT NOT NULL,
    read_at    TEXT NOT NULL,
    -- Which case this serve belonged to, when the agent opened one.
    case_id    TEXT,
    -- Which application asked. Null for a read taken before the gateway
    -- recorded it, and for a client that sends no clientInfo.
    app        TEXT
);

CREATE INDEX IF NOT EXISTS idx_tool_reads ON tool_reads(object_id, read_at DESC);
CREATE INDEX IF NOT EXISTS idx_tool_reads_app ON tool_reads(app, read_at DESC);
CREATE INDEX IF NOT EXISTS idx_tool_reads_case ON tool_reads(case_id);

-- ----------------------------------------------------------------- work ----

-- The durable queue. Work survives a crash, a closed laptop and a network
-- that comes back an hour later, because the queue is a table rather than a
-- collection of spawned tasks.
CREATE TABLE IF NOT EXISTS jobs (
    id               INTEGER PRIMARY KEY,
    tenant_id        TEXT NOT NULL DEFAULT 'local',
    kind             TEXT NOT NULL,
    payload_json     TEXT NOT NULL,
    -- Effectively-once: a retried job recognises work it already did instead
    -- of duplicating it.
    idempotency_key  TEXT NOT NULL UNIQUE,
    state            TEXT NOT NULL,   -- queued | leased | done | failed | dead
    -- Lower runs first. Maintenance must never overtake the document the
    -- owner is waiting on: an hourly re-check of stored quotes is worth
    -- doing and worth doing last.
    priority         INTEGER NOT NULL DEFAULT 0,
    attempts         INTEGER NOT NULL DEFAULT 0,
    -- A lease, not a flag. If the process dies the lease expires and the job
    -- returns to the queue by itself.
    lease_until      TEXT,
    run_after        TEXT NOT NULL,
    last_error       TEXT,
    created_at       TEXT NOT NULL,
    finished_at      TEXT
);

CREATE INDEX IF NOT EXISTS idx_jobs_ready ON jobs(state, priority, run_after);

CREATE TABLE IF NOT EXISTS settings (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);

-- --------------------------------------------------------- merge hints ----

-- Two objects that may be the same thing said twice.
--
-- This table exists because the alternative was worse in both directions.
-- Merging automatically means a similarity score decides that two of the
-- company's rules are one rule, and a wrong merge silently deletes a rule
-- nobody will notice is gone. Not merging at all leaves the owner with the
-- same discount written twice under two titles, which is what a twelve-file
-- folder actually produced.
--
-- So the machine proposes and the owner disposes, and the answer is kept:
-- a dismissed pair is never offered again, because being asked the same
-- question after every rescan is how a review queue becomes noise.
CREATE TABLE IF NOT EXISTS merge_hints (
    left_id     TEXT NOT NULL,
    right_id    TEXT NOT NULL,
    -- duplicate | disagreement. The second is the more serious: one subject
    -- with two live figures, which stage 3 cannot see because it only looks
    -- inside a group and these two are in different groups.
    kind        TEXT NOT NULL DEFAULT 'duplicate',
    score       REAL NOT NULL,
    state       TEXT NOT NULL,   -- open | dismissed | merged
    created_at  TEXT NOT NULL,
    decided_at  TEXT,
    PRIMARY KEY (left_id, right_id)
);

CREATE INDEX IF NOT EXISTS idx_merge_hints_state ON merge_hints(state);

-- ------------------------------------------------------------ candidates --

-- What the engine said about one document, kept.
--
-- The compiler's third stage decides which claim is current, which document
-- supersedes which, and where two documents disagree — and all three are
-- properties of the *set*, not of a document. A worker that compiles one
-- document at a time and stores the result has already thrown away the
-- comparison before it could be made.
--
-- So the expensive half is stored and the cheap half is redone. One model
-- call per document, once; consolidation over everything, as often as the
-- set changes. A document whose text has not moved is never read again.
CREATE TABLE IF NOT EXISTS candidates (
    document_id  TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    ordinal      INTEGER NOT NULL,
    json         TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    PRIMARY KEY (document_id, ordinal)
);

-- ------------------------------------------------------------ retrieval --

-- Objects are searched by their own words, not by the words of the documents
-- they came from. An approved rule is written in the owner's language and is
-- short; the paragraph behind it is long and may be a scanned contract. An
-- agent asking "what is the payment term" should match the rule.
--
-- Diacritics folded here for the same reason as in `blocks_fts`: an agent
-- writing "placanje" is not making a mistake, it is typing what a keyboard
-- gave it.
CREATE VIRTUAL TABLE IF NOT EXISTS objects_fts USING fts5(
    title,
    body,
    object_id UNINDEXED,
    tokenize = "unicode61 remove_diacritics 2"
);

-- ---------------------------------------------------------------- cases --

-- One piece of work an agent was asked to do.
--
-- This table is what lets the gateway say "you have not looked at the
-- warranty" instead of answering whatever was asked and hoping. Retrieval
-- systems cannot normally make that statement: having returned five
-- passages, they have no idea what the sixth would have been. Here the set
-- of approved objects is finite and related, so the ones a question touches
-- can be listed up front and ticked off as they are actually read.
CREATE TABLE IF NOT EXISTS cases (
    id           TEXT PRIMARY KEY,
    tenant_id    TEXT NOT NULL DEFAULT 'local',
    question     TEXT NOT NULL,
    -- The object ids the gateway said were relevant when the case opened.
    -- Stored rather than recomputed, so closing a case measures what was
    -- actually asked for and not what the lake looks like now.
    areas_json   TEXT NOT NULL,
    opened_at    TEXT NOT NULL,
    closed_at    TEXT,
    -- What the agent said it concluded. Kept for the owner to read, never
    -- served back as knowledge.
    summary      TEXT,
    -- Which application asked. Null for a client we do not recognise.
    app          TEXT
);

CREATE INDEX IF NOT EXISTS idx_cases_open ON cases(closed_at, opened_at DESC);
