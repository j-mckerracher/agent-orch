-- Updated: 2026-11-08T16:58:08+00:00.
-- The journal assigns one durable sequence across task, session, approval, and delivery events.
CREATE TABLE durable_events (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    session_id TEXT,
    kind TEXT NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX durable_events_by_task_sequence
    ON durable_events(task_id, sequence);
CREATE INDEX durable_events_by_session_sequence
    ON durable_events(task_id, session_id, sequence);

INSERT INTO durable_events (task_id, kind, payload, created_at)
SELECT task_id, 'task.' || kind, payload, created_at
FROM events
ORDER BY seq;

INSERT INTO durable_events (task_id, session_id, kind, payload)
SELECT task_id, id, 'session.snapshot', json_object('state', json(state))
FROM sessions
ORDER BY sequence;

INSERT INTO durable_events (task_id, kind, payload)
SELECT task_id, 'approval.' || kind,
       json_object('approval_id', approval_id, 'event', json(payload))
FROM approval_events
ORDER BY seq;

INSERT INTO durable_events (task_id, kind, payload)
SELECT task_id, 'delivery.' || kind,
       json_object(
           'step_id', delivery_step_id,
           'outcome', json_extract(payload, '$.outcome')
       )
FROM delivery_step_events
ORDER BY seq;

CREATE TABLE idempotency_records (
    key TEXT PRIMARY KEY CHECK (length(key) BETWEEN 1 AND 128),
    method TEXT NOT NULL,
    fingerprint TEXT NOT NULL CHECK (
        length(fingerprint) = 64 AND fingerprint NOT GLOB '*[^0-9a-f]*'
    ),
    result TEXT CHECK (result IS NULL OR json_valid(result)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
