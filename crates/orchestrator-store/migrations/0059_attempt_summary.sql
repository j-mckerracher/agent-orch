-- What an attempt's agent reported when the attempt ended (DPES-1106). Both columns stay NULL
-- until the executor records them, and attempts that ended earlier keep NULL.
ALTER TABLE attempts ADD COLUMN summary TEXT;
ALTER TABLE attempts ADD COLUMN changed_files INTEGER;
