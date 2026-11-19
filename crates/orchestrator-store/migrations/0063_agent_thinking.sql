-- Agent thinking (DPES-1106). Two places keep it:
--
-- - A chat turn's thinking is a `thinking_chunk` entry in conversation_entries, beside the
--   reply. It counts against its own byte budget (`conversation_turns.thinking_bytes`), never
--   against `reply_bytes`, so thinking cannot crowd out a reply.
-- - A task session's thinking goes to `session_thinking`, capped per session by the store.
--
-- Thinking is display data only. Nothing reads it back as a command or into a prompt.

-- Rebuild conversation_entries: the kind CHECK gains 'thinking_chunk' and the author CHECK
-- covers it. Same rebuild pattern as 0054: the store runs migrations with foreign keys off and
-- checks them afterward. No table references conversation_entries. Dropping the old table
-- drops its indexes and triggers; both triggers are recreated as 0032 left them.
CREATE TABLE conversation_entries_new (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE CHECK (length(id) > 0),
    conversation_id TEXT NOT NULL REFERENCES conversations(id),
    turn_id TEXT NOT NULL REFERENCES conversation_turns(id),
    kind TEXT NOT NULL CHECK (
        kind IN ('user_message', 'reply_chunk', 'thinking_chunk', 'turn_completed',
                 'turn_failed', 'turn_cancelled')
    ),
    author TEXT NOT NULL CHECK (author IN ('user', 'orchestrator', 'system')),
    body TEXT NOT NULL,
    error_code TEXT CHECK (error_code IS NULL OR length(error_code) BETWEEN 1 AND 64),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK ((kind = 'user_message') = (author = 'user')),
    CHECK ((kind IN ('reply_chunk', 'thinking_chunk')) = (author = 'orchestrator')),
    CHECK ((kind = 'turn_failed') = (error_code IS NOT NULL))
);

INSERT INTO conversation_entries_new
    (sequence, id, conversation_id, turn_id, kind, author, body, error_code, created_at)
SELECT sequence, id, conversation_id, turn_id, kind, author, body, error_code, created_at
FROM conversation_entries;

-- The AUTOINCREMENT counter may be ahead of the newest row (a history clear deletes rows).
-- Carry it over so a sequence is never reused; the rename below renames this counter row.
DELETE FROM sqlite_sequence WHERE name = 'conversation_entries_new';
INSERT INTO sqlite_sequence (name, seq)
SELECT 'conversation_entries_new', seq FROM sqlite_sequence WHERE name = 'conversation_entries';

DROP TABLE conversation_entries;
ALTER TABLE conversation_entries_new RENAME TO conversation_entries;

CREATE INDEX conversation_entries_by_conversation_sequence
    ON conversation_entries(conversation_id, sequence);
CREATE INDEX conversation_entries_by_turn ON conversation_entries(turn_id, sequence);

CREATE TRIGGER conversation_entries_no_update
BEFORE UPDATE ON conversation_entries
BEGIN
    SELECT RAISE(ABORT, 'conversation entries are append-only');
END;

CREATE TRIGGER conversation_entries_gated_delete
BEFORE DELETE ON conversation_entries
WHEN NOT EXISTS (SELECT 1 FROM conversation_clear_gate WHERE id = 1 AND open = 1)
BEGIN
    SELECT RAISE(ABORT, 'conversation entries are deleted only by a history clear');
END;

ALTER TABLE conversation_turns
    ADD COLUMN thinking_bytes INTEGER NOT NULL DEFAULT 0 CHECK (thinking_bytes >= 0);

-- `session_id` carries no foreign key: sessions.id is unique only together with task_id
-- (0001), so SQLite would reject a reference to it. No code path deletes a session row.
CREATE TABLE session_thinking (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL CHECK (length(session_id) > 0),
    body TEXT NOT NULL CHECK (length(body) > 0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX session_thinking_by_session ON session_thinking(session_id, sequence);

CREATE TRIGGER session_thinking_no_update
BEFORE UPDATE ON session_thinking
BEGIN
    SELECT RAISE(ABORT, 'session thinking is append-only');
END;
