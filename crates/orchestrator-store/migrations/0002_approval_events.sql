-- Pending approvals have no decision actor. Keep their grantor column nullable.
-- Approval events stay separate from the task-only events table.
-- Updated: 2026-11-08T00:04:58-07:00.

CREATE TABLE approvals_new (
    id TEXT PRIMARY KEY CHECK (length(id) > 0),
    task_id TEXT NOT NULL REFERENCES tasks(id),
    action TEXT NOT NULL CHECK (json_valid(action)),
    status TEXT NOT NULL,
    granted_by TEXT
);

INSERT INTO approvals_new (id, task_id, action, status, granted_by)
SELECT id, task_id, action, status, granted_by FROM approvals;

DROP TABLE approvals;
ALTER TABLE approvals_new RENAME TO approvals;

CREATE TABLE approval_events (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    approval_id TEXT NOT NULL REFERENCES approvals(id),
    task_id TEXT NOT NULL REFERENCES tasks(id),
    kind TEXT NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX approval_events_by_id_sequence
    ON approval_events(approval_id, seq);
CREATE INDEX approval_events_by_task_sequence
    ON approval_events(task_id, seq);
