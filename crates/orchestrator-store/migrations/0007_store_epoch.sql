-- The epoch names this database file. A client resume cursor is only meaningful
-- against the journal that issued it, so a replaced database gets a new epoch.
CREATE TABLE store_identity (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    epoch TEXT NOT NULL CHECK (length(epoch) = 32)
);

INSERT INTO store_identity (id, epoch) VALUES (1, lower(hex(randomblob(16))));
