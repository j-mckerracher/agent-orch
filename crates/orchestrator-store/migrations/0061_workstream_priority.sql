-- Workstream priority: the operator's order of initiatives. Lower runs first.
-- Existing workstreams keep their creation order; a new workstream goes last
-- and `workstreams.prioritize` moves one to the top. Advisory, like
-- `suspended`: the scheduler does not consult it.
-- Version 0061 leaves 0059-0060 free for concurrently developed migrations.

ALTER TABLE workstreams ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;

UPDATE workstreams SET priority = sequence;

CREATE INDEX workstreams_by_priority ON workstreams(priority, sequence);
