-- DPES-1105 phase 4b: one row per project that has a default. No row means local-only,
-- the behavior before this migration. The row exists only when the user sets it.
--
-- A branch name or prefix is bounded in length and holds no control character (1 to 31 and
-- 127). The Git ref rules (`..`, `@{`, `HEAD`, allowed characters) are checked by the daemon,
-- which calls `is_safe_branch_name`; they are not repeated here.

CREATE TABLE project_defaults (
    project_id TEXT PRIMARY KEY REFERENCES projects(id),
    mode TEXT NOT NULL DEFAULT 'local-only'
        CHECK (mode IN ('local-only', 'direct-pr', 'no-mistakes')),
    -- NULL means "HEAD of the primary checkout", the behavior before this migration.
    base_branch TEXT
        CHECK (base_branch IS NULL OR (
            length(base_branch) BETWEEN 1 AND 100
            AND base_branch NOT GLOB ('*[' || char(1) || '-' || char(31) || char(127) || ']*')
        )),
    -- NULL means the machine-wide prefix (`BranchPrefix::read`, default `moa/`).
    branch_prefix TEXT
        CHECK (branch_prefix IS NULL OR (
            length(branch_prefix) BETWEEN 2 AND 64
            AND branch_prefix NOT GLOB ('*[' || char(1) || '-' || char(31) || char(127) || ']*')
        )),
    -- Argument arrays, as VerificationPolicy::Commands holds them. [] means none.
    checks_json TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(checks_json) AND length(checks_json) <= 4096),
    merge_method TEXT NOT NULL DEFAULT 'squash'
        CHECK (merge_method IN ('merge', 'squash', 'rebase')),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TRIGGER project_defaults_gated_delete
BEFORE DELETE ON project_defaults
BEGIN
    SELECT RAISE(ABORT, 'project defaults are replaced, not deleted');
END;

-- Soft archive for project.remove (4d). A project row is never deleted: tasks reference it.
ALTER TABLE projects ADD COLUMN archived_at TEXT
    CHECK (archived_at IS NULL OR length(archived_at) > 0);

-- A machine-wide switch, off by default. The row is never rebuilt, so it is added here.
ALTER TABLE orchestrator_settings ADD COLUMN chat_pr_delivery INTEGER NOT NULL DEFAULT 0
    CHECK (chat_pr_delivery IN (0, 1));
