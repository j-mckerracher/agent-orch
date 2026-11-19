-- Authority model (DPES-1108 phase 5, plan 5.5).
--
-- autonomous_actions: the user's switch for actions the orchestrator runs on its own. On by
-- default (the user chose the firstmate model); off means every action is a card again.
--
-- task_origins: the user instruction a task the orchestrator started on its own cites. One row
-- per task, written right after the task exists, never changed. Only a user turn is written
-- today. 'routed_request' is allowed now for phase 7's routed requests, so this table is not
-- rebuilt to add it. A wake turn is never an origin; the store checks the turn on insert.
--
-- task_overrides: an explicit departure from a project default for one task, with the words of
-- the user turn that asked for it. Read only for that task.

ALTER TABLE orchestrator_settings ADD COLUMN autonomous_actions INTEGER NOT NULL DEFAULT 1
    CHECK (autonomous_actions IN (0, 1));

CREATE TABLE task_origins (
    task_id TEXT PRIMARY KEY REFERENCES tasks(id),
    origin_kind TEXT NOT NULL CHECK (origin_kind IN ('user_turn', 'routed_request')),
    origin_ref TEXT NOT NULL CHECK (length(origin_ref) BETWEEN 1 AND 128),
    proposal_id TEXT NOT NULL CHECK (length(proposal_id) BETWEEN 1 AND 128),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TRIGGER task_origins_no_update BEFORE UPDATE ON task_origins
BEGIN SELECT RAISE(ABORT, 'a task origin never changes'); END;

CREATE TRIGGER task_origins_no_delete BEFORE DELETE ON task_origins
BEGIN SELECT RAISE(ABORT, 'a task origin is never deleted'); END;

CREATE TABLE task_overrides (
    task_id TEXT NOT NULL REFERENCES tasks(id),
    field TEXT NOT NULL CHECK (field IN ('delivery')),
    project_default TEXT NOT NULL CHECK (length(project_default) BETWEEN 1 AND 64),
    chosen TEXT NOT NULL CHECK (length(chosen) BETWEEN 1 AND 64),
    user_words TEXT NOT NULL CHECK (length(user_words) BETWEEN 1 AND 16384),
    turn_id TEXT NOT NULL CHECK (length(turn_id) BETWEEN 1 AND 128),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (task_id, field),
    CHECK (project_default <> chosen)
);

CREATE TRIGGER task_overrides_no_update BEFORE UPDATE ON task_overrides
BEGIN SELECT RAISE(ABORT, 'a task override never changes'); END;

CREATE TRIGGER task_overrides_no_delete BEFORE DELETE ON task_overrides
BEGIN SELECT RAISE(ABORT, 'a task override is never deleted'); END;
