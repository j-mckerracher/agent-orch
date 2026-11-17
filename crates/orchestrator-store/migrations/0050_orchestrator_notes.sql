-- DPES-1105 phase 5b: the user's own orchestrator notes.
-- Data that the snapshot lists, never instructions. Written only by notes.set (user-only).
-- scope is 'global' or 'workstream:' plus a workstream id.
CREATE TABLE orchestrator_notes (
    scope TEXT NOT NULL CHECK (scope = 'global' OR scope GLOB 'workstream:?*'),
    position INTEGER NOT NULL CHECK (position BETWEEN 0 AND 9),
    body TEXT NOT NULL CHECK (length(CAST(body AS BLOB)) BETWEEN 1 AND 200),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (scope, position)
);
