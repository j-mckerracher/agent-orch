-- DPES-1105 phase 4d: task lineage (revise, promote) and held decisions.
--
-- The soft archive for project.remove needs no schema of its own: `projects.archived_at` was
-- added by 0043. The store functions in sqlite_archive.rs set and clear it.

-- A new task links to the task it came from. One parent per task (the primary key), written
-- once, never changed, never deleted. The old task is never deleted either: this foreign key
-- refuses it. The store refuses a link that would close a cycle.
CREATE TABLE task_lineage (
    task_id TEXT PRIMARY KEY REFERENCES tasks(id),
    parent_task_id TEXT NOT NULL REFERENCES tasks(id),
    relation TEXT NOT NULL CHECK (relation IN ('revised_from', 'promoted_from')),
    added_intent TEXT CHECK (added_intent IS NULL OR length(added_intent) <= 2048),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (task_id <> parent_task_id)
);

CREATE INDEX task_lineage_by_parent ON task_lineage(parent_task_id);

CREATE TRIGGER task_lineage_append_only BEFORE UPDATE ON task_lineage
BEGIN SELECT RAISE(ABORT, 'task lineage never changes'); END;

CREATE TRIGGER task_lineage_no_delete BEFORE DELETE ON task_lineage
BEGIN SELECT RAISE(ABORT, 'task lineage is never deleted'); END;

-- A held decision hides a subject's pending approvals until a date. 'approval' holds every
-- pending approval of the task named by subject_id; 'proposal' holds one action card.
-- until_ms is the end of the named UTC day, supplied by the caller; the store compares it
-- with the caller's clock and never reads SQL time for that. Bounded: after 1970 and before
-- 2100-01-01, and always the last millisecond of a day.
CREATE TABLE decision_holds (
    id TEXT PRIMARY KEY CHECK (length(id) BETWEEN 1 AND 128),
    subject_kind TEXT NOT NULL CHECK (subject_kind IN ('approval', 'proposal')),
    subject_id TEXT NOT NULL CHECK (length(subject_id) BETWEEN 1 AND 160),
    until_date TEXT NOT NULL CHECK (until_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    until_ms INTEGER NOT NULL
        CHECK (until_ms BETWEEN 1 AND 4102444800000 AND until_ms % 86400000 = 86399999),
    set_by TEXT NOT NULL CHECK (set_by IN ('user')),
    state TEXT NOT NULL DEFAULT 'active' CHECK (state IN ('active', 'released')),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    released_at TEXT,
    CHECK ((state = 'released') = (released_at IS NOT NULL))
);

-- One active hold per subject. Changing a date is a release and a new hold.
CREATE UNIQUE INDEX decision_holds_one_active
    ON decision_holds(subject_kind, subject_id) WHERE state = 'active';

-- A hold may be placed only on a decision that is still open: an approval that is requested
-- or a proposal that is pending. A settled one has nothing left to hide.
CREATE TRIGGER decision_holds_open_decision BEFORE INSERT ON decision_holds
WHEN (NEW.subject_kind = 'approval' AND NOT EXISTS (
        SELECT 1 FROM approvals WHERE task_id = NEW.subject_id AND status = 'requested'))
  OR (NEW.subject_kind = 'proposal' AND NOT EXISTS (
        SELECT 1 FROM action_proposals WHERE id = NEW.subject_id AND state = 'pending'))
BEGIN SELECT RAISE(ABORT, 'a hold needs an open decision'); END;

CREATE TRIGGER decision_holds_fixed
BEFORE UPDATE OF id, subject_kind, subject_id, until_date, until_ms, set_by, created_at
ON decision_holds
BEGIN SELECT RAISE(ABORT, 'a decision hold changes only by release'); END;

CREATE TRIGGER decision_holds_released_stays_released BEFORE UPDATE ON decision_holds
WHEN OLD.state = 'released'
BEGIN SELECT RAISE(ABORT, 'a released hold stays released'); END;

CREATE TRIGGER decision_holds_no_delete BEFORE DELETE ON decision_holds
BEGIN SELECT RAISE(ABORT, 'decision holds are released, not deleted'); END;
