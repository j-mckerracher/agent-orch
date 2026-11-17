-- DPES-1105 phase 2c: orchestrator settings, the machine load limits (N12).
--
-- One row, held by the database: id is always 1, so a second row cannot be added and
-- the delete gate below keeps the one row. Later phases add columns with
-- ALTER TABLE ADD COLUMN, so this table is never rebuilt and has no table-level CHECK.
-- A limit is a percentage from 50 to 100. A limit of 100 turns that meter off.

CREATE TABLE orchestrator_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    load_cpu_percent INTEGER NOT NULL DEFAULT 80
        CHECK (load_cpu_percent BETWEEN 50 AND 100),
    load_memory_percent INTEGER NOT NULL DEFAULT 85
        CHECK (load_memory_percent BETWEEN 50 AND 100),
    load_disk_percent INTEGER NOT NULL DEFAULT 90
        CHECK (load_disk_percent BETWEEN 50 AND 100),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO orchestrator_settings (id) VALUES (1);

CREATE TRIGGER orchestrator_settings_keep_row
BEFORE DELETE ON orchestrator_settings
BEGIN
    SELECT RAISE(ABORT, 'the orchestrator settings row is never deleted');
END;
