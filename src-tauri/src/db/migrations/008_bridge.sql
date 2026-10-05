-- Bridge v0.4 (docs/specs/bridge-v0.4.md): what the app last wrote into each
-- of our addon's data slots, and what each character's addon saw of them.

-- One row per slot and flavor: the last write, or why it didn't happen.
CREATE TABLE bridge_slots (
    flavor      TEXT NOT NULL,
    slot        TEXT NOT NULL,             -- "Tooltip1", …
    stamp       INTEGER,                   -- the stamp in the last file written
    written_at  TEXT,                      -- RFC 3339, UTC
    bytes       INTEGER,
    status      TEXT NOT NULL,             -- written | too_large | refused | failed
    error       TEXT,
    PRIMARY KEY (flavor, slot)
) STRICT;

-- The addon's receipts (ForeverBuddyDB.bridge): the stamp each slot carried
-- when the character's addon loaded it. Only a newer `seen_at` replaces one.
CREATE TABLE bridge_receipts (
    character_id INTEGER NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    slot         TEXT NOT NULL,
    stamp        INTEGER,
    schema       INTEGER,
    seen_at      INTEGER NOT NULL,         -- unix seconds
    PRIMARY KEY (character_id, slot)
) STRICT;
