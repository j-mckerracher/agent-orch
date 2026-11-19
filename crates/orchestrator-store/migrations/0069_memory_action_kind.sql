-- DPES-1108 phase 6: action_proposals gains the kind memory.set.
--
-- SQLite cannot alter a CHECK, so the table is rebuilt exactly as 0065 left it: same columns,
-- indexes and triggers, every row keeps its rowid, and nothing has a foreign key to it. The
-- 0064 insert trigger on decision_holds reads action_proposals, and SQLite checks every trigger
-- when a table is renamed, so it is dropped first and created again unchanged.
--
-- Note for merges: other DPES-1108 phases may rebuild this table from the same 0065 shape. The
-- later migration must carry the union of every kind and column.

DROP TRIGGER decision_holds_open_decision;

-- action_proposals: the rules of 0065, with memory.set.
CREATE TABLE action_proposals_new (
    id TEXT PRIMARY KEY CHECK (length(id) > 0),
    scope TEXT NOT NULL REFERENCES conversations(id),
    turn_id TEXT NOT NULL REFERENCES conversation_turns(id),
    idx INTEGER NOT NULL CHECK (idx BETWEEN 0 AND 9),
    kind TEXT NOT NULL CHECK (kind IN
        ('workstream.create', 'workstream.update', 'objective.add', 'objective.revise',
         'task.propose', 'task.cancel', 'task.retry', 'workstream.link_task',
         'task.revise', 'delivery.request', 'delivery.merge', 'project.register',
         'project.remove', 'decision.hold', 'decision.ask', 'task.message', 'memory.set')),
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

-- The 0064 trigger, unchanged.
CREATE TRIGGER decision_holds_open_decision BEFORE INSERT ON decision_holds
WHEN (NEW.subject_kind = 'approval' AND NOT EXISTS (
        SELECT 1 FROM approvals WHERE task_id = NEW.subject_id AND status = 'requested'))
  OR (NEW.subject_kind = 'proposal' AND NOT EXISTS (
        SELECT 1 FROM action_proposals WHERE id = NEW.subject_id AND state = 'pending'))
  OR (NEW.subject_kind = 'question' AND NOT EXISTS (
        SELECT 1 FROM tasks WHERE id = NEW.subject_id))
BEGIN SELECT RAISE(ABORT, 'a hold needs an open decision'); END;
