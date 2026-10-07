-- B3b: bag cleanup suggestions (IMPLEMENTING §18, INGAME §14).

-- Where an item is worn ("INVTYPE_HEAD") and the level it needs, from the
-- game's item info (addon 0.8.0), so the app can tell outgrown gear and
-- which other character it would upgrade.
ALTER TABLE items ADD COLUMN equip_loc TEXT;
ALTER TABLE items ADD COLUMN min_level INTEGER;

-- Whether `bound` is known: the game reported the bind state (true or
-- false). An unknown one never gets a "send" suggestion.
ALTER TABLE char_items ADD COLUMN bind_known INTEGER NOT NULL DEFAULT 0 CHECK (bind_known IN (0, 1));

-- Why a mark was made, as a fixed code (never free text): a grey, gear this
-- character has outgrown, or an upgrade of `gain` item levels for the
-- character it's sent to. Who made it: 'app' or 'agent:<client name>'.
ALTER TABLE cleanup_marks ADD COLUMN reason TEXT CHECK (reason IN ('grey', 'outgrown', 'upgrade'));
ALTER TABLE cleanup_marks ADD COLUMN gain INTEGER;
ALTER TABLE cleanup_marks ADD COLUMN producer TEXT NOT NULL DEFAULT 'app';

-- Suggestions the player dismissed: they don't come back for that item.
CREATE TABLE cleanup_dismissed (
    character_id INTEGER NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    item_id      INTEGER NOT NULL,
    PRIMARY KEY (character_id, item_id)
) STRICT;
