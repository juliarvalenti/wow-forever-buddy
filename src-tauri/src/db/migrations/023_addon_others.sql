-- COMPAT1: which of the player's other addons that do one of our jobs
-- (DialogueUI, DialogKey, Baganator, Questie) the game last had loaded. The
-- addon reports only those few names, never the whole addon list. One row
-- per flavor, from the newest logout file; lints with an addon fire on it.
CREATE TABLE addon_others (
    flavor   TEXT PRIMARY KEY,
    names    TEXT NOT NULL,          -- JSON array of addon folder names
    seen_at  INTEGER NOT NULL
) STRICT;
