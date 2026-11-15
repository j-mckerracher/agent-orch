-- DPES-1105: clear conversation history and remove project coordinators.
--
-- Amends the append-only rule of 0031 for one exception: a history clear may
-- delete conversation rows, permanently. Everything else stays append-only.
--
-- - UPDATE is still blocked on conversation_entries.
-- - DELETE from conversation_entries, conversation_turns and coordinator_entries
--   aborts unless the clear gate is open. Only the store's clear transaction opens
--   it, and it closes the gate before it commits. A rollback restores the closed
--   gate too, because the gate is an ordinary row.
--
-- See docs/m7-08-conversation-service.md, "Clear history and remove coordinator".

DROP TRIGGER conversation_entries_no_update;
DROP TRIGGER conversation_entries_no_delete;

CREATE TRIGGER conversation_entries_no_update
BEFORE UPDATE ON conversation_entries
BEGIN
    SELECT RAISE(ABORT, 'conversation entries are append-only');
END;

CREATE TABLE conversation_clear_gate (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    open INTEGER NOT NULL DEFAULT 0 CHECK (open IN (0, 1))
);

INSERT INTO conversation_clear_gate (id, open) VALUES (1, 0);

CREATE TRIGGER conversation_entries_gated_delete
BEFORE DELETE ON conversation_entries
WHEN NOT EXISTS (SELECT 1 FROM conversation_clear_gate WHERE id = 1 AND open = 1)
BEGIN
    SELECT RAISE(ABORT, 'conversation entries are deleted only by a history clear');
END;

CREATE TRIGGER conversation_turns_gated_delete
BEFORE DELETE ON conversation_turns
WHEN NOT EXISTS (SELECT 1 FROM conversation_clear_gate WHERE id = 1 AND open = 1)
BEGIN
    SELECT RAISE(ABORT, 'conversation turns are deleted only by a history clear');
END;

CREATE TRIGGER coordinator_entries_gated_delete
BEFORE DELETE ON coordinator_entries
WHEN NOT EXISTS (SELECT 1 FROM conversation_clear_gate WHERE id = 1 AND open = 1)
BEGIN
    SELECT RAISE(ABORT, 'coordinator entries are deleted only by a history clear');
END;

-- Each conversation counts its clears. A reader that saw an earlier value must
-- discard what it read: the log it holds may include messages that are now gone.
ALTER TABLE conversations
    ADD COLUMN clear_generation INTEGER NOT NULL DEFAULT 0 CHECK (clear_generation >= 0);

-- A project's coordinator is active unless a row says otherwise. Removal deletes
-- the coordinator's log and records `removed`; restore clears the flag. The
-- project and its tasks are never touched.
CREATE TABLE coordinator_scopes (
    project_id TEXT PRIMARY KEY REFERENCES projects(id),
    removed INTEGER NOT NULL DEFAULT 0 CHECK (removed IN (0, 1)),
    clear_generation INTEGER NOT NULL DEFAULT 0 CHECK (clear_generation >= 0),
    changed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
