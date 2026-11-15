-- DPES-1105: stop keeping message text in idempotency results.
--
-- Before this migration a cached `conversations.send` or `coordinator.*` reply held
-- the whole entry, body included. The result now holds references only, and a replay
-- reads the entry back from its table. Each entry still exists when this runs, because
-- entries were append-only before 0032, so every reference resolves.
--
-- Fingerprints are kept. A live message's fingerprint leaks nothing its entry does
-- not already expose, and a replay needs it to detect a reused key.

UPDATE idempotency_records
SET result = json_object(
    'status', 'success',
    'body', json_object(
        'entry_id', json_extract(result, '$.body.entry.id'),
        'entry_sequence', json_extract(result, '$.body.entry.sequence'),
        'entry_created_at', json_extract(result, '$.body.entry.created_at'),
        'turn_id', json_extract(result, '$.body.turn.id'),
        'epoch', json_extract(result, '$.body.epoch')
    )
)
WHERE method = 'conversations.send'
  AND json_extract(result, '$.status') = 'success';

UPDATE idempotency_records
SET result = json_object(
    'status', 'success',
    'body', json_object(
        'entry_id', json_extract(result, '$.body.id'),
        'entry_sequence', json_extract(result, '$.body.sequence'),
        'entry_created_at', json_extract(result, '$.body.created_at')
    )
)
WHERE method IN ('coordinator.send_message', 'coordinator.propose')
  AND json_extract(result, '$.status') = 'success';
