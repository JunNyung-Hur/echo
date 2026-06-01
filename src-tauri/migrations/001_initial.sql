-- Echo SQLite schema — initial (Phase 0).
--
-- Translated 1:1 from old PostgreSQL schema per migration/model-mapping.md.
-- All guards in migration/guards-and-invariants.md G-DB-* are enforced here.
--
-- invariant: G-DB-003 — PRAGMA foreign_keys must be ON for every connection.
--   See src-tauri/src/db.rs init_pool — SqliteConnectOptions::foreign_keys(true).

-- ============================================================================
-- notes  (old: meetings)  — D-003 rename
-- ============================================================================
CREATE TABLE notes (
    id              TEXT PRIMARY KEY,
    title           TEXT NOT NULL,
    description     TEXT,
    location        TEXT,
    language        TEXT NOT NULL DEFAULT 'auto'
                        CHECK (language IN ('auto','kor','eng')),
    started_at      TEXT,
    -- D-019: pre-add for Phase 4 text_quick capture. 'audio' until then.
    source_type     TEXT NOT NULL DEFAULT 'audio'
                        CHECK (source_type IN ('audio','text_quick')),
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- invariant: G-DB-007 — auto-update updated_at on row mutation
CREATE TRIGGER trg_notes_updated_at
AFTER UPDATE ON notes
FOR EACH ROW
BEGIN
    UPDATE notes SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- ============================================================================
-- recordings  (file_deleted column dropped per D-009 — no 14-day cleanup)
-- ============================================================================
CREATE TABLE recordings (
    id                  TEXT PRIMARY KEY,
    note_id             TEXT NOT NULL,
    file_path           TEXT NOT NULL,
    original_filename   TEXT NOT NULL,
    duration            REAL,
    -- G-REC-001/010/011 boundary states
    format              TEXT NOT NULL
                            CHECK (format IN ('recording','finalizing','webm','mp3','wav','m4a','failed')),
    last_chunk_at       TEXT,   -- G-REC-002 heartbeat
    finalized_at        TEXT,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),

    -- invariant: G-DB-002 — note → recordings CASCADE
    FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE CASCADE
);
CREATE INDEX idx_recordings_note ON recordings(note_id);

-- ============================================================================
-- transcripts
-- ============================================================================
CREATE TABLE transcripts (
    id              TEXT PRIMARY KEY,
    note_id         TEXT NOT NULL,
    -- recording_id NULL allows future text_quick notes (Phase 4)
    recording_id    TEXT,
    raw_path        TEXT,
    corrected_path  TEXT,
    -- invariant: G-DB-005 — status enum at DB level
    status          TEXT NOT NULL DEFAULT 'pending'
                        CHECK (status IN ('pending','processing','completed','failed','cancelled','empty')),
    task_id         TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (note_id)      REFERENCES notes(id)      ON DELETE CASCADE,
    FOREIGN KEY (recording_id) REFERENCES recordings(id) ON DELETE SET NULL
);
CREATE INDEX idx_transcripts_note ON transcripts(note_id);
CREATE INDEX idx_transcripts_recording ON transcripts(recording_id);

CREATE TRIGGER trg_transcripts_updated_at
AFTER UPDATE ON transcripts
FOR EACH ROW
BEGIN
    UPDATE transcripts SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- ============================================================================
-- note_bodies  (old: minutes)  — D-015 rename
-- ============================================================================
CREATE TABLE note_bodies (
    id                          TEXT PRIMARY KEY,
    note_id                     TEXT NOT NULL,
    -- nullable for Phase 4 text_quick path
    transcript_id               TEXT,
    content_path                TEXT,
    status                      TEXT NOT NULL DEFAULT 'pending'
                                    CHECK (status IN ('pending','processing','completed','failed')),
    task_id                     TEXT,
    -- invariant: G-DB-001 — context_snapshot NOT NULL + JSON valid
    context_snapshot            TEXT NOT NULL
                                    CHECK (json_valid(context_snapshot)),
    -- G-TASK-007 — captured once at first generate, carried forward across versions
    initial_content_path        TEXT,
    initial_context_snapshot    TEXT
                                    CHECK (initial_context_snapshot IS NULL OR json_valid(initial_context_snapshot)),
    -- G-VERSION-002/003 — archive-and-create pattern + manual edit flag
    archived                    INTEGER NOT NULL DEFAULT 0,
    is_manual_edit              INTEGER NOT NULL DEFAULT 0,
    created_at                  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at                  TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (note_id)       REFERENCES notes(id)       ON DELETE CASCADE,
    FOREIGN KEY (transcript_id) REFERENCES transcripts(id) ON DELETE SET NULL
);
CREATE INDEX idx_note_bodies_note ON note_bodies(note_id);

-- invariant: G-DB-004 — at most one active completed body per note
CREATE UNIQUE INDEX idx_note_bodies_one_active
    ON note_bodies(note_id)
    WHERE archived = 0 AND status = 'completed';

CREATE TRIGGER trg_note_bodies_updated_at
AFTER UPDATE ON note_bodies
FOR EACH ROW
BEGIN
    UPDATE note_bodies SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- ============================================================================
-- note_chat_messages  (old: meeting_chat_messages)
-- ============================================================================
CREATE TABLE note_chat_messages (
    id                      TEXT PRIMARY KEY,
    note_id                 TEXT NOT NULL,
    role                    TEXT NOT NULL
                                CHECK (role IN ('user','assistant')),
    content                 TEXT NOT NULL,
    -- G-SSE-005 — "이 시점 노트 보기" chip linkage
    note_body_version_id    TEXT,
    -- G-SSE-001 — tool_calls JSON for turn-merge persistence
    tool_calls              TEXT
                                CHECK (tool_calls IS NULL OR json_valid(tool_calls)),
    created_at              TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (note_id)              REFERENCES notes(id)        ON DELETE CASCADE,
    FOREIGN KEY (note_body_version_id) REFERENCES note_bodies(id)  ON DELETE SET NULL
);
CREATE INDEX idx_note_chat_messages_note    ON note_chat_messages(note_id);
CREATE INDEX idx_note_chat_messages_created ON note_chat_messages(created_at);

-- ============================================================================
-- note_timeline_events  (old: meeting_timeline_events)
-- G-LIFE-001/002 — separate table from chat, fixed kind catalog
-- ============================================================================
CREATE TABLE note_timeline_events (
    id          TEXT PRIMARY KEY,
    note_id     TEXT NOT NULL,
    kind        TEXT NOT NULL
                    CHECK (kind IN (
                        'recording_started','recording_stopped',
                        'transcribe_started','transcribe_completed',
                        'transcribe_failed','transcribe_cancelled',
                        'minutes_generated','minutes_failed','minutes_cancelled'
                    )),
    content     TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE CASCADE
);
CREATE INDEX idx_note_timeline_events_note    ON note_timeline_events(note_id);
CREATE INDEX idx_note_timeline_events_created ON note_timeline_events(created_at);

-- ============================================================================
-- ai_endpoints  (old: ai_models, slimmed for 1-user app per D-007/D-016)
-- ============================================================================
CREATE TABLE ai_endpoints (
    id                      TEXT PRIMARY KEY,
    kind                    TEXT NOT NULL
                                CHECK (kind IN ('llm','asr')),
    name                    TEXT NOT NULL,
    model_id                TEXT NOT NULL,
    api_base_url            TEXT NOT NULL,
    -- D-016: SQLite plaintext for v1 simplicity.
    api_key                 TEXT NOT NULL DEFAULT '',
    audio_format            TEXT NOT NULL DEFAULT 'audio_url',
    chunk_seconds           INTEGER,
    max_tokens              INTEGER,
    postprocess_enabled     INTEGER NOT NULL DEFAULT 1,
    is_active               INTEGER NOT NULL DEFAULT 0,
    created_at              TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at              TEXT NOT NULL DEFAULT (datetime('now'))
);

-- invariant: G-DB-006 — at most one active endpoint per kind
CREATE UNIQUE INDEX idx_ai_endpoints_one_active_per_kind
    ON ai_endpoints(kind)
    WHERE is_active = 1;

CREATE TRIGGER trg_ai_endpoints_updated_at
AFTER UPDATE ON ai_endpoints
FOR EACH ROW
BEGIN
    UPDATE ai_endpoints SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- ============================================================================
-- settings  (simple KV — user_id column dropped, single-user app)
-- ============================================================================
CREATE TABLE settings (
    key     TEXT PRIMARY KEY,
    value   TEXT NOT NULL DEFAULT ''
);

-- ============================================================================
-- tags  (D-006 + D-020)
-- ============================================================================
CREATE TABLE tags (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    color       TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE UNIQUE INDEX idx_tags_name ON tags(name COLLATE NOCASE);

CREATE TABLE note_tags (
    note_id     TEXT NOT NULL,
    tag_id      TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (note_id, tag_id),
    FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id)  REFERENCES tags(id)  ON DELETE CASCADE
);
CREATE INDEX idx_note_tags_tag ON note_tags(tag_id);
