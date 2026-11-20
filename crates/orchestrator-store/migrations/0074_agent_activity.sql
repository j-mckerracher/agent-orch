-- Live agent activity (DPES-1044). Tool calls join thinking and replies in one ordered
-- transcript:
--
-- - A chat turn's tool calls are `tool_started` and `tool_finished` entries in
--   conversation_entries, joined by the tool id in the entry body. `activity_count` counts the
--   turn's stored `tool_started` rows (cap marker included) so the cap needs no scan.
-- - A task session's `session_thinking` becomes `session_activity` with a `kind` column. Rows
--   written before this migration are thinking rows, which the column default says.

-- Rebuild conversation_entries: the kind CHECK gains the two tool kinds. Same rebuild pattern
-- as 0063: the store runs migrations with foreign keys off and checks them afterward. No table
-- references conversation_entries. Dropping the old table drops its indexes and triggers;
-- both triggers are recreated as 0063 left them.
CREATE TABLE conversation_entries_new (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE CHECK (length(id) > 0),
    conversation_id TEXT NOT NULL REFERENCES conversations(id),
    turn_id TEXT NOT NULL REFERENCES conversation_turns(id),
    kind TEXT NOT NULL CHECK (
        kind IN ('user_message', 'reply_chunk', 'thinking_chunk', 'tool_started',
                 'tool_finished', 'turn_completed', 'turn_failed', 'turn_cancelled')
    ),
    author TEXT NOT NULL CHECK (author IN ('user', 'orchestrator', 'system')),
    body TEXT NOT NULL,
    error_code TEXT CHECK (error_code IS NULL OR length(error_code) BETWEEN 1 AND 64),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK ((kind = 'user_message') = (author = 'user')),
    CHECK (
        (kind IN ('reply_chunk', 'thinking_chunk', 'tool_started', 'tool_finished'))
        = (author = 'orchestrator')
    ),
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
    ADD COLUMN activity_count INTEGER NOT NULL DEFAULT 0 CHECK (activity_count >= 0);

DROP TRIGGER session_thinking_no_update;
DROP INDEX session_thinking_by_session;
ALTER TABLE session_thinking RENAME TO session_activity;
ALTER TABLE session_activity
    ADD COLUMN kind TEXT NOT NULL DEFAULT 'thinking'
    CHECK (kind IN ('thinking', 'tool_started', 'tool_finished'));

CREATE INDEX session_activity_by_session ON session_activity(session_id, sequence);

CREATE TRIGGER session_activity_no_update
BEFORE UPDATE ON session_activity
BEGIN
    SELECT RAISE(ABORT, 'session activity is append-only');
END;
