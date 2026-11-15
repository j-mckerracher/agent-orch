-- DPES-1105: the permission level and argument profile a chat turn ran with.
--
-- Both columns are nullable. A NULL row is a turn from before permission levels,
-- and it is never read as a level. The desktop always sends a level, so a turn
-- recorded as `auto` was asked for as `auto` (by the default or by the user).

ALTER TABLE conversation_turns
    ADD COLUMN permission_level TEXT CHECK (
        permission_level IS NULL
        OR permission_level IN ('restricted', 'extended', 'auto', 'full')
    );

ALTER TABLE conversation_turns
    ADD COLUMN permission_profile TEXT CHECK (
        permission_profile IS NULL OR length(permission_profile) BETWEEN 1 AND 64
    );
