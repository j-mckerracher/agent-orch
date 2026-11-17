-- DPES-1105 phase 4c: the pull requests the daemon watches, and a ledger of merge attempts.
--
-- Plan 7.2 reserved 0046 for this file; 0047 to 0050 were taken first, so it is 0051. The table
-- shapes are the plan's, with three differences: required_unknown (a required check whose state
-- could not be read, which never counts as passed), a larger checks_json cap (100 checks of
-- 80-character names do not fit in 8192), and the partial unique index on the ledger below.
--
-- No token, no URL with credentials, no diff. A title, a branch and a check name are remote
-- text: capped, and without control characters (1 to 31 and 127).

CREATE TABLE pull_requests (
    task_id TEXT PRIMARY KEY REFERENCES tasks(id),
    project_id TEXT NOT NULL REFERENCES projects(id),
    number INTEGER NOT NULL CHECK (number BETWEEN 1 AND 999999),
    branch TEXT NOT NULL
        CHECK (length(branch) BETWEEN 1 AND 255
            AND branch NOT GLOB ('*[' || char(1) || '-' || char(31) || char(127) || ']*')),
    base_branch TEXT NOT NULL
        CHECK (length(base_branch) BETWEEN 1 AND 255
            AND base_branch NOT GLOB ('*[' || char(1) || '-' || char(31) || char(127) || ']*')),
    state TEXT NOT NULL CHECK (state IN ('open', 'merged', 'closed')),
    head_sha TEXT NOT NULL CHECK (length(head_sha) = 40 AND head_sha NOT GLOB '*[^0-9a-f]*'),
    mergeable TEXT NOT NULL CHECK (mergeable IN ('yes', 'no', 'unknown')),
    required_total INTEGER NOT NULL CHECK (required_total BETWEEN 0 AND 100),
    required_passed INTEGER NOT NULL CHECK (required_passed BETWEEN 0 AND 100),
    required_failed INTEGER NOT NULL CHECK (required_failed BETWEEN 0 AND 100),
    required_unknown INTEGER NOT NULL CHECK (required_unknown BETWEEN 0 AND 100),
    -- [{"name": ..., "result": "passed" | "failed" | "pending" | "unknown"}], required checks only.
    checks_json TEXT NOT NULL CHECK (json_valid(checks_json) AND length(checks_json) <= 32768),
    title TEXT NOT NULL
        CHECK (length(title) <= 200
            AND title NOT GLOB ('*[' || char(1) || '-' || char(31) || char(127) || ']*')),
    observed_ms INTEGER NOT NULL CHECK (observed_ms > 0),
    changed_ms INTEGER NOT NULL CHECK (changed_ms > 0),
    poll_failures INTEGER NOT NULL DEFAULT 0 CHECK (poll_failures >= 0),
    UNIQUE (project_id, number),
    CHECK (required_passed + required_failed + required_unknown <= required_total)
);

-- The target of a row never changes: a task follows one pull request.
CREATE TRIGGER pull_requests_fixed
BEFORE UPDATE OF task_id, project_id, number, branch ON pull_requests
BEGIN
    SELECT RAISE(ABORT, 'a pull request row never changes its target');
END;

-- Merged and closed are final: the poller stops watching a pull request that left 'open'.
CREATE TRIGGER pull_requests_settled_is_final
BEFORE UPDATE ON pull_requests
WHEN OLD.state <> 'open'
BEGIN
    SELECT RAISE(ABORT, 'a merged or closed pull request is final');
END;

-- One row per merge ATTEMPT of a proposal. The proposal id is the key, so a proposal claims a
-- merge at most once, ever.
CREATE TABLE pull_request_merges (
    proposal_id TEXT PRIMARY KEY CHECK (length(proposal_id) BETWEEN 1 AND 128),
    task_id TEXT NOT NULL CHECK (length(task_id) > 0),
    number INTEGER NOT NULL CHECK (number BETWEEN 1 AND 999999),
    head_sha TEXT NOT NULL CHECK (length(head_sha) = 40 AND head_sha NOT GLOB '*[^0-9a-f]*'),
    method TEXT NOT NULL CHECK (method IN ('merge', 'squash', 'rebase')),
    state TEXT NOT NULL CHECK (state IN ('claimed', 'issued', 'merged', 'refused', 'unknown')),
    refusal TEXT CHECK (refusal IS NULL OR length(refusal) BETWEEN 1 AND 64),
    merge_sha TEXT CHECK (merge_sha IS NULL OR (length(merge_sha) = 40 AND merge_sha NOT GLOB '*[^0-9a-f]*')),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK ((state = 'refused') = (refusal IS NOT NULL)),
    CHECK (merge_sha IS NULL OR state = 'merged')
);

-- While one attempt on a pull request is open (claimed, issued or unknown) no other attempt
-- may be claimed for it: the first may have landed, and a second must not be sent blind.
CREATE UNIQUE INDEX pull_request_merges_one_open
    ON pull_request_merges(task_id, number)
    WHERE state IN ('claimed', 'issued', 'unknown');

-- A pull request merges once: at most one attempt per pull request ends 'merged'.
CREATE UNIQUE INDEX pull_request_merges_one_merged
    ON pull_request_merges(task_id, number)
    WHERE state = 'merged';

CREATE TRIGGER pull_request_merges_fixed
BEFORE UPDATE OF proposal_id, task_id, number, head_sha, method, created_at ON pull_request_merges
BEGIN
    SELECT RAISE(ABORT, 'a merge attempt never changes its target');
END;

-- claimed -> issued -> merged | unknown | refused; claimed -> refused;
-- unknown -> merged | refused, and only through a read of the remote.
CREATE TRIGGER pull_request_merges_transitions
BEFORE UPDATE OF state ON pull_request_merges
WHEN NOT ((OLD.state = 'claimed' AND NEW.state IN ('issued', 'refused'))
       OR (OLD.state = 'issued' AND NEW.state IN ('merged', 'unknown', 'refused'))
       OR (OLD.state = 'unknown' AND NEW.state IN ('merged', 'refused'))
       OR OLD.state = NEW.state)
BEGIN
    SELECT RAISE(ABORT, 'illegal merge attempt transition');
END;

-- A settled attempt keeps its outcome: refusal and merge_sha are written with the state.
CREATE TRIGGER pull_request_merges_settled_is_final
BEFORE UPDATE ON pull_request_merges
WHEN OLD.state IN ('merged', 'refused')
BEGIN
    SELECT RAISE(ABORT, 'a settled merge attempt never changes');
END;

CREATE TRIGGER pull_request_merges_no_delete
BEFORE DELETE ON pull_request_merges
BEGIN
    SELECT RAISE(ABORT, 'merge attempts are never deleted');
END;
