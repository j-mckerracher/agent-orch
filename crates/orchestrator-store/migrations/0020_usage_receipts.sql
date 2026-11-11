-- M7-06: per-session AI usage receipts.
-- Version 0020 leaves 0011-0019 free for concurrently developed migrations.
--
-- A receipt is one observation of what a provider reported for a session.
-- NULL means the provider did not report that figure; it is never zero.
-- The unique key makes a repeated observation idempotent and lets a later,
-- revised observation replace the earlier one (late-arriving reconcile).
-- A receipt belongs to the day of window_end_ms, not to its insertion time.
-- Costs are integer nano-units of `currency`; no price is ever computed here.

CREATE TABLE usage_receipts (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('provider_reported', 'estimated')),
    receipt_key TEXT NOT NULL CHECK (length(receipt_key) BETWEEN 1 AND 128),
    window_start_ms INTEGER NOT NULL CHECK (window_start_ms >= 0),
    window_end_ms INTEGER NOT NULL CHECK (window_end_ms >= window_start_ms),
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
    UNIQUE (task_id, session_id, source, receipt_key),
    FOREIGN KEY (task_id, session_id) REFERENCES sessions(task_id, id)
);

CREATE INDEX usage_receipts_by_window_end ON usage_receipts(window_end_ms);
