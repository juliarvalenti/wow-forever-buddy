-- B3: bag cleanup (IMPLEMENTING §18, INGAME §14). Items marked in the app
-- to sell or to send to another character; the addon shows the marks in
-- the bags (the Cleanup slot). Nothing here sells or sends.

-- Whether the game reported an item soulbound (addon 0.8.0 on), so the app
-- never offers to mail it.
ALTER TABLE char_items ADD COLUMN bound INTEGER NOT NULL DEFAULT 0 CHECK (bound IN (0, 1));

-- One mark per character and item. A mark clears once the item has left
-- the character (seen at the next ingest).
CREATE TABLE cleanup_marks (
    character_id    INTEGER NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    item_id         INTEGER NOT NULL,
    action          TEXT NOT NULL CHECK (action IN ('sell', 'send')),
    to_character_id INTEGER REFERENCES characters (id) ON DELETE CASCADE,
    created_at      TEXT NOT NULL,          -- RFC 3339, UTC
    CHECK ((action = 'send') = (to_character_id IS NOT NULL)),
    PRIMARY KEY (character_id, item_id)
) STRICT;

-- When the player last changed any mark of a flavor, for "Sent to the game".
CREATE TABLE cleanup_changed (
    flavor     TEXT PRIMARY KEY,
    changed_at TEXT NOT NULL               -- RFC 3339, UTC
) STRICT;
