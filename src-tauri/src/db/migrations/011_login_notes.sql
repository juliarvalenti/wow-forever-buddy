-- B1 (INGAME §9, IMPLEMENTING §15): login notes, written in the app for one
-- character and shown in the game's chat when that character logs in.
-- "Once" notes are archived after the first login that shows them (the
-- addon's `briefed` receipt); "until" notes show at each login until then.
CREATE TABLE login_notes (
    id           INTEGER PRIMARY KEY,
    character_id INTEGER NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    text         TEXT NOT NULL,
    once         INTEGER NOT NULL DEFAULT 1, -- 1: next login only; 0: until `until_at`
    until_at     INTEGER,                    -- unix seconds, for `once` = 0
    author       TEXT NOT NULL DEFAULT 'you' CHECK (author IN ('you', 'claude')),
    created_at   INTEGER NOT NULL,
    shown_at     INTEGER,                    -- the first login that showed it
    archived_at  INTEGER                     -- shown (once), expired or deleted
) STRICT;

CREATE INDEX login_notes_character ON login_notes (character_id, archived_at);
