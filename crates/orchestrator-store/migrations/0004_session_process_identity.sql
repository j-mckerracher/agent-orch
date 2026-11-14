-- Launch-time OS identity is separate from the core session lifecycle.
-- Updated: 2026-11-08T08:12:54+00:00.
CREATE TABLE session_process_identities (
    task_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    pid INTEGER NOT NULL CHECK (pid > 0),
    process_group INTEGER CHECK (process_group IS NULL OR process_group > 0),
    start_time_micros INTEGER CHECK (
        start_time_micros IS NULL OR start_time_micros >= 0
    ),
    platform TEXT NOT NULL CHECK (platform IN ('macos', 'unix', 'windows', 'other')),
    PRIMARY KEY (task_id, session_id),
    FOREIGN KEY (task_id, session_id)
        REFERENCES sessions(task_id, id)
        ON DELETE CASCADE
);
