-- DPES-1108 phase 3: user decisions. A question for the user is a decision hold of the new
-- subject kind 'question', bound to a task. Its answer is an append-only record holding the
-- user's exact words. The action kind decision.ask raises one from a chat proposal.
--
-- SQLite cannot alter a CHECK, so decision_holds (0045) and action_proposals (0042, 0047) are
-- rebuilt the way 0042 did it: every row keeps its rowid, and nothing has a foreign key to
-- either table. The 0045 insert trigger on decision_holds reads action_proposals, and SQLite
-- checks every trigger when a table is renamed, so that trigger is dropped first; it is
-- created again, extended, with the new decision_holds below.

DROP TRIGGER decision_holds_open_decision;

-- action_proposals: the rules of 0042 and the grant_tried column of 0047, with decision.ask.
CREATE TABLE action_proposals_new (
    id TEXT PRIMARY KEY CHECK (length(id) > 0),
    scope TEXT NOT NULL REFERENCES conversations(id),
    turn_id TEXT NOT NULL REFERENCES conversation_turns(id),
    idx INTEGER NOT NULL CHECK (idx BETWEEN 0 AND 9),
    kind TEXT NOT NULL CHECK (kind IN
        ('workstream.create', 'workstream.update', 'objective.add', 'objective.revise',
         'task.propose', 'task.cancel', 'task.retry', 'workstream.link_task',
         'task.revise', 'delivery.request', 'delivery.merge', 'project.register',
         'project.remove', 'decision.hold', 'decision.ask')),
    params_json TEXT NOT NULL CHECK (json_valid(params_json) AND length(params_json) <= 20480),
    digest TEXT NOT NULL CHECK (length(digest) = 64 AND digest NOT GLOB '*[^0-9a-f]*'),
    subject TEXT NOT NULL CHECK (length(subject) BETWEEN 1 AND 512),
    depends_on TEXT CHECK (depends_on IS NULL OR length(depends_on) > 0),
    state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN
        ('pending', 'approved', 'executed', 'failed', 'rejected', 'superseded', 'expired')),
    result_ref TEXT CHECK (result_ref IS NULL OR length(result_ref) BETWEEN 1 AND 256),
    error_code TEXT CHECK (error_code IS NULL OR length(error_code) BETWEEN 1 AND 64),
    denial TEXT CHECK (denial IS NULL OR length(denial) BETWEEN 1 AND 64),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    grant_tried INTEGER NOT NULL DEFAULT 0 CHECK (grant_tried IN (0, 1)),
    UNIQUE (turn_id, idx),
    CHECK ((state = 'failed') = (error_code IS NOT NULL)),
    CHECK ((denial IS NOT NULL) = (error_code IS NOT NULL AND error_code = 'action_denied'))
);

INSERT INTO action_proposals_new
    (rowid, id, scope, turn_id, idx, kind, params_json, digest, subject, depends_on, state,
     result_ref, error_code, denial, created_at, updated_at, grant_tried)
SELECT rowid, id, scope, turn_id, idx, kind, params_json, digest, subject, depends_on, state,
       result_ref, error_code, denial, created_at, updated_at, grant_tried
FROM action_proposals;

DROP TABLE action_proposals;
ALTER TABLE action_proposals_new RENAME TO action_proposals;

CREATE INDEX action_proposals_by_scope ON action_proposals(scope, created_at);
CREATE UNIQUE INDEX action_proposals_one_pending_digest
    ON action_proposals(scope, kind, digest) WHERE state = 'pending';
CREATE INDEX action_proposals_pending_subject
    ON action_proposals(scope, kind, subject) WHERE state = 'pending';
CREATE INDEX action_proposals_grant_untried ON action_proposals(created_at)
    WHERE state = 'pending' AND grant_tried = 0;

CREATE TRIGGER action_proposals_gated_delete
BEFORE DELETE ON action_proposals
WHEN NOT EXISTS (SELECT 1 FROM conversation_clear_gate WHERE id = 1 AND open = 1)
BEGIN
    SELECT RAISE(ABORT, 'action proposals are deleted only by a history clear');
END;

CREATE TRIGGER action_proposals_fixed_parameters
BEFORE UPDATE OF id, scope, turn_id, idx, kind, params_json, digest, subject, depends_on, created_at
ON action_proposals
BEGIN
    SELECT RAISE(ABORT, 'action proposal parameters never change');
END;

CREATE TRIGGER action_proposals_denial_once
BEFORE UPDATE OF denial ON action_proposals
WHEN OLD.denial IS NOT NULL OR OLD.state <> 'pending'
BEGIN
    SELECT RAISE(ABORT, 'a proposal denial is written once, on a pending proposal');
END;

CREATE TRIGGER action_proposals_state_transitions
BEFORE UPDATE OF state ON action_proposals
WHEN NOT (
    (OLD.state = 'pending' AND NEW.state IN ('approved', 'rejected', 'superseded', 'expired', 'failed'))
    OR (OLD.state = 'approved' AND NEW.state IN ('executed', 'failed'))
)
BEGIN
    SELECT RAISE(ABORT, 'illegal action proposal state transition');
END;

-- decision_holds: the rules of 0045, plus questions. A question row has no date of its own
-- (a deferral is a record below), names who raised it in set_by, and carries its text, its
-- options (a JSON array of strings, possibly empty) and an optional key. A hold row stays as
-- 0045 made it: a date, set by the user, no question columns. 'active' is an open question,
-- 'released' a closed one.
CREATE TABLE decision_holds_new (
    id TEXT PRIMARY KEY CHECK (length(id) BETWEEN 1 AND 128),
    subject_kind TEXT NOT NULL CHECK (subject_kind IN ('approval', 'proposal', 'question')),
    subject_id TEXT NOT NULL CHECK (length(subject_id) BETWEEN 1 AND 160),
    until_date TEXT
        CHECK (until_date IS NULL OR until_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    until_ms INTEGER CHECK (until_ms IS NULL
        OR (until_ms BETWEEN 1 AND 4102444800000 AND until_ms % 86400000 = 86399999)),
    set_by TEXT NOT NULL CHECK (set_by IN ('user', 'orchestrator', 'worker')),
    state TEXT NOT NULL DEFAULT 'active' CHECK (state IN ('active', 'released')),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    released_at TEXT,
    question TEXT CHECK (question IS NULL OR length(question) BETWEEN 1 AND 1024),
    options_json TEXT CHECK (options_json IS NULL OR (json_valid(options_json)
        AND json_type(options_json) = 'array' AND json_array_length(options_json) <= 6
        AND length(options_json) <= 2048)),
    question_key TEXT CHECK (question_key IS NULL OR (length(question_key) BETWEEN 1 AND 64
        AND question_key NOT GLOB '*[^a-z0-9-]*')),
    CHECK ((state = 'released') = (released_at IS NOT NULL)),
    CHECK ((until_date IS NULL) = (until_ms IS NULL)),
    CHECK (CASE WHEN subject_kind = 'question'
        THEN question IS NOT NULL AND options_json IS NOT NULL AND until_date IS NULL
             AND set_by IN ('orchestrator', 'worker')
        ELSE question IS NULL AND options_json IS NULL AND question_key IS NULL
             AND until_date IS NOT NULL AND set_by = 'user'
        END)
);

INSERT INTO decision_holds_new
    (rowid, id, subject_kind, subject_id, until_date, until_ms, set_by, state, created_at,
     released_at)
SELECT rowid, id, subject_kind, subject_id, until_date, until_ms, set_by, state, created_at,
       released_at
FROM decision_holds;

DROP TABLE decision_holds;
ALTER TABLE decision_holds_new RENAME TO decision_holds;

-- One active hold per subject. Questions are left out: a task may have several open.
CREATE UNIQUE INDEX decision_holds_one_active
    ON decision_holds(subject_kind, subject_id)
    WHERE state = 'active' AND subject_kind <> 'question';
-- One open question per task and key.
CREATE UNIQUE INDEX decision_holds_one_open_key
    ON decision_holds(subject_id, question_key)
    WHERE subject_kind = 'question' AND state = 'active' AND question_key IS NOT NULL;
-- The scheduler reads the tasks with an open question on every pass.
CREATE INDEX decision_holds_open_questions
    ON decision_holds(subject_id) WHERE subject_kind = 'question' AND state = 'active';

-- A hold needs an open decision, as in 0045; a question needs its task.
CREATE TRIGGER decision_holds_open_decision BEFORE INSERT ON decision_holds
WHEN (NEW.subject_kind = 'approval' AND NOT EXISTS (
        SELECT 1 FROM approvals WHERE task_id = NEW.subject_id AND status = 'requested'))
  OR (NEW.subject_kind = 'proposal' AND NOT EXISTS (
        SELECT 1 FROM action_proposals WHERE id = NEW.subject_id AND state = 'pending'))
  OR (NEW.subject_kind = 'question' AND NOT EXISTS (
        SELECT 1 FROM tasks WHERE id = NEW.subject_id))
BEGIN SELECT RAISE(ABORT, 'a hold needs an open decision'); END;

-- A question is raised open.
CREATE TRIGGER decision_holds_question_raised_open BEFORE INSERT ON decision_holds
WHEN NEW.subject_kind = 'question' AND NEW.state <> 'active'
BEGIN SELECT RAISE(ABORT, 'a question is raised open'); END;

CREATE TRIGGER decision_holds_fixed
BEFORE UPDATE OF id, subject_kind, subject_id, until_date, until_ms, set_by, created_at,
    question, options_json, question_key
ON decision_holds
BEGIN SELECT RAISE(ABORT, 'a decision hold changes only by release'); END;

CREATE TRIGGER decision_holds_released_stays_released BEFORE UPDATE ON decision_holds
WHEN OLD.state = 'released'
BEGIN SELECT RAISE(ABORT, 'a released hold stays released'); END;

CREATE TRIGGER decision_holds_no_delete BEFORE DELETE ON decision_holds
BEGIN SELECT RAISE(ABORT, 'decision holds are released, not deleted'); END;

-- What the user did with a question, append-only. 'answer' holds the user's exact words and
-- 'reconcile' the evidence that the question no longer matters; either closes the question.
-- 'defer' holds the end of the named UTC day, as a hold does, and leaves the question open:
-- the newest deferral whose day has not passed hides it from the list until then.
CREATE TABLE decision_answers (
    id INTEGER PRIMARY KEY,
    hold_id TEXT NOT NULL REFERENCES decision_holds(id),
    kind TEXT NOT NULL CHECK (kind IN ('answer', 'defer', 'reconcile')),
    text TEXT CHECK (text IS NULL OR length(text) BETWEEN 1 AND 4096),
    until_date TEXT
        CHECK (until_date IS NULL OR until_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    until_ms INTEGER CHECK (until_ms IS NULL
        OR (until_ms BETWEEN 1 AND 4102444800000 AND until_ms % 86400000 = 86399999)),
    recorded_by TEXT NOT NULL CHECK (recorded_by IN ('user')),
    recorded_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK ((until_date IS NULL) = (until_ms IS NULL)),
    CHECK (CASE WHEN kind = 'defer' THEN until_date IS NOT NULL AND text IS NULL
        ELSE text IS NOT NULL AND until_date IS NULL END)
);

CREATE INDEX decision_answers_by_hold ON decision_answers(hold_id, id);

-- Only an open question takes a record.
CREATE TRIGGER decision_answers_open_question BEFORE INSERT ON decision_answers
WHEN NOT EXISTS (SELECT 1 FROM decision_holds
    WHERE id = NEW.hold_id AND subject_kind = 'question' AND state = 'active')
BEGIN SELECT RAISE(ABORT, 'a decision record needs an open question'); END;

CREATE TRIGGER decision_answers_append_only BEFORE UPDATE ON decision_answers
BEGIN SELECT RAISE(ABORT, 'a decision record never changes'); END;

CREATE TRIGGER decision_answers_no_delete BEFORE DELETE ON decision_answers
BEGIN SELECT RAISE(ABORT, 'decision records are never deleted'); END;

-- A question closes only with an answer or a reconcile on record. Nothing else releases it:
-- not a deferral, not the task finishing, not a history clear.
CREATE TRIGGER decision_holds_question_closes_by_record BEFORE UPDATE OF state ON decision_holds
WHEN NEW.subject_kind = 'question' AND NEW.state = 'released' AND NOT EXISTS (
    SELECT 1 FROM decision_answers WHERE hold_id = NEW.id AND kind IN ('answer', 'reconcile'))
BEGIN SELECT RAISE(ABORT, 'a question closes only by an answer or a reconcile'); END;
