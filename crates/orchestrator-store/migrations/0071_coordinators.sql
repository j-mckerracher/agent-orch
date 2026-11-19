-- DPES-1108 phase 7: project coordinators as domain orchestrators.
--
-- A coordinator is the conversation of one project (coordinator_scopes, 0032 and 0053). This
-- migration gives it what it needs to own work and to answer to the global orchestrator:
--
-- - coordinator_scopes.scope_sentence: the one line its charter is rendered from, and what the
--   global orchestrator routes by.
-- - routed_requests: work the global orchestrator routed to it (coordinator.route). The id is
--   the correlation id. A request is open until a coordinator report with verb 'answer' names
--   it; nothing else closes it, and requests are never changed or deleted.
-- - tasks.coordinator_project_id and tasks.routed_request_id: the coordinator that owns a task,
--   and the routed request it serves. Written once. Wakes for an owned task go to the
--   coordinator, not the global orchestrator.
-- - coordinator_reports: the parent channel. The daemon writes the durable facts (a task ready,
--   done or failed, a question raised or answered, a merge); the coordinator writes only its
--   judgment (verb answer, caveat or blocked). The global orchestrator reads these, never the
--   coordinator's chat.
--
-- No table here has a trigger or foreign key on action_proposals or wake_records, so a later
-- rebuild of either needs nothing from this file. The origin of a request or a report (the
-- proposal that made it) is checked in the store transaction that writes it.

ALTER TABLE coordinator_scopes ADD COLUMN scope_sentence TEXT
    CHECK (scope_sentence IS NULL OR (length(scope_sentence) BETWEEN 1 AND 300
        AND scope_sentence NOT GLOB ('*[' || char(1) || '-' || char(31) || char(127) || ']*')));

CREATE TABLE routed_requests (
    id TEXT PRIMARY KEY CHECK (length(id) BETWEEN 3 AND 64 AND id GLOB 'r-*'
        AND id NOT GLOB '*[^0-9a-z-]*'),
    project_id TEXT NOT NULL REFERENCES projects(id),
    work TEXT NOT NULL CHECK (length(work) BETWEEN 1 AND 4096),
    -- The executed coordinator.route proposal of the global conversation that made it.
    origin_proposal_id TEXT NOT NULL UNIQUE CHECK (length(origin_proposal_id) BETWEEN 1 AND 128),
    -- Set once the coordinator's wake record for it is written.
    woken INTEGER NOT NULL DEFAULT 0 CHECK (woken IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX routed_requests_by_project ON routed_requests(project_id, created_at);
CREATE INDEX routed_requests_unwoken ON routed_requests(created_at) WHERE woken = 0;

-- Work is routed only to a coordinator that exists now.
CREATE TRIGGER routed_requests_need_a_coordinator BEFORE INSERT ON routed_requests
WHEN NOT EXISTS (
    SELECT 1 FROM coordinator_scopes WHERE project_id = NEW.project_id AND removed = 0)
BEGIN SELECT RAISE(ABORT, 'work is routed only to an active coordinator'); END;

CREATE TRIGGER routed_requests_only_woken BEFORE UPDATE ON routed_requests
WHEN NOT (OLD.woken = 0 AND NEW.woken = 1)
    OR NEW.id IS NOT OLD.id OR NEW.project_id IS NOT OLD.project_id
    OR NEW.work IS NOT OLD.work OR NEW.origin_proposal_id IS NOT OLD.origin_proposal_id
    OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'a routed request only becomes woken, once'); END;

CREATE TRIGGER routed_requests_no_delete BEFORE DELETE ON routed_requests
BEGIN SELECT RAISE(ABORT, 'routed requests are never deleted'); END;

CREATE TABLE coordinator_reports (
    id TEXT PRIMARY KEY CHECK (length(id) BETWEEN 1 AND 128),
    project_id TEXT NOT NULL REFERENCES projects(id),
    author TEXT NOT NULL CHECK (author IN ('daemon', 'coordinator')),
    verb TEXT NOT NULL CHECK (verb IN
        ('task_ready', 'report_ready', 'task_done', 'task_failed', 'worker_blocked',
         'question_raised', 'question_answered', 'merged', 'answer', 'caveat', 'blocked')),
    request_id TEXT REFERENCES routed_requests(id),
    task_id TEXT REFERENCES tasks(id),
    report_key TEXT CHECK (report_key IS NULL OR (length(report_key) BETWEEN 1 AND 64
        AND report_key NOT GLOB '*[^a-z0-9-]*')),
    -- Coordinator text, already checked and redacted. Daemon facts carry no note: the daemon
    -- renders them from the verb and the task.
    note TEXT CHECK (note IS NULL OR length(note) BETWEEN 1 AND 1024),
    -- The executed coordinator.report proposal that made a coordinator row.
    origin_proposal_id TEXT UNIQUE CHECK (origin_proposal_id IS NULL
        OR length(origin_proposal_id) BETWEEN 1 AND 128),
    -- Set once the global orchestrator's wake record for it is written.
    woken INTEGER NOT NULL DEFAULT 0 CHECK (woken IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK ((author = 'coordinator') = (verb IN ('answer', 'caveat', 'blocked'))),
    CHECK ((author = 'coordinator') = (origin_proposal_id IS NOT NULL)),
    CHECK ((author = 'coordinator') = (note IS NOT NULL)),
    CHECK (verb <> 'answer' OR request_id IS NOT NULL)
);

-- A routed request has one answer. The answer is what closes it.
CREATE UNIQUE INDEX coordinator_reports_one_answer
    ON coordinator_reports(request_id) WHERE verb = 'answer';
CREATE INDEX coordinator_reports_by_project ON coordinator_reports(project_id, created_at);
CREATE INDEX coordinator_reports_unwoken ON coordinator_reports(created_at) WHERE woken = 0;

CREATE TRIGGER coordinator_reports_same_project BEFORE INSERT ON coordinator_reports
WHEN (NEW.request_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM routed_requests WHERE id = NEW.request_id AND project_id = NEW.project_id))
  OR (NEW.task_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM tasks WHERE id = NEW.task_id AND project_id = NEW.project_id))
BEGIN SELECT RAISE(ABORT, 'a coordinator report names only its own project'); END;

CREATE TRIGGER coordinator_reports_only_woken BEFORE UPDATE ON coordinator_reports
WHEN NOT (OLD.woken = 0 AND NEW.woken = 1)
    OR NEW.id IS NOT OLD.id OR NEW.project_id IS NOT OLD.project_id
    OR NEW.author IS NOT OLD.author OR NEW.verb IS NOT OLD.verb
    OR NEW.request_id IS NOT OLD.request_id OR NEW.task_id IS NOT OLD.task_id
    OR NEW.report_key IS NOT OLD.report_key OR NEW.note IS NOT OLD.note
    OR NEW.origin_proposal_id IS NOT OLD.origin_proposal_id
    OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'a coordinator report only becomes woken, once'); END;

CREATE TRIGGER coordinator_reports_no_delete BEFORE DELETE ON coordinator_reports
BEGIN SELECT RAISE(ABORT, 'coordinator reports are never deleted'); END;

ALTER TABLE tasks ADD COLUMN coordinator_project_id TEXT REFERENCES projects(id);
ALTER TABLE tasks ADD COLUMN routed_request_id TEXT REFERENCES routed_requests(id);

CREATE INDEX tasks_by_coordinator ON tasks(coordinator_project_id)
    WHERE coordinator_project_id IS NOT NULL;

-- Ownership is written once, only by the task's own project's coordinator, and a request link
-- needs the owner and an open request of the same project.
CREATE TRIGGER tasks_coordinator_owner_once
BEFORE UPDATE OF coordinator_project_id, routed_request_id ON tasks
WHEN (OLD.coordinator_project_id IS NOT NULL
        AND NEW.coordinator_project_id IS NOT OLD.coordinator_project_id)
  OR (OLD.routed_request_id IS NOT NULL AND NEW.routed_request_id IS NOT OLD.routed_request_id)
  OR (NEW.coordinator_project_id IS NOT NULL AND NEW.coordinator_project_id <> NEW.project_id)
  OR (NEW.routed_request_id IS NOT NULL AND OLD.routed_request_id IS NULL AND (
        NEW.coordinator_project_id IS NULL
        OR NOT EXISTS (SELECT 1 FROM routed_requests r
                       WHERE r.id = NEW.routed_request_id
                         AND r.project_id = NEW.coordinator_project_id)
        OR EXISTS (SELECT 1 FROM coordinator_reports c
                   WHERE c.request_id = NEW.routed_request_id AND c.verb = 'answer')))
BEGIN SELECT RAISE(ABORT, 'a task owner is its own coordinator, written once'); END;

-- Retiring a coordinator is refused while it owns unfinished work or a routed request it has not
-- answered. Failed tasks do not count: they run nothing and only a retry restarts them.
CREATE TRIGGER coordinator_scopes_retire_only_idle
BEFORE UPDATE OF removed ON coordinator_scopes
WHEN NEW.removed = 1 AND OLD.removed = 0 AND (
    EXISTS (SELECT 1 FROM tasks
            WHERE coordinator_project_id = NEW.project_id
              AND state IN ('"Created"', '"Queued"', '"Running"', '"Waiting"', '"Verifying"'))
    OR EXISTS (SELECT 1 FROM routed_requests r
               WHERE r.project_id = NEW.project_id
                 AND NOT EXISTS (SELECT 1 FROM coordinator_reports c
                                 WHERE c.request_id = r.id AND c.verb = 'answer')))
BEGIN SELECT RAISE(ABORT, 'a coordinator with open work cannot be removed'); END;
