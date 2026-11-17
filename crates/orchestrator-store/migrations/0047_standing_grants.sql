-- DPES-1105 phase 5a: standing grants and their audit trail.
--
-- A grant is created by grants.create (user-only) and never edited. Its terms are fixed by
-- trigger; only uses_used (up by exactly one per use, never after a revoke) and revoked_at_ms
-- (set once) change. Both times are supplied by the caller; the store reads no SQL clock here.
-- standing_grant_events is append-only and has no foreign key, so the audit and the token
-- budget survive a history clear that deletes proposals. Status (active, expired, exhausted,
-- revoked) is computed on read, never stored. A grant is never deleted.
--
-- 0046 stays unused (the unbuilt merge worker), so there is no merge kind here.

CREATE TABLE standing_grants (
    id TEXT PRIMARY KEY CHECK (length(id) BETWEEN 1 AND 120),
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('project', 'workstream')),
    scope_id TEXT NOT NULL CHECK (length(scope_id) BETWEEN 1 AND 128),
    -- JSON array of wire kind names, 1 to 16 entries. The core crate validates each one
    -- against the grantable list; SQLite checks the shape and refuses the never-grantable names.
    kinds TEXT NOT NULL CHECK (json_valid(kinds) AND json_type(kinds) = 'array'
        AND json_array_length(kinds) BETWEEN 1 AND 16
        AND instr(kinds, '"project.remove"') = 0
        AND instr(kinds, '"delivery.merge"') = 0
        AND instr(kinds, '"task.cancel"') = 0
        AND instr(kinds, '"task.revise"') = 0),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    expires_at_ms INTEGER NOT NULL,
    max_uses INTEGER NOT NULL CHECK (max_uses BETWEEN 1 AND 200),
    uses_used INTEGER NOT NULL DEFAULT 0,
    token_budget INTEGER NOT NULL CHECK (token_budget BETWEEN 1 AND 1000000000),
    -- A proposal matches only if its creation event sequence is above this: no retroactive consent.
    proposal_floor INTEGER NOT NULL CHECK (proposal_floor >= 0),
    revoked_at_ms INTEGER CHECK (revoked_at_ms IS NULL OR revoked_at_ms >= created_at_ms),
    CHECK (expires_at_ms > created_at_ms AND expires_at_ms - created_at_ms <= 604800000),
    CHECK (uses_used BETWEEN 0 AND max_uses)
);
CREATE INDEX standing_grants_open ON standing_grants(expires_at_ms) WHERE revoked_at_ms IS NULL;

CREATE TRIGGER standing_grants_fixed_terms
BEFORE UPDATE OF id, scope_kind, scope_id, kinds, created_at_ms, expires_at_ms, max_uses,
    token_budget, proposal_floor
ON standing_grants
BEGIN
    SELECT RAISE(ABORT, 'standing grant terms never change');
END;

CREATE TRIGGER standing_grants_use_counter
BEFORE UPDATE OF uses_used ON standing_grants
WHEN NEW.uses_used <> OLD.uses_used + 1 OR OLD.revoked_at_ms IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'a standing grant use count rises by one, and never after a revoke');
END;

CREATE TRIGGER standing_grants_revoke_once
BEFORE UPDATE OF revoked_at_ms ON standing_grants
WHEN OLD.revoked_at_ms IS NOT NULL OR NEW.revoked_at_ms IS NULL
BEGIN
    SELECT RAISE(ABORT, 'a standing grant is revoked once and never restored');
END;

CREATE TRIGGER standing_grants_no_delete
BEFORE DELETE ON standing_grants
BEGIN
    SELECT RAISE(ABORT, 'standing grants are never deleted');
END;

CREATE TABLE standing_grant_events (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    grant_id TEXT NOT NULL CHECK (length(grant_id) > 0),
    event TEXT NOT NULL CHECK (event IN ('created', 'used', 'executed', 'stopped', 'revoked')),
    -- 'used' is written in the transaction that consumes the use. 'executed' and 'stopped'
    -- (an execution failure code) close it later.
    proposal_id TEXT CHECK (proposal_id IS NULL OR length(proposal_id) > 0),
    digest TEXT CHECK (digest IS NULL OR (length(digest) = 64 AND digest NOT GLOB '*[^0-9a-f]*')),
    -- The task an execution created or acted on, so the token budget can sum its receipts.
    task_id TEXT CHECK (task_id IS NULL OR length(task_id) > 0),
    code TEXT CHECK (code IS NULL OR length(code) BETWEEN 1 AND 64),
    at_ms INTEGER NOT NULL CHECK (at_ms >= 0),
    CHECK ((event IN ('used', 'executed', 'stopped')) = (proposal_id IS NOT NULL AND digest IS NOT NULL))
);
CREATE INDEX standing_grant_events_by_grant ON standing_grant_events(grant_id, sequence);
CREATE INDEX standing_grant_events_tasks ON standing_grant_events(grant_id, task_id)
    WHERE task_id IS NOT NULL;

CREATE TRIGGER standing_grant_events_no_update
BEFORE UPDATE ON standing_grant_events
BEGIN
    SELECT RAISE(ABORT, 'standing grant events are append-only');
END;

CREATE TRIGGER standing_grant_events_no_delete
BEFORE DELETE ON standing_grant_events
BEGIN
    SELECT RAISE(ABORT, 'standing grant events are append-only');
END;

-- The matcher tries each new proposal once. Not a fixed parameter, so the 0042 trigger that
-- guards parameters does not list it.
ALTER TABLE action_proposals ADD COLUMN grant_tried INTEGER NOT NULL DEFAULT 0
    CHECK (grant_tried IN (0, 1));
CREATE INDEX action_proposals_grant_untried ON action_proposals(created_at)
    WHERE state = 'pending' AND grant_tried = 0;
-- Proposals that exist when this migration runs are never matched.
UPDATE action_proposals SET grant_tried = 1;
