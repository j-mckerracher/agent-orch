-- The home directory: where a task runs when it is not clear which project it belongs to.
-- NULL means none is set. The daemon stores the canonical absolute path it validated.
ALTER TABLE orchestrator_settings ADD COLUMN home_directory TEXT
    CHECK (home_directory IS NULL OR length(home_directory) BETWEEN 1 AND 4096);
