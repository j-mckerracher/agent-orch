-- DPES-1105 phase 4: six more kinds, and a denial that can be written when a pending
-- proposal is refused at approve time.
--
-- SQLite cannot alter a CHECK, and 0038 lists eight kinds, so action_proposals is rebuilt
-- the way 0038 did it. Nothing has a foreign key to it, action_proposal_events is not
-- touched, and every row keeps its rowid. DROP TABLE removes the old table's indexes and
-- triggers, and the implicit delete it runs fires no trigger, so the delete gate is not in
-- the way. All of them are created again below with the rules of 0038, except that
-- `denial` is no longer among the fixed columns: action_proposals_denial_once guards it.
--
-- The kind list holds all fourteen design kinds, including delivery.request and
-- project.register, which this build does not parse, so no later phase rebuilds the table.

CREATE TABLE action_proposals_new (
    id TEXT PRIMARY KEY CHECK (length(id) > 0),
    scope TEXT NOT NULL REFERENCES conversations(id),
    turn_id TEXT NOT NULL REFERENCES conversation_turns(id),
    idx INTEGER NOT NULL CHECK (idx BETWEEN 0 AND 9),
    kind TEXT NOT NULL CHECK (kind IN
        ('workstream.create', 'workstream.update', 'objective.add', 'objective.revise',
         'task.propose', 'task.cancel', 'task.retry', 'workstream.link_task',
         'task.revise', 'delivery.request', 'delivery.merge', 'project.register',
         'project.remove', 'decision.hold')),
    -- Canonical JSON can be longer than the block body: a ref is replaced by a 73-char
    -- proposal id, so a valid near-cap block must still fit here.
    params_json TEXT NOT NULL CHECK (json_valid(params_json) AND length(params_json) <= 20480),
    digest TEXT NOT NULL CHECK (length(digest) = 64 AND digest NOT GLOB '*[^0-9a-f]*'),
    subject TEXT NOT NULL CHECK (length(subject) BETWEEN 1 AND 512),
    depends_on TEXT CHECK (depends_on IS NULL OR length(depends_on) > 0),
    state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN
        ('pending', 'approved', 'executed', 'failed', 'rejected', 'superseded', 'expired')),
    result_ref TEXT CHECK (result_ref IS NULL OR length(result_ref) BETWEEN 1 AND 256),
    error_code TEXT CHECK (error_code IS NULL OR length(error_code) BETWEEN 1 AND 64),
    -- Why the envelope refused a proposal, at insert or at approve time. Written once and
    -- never changed, and set exactly when error_code is 'action_denied'.
    denial TEXT CHECK (denial IS NULL OR length(denial) BETWEEN 1 AND 64),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (turn_id, idx),
    CHECK ((state = 'failed') = (error_code IS NOT NULL)),
    CHECK ((denial IS NOT NULL) = (error_code IS NOT NULL AND error_code = 'action_denied'))
);

INSERT INTO action_proposals_new
    (rowid, id, scope, turn_id, idx, kind, params_json, digest, subject, depends_on, state,
     result_ref, error_code, denial, created_at, updated_at)
SELECT rowid, id, scope, turn_id, idx, kind, params_json, digest, subject, depends_on, state,
       result_ref, error_code, denial, created_at, updated_at
FROM action_proposals;

DROP TABLE action_proposals;
ALTER TABLE action_proposals_new RENAME TO action_proposals;

CREATE INDEX action_proposals_by_scope ON action_proposals(scope, created_at);
-- At most one pending card per request: the dedup rule, held by the database.
CREATE UNIQUE INDEX action_proposals_one_pending_digest
    ON action_proposals(scope, kind, digest) WHERE state = 'pending';
CREATE INDEX action_proposals_pending_subject
    ON action_proposals(scope, kind, subject) WHERE state = 'pending';

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

-- The denial is written once, on a pending row, in the statement that also sets state and
-- error_code (the table CHECK ties all three together), and never changes after. A denied
-- task inserted already failed is an insert, so this trigger never sees it.
CREATE TRIGGER action_proposals_denial_once
BEFORE UPDATE OF denial ON action_proposals
WHEN OLD.denial IS NOT NULL OR OLD.state <> 'pending'
BEGIN
    SELECT RAISE(ABORT, 'a proposal denial is written once, on a pending proposal');
END;

-- Only these state changes exist, for every kind. A terminal row (executed, failed,
-- rejected, superseded, expired) never changes again. A denied task is inserted
-- already failed, so it never takes an edge.
CREATE TRIGGER action_proposals_state_transitions
BEFORE UPDATE OF state ON action_proposals
WHEN NOT (
    (OLD.state = 'pending' AND NEW.state IN ('approved', 'rejected', 'superseded', 'expired', 'failed'))
    OR (OLD.state = 'approved' AND NEW.state IN ('executed', 'failed'))
)
BEGIN
    SELECT RAISE(ABORT, 'illegal action proposal state transition');
END;
