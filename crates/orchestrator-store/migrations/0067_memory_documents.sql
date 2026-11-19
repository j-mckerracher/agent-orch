-- DPES-1108 phase 6: preferences and learnings replace the orchestrator notes.
--
-- A document is the entries of one (kind, scope). kind 'preference' holds the user's standing
-- defaults: in scope 'global' the shared preferences every conversation reads, in a
-- 'workstream:' or 'project:' scope the preferences of that one conversation. kind 'learning'
-- holds operational facts the orchestrator of that scope recorded. An entry is one keyed line:
-- writing the same key updates it in place. Byte budgets, the key shape and the text rules are
-- checked in Rust (orchestrator_core::memory), not here, so this migration can never refuse a
-- note it copies.
CREATE TABLE memory_entries (
    kind TEXT NOT NULL CHECK (kind IN ('preference', 'learning')),
    scope TEXT NOT NULL
        CHECK (scope = 'global' OR scope GLOB 'workstream:?*' OR scope GLOB 'project:?*'),
    key TEXT NOT NULL CHECK (length(key) >= 1),
    text TEXT NOT NULL CHECK (length(CAST(text AS BLOB)) >= 1),
    written_by TEXT NOT NULL CHECK (written_by IN ('user', 'orchestrator')),
    created_at_ms INTEGER NOT NULL,
    confirmed_at_ms INTEGER NOT NULL,
    PRIMARY KEY (kind, scope, key)
);

-- Every entry that leaves a document: removed by the user or replaced by a newer text. Rows are
-- never deleted, and only restored_at_ms changes, once, when the user restores the entry.
CREATE TABLE memory_archive (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL CHECK (kind IN ('preference', 'learning')),
    scope TEXT NOT NULL,
    key TEXT NOT NULL,
    text TEXT NOT NULL,
    written_by TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    confirmed_at_ms INTEGER NOT NULL,
    archived_at_ms INTEGER NOT NULL,
    reason TEXT NOT NULL CHECK (reason IN ('removed', 'replaced')),
    archived_by TEXT NOT NULL CHECK (archived_by IN ('user', 'orchestrator')),
    restored_at_ms INTEGER
);

CREATE INDEX memory_archive_by_document ON memory_archive(kind, scope, id);

CREATE TRIGGER memory_archive_no_delete BEFORE DELETE ON memory_archive
BEGIN
    SELECT RAISE(ABORT, 'archived memory entries are never deleted');
END;

CREATE TRIGGER memory_archive_restored_once BEFORE UPDATE ON memory_archive
WHEN OLD.restored_at_ms IS NOT NULL OR NEW.restored_at_ms IS NULL
  OR NEW.id IS NOT OLD.id OR NEW.kind IS NOT OLD.kind OR NEW.scope IS NOT OLD.scope
  OR NEW.key IS NOT OLD.key OR NEW.text IS NOT OLD.text OR NEW.written_by IS NOT OLD.written_by
  OR NEW.created_at_ms IS NOT OLD.created_at_ms OR NEW.confirmed_at_ms IS NOT OLD.confirmed_at_ms
  OR NEW.archived_at_ms IS NOT OLD.archived_at_ms OR NEW.reason IS NOT OLD.reason
  OR NEW.archived_by IS NOT OLD.archived_by
BEGIN
    SELECT RAISE(ABORT, 'an archived memory entry only records its restore, once');
END;

-- The notes move into the preferences of the same scope, so each keeps its reach: a global
-- note becomes a shared preference, a workstream note a preference of that workstream (0050
-- allowed no other scope). The key is the note's place, note-1 to note-10, and the note's last
-- write is its last confirmation. orchestrator_notes is kept, unread, as the source.
INSERT INTO memory_entries
    (kind, scope, key, text, written_by, created_at_ms, confirmed_at_ms)
SELECT 'preference', scope, 'note-' || (position + 1), body, 'user',
       COALESCE(CAST((julianday(updated_at) - 2440587.5) * 86400000 AS INTEGER),
                CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)),
       COALESCE(CAST((julianday(updated_at) - 2440587.5) * 86400000 AS INTEGER),
                CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
FROM orchestrator_notes;
