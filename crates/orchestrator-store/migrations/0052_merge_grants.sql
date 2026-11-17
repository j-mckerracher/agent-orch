-- DPES-1105 phase 4c (N8): the unattended merge grant.
--
-- A merge grant lets the daemon merge a pull request with no per-action click. 0047 refused the
-- `delivery.merge` name outright, and SQLite cannot alter a CHECK, so standing_grants is rebuilt
-- the way 0042 rebuilt action_proposals. Nothing has a foreign key to it and
-- standing_grant_events has none either. Every row keeps its rowid (the list order) and its
-- values, `confirm_unattended_merge` is 0 and `branch_prefix` is NULL for the rows that exist.
-- DROP TABLE removes the old table's index and triggers, and the implicit delete it runs fires
-- no trigger, so the no-delete gate is not in the way. All of them are created again below with
-- the rules of 0047, and the two new columns are fixed terms too.
--
-- The kind rules of the core crate are held here as well. A grant that names `delivery.merge`
-- names no other kind, is scoped to a project, carries the confirmation, has at most 20 uses and
-- lives at most 24 hours. A branch prefix belongs to a merge grant alone. Nothing here turns a
-- merge grant on: no row exists until the user creates one (grants.create is user-only).

CREATE TABLE standing_grants_new (
    id TEXT PRIMARY KEY CHECK (length(id) BETWEEN 1 AND 120),
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('project', 'workstream')),
    scope_id TEXT NOT NULL CHECK (length(scope_id) BETWEEN 1 AND 128),
    -- JSON array of wire kind names, 1 to 16 entries. The core crate validates each one
    -- against the grantable list; SQLite checks the shape and refuses the never-grantable names.
    kinds TEXT NOT NULL CHECK (json_valid(kinds) AND json_type(kinds) = 'array'
        AND json_array_length(kinds) BETWEEN 1 AND 16
        AND instr(kinds, '"project.remove"') = 0
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
    -- The user's explicit consent to merging with no per-action click. Merge grants only.
    confirm_unattended_merge INTEGER NOT NULL DEFAULT 0 CHECK (confirm_unattended_merge IN (0, 1)),
    -- Head-branch prefix a merge grant covers. NULL covers every branch of the project.
    branch_prefix TEXT CHECK (branch_prefix IS NULL OR length(branch_prefix) BETWEEN 1 AND 64),
    CHECK (expires_at_ms > created_at_ms AND expires_at_ms - created_at_ms <= 604800000),
    CHECK (uses_used BETWEEN 0 AND max_uses),
    CHECK (instr(kinds, '"delivery.merge"') = 0 OR (
        json_array_length(kinds) = 1
        AND scope_kind = 'project'
        AND confirm_unattended_merge = 1
        AND max_uses <= 20
        AND expires_at_ms - created_at_ms <= 86400000)),
    CHECK (branch_prefix IS NULL OR instr(kinds, '"delivery.merge"') > 0)
);

INSERT INTO standing_grants_new (rowid, id, scope_kind, scope_id, kinds, created_at_ms,
    expires_at_ms, max_uses, uses_used, token_budget, proposal_floor, revoked_at_ms)
SELECT rowid, id, scope_kind, scope_id, kinds, created_at_ms, expires_at_ms, max_uses,
    uses_used, token_budget, proposal_floor, revoked_at_ms
FROM standing_grants;

DROP TABLE standing_grants;
ALTER TABLE standing_grants_new RENAME TO standing_grants;

CREATE INDEX standing_grants_open ON standing_grants(expires_at_ms) WHERE revoked_at_ms IS NULL;

CREATE TRIGGER standing_grants_fixed_terms
BEFORE UPDATE OF id, scope_kind, scope_id, kinds, created_at_ms, expires_at_ms, max_uses,
    token_budget, proposal_floor, confirm_unattended_merge, branch_prefix
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
