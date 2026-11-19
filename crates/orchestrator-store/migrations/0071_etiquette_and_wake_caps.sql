-- DPES-1108 phase 8: wake caps sized for autonomous supervision, and the pull request links of a
-- turn.
--
-- 1. The defaults move from 6 to 20 wake turns an hour and from 40 to 160 a day, per scope. A
--    value still at the old default moves; a value the user changed, or set to no limit (NULL),
--    stays. A user who chose exactly 6 or 40 on purpose is moved too: the store cannot tell that
--    apart from the default. The column DEFAULTs stay: the settings row exists since 0039 and no
--    insert reads them.
UPDATE orchestrator_settings SET wake_hourly_cap = 20 WHERE wake_hourly_cap = 6;
UPDATE orchestrator_settings SET wake_daily_cap = 160 WHERE wake_daily_cap = 40;

-- 2. The pull requests of the tasks a completed turn acted on or reported, written by the daemon
--    in the transaction that completes the turn, from `pull_requests` and `pull_request_repos`
--    only, never from reply text. A JSON array of at most 5 {"task_id", "number", "url"}; NULL
--    when there are none. It goes with its turn when the history is cleared.
ALTER TABLE conversation_turns ADD COLUMN pull_request_links TEXT
    CHECK (pull_request_links IS NULL
        OR (json_valid(pull_request_links)
            AND json_type(pull_request_links) = 'array'
            AND length(pull_request_links) <= 4096));
