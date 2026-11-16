-- A launch failure belongs to one attempt and must survive retries.
ALTER TABLE attempts ADD COLUMN failure_reason TEXT;
