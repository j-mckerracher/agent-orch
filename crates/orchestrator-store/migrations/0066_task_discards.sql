-- Unlanded-work guard (DPES-1108 phase 5, plan 5.5).
--
-- A task whose run committed a candidate may not be archived, and its treehouse slot is not
-- returned, until that candidate is merged, delivered as a pull request, delivered to a local
-- review ref, or the user discards it. A row here is the user's discard. It deletes nothing:
-- the task's branch stays in the repository. The row is written only by `tasks.discard`, a
-- user-only method, and never changes.

CREATE TABLE task_discards (
    task_id TEXT PRIMARY KEY REFERENCES tasks(id),
    discarded_by TEXT NOT NULL CHECK (discarded_by = 'user'),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TRIGGER task_discards_no_update BEFORE UPDATE ON task_discards
BEGIN SELECT RAISE(ABORT, 'a task discard never changes'); END;

CREATE TRIGGER task_discards_no_delete BEFORE DELETE ON task_discards
BEGIN SELECT RAISE(ABORT, 'a task discard is never deleted'); END;
