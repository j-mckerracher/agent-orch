-- Pause automatic updates after repeated agent failures (DPES-1106).
-- Adds agent_failing to the event-kind and skip-reason CHECKs. Same rebuild as 0057: no
-- table has a foreign key to wake_records, and every row is copied in place.
CREATE TABLE wake_records_new (
    id TEXT PRIMARY KEY CHECK (length(id) BETWEEN 1 AND 128),
    scope TEXT NOT NULL CHECK (length(scope) > 0),
    event_kind TEXT NOT NULL CHECK (event_kind IN
        ('task_failed', 'task_stalled', 'delivery_waiting', 'proposal_waiting',
         'sign_in_needed', 'daemon_start', 'work_check', 'scout_report_ready', 'agent_failing')),
    subject TEXT NOT NULL CHECK (length(subject) BETWEEN 1 AND 128),
    dedupe_key TEXT NOT NULL CHECK (length(dedupe_key) BETWEEN 1 AND 160),
    notice TEXT NOT NULL CHECK (length(notice) BETWEEN 1 AND 512),
    needs_model INTEGER NOT NULL CHECK (needs_model IN (0, 1)),
    state TEXT NOT NULL CHECK (state IN
        ('noticed', 'pending', 'running', 'handled', 'skipped', 'failed')),
    outcome TEXT CHECK (outcome IS NULL OR outcome IN ('replied', 'quiet')),
    skip_reason TEXT CHECK (skip_reason IS NULL OR skip_reason IN
        ('mode_off', 'limit_hourly', 'limit_daily', 'limit_tokens', 'agent_failing')),
    batch_id TEXT CHECK (batch_id IS NULL OR length(batch_id) > 0),
    turn_id TEXT CHECK (turn_id IS NULL OR length(turn_id) > 0),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 2),
    anchor_sequence INTEGER NOT NULL DEFAULT 0 CHECK (anchor_sequence >= 0),
    notify_reply INTEGER NOT NULL DEFAULT 0 CHECK (notify_reply IN (0, 1)),
    seen INTEGER NOT NULL DEFAULT 0 CHECK (seen IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (scope, dedupe_key),
    CHECK ((state = 'skipped') = (skip_reason IS NOT NULL)),
    CHECK ((state = 'handled') = (outcome IS NOT NULL)),
    CHECK (needs_model = 1 OR state = 'noticed')
);

INSERT INTO wake_records_new
    (rowid, id, scope, event_kind, subject, dedupe_key, notice, needs_model, state,
     outcome, skip_reason, batch_id, turn_id, attempts, anchor_sequence, notify_reply,
     seen, created_at, updated_at)
SELECT rowid, id, scope, event_kind, subject, dedupe_key, notice, needs_model, state,
       outcome, skip_reason, batch_id, turn_id, attempts, anchor_sequence, notify_reply,
       seen, created_at, updated_at
FROM wake_records;

DROP TABLE wake_records;
ALTER TABLE wake_records_new RENAME TO wake_records;

CREATE INDEX wake_records_by_scope ON wake_records(scope, created_at);
CREATE INDEX wake_records_by_state ON wake_records(state) WHERE state IN ('pending', 'running');
CREATE INDEX wake_records_by_batch ON wake_records(batch_id) WHERE batch_id IS NOT NULL;

CREATE TRIGGER wake_records_gated_delete
BEFORE DELETE ON wake_records
WHEN NOT EXISTS (SELECT 1 FROM conversation_clear_gate WHERE id = 1 AND open = 1)
BEGIN
    SELECT RAISE(ABORT, 'wake records are deleted only by a history clear');
END;

CREATE TRIGGER wake_records_fixed_fields
BEFORE UPDATE OF id, scope, event_kind, subject, dedupe_key, notice, needs_model,
    anchor_sequence, created_at
ON wake_records
BEGIN
    SELECT RAISE(ABORT, 'wake record identity never changes');
END;

CREATE TRIGGER wake_records_state_transitions
BEFORE UPDATE OF state ON wake_records
WHEN NOT (
    (OLD.state = 'pending' AND NEW.state IN ('running', 'skipped'))
    OR (OLD.state = 'running' AND NEW.state IN ('handled', 'failed', 'pending', 'skipped'))
)
BEGIN
    SELECT RAISE(ABORT, 'illegal wake record state transition');
END;

