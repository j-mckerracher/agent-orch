-- Delivery outcomes and audit events are stored separately from task events.
-- Updated: 2026-11-08T09:15:26Z.
CREATE TABLE delivery_steps (
    id TEXT PRIMARY KEY CHECK (length(id) > 0),
    task_id TEXT NOT NULL REFERENCES tasks(id),
    action_kind TEXT NOT NULL CHECK (action_kind IN ('report_only', 'local_review')),
    action TEXT NOT NULL CHECK (json_valid(action)),
    parameter_digest TEXT NOT NULL CHECK (
        length(parameter_digest) = 64
        AND parameter_digest NOT GLOB '*[^0-9a-f]*'
    ),
    candidate_sha TEXT NOT NULL CHECK (
        (length(candidate_sha) = 40 OR length(candidate_sha) = 64)
        AND candidate_sha NOT GLOB '*[^0-9a-f]*'
    ),
    approval_id TEXT NOT NULL CHECK (length(approval_id) > 0),
    verification_run_id TEXT NOT NULL CHECK (length(verification_run_id) > 0),
    destination TEXT NOT NULL CHECK (length(destination) > 0),
    target_ref TEXT,
    outcome TEXT NOT NULL CHECK (
        outcome IN ('reported', 'review_ref_created', 'review_ref_already_matches', 'refused')
    ),
    detail TEXT,
    diff_summary TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (
        outcome = 'refused'
        OR (action_kind = 'report_only' AND outcome = 'reported')
        OR (
            action_kind = 'local_review'
            AND outcome IN ('review_ref_created', 'review_ref_already_matches')
        )
    )
);

CREATE INDEX delivery_steps_by_task_created
    ON delivery_steps(task_id, created_at, id);

CREATE TABLE delivery_step_events (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    delivery_step_id TEXT NOT NULL UNIQUE REFERENCES delivery_steps(id),
    task_id TEXT NOT NULL REFERENCES tasks(id),
    kind TEXT NOT NULL CHECK (
        kind IN ('reported', 'review_ref_created', 'review_ref_already_matches', 'refused')
    ),
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX delivery_step_events_by_task_sequence
    ON delivery_step_events(task_id, seq);
