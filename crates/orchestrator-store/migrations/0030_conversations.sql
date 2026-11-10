-- M7-08: orchestrator conversation transcripts (global and per workstream).
-- Version 0030 is reserved for this item so concurrently developed migrations
-- (0011-0029) cannot collide with it.
--
-- A conversation never authorizes anything. Nothing here references tasks,
-- approvals, deliveries, or policy, and no row is read back as a command.

-- One conversation per target. `workstream_id` is an OPAQUE reference: M7-07
-- owns the workstream entity and decides how this column is validated (an
-- application-level check or a foreign key added by its own migration). Until
-- then the daemon refuses every workstream target it cannot resolve.
CREATE TABLE conversations (
    id TEXT PRIMARY KEY CHECK (length(id) > 0),
    target_kind TEXT NOT NULL CHECK (target_kind IN ('global', 'workstream')),
    workstream_id TEXT CHECK (workstream_id IS NULL OR length(workstream_id) > 0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK ((target_kind = 'workstream') = (workstream_id IS NOT NULL))
);

CREATE UNIQUE INDEX conversations_one_per_target
    ON conversations(target_kind, COALESCE(workstream_id, ''));

-- One user message and the reply it produces. At most one turn per
-- conversation is `running`; that is what keeps messages and replies ordered.
-- The runtime identity is recorded so a transcript names what produced it.
CREATE TABLE conversation_turns (
    id TEXT PRIMARY KEY CHECK (length(id) > 0),
    conversation_id TEXT NOT NULL REFERENCES conversations(id),
    state TEXT NOT NULL CHECK (state IN ('running', 'completed', 'failed', 'cancelled')),
    error_code TEXT CHECK (error_code IS NULL OR length(error_code) BETWEEN 1 AND 64),
    runtime_name TEXT NOT NULL CHECK (length(runtime_name) > 0),
    runtime_version TEXT NOT NULL CHECK (length(runtime_version) > 0),
    reply_bytes INTEGER NOT NULL DEFAULT 0 CHECK (reply_bytes >= 0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK ((state = 'failed') = (error_code IS NOT NULL))
);

CREATE UNIQUE INDEX conversation_turns_one_running
    ON conversation_turns(conversation_id) WHERE state = 'running';
CREATE INDEX conversation_turns_by_conversation
    ON conversation_turns(conversation_id, created_at);

-- Append-only transcript. `sequence` is the replay cursor; `turn_id`
-- correlates a reply, its terminal marker and its error with the user message
-- that caused them.
CREATE TABLE conversation_entries (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE CHECK (length(id) > 0),
    conversation_id TEXT NOT NULL REFERENCES conversations(id),
    turn_id TEXT NOT NULL REFERENCES conversation_turns(id),
    kind TEXT NOT NULL CHECK (
        kind IN ('user_message', 'reply_chunk', 'turn_completed', 'turn_failed', 'turn_cancelled')
    ),
    author TEXT NOT NULL CHECK (author IN ('user', 'orchestrator', 'system')),
    body TEXT NOT NULL,
    error_code TEXT CHECK (error_code IS NULL OR length(error_code) BETWEEN 1 AND 64),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK ((kind = 'user_message') = (author = 'user')),
    CHECK ((kind = 'reply_chunk') = (author = 'orchestrator')),
    CHECK ((kind = 'turn_failed') = (error_code IS NOT NULL))
);

CREATE INDEX conversation_entries_by_conversation_sequence
    ON conversation_entries(conversation_id, sequence);
CREATE INDEX conversation_entries_by_turn ON conversation_entries(turn_id, sequence);

CREATE TRIGGER conversation_entries_no_update
BEFORE UPDATE ON conversation_entries
BEGIN
    SELECT RAISE(ABORT, 'conversation entries are append-only');
END;

CREATE TRIGGER conversation_entries_no_delete
BEFORE DELETE ON conversation_entries
BEGIN
    SELECT RAISE(ABORT, 'conversation entries are append-only');
END;
