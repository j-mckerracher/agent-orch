-- DPES-1105: audit of history clears, coordinator removals and restores.
--
-- Append-only: both UPDATE and DELETE abort, including during a clear. A row
-- records what happened, to which scope, and how many rows were deleted. It
-- never holds message text, and it is not a row of durable_events.

CREATE TABLE conversation_audit (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    action TEXT NOT NULL CHECK (action IN ('clear', 'remove', 'restore')),
    -- conversation:global, conversation:workstream:<id>, or coordinator:<project_id>
    scope TEXT NOT NULL CHECK (length(scope) > 0),
    -- Nullable: the daemon has no caller identity today, so rows record no actor.
    actor TEXT CHECK (actor IS NULL OR length(actor) BETWEEN 1 AND 128),
    entries_deleted INTEGER NOT NULL DEFAULT 0 CHECK (entries_deleted >= 0),
    turns_deleted INTEGER NOT NULL DEFAULT 0 CHECK (turns_deleted >= 0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX conversation_audit_by_scope ON conversation_audit(scope, sequence);

CREATE TRIGGER conversation_audit_no_update
BEFORE UPDATE ON conversation_audit
BEGIN
    SELECT RAISE(ABORT, 'conversation audit is append-only');
END;

CREATE TRIGGER conversation_audit_no_delete
BEFORE DELETE ON conversation_audit
BEGIN
    SELECT RAISE(ABORT, 'conversation audit is append-only');
END;
