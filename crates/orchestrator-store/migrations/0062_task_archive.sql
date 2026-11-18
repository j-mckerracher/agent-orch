-- Soft archive of finished tasks (DPES-1106). `tasks.archive` sets `archived_at` on a task that
-- is Succeeded, Failed, VerificationFailed or Cancelled. The row, its attempts, sessions and
-- events stay; the daemon only leaves an archived task out of `tasks.list` and workstream views.
-- NULL means not archived, so every existing task stays visible.

ALTER TABLE tasks ADD COLUMN archived_at TEXT;
