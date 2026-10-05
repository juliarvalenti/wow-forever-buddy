-- The bags and bank tabs themselves (V7): name, size and free slots, for the
-- Characters cards ("3 free") and the sheet's Satchels panel ("68 of 80
-- used"). char_items holds what's in them; this holds the containers.
-- Replaced together with that location's items, so they never disagree.

CREATE TABLE char_bags (
    character_id INTEGER NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    location     TEXT NOT NULL CHECK (location IN ('bag', 'bank')),
    container    INTEGER NOT NULL,          -- bag index (0 = backpack) or bank tab
    name         TEXT,
    size         INTEGER,
    free         INTEGER,
    as_of        INTEGER NOT NULL,
    PRIMARY KEY (character_id, location, container)
) STRICT;
