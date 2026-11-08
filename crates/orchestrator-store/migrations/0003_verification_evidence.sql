ALTER TABLE verification_runs
    ADD COLUMN result_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(result_json));

ALTER TABLE verification_runs
    ADD COLUMN started_at_ms INTEGER;

ALTER TABLE verification_runs
    ADD COLUMN finished_at_ms INTEGER CHECK (
        (started_at_ms IS NULL AND finished_at_ms IS NULL)
        OR (
            started_at_ms IS NOT NULL
            AND finished_at_ms IS NOT NULL
            AND started_at_ms >= 0
            AND finished_at_ms >= started_at_ms
        )
    );
