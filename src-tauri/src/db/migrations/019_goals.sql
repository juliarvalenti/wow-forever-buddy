-- G1 (IMPLEMENTING §21): goals, set in the app or proposed by an agent and
-- approved (`producer` "app" or "agent:<client>", a claim). Two kinds: a
-- character reaching a level, or a character (or, with no character, the
-- whole account) holding an amount of gold. Item goals are Lists.
--
-- Progress isn't stored: it's read from what ingest keeps (level and XP,
-- gold snapshots). `start` is the value when the goal was set, for the pace
-- ("~0.9 levels a day"). `done_at` is stamped the first time an ingest finds
-- the goal reached.
CREATE TABLE goals (
    id           INTEGER PRIMARY KEY,
    flavor       TEXT NOT NULL,
    character_id INTEGER REFERENCES characters (id) ON DELETE CASCADE, -- NULL: the account (gold only)
    kind         TEXT NOT NULL CHECK (kind IN ('level', 'gold')),
    target       INTEGER NOT NULL,        -- a level, or copper
    label        TEXT,                    -- gold only: "for the mount", up to 24 characters
    start        REAL NOT NULL,           -- the value when set: level with XP fraction, or copper
    by_at        INTEGER,                 -- optional date, unix seconds
    producer     TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    done_at      INTEGER,
    archived_at  INTEGER,                 -- removed by the player
    CHECK (character_id IS NOT NULL OR kind = 'gold')
) STRICT;

CREATE INDEX goals_open ON goals (flavor, archived_at, done_at);
