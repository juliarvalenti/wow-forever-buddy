-- P4b (context/specs/p4-claude-settings §4): the addon's own settings.
--
-- What the game has: each logout's file says every option's value and the
-- last profile the addon applied. The settings are account-wide, so one
-- row per flavor, from the newest file.
CREATE TABLE addon_settings_seen (
    flavor   TEXT PRIMARY KEY,
    profile  INTEGER,                -- the last profile the addon applied, if any
    vals     TEXT NOT NULL,          -- JSON { option id: value }
    seen_at  INTEGER NOT NULL
) STRICT;

-- Profiles approved in the app, newest last. The newest goes to the game in
-- the Settings slot; the addon applies a profile once, by its id.
CREATE TABLE addon_profiles (
    id          INTEGER PRIMARY KEY,
    flavor      TEXT NOT NULL,
    vals        TEXT NOT NULL,       -- JSON { option id: value }: every option
    base        TEXT,                -- JSON: what the game had before these changes; NULL for a "go back"
    created_at  INTEGER NOT NULL,
    decision    TEXT CHECK (decision IN ('kept', 'undone'))  -- keep or go back, once applied
) STRICT;

CREATE INDEX addon_profiles_flavor ON addon_profiles (flavor, id);
