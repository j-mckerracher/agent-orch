-- DPES-1105 phase 3b/3c: wake records, the wake turn ledger and the supervision settings.

-- One row per classified event. `notice` is composed by the daemon from fixed templates,
-- ids and a short sanitized objective; it never holds agent text. No foreign key to turns:
-- `scope` is a conversation id and the conversation may not exist yet.
CREATE TABLE wake_records (
    id TEXT PRIMARY KEY CHECK (length(id) BETWEEN 1 AND 128),
    scope TEXT NOT NULL CHECK (length(scope) > 0),
    event_kind TEXT NOT NULL CHECK (event_kind IN
        ('task_failed', 'task_stalled', 'delivery_waiting', 'proposal_waiting',
         'sign_in_needed', 'daemon_start')),
    subject TEXT NOT NULL CHECK (length(subject) BETWEEN 1 AND 128),
    dedupe_key TEXT NOT NULL CHECK (length(dedupe_key) BETWEEN 1 AND 160),
    notice TEXT NOT NULL CHECK (length(notice) BETWEEN 1 AND 512),
    needs_model INTEGER NOT NULL CHECK (needs_model IN (0, 1)),
    state TEXT NOT NULL CHECK (state IN
        ('noticed', 'pending', 'running', 'handled', 'skipped', 'failed')),
    outcome TEXT CHECK (outcome IS NULL OR outcome IN ('replied', 'quiet')),
    skip_reason TEXT CHECK (skip_reason IS NULL OR skip_reason IN
        ('mode_off', 'limit_hourly', 'limit_daily', 'limit_tokens')),
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
CREATE INDEX wake_records_by_scope ON wake_records(scope, created_at);
CREATE INDEX wake_records_by_state ON wake_records(state) WHERE state IN ('pending', 'running');
CREATE INDEX wake_records_by_batch ON wake_records(batch_id) WHERE batch_id IS NOT NULL;

-- Deleted only by a history clear (the same gate as action_proposals).
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

-- noticed, handled, skipped and failed are final. running may go back to pending
-- (user precedence, restart). pending never goes to handled without running first.
CREATE TRIGGER wake_records_state_transitions
BEFORE UPDATE OF state ON wake_records
WHEN NOT (
    (OLD.state = 'pending' AND NEW.state IN ('running', 'skipped'))
    OR (OLD.state = 'running' AND NEW.state IN ('handled', 'failed', 'pending', 'skipped'))
)
BEGIN
    SELECT RAISE(ABORT, 'illegal wake record state transition');
END;

-- One row per wake turn started. It survives a history clear, so a cap cannot be reset
-- by clearing. It holds no text. Rows older than 48 hours are pruned by the scheduler.
CREATE TABLE wake_turn_ledger (
    turn_id TEXT PRIMARY KEY CHECK (length(turn_id) > 0),
    scope TEXT NOT NULL CHECK (length(scope) > 0),
    started_ms INTEGER NOT NULL CHECK (started_ms >= 0)
);
CREATE INDEX wake_turn_ledger_by_scope ON wake_turn_ledger(scope, started_ms);

CREATE TRIGGER wake_turn_ledger_no_update
BEFORE UPDATE ON wake_turn_ledger
BEGIN
    SELECT RAISE(ABORT, 'wake turn ledger rows never change');
END;

-- orchestrator_settings exists since 0039 (one row, the three load limits). This migration only
-- adds columns. Every column has a constant DEFAULT, so the existing row takes the defaults, and
-- a column-level CHECK (SQLite cannot add a table-level CHECK with ALTER). The cross-field rule
-- `min_interval_secs <= base_interval_secs` is enforced by `IntervalPolicy::new` on every write.
-- 'away' is allowed in the mode CHECK now so the column is never rebuilt in phase 5;
-- monitoring.set refuses it until then.
ALTER TABLE orchestrator_settings ADD COLUMN monitoring_mode TEXT NOT NULL DEFAULT 'on'
    CHECK (monitoring_mode IN ('on', 'quiet', 'off', 'away'));
-- Seconds, because the minimum may be as low as 10 seconds. Base 1 to 240 minutes.
-- More work shortens the interval from the base down to the minimum; there is no maximum.
ALTER TABLE orchestrator_settings ADD COLUMN base_interval_secs INTEGER NOT NULL DEFAULT 600
    CHECK (base_interval_secs BETWEEN 60 AND 14400);
ALTER TABLE orchestrator_settings ADD COLUMN min_interval_secs INTEGER NOT NULL DEFAULT 60
    CHECK (min_interval_secs BETWEEN 10 AND 14400);
-- NULL means no cap.
ALTER TABLE orchestrator_settings ADD COLUMN wake_hourly_cap INTEGER DEFAULT 6
    CHECK (wake_hourly_cap IS NULL OR wake_hourly_cap BETWEEN 1 AND 1000);
ALTER TABLE orchestrator_settings ADD COLUMN wake_daily_cap INTEGER DEFAULT 40
    CHECK (wake_daily_cap IS NULL OR wake_daily_cap BETWEEN 1 AND 10000);
ALTER TABLE orchestrator_settings ADD COLUMN wake_daily_token_budget INTEGER
    CHECK (wake_daily_token_budget IS NULL OR wake_daily_token_budget > 0);
ALTER TABLE orchestrator_settings ADD COLUMN attempt_wall_minutes INTEGER NOT NULL DEFAULT 120
    CHECK (attempt_wall_minutes BETWEEN 5 AND 1440);
-- Last durable event the classifier has seen. Internal state, not a setting: it is not part of
-- `OrchestratorSettings` and no `settings.set` field reaches it. It advances in the same
-- transaction that records the wake records for the events before it.
ALTER TABLE orchestrator_settings ADD COLUMN wake_cursor INTEGER NOT NULL DEFAULT 0
    CHECK (wake_cursor >= 0);
-- Start at the newest event, so history from before this migration is never replayed. The row
-- already exists (0039), so this is an UPDATE, never an INSERT.
UPDATE orchestrator_settings
   SET wake_cursor = (SELECT COALESCE(MAX(sequence), 0) FROM durable_events)
 WHERE id = 1;
