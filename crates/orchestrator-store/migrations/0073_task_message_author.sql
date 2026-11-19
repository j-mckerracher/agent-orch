-- DPES-1106: who wrote a task message, so the worker is told whether the user or the orchestrator
-- wrote it. Before this migration every 'message' row was the orchestrator's (task.message), and
-- every 'answer' and 'reconcile' row was the user's (decisions.answer, decisions.reconcile). The
-- author is fixed with the row, so the delivered-once trigger refuses a change to it.
ALTER TABLE task_messages ADD COLUMN author TEXT NOT NULL DEFAULT 'orchestrator'
    CHECK (author IN ('orchestrator', 'user'));

DROP TRIGGER task_messages_delivered_once;

-- The default above is the orchestrator's, so set the user's for the rows that are the user's.
-- The old trigger would refuse this on a delivered row, which is why it is dropped first.
UPDATE task_messages SET author = 'user' WHERE kind IN ('answer', 'reconcile');

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
