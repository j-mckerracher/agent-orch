-- DPES-1105 phase 4a: the text a finished scout (Investigate, ReportOnly) leaves behind.
--
-- The body is the agent's last message: untrusted agent output. The daemon redacts it before it
-- is stored. The store never puts it in a prompt or a snapshot; the user's relay does, once.
-- A report is written once (the key is the task), its text and time never change, and the only
-- column that moves is relayed_at, set once. A report is never deleted: this is not part of
-- conversation history, so clearing history leaves it alone.

CREATE TABLE task_reports (
    task_id TEXT PRIMARY KEY REFERENCES tasks(id),
    attempt_id TEXT NOT NULL CHECK (length(attempt_id) > 0),
    -- length() counts characters here; the store also caps the bytes at 16 KiB.
    body TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 16384),
    -- 1 when the text was cut to fit the cap.
    truncated INTEGER NOT NULL DEFAULT 0 CHECK (truncated IN (0, 1)),
    captured_at TEXT NOT NULL CHECK (length(captured_at) > 0),
    relayed_at TEXT CHECK (relayed_at IS NULL OR length(relayed_at) > 0)
);

CREATE TRIGGER task_reports_fixed
BEFORE UPDATE OF task_id, attempt_id, body, truncated, captured_at ON task_reports
BEGIN
    SELECT RAISE(ABORT, 'a task report never changes after capture');
END;

CREATE TRIGGER task_reports_relayed_once
BEFORE UPDATE OF relayed_at ON task_reports
WHEN OLD.relayed_at IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'a task report is relayed once');
END;

CREATE TRIGGER task_reports_no_delete
BEFORE DELETE ON task_reports
BEGIN
    SELECT RAISE(ABORT, 'task reports are never deleted');
END;

-- Set on the turn the user's relay handed a report to, so a proposal made in it can say so.
ALTER TABLE conversation_turns ADD COLUMN read_report INTEGER NOT NULL DEFAULT 0
    CHECK (read_report IN (0, 1));
