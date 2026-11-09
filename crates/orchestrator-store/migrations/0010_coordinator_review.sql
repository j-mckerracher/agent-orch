-- M5-02c: coordinator conversation, readable approval parameters, change requests.
-- Version 0010 leaves 0007-0009 free for concurrently developed migrations.

-- Project-scoped coordinator messages and proposals. A proposal row carries the
-- structured validation outcome; it never creates tasks, approvals, or policy.
CREATE TABLE coordinator_entries (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE CHECK (length(id) > 0),
    project_id TEXT NOT NULL REFERENCES projects(id),
    kind TEXT NOT NULL CHECK (kind IN ('message', 'proposal')),
    author TEXT NOT NULL CHECK (author IN ('user', 'coordinator')),
    body TEXT NOT NULL,
    validation TEXT CHECK (validation IS NULL OR json_valid(validation)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK ((kind = 'proposal') = (validation IS NOT NULL))
);

CREATE INDEX coordinator_entries_by_project_sequence
    ON coordinator_entries(project_id, sequence);

-- The approval row stores only a parameter digest. These readable parameters
-- are written in the request transaction so a reviewer sees what the digest covers.
CREATE TABLE approval_parameters (
    approval_id TEXT PRIMARY KEY REFERENCES approvals(id),
    parameters TEXT NOT NULL CHECK (json_valid(parameters))
);

-- Feedback bound to one approval and its candidate. It is not a decision.
CREATE TABLE change_requests (
    id TEXT PRIMARY KEY CHECK (length(id) > 0),
    task_id TEXT NOT NULL REFERENCES tasks(id),
    approval_id TEXT NOT NULL REFERENCES approvals(id),
    candidate_commit TEXT NOT NULL,
    feedback TEXT NOT NULL CHECK (length(feedback) > 0),
    requested_by TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX change_requests_one_per_approval
    ON change_requests(approval_id);
CREATE INDEX change_requests_by_task ON change_requests(task_id);
