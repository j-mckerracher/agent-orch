-- DPES-1105: action proposals parsed from orchestrator replies.
--
-- A proposal comes only from the action block of a completed turn and does
-- nothing until the user approves it (actions.resolve, a later change).
-- - action_proposals rows change state, never parameters. A history clear
--   deletes a conversation's proposals under the clear gate, before its turns.
-- - action_proposal_events is append-only, has no foreign key and holds digests,
--   never parameters, so it outlives a clear without keeping text.
-- - conversation_turns.action_block records what happened to a turn's block.

CREATE TABLE action_proposals (
    id TEXT PRIMARY KEY CHECK (length(id) > 0),
    scope TEXT NOT NULL REFERENCES conversations(id),
    turn_id TEXT NOT NULL REFERENCES conversation_turns(id),
    idx INTEGER NOT NULL CHECK (idx BETWEEN 0 AND 9),
    kind TEXT NOT NULL CHECK (kind IN
        ('workstream.create', 'workstream.update', 'objective.add', 'objective.revise')),
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
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (turn_id, idx),
    CHECK ((state = 'failed') = (error_code IS NOT NULL))
);
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

-- Only these state changes exist. The failed edges serve dependency_not_executed and
-- execution failures in phase 1c. A terminal row (executed, failed, rejected,
-- superseded, expired) never changes again.
CREATE TRIGGER action_proposals_state_transitions
BEFORE UPDATE OF state ON action_proposals
WHEN NOT (
    (OLD.state = 'pending' AND NEW.state IN ('approved', 'rejected', 'superseded', 'expired', 'failed'))
    OR (OLD.state = 'approved' AND NEW.state IN ('executed', 'failed'))
)
BEGIN
    SELECT RAISE(ABORT, 'illegal action proposal state transition');
END;

CREATE TABLE action_proposal_events (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    proposal_id TEXT NOT NULL CHECK (length(proposal_id) > 0),
    scope TEXT NOT NULL CHECK (length(scope) > 0),
    from_state TEXT CHECK (from_state IS NULL OR from_state IN
        ('pending', 'approved', 'executed', 'failed', 'rejected', 'superseded', 'expired')),
    to_state TEXT NOT NULL CHECK (to_state IN
        ('pending', 'approved', 'executed', 'failed', 'rejected', 'superseded', 'expired')),
    -- 'grant:<id>' is phase 5; allowed now so this table is never rebuilt.
    actor TEXT NOT NULL CHECK (actor IN ('agent', 'user')
        OR (actor GLOB 'grant:?*' AND length(actor) <= 134)),
    digest TEXT NOT NULL CHECK (length(digest) = 64 AND digest NOT GLOB '*[^0-9a-f]*'),
    error_code TEXT CHECK (error_code IS NULL OR length(error_code) BETWEEN 1 AND 64),
    result_ref TEXT CHECK (result_ref IS NULL OR length(result_ref) BETWEEN 1 AND 256),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (from_state IS NOT NULL OR to_state = 'pending')
);
CREATE INDEX action_proposal_events_by_proposal ON action_proposal_events(proposal_id, sequence);

CREATE TRIGGER action_proposal_events_no_update
BEFORE UPDATE ON action_proposal_events
BEGIN
    SELECT RAISE(ABORT, 'action proposal events are append-only');
END;

CREATE TRIGGER action_proposal_events_no_delete
BEFORE DELETE ON action_proposal_events
BEGIN
    SELECT RAISE(ABORT, 'action proposal events are append-only');
END;

ALTER TABLE conversation_turns ADD COLUMN action_block TEXT
    CHECK (action_block IS NULL OR action_block IN ('accepted', 'rejected', 'stripped'));
