-- Rebuild the parent table with a distinct project target. The store holds one
-- connection with foreign keys disabled during migration and checks them afterward.
CREATE TABLE conversations_new (
    id TEXT PRIMARY KEY CHECK (length(id) > 0),
    target_kind TEXT NOT NULL CHECK (target_kind IN ('global', 'workstream', 'project')),
    workstream_id TEXT CHECK (workstream_id IS NULL OR length(workstream_id) > 0),
    project_id TEXT REFERENCES projects(id),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    clear_generation INTEGER NOT NULL DEFAULT 0 CHECK (clear_generation >= 0),
    CHECK ((target_kind = 'workstream') = (workstream_id IS NOT NULL)),
    CHECK ((target_kind = 'project') = (project_id IS NOT NULL))
);

INSERT INTO conversations_new (rowid, id, target_kind, workstream_id, created_at, clear_generation)
SELECT rowid, id, target_kind, workstream_id, created_at, clear_generation FROM conversations;
DROP TABLE conversations;
ALTER TABLE conversations_new RENAME TO conversations;

CREATE UNIQUE INDEX conversations_one_per_target
    ON conversations(target_kind, COALESCE(workstream_id, project_id, ''));

INSERT INTO conversations (id, target_kind, project_id, clear_generation)
SELECT 'conversation:project:' || project_id, 'project', project_id, clear_generation
FROM coordinator_scopes;
