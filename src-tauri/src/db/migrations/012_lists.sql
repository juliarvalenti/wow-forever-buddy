-- B2: shopping lists (IMPLEMENTING §15, INGAME §10). Lists the player makes
-- in the app, or approves from an agent's proposal (P2's queue, never here),
-- each optionally for one character: what it still needs becomes errands
-- for the alts holding it. Sent to the game in the bridge's Lists slot.
CREATE TABLE lists (
    id               INTEGER PRIMARY KEY,
    flavor           TEXT NOT NULL,
    name             TEXT NOT NULL,
    for_character_id INTEGER REFERENCES characters (id) ON DELETE SET NULL,
    producer         TEXT NOT NULL,        -- 'app' or 'agent:<client name>'
    created_at       TEXT NOT NULL         -- RFC 3339, UTC
) STRICT;
CREATE INDEX lists_flavor ON lists (flavor);

-- An item by id (one your characters have seen) or, typed as free text, by
-- name alone: the game then matches it by name at vendors and the AH.
CREATE TABLE list_items (
    id       INTEGER PRIMARY KEY,
    list_id  INTEGER NOT NULL REFERENCES lists (id) ON DELETE CASCADE,
    item_id  INTEGER,
    name     TEXT,
    need     INTEGER NOT NULL CHECK (need BETWEEN 1 AND 9999),
    position INTEGER NOT NULL,
    CHECK (item_id IS NOT NULL OR name IS NOT NULL)
) STRICT;
CREATE UNIQUE INDEX list_items_item ON list_items (list_id, item_id) WHERE item_id IS NOT NULL;
CREATE INDEX list_items_list ON list_items (list_id, position);

-- When the player last changed any list of a flavor, for "Sent to the game".
CREATE TABLE lists_changed (
    flavor     TEXT PRIMARY KEY,
    changed_at TEXT NOT NULL               -- RFC 3339, UTC
) STRICT;
