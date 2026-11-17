-- DPES-1105 phase 5b: the away window and the return brief window.
-- orchestrator_settings is the one row from 0039; this adds columns only.

-- Set while the monitoring mode is 'away'. The brief covers [away_since_ms, now].
ALTER TABLE orchestrator_settings ADD COLUMN away_since_ms INTEGER
    CHECK (away_since_ms IS NULL OR away_since_ms >= 0);
-- The closed window of the last away period, kept until the user acknowledges the brief.
ALTER TABLE orchestrator_settings ADD COLUMN brief_from_ms INTEGER
    CHECK (brief_from_ms IS NULL OR brief_from_ms >= 0);
ALTER TABLE orchestrator_settings ADD COLUMN brief_to_ms INTEGER
    CHECK (brief_to_ms IS NULL OR brief_to_ms >= 0);
