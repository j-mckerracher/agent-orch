-- M7-07: workstreams, objectives, and task links.
-- Version 0030 leaves 0011-0029 free for concurrently developed migrations.
-- Additive only: no existing table is altered, so existing data directories
-- migrate without loss. A task's contract, parent, and project are untouched;
-- a link row only says which workstream a task serves.

CREATE TABLE workstreams (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE CHECK (length(id) > 0),
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    summary TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('active', 'suspended', 'archived')),
    revision INTEGER NOT NULL CHECK (revision > 0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Gates are a validated JSON array: they are always read and replaced with the
-- objective, and the domain model keeps them append-only.
CREATE TABLE objectives (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE CHECK (length(id) > 0),
    workstream_id TEXT NOT NULL REFERENCES workstreams(id),
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    description TEXT NOT NULL,
    gates_json TEXT NOT NULL CHECK (json_valid(gates_json) AND json_type(gates_json) = 'array'),
    revision INTEGER NOT NULL CHECK (revision > 0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (id, workstream_id)
);

CREATE INDEX objectives_by_workstream ON objectives(workstream_id, sequence);

-- A task serves at most one workstream. The composite foreign key keeps a
-- linked objective inside the linked workstream; it is skipped when the task
-- is linked to the workstream alone (objective_id NULL).
CREATE TABLE workstream_tasks (
    task_id TEXT PRIMARY KEY REFERENCES tasks(id),
    workstream_id TEXT NOT NULL REFERENCES workstreams(id),
    objective_id TEXT,
    linked_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (objective_id, workstream_id) REFERENCES objectives(id, workstream_id)
);

CREATE INDEX workstream_tasks_by_workstream ON workstream_tasks(workstream_id);

-- Append-only audit of every accepted change, one snapshot per revision.
CREATE TABLE workstream_revisions (
    entity_kind TEXT NOT NULL CHECK (entity_kind IN ('workstream', 'objective')),
    entity_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    snapshot_json TEXT NOT NULL CHECK (json_valid(snapshot_json)),
    actor TEXT NOT NULL CHECK (length(actor) > 0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (entity_kind, entity_id, revision)
);

CREATE TRIGGER workstream_revisions_no_update
BEFORE UPDATE ON workstream_revisions
BEGIN
    SELECT RAISE(ABORT, 'workstream_revisions is append-only');
END;

CREATE TRIGGER workstream_revisions_no_delete
BEFORE DELETE ON workstream_revisions
BEGIN
    SELECT RAISE(ABORT, 'workstream_revisions is append-only');
END;
