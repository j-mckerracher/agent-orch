-- Coordinators are opt-in.
--
-- Until now a project's coordinator was active unless a coordinator_scopes row said
-- otherwise (0032), so registering a directory also created a coordinator. From
-- here on a project has a coordinator only while its row has removed = 0, and only
-- `coordinator.restore` ("Add coordinator") creates one. Registering a project
-- just makes its directory available for work.
--
-- This migration removes every coordinator that is active now, exactly as
-- `coordinator.remove` does: it drops the idempotency fingerprints of their sends,
-- writes one 'remove' audit row per coordinator, deletes their logs with the clear
-- gate open, and marks them removed with a bumped generation. Rows are kept, so a
-- generation never goes backwards. Coordinators that are already removed are
-- unchanged.

UPDATE conversation_clear_gate SET open = 1 WHERE id = 1;

UPDATE idempotency_records
SET fingerprint = '0000000000000000000000000000000000000000000000000000000000000000'
WHERE method IN ('coordinator.send_message', 'coordinator.propose')
  AND result IS NOT NULL
  AND json_extract(result, '$.body.entry_id') IN (SELECT id FROM coordinator_entries);

INSERT INTO conversation_audit (action, scope, entries_deleted, turns_deleted)
SELECT 'remove',
       'coordinator:' || p.id,
       (SELECT COUNT(*) FROM coordinator_entries e WHERE e.project_id = p.id),
       0
FROM projects p
WHERE NOT EXISTS (
    SELECT 1 FROM coordinator_scopes s WHERE s.project_id = p.id AND s.removed = 1
)
ORDER BY p.id;

DELETE FROM coordinator_entries;

UPDATE conversation_clear_gate SET open = 0 WHERE id = 1;

INSERT INTO coordinator_scopes (project_id, removed, clear_generation)
SELECT id, 1, 1 FROM projects WHERE true
ON CONFLICT(project_id) DO UPDATE SET
    removed = 1,
    clear_generation = clear_generation + 1,
    changed_at = CURRENT_TIMESTAMP
WHERE coordinator_scopes.removed = 0;
