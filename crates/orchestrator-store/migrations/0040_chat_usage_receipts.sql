-- DPES-1105 phase 3a: usage receipts for chat turns.
--
-- A receipt is figures only, never text. It has no foreign key on purpose: a history
-- clear deletes turns, and the wake token budget must not reset because the user cleared
-- a transcript (the same reason action_proposal_events has no foreign key).
-- NULL means the provider did not report that figure; it is never zero.
CREATE TABLE conversation_usage_receipts (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id TEXT NOT NULL CHECK (length(conversation_id) > 0),
    turn_id TEXT NOT NULL CHECK (length(turn_id) > 0),
    trigger_kind TEXT NOT NULL CHECK (trigger_kind IN ('user', 'wake')),
    receipt_key TEXT NOT NULL CHECK (length(receipt_key) BETWEEN 1 AND 128),
    reported_at_ms INTEGER NOT NULL CHECK (reported_at_ms >= 0),
    input_tokens INTEGER CHECK (input_tokens IS NULL OR input_tokens >= 0),
    cache_read_tokens INTEGER CHECK (cache_read_tokens IS NULL OR cache_read_tokens >= 0),
    cache_write_tokens INTEGER CHECK (cache_write_tokens IS NULL OR cache_write_tokens >= 0),
    output_tokens INTEGER CHECK (output_tokens IS NULL OR output_tokens >= 0),
    reasoning_tokens INTEGER CHECK (reasoning_tokens IS NULL OR reasoning_tokens >= 0),
    cost_nanos INTEGER CHECK (cost_nanos IS NULL OR cost_nanos >= 0),
    currency TEXT CHECK (currency IS NULL OR length(currency) BETWEEN 3 AND 8),
    model TEXT,
    CHECK ((cost_nanos IS NULL) = (currency IS NULL)),
    UNIQUE (turn_id, receipt_key)
);
CREATE INDEX conversation_usage_receipts_by_scope
    ON conversation_usage_receipts(conversation_id, trigger_kind, reported_at_ms);

-- Which kind of event opened a turn. 'user' is every turn before this migration.
-- The design calls this column `trigger`; SQLite accepts that name but it is a keyword,
-- so the column is `trigger_kind`.
ALTER TABLE conversation_turns ADD COLUMN trigger_kind TEXT NOT NULL DEFAULT 'user'
    CHECK (trigger_kind IN ('user', 'wake'));
