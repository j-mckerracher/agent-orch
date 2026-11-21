-- A user's message to a finished task (reopen) or to one that has not started (Created, Queued).
--
-- A finished task takes the message under a new attempt that continues the agent session of the
-- attempt it names here. An attempt with no such link is an ordinary one: the retry bound counts
-- only those, so a follow-up never uses up a task's retries or repairs.
ALTER TABLE attempts ADD COLUMN continues_attempt_id TEXT;

-- 1 when the attempt it continues ran in a slot that has since been returned to the pool
-- (the task was archived or its pull request merged). The resume then leases a new slot.
ALTER TABLE attempts ADD COLUMN fresh_workspace INTEGER NOT NULL DEFAULT 0
    CHECK (fresh_workspace IN (0, 1));

-- A task that has not started has no attempt, so the user's message for it has none either. The
-- launch delivers it with the brief. Rebuilt because SQLite cannot relax NOT NULL in place.
CREATE TABLE task_messages_new (
    id INTEGER PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    attempt_id TEXT,
    kind TEXT NOT NULL CHECK (kind IN ('answer', 'reconcile', 'message')),
    question_id TEXT REFERENCES decision_holds(id),
    text TEXT CHECK (text IS NULL OR length(text) BETWEEN 1 AND 4096),
    message_key TEXT UNIQUE CHECK (message_key IS NULL OR length(message_key) BETWEEN 1 AND 160),
    delivered_session_id TEXT CHECK (delivered_session_id IS NULL
        OR length(delivered_session_id) > 0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    author TEXT NOT NULL DEFAULT 'orchestrator' CHECK (author IN ('orchestrator', 'user')),
    CHECK (CASE WHEN kind = 'message'
        THEN text IS NOT NULL AND question_id IS NULL AND message_key IS NOT NULL
        ELSE text IS NULL AND question_id IS NOT NULL AND message_key IS NULL END),
    CHECK (attempt_id IS NOT NULL OR (kind = 'message' AND author = 'user')),
    FOREIGN KEY (task_id, attempt_id) REFERENCES attempts(task_id, id)
);

INSERT INTO task_messages_new
    (id, task_id, attempt_id, kind, question_id, text, message_key, delivered_session_id,
     created_at, author)
SELECT id, task_id, attempt_id, kind, question_id, text, message_key, delivered_session_id,
       created_at, author
FROM task_messages;

DROP TABLE task_messages;
ALTER TABLE task_messages_new RENAME TO task_messages;

CREATE INDEX task_messages_undelivered
    ON task_messages(task_id) WHERE delivered_session_id IS NULL;

CREATE TRIGGER task_messages_delivered_once
BEFORE UPDATE ON task_messages
WHEN OLD.delivered_session_id IS NOT NULL
    OR NEW.delivered_session_id IS NULL
    OR NEW.id IS NOT OLD.id OR NEW.task_id IS NOT OLD.task_id
    OR NEW.attempt_id IS NOT OLD.attempt_id OR NEW.kind IS NOT OLD.kind
    OR NEW.question_id IS NOT OLD.question_id OR NEW.text IS NOT OLD.text
    OR NEW.message_key IS NOT OLD.message_key OR NEW.created_at IS NOT OLD.created_at
    OR NEW.author IS NOT OLD.author
BEGIN
    SELECT RAISE(ABORT, 'a task message is only marked delivered, once');
END;

CREATE TRIGGER task_messages_no_delete BEFORE DELETE ON task_messages
BEGIN SELECT RAISE(ABORT, 'task messages are never deleted'); END;
