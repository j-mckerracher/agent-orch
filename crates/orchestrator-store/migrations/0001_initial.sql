CREATE TABLE projects (
    id TEXT PRIMARY KEY CHECK (length(id) > 0),
    root_path TEXT NOT NULL,
    settings TEXT NOT NULL CHECK (json_valid(settings))
);

CREATE TABLE tasks (
    id TEXT PRIMARY KEY CHECK (length(id) > 0),
    project_id TEXT NOT NULL REFERENCES projects(id),
    parent_id TEXT REFERENCES tasks(id),
    state TEXT NOT NULL,
    contract_json TEXT NOT NULL CHECK (json_valid(contract_json)),
    version INTEGER NOT NULL CHECK (version > 0)
);

CREATE TABLE task_dependencies (
    task_id TEXT NOT NULL REFERENCES tasks(id),
    depends_on_id TEXT NOT NULL REFERENCES tasks(id),
    PRIMARY KEY (task_id, depends_on_id),
    CHECK (task_id <> depends_on_id)
);

CREATE INDEX task_dependencies_by_dependency ON task_dependencies(depends_on_id);

CREATE TABLE attempts (
    id TEXT NOT NULL,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    attempt_number INTEGER NOT NULL CHECK (attempt_number > 0),
    state TEXT NOT NULL,
    PRIMARY KEY (task_id, id),
    UNIQUE (task_id, attempt_number)
);

CREATE TABLE sessions (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    attempt_id TEXT NOT NULL,
    adapter TEXT NOT NULL,
    state TEXT NOT NULL,
    UNIQUE (task_id, id),
    FOREIGN KEY (task_id, attempt_id) REFERENCES attempts(task_id, id)
);

CREATE TABLE workspaces (
    id TEXT PRIMARY KEY CHECK (length(id) > 0),
    task_id TEXT NOT NULL REFERENCES tasks(id),
    path TEXT NOT NULL,
    branch TEXT NOT NULL,
    base_commit TEXT NOT NULL
);

CREATE TABLE approvals (
    id TEXT PRIMARY KEY CHECK (length(id) > 0),
    task_id TEXT NOT NULL REFERENCES tasks(id),
    action TEXT NOT NULL CHECK (json_valid(action)),
    status TEXT NOT NULL,
    granted_by TEXT NOT NULL
);

CREATE TABLE verification_runs (
    id TEXT PRIMARY KEY CHECK (length(id) > 0),
    task_id TEXT NOT NULL REFERENCES tasks(id),
    "commit" TEXT NOT NULL,
    status TEXT NOT NULL,
    log_path TEXT
);

CREATE TABLE artifacts (
    id TEXT PRIMARY KEY CHECK (length(id) > 0),
    task_id TEXT NOT NULL REFERENCES tasks(id),
    type TEXT NOT NULL,
    path TEXT NOT NULL,
    checksum TEXT NOT NULL
);

CREATE TABLE events (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    kind TEXT NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX events_by_task_sequence ON events(task_id, seq);
