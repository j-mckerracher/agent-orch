-- DPES-1108 phase 4: worker status, waits and steering by resume.
--
-- A task worker reports on a status file the daemon tails; each accepted line is one row of
-- task_status_events. A worker that reports needs-decision or blocked ends its turn and its
-- task waits (the task state "Waiting", no CHECK on tasks.state). A reply for a waiting task
-- (the user's answer, a reconcile, or a task.message) is a task_messages row, delivered once,
-- by resuming the same agent session: sessions.agent_session_id names it.
--
-- action_proposals gains the kind task.message and wake_records the kinds
-- worker_needs_decision and worker_blocked. SQLite cannot alter a CHECK, so both are rebuilt
-- as 0063 and 0060 did: every row keeps its rowid, and nothing has a foreign key to either. The
-- 0063 insert trigger on decision_holds reads action_proposals, and SQLite checks every
-- trigger when a table is renamed, so it is dropped first and created again unchanged.

-- The id the agent CLI gave the session (Codex reports it) or was given (Claude and Copilot
-- take one). Written once.
ALTER TABLE sessions ADD COLUMN agent_session_id TEXT
    CHECK (agent_session_id IS NULL OR (length(agent_session_id) BETWEEN 1 AND 128
        AND agent_session_id NOT GLOB '*[^0-9A-Za-z-]*'));

CREATE TRIGGER sessions_agent_session_id_once
BEFORE UPDATE OF agent_session_id ON sessions
WHEN OLD.agent_session_id IS NOT NULL AND NEW.agent_session_id IS NOT OLD.agent_session_id
BEGIN
    SELECT RAISE(ABORT, 'an agent session id is written once');
END;

-- One accepted status line. line_no is its 1-based line in the attempt's status file, so a
-- second read of the same file adds nothing. The note is worker text, redacted before it is
-- stored, and never an instruction.
CREATE TABLE task_status_events (
    id INTEGER PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    attempt_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    line_no INTEGER NOT NULL CHECK (line_no BETWEEN 1 AND 200),
    verb TEXT NOT NULL CHECK (verb IN
        ('working', 'needs-decision', 'blocked', 'done', 'failed')),
    status_key TEXT CHECK (status_key IS NULL OR (length(status_key) BETWEEN 1 AND 64
        AND status_key NOT GLOB '*[^a-z0-9-]*')),
    note TEXT CHECK (note IS NULL OR length(note) BETWEEN 1 AND 1024),
    recorded_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (task_id, attempt_id, line_no),
    CHECK (verb <> 'needs-decision' OR note IS NOT NULL),
    FOREIGN KEY (task_id, attempt_id) REFERENCES attempts(task_id, id)
);

CREATE TRIGGER task_status_events_append_only BEFORE UPDATE ON task_status_events
BEGIN SELECT RAISE(ABORT, 'a status event never changes'); END;

CREATE TRIGGER task_status_events_no_delete BEFORE DELETE ON task_status_events
BEGIN SELECT RAISE(ABORT, 'status events are never deleted'); END;

-- A reply for a waiting task. 'answer' and 'reconcile' copy nothing: they name the question,
-- whose decision_answers record holds the user's words. 'message' holds a task.message text.
-- delivered_session_id is set once, by the resume that carries it. message_key is the
-- execution key of the task.message that queued it, so a rerun queues nothing twice.
CREATE TABLE task_messages (
    id INTEGER PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    attempt_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('answer', 'reconcile', 'message')),
    question_id TEXT REFERENCES decision_holds(id),
    text TEXT CHECK (text IS NULL OR length(text) BETWEEN 1 AND 4096),
    message_key TEXT UNIQUE CHECK (message_key IS NULL OR length(message_key) BETWEEN 1 AND 160),
    delivered_session_id TEXT CHECK (delivered_session_id IS NULL
        OR length(delivered_session_id) > 0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (CASE WHEN kind = 'message'
        THEN text IS NOT NULL AND question_id IS NULL AND message_key IS NOT NULL
        ELSE text IS NULL AND question_id IS NOT NULL AND message_key IS NULL END),
    FOREIGN KEY (task_id, attempt_id) REFERENCES attempts(task_id, id)
);

CREATE INDEX task_messages_undelivered
    ON task_messages(task_id) WHERE delivered_session_id IS NULL;

CREATE TRIGGER task_messages_delivered_once
BEFORE UPDATE ON task_messages
WHEN OLD.delivered_session_id IS NOT NULL
    OR NEW.delivered_session_id IS NULL
    OR NEW.id IS NOT OLD.id OR NEW.task_id IS NOT OLD.task_id
    OR NEW.attempt_id IS NOT OLD.attempt_id OR NEW.kind IS NOT OLD.kind
    OR NEW.question_id IS NOT OLD.question_id OR NEW.text IS NOT OLD.text
    OR NEW.message_key IS NOT OLD.message_key OR NEW.created_at IS NOT OLD.created_at
BEGIN
    SELECT RAISE(ABORT, 'a task message is only marked delivered, once');
END;

CREATE TRIGGER task_messages_no_delete BEFORE DELETE ON task_messages
BEGIN SELECT RAISE(ABORT, 'task messages are never deleted'); END;

DROP TRIGGER decision_holds_open_decision;

-- action_proposals: the rules of 0063, with task.message.
CREATE TABLE action_proposals_new (
    id TEXT PRIMARY KEY CHECK (length(id) > 0),
    scope TEXT NOT NULL REFERENCES conversations(id),
    turn_id TEXT NOT NULL REFERENCES conversation_turns(id),
    idx INTEGER NOT NULL CHECK (idx BETWEEN 0 AND 9),
    kind TEXT NOT NULL CHECK (kind IN
        ('workstream.create', 'workstream.update', 'objective.add', 'objective.revise',
         'task.propose', 'task.cancel', 'task.retry', 'workstream.link_task',
         'task.revise', 'delivery.request', 'delivery.merge', 'project.register',
         'project.remove', 'decision.hold', 'decision.ask', 'task.message')),
    params_json TEXT NOT NULL CHECK (json_valid(params_json) AND length(params_json) <= 20480),
    digest TEXT NOT NULL CHECK (length(digest) = 64 AND digest NOT GLOB '*[^0-9a-f]*'),
    subject TEXT NOT NULL CHECK (length(subject) BETWEEN 1 AND 512),
    depends_on TEXT CHECK (depends_on IS NULL OR length(depends_on) > 0),
    state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN
        ('pending', 'approved', 'executed', 'failed', 'rejected', 'superseded', 'expired')),
    result_ref TEXT CHECK (result_ref IS NULL OR length(result_ref) BETWEEN 1 AND 256),
    error_code TEXT CHECK (error_code IS NULL OR length(error_code) BETWEEN 1 AND 64),
    denial TEXT CHECK (denial IS NULL OR length(denial) BETWEEN 1 AND 64),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    grant_tried INTEGER NOT NULL DEFAULT 0 CHECK (grant_tried IN (0, 1)),
    UNIQUE (turn_id, idx),
    CHECK ((state = 'failed') = (error_code IS NOT NULL)),
    CHECK ((denial IS NOT NULL) = (error_code IS NOT NULL AND error_code = 'action_denied'))
);

INSERT INTO action_proposals_new
    (rowid, id, scope, turn_id, idx, kind, params_json, digest, subject, depends_on, state,
     result_ref, error_code, denial, created_at, updated_at, grant_tried)
SELECT rowid, id, scope, turn_id, idx, kind, params_json, digest, subject, depends_on, state,
       result_ref, error_code, denial, created_at, updated_at, grant_tried
FROM action_proposals;

DROP TABLE action_proposals;
ALTER TABLE action_proposals_new RENAME TO action_proposals;

CREATE INDEX action_proposals_by_scope ON action_proposals(scope, created_at);
CREATE UNIQUE INDEX action_proposals_one_pending_digest
    ON action_proposals(scope, kind, digest) WHERE state = 'pending';
CREATE INDEX action_proposals_pending_subject
    ON action_proposals(scope, kind, subject) WHERE state = 'pending';
CREATE INDEX action_proposals_grant_untried ON action_proposals(created_at)
    WHERE state = 'pending' AND grant_tried = 0;

CREATE TRIGGER action_proposals_gated_delete
BEFORE DELETE ON action_proposals
WHEN NOT EXISTS (SELECT 1 FROM conversation_clear_gate WHERE id = 1 AND open = 1)
BEGIN
    SELECT RAISE(ABORT, 'action proposals are deleted only by a history clear');
END;

CREATE TRIGGER action_proposals_fixed_parameters
BEFORE UPDATE OF id, scope, turn_id, idx, kind, params_json, digest, subject, depends_on, created_at
ON action_proposals
BEGIN
    SELECT RAISE(ABORT, 'action proposal parameters never change');
END;

CREATE TRIGGER action_proposals_denial_once
BEFORE UPDATE OF denial ON action_proposals
WHEN OLD.denial IS NOT NULL OR OLD.state <> 'pending'
BEGIN
    SELECT RAISE(ABORT, 'a proposal denial is written once, on a pending proposal');
END;

CREATE TRIGGER action_proposals_state_transitions
BEFORE UPDATE OF state ON action_proposals
WHEN NOT (
    (OLD.state = 'pending' AND NEW.state IN ('approved', 'rejected', 'superseded', 'expired', 'failed'))
    OR (OLD.state = 'approved' AND NEW.state IN ('executed', 'failed'))
)
BEGIN
    SELECT RAISE(ABORT, 'illegal action proposal state transition');
END;

-- The 0063 trigger, unchanged.
CREATE TRIGGER decision_holds_open_decision BEFORE INSERT ON decision_holds
WHEN (NEW.subject_kind = 'approval' AND NOT EXISTS (
        SELECT 1 FROM approvals WHERE task_id = NEW.subject_id AND status = 'requested'))
  OR (NEW.subject_kind = 'proposal' AND NOT EXISTS (
        SELECT 1 FROM action_proposals WHERE id = NEW.subject_id AND state = 'pending'))
  OR (NEW.subject_kind = 'question' AND NOT EXISTS (
        SELECT 1 FROM tasks WHERE id = NEW.subject_id))
BEGIN SELECT RAISE(ABORT, 'a hold needs an open decision'); END;

-- wake_records: the rules of 0060, with the two worker wait kinds.
CREATE TABLE wake_records_new (
    id TEXT PRIMARY KEY CHECK (length(id) BETWEEN 1 AND 128),
    scope TEXT NOT NULL CHECK (length(scope) > 0),
    event_kind TEXT NOT NULL CHECK (event_kind IN
        ('task_failed', 'task_stalled', 'delivery_waiting', 'proposal_waiting',
         'sign_in_needed', 'daemon_start', 'work_check', 'scout_report_ready', 'agent_failing',
         'candidate_ready', 'worker_needs_decision', 'worker_blocked')),
    subject TEXT NOT NULL CHECK (length(subject) BETWEEN 1 AND 128),
    dedupe_key TEXT NOT NULL CHECK (length(dedupe_key) BETWEEN 1 AND 160),
    notice TEXT NOT NULL CHECK (length(notice) BETWEEN 1 AND 512),
    needs_model INTEGER NOT NULL CHECK (needs_model IN (0, 1)),
    state TEXT NOT NULL CHECK (state IN
        ('noticed', 'pending', 'running', 'handled', 'skipped', 'failed')),
    outcome TEXT CHECK (outcome IS NULL OR outcome IN ('replied', 'quiet', 'unanswered')),
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
BEFORE UPDATE OF id, scope, event_kind, subject, dedupe_key, needs_model,
    anchor_sequence, created_at
ON wake_records
BEGIN
    SELECT RAISE(ABORT, 'wake record identity never changes');
END;

CREATE TRIGGER wake_records_notice_fixed
BEFORE UPDATE OF notice ON wake_records
WHEN NEW.notice IS NOT OLD.notice AND NOT (OLD.state = 'running' AND NEW.state = 'failed')
BEGIN
    SELECT RAISE(ABORT, 'wake record notice changes only when its running turn fails');
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
