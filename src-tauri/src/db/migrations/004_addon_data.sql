-- v0.2 addon data (docs/specs/v0.2-addon.md §4): what ingest reads out of
-- each character's ForeverBuddy.lua. Unlike the v0.1 tables this can't all be
-- rebuilt from the game's files (the addon keeps only the last 10 sessions),
-- so the db gets daily copies (db/copies.rs).
--
-- Times from the game (`GetServerTime()`: snapshot `at`, session login and
-- logout, event `t`) are INTEGER Unix seconds, exactly as the addon wrote
-- them. Times the app records itself are TEXT RFC 3339, UTC, like v0.1.

-- One row per character folder: WTF/Account/<account>/<group_dir>/<char_dir>.
-- Identity is the raw folder names, never split or reinterpreted (probe run 1):
-- `group_dir` is an opaque group id on Forever ("70") or the realm on the
-- legacy layout, and `char_dir` is the full name, first plus optional
-- surname ("Ellygie-Vargur"). Name, surname and realm for display come from
-- the addon (UnitName's two returns, GetRealmName), stored separately. Per
-- flavor, so the same folders under Classic and Forever never mix.
CREATE TABLE characters (
    id          INTEGER PRIMARY KEY,
    flavor      TEXT NOT NULL,             -- folder name, e.g. _classic_beta_
    -- Case-insensitive, like Windows folders, so a case change on disk
    -- doesn't split a character's history (SQLite NOCASE folds ASCII only).
    -- Look characters up with `WHERE char_dir = ?` so this collation applies.
    account     TEXT NOT NULL COLLATE NOCASE,  -- WTF/Account/<account>
    group_dir   TEXT NOT NULL COLLATE NOCASE,  -- the folder between account and character
    char_dir    TEXT NOT NULL COLLATE NOCASE,  -- the character's own folder
    name        TEXT NOT NULL,             -- UnitName('player'), first return
    surname     TEXT,                      -- UnitName's second return, if any
    realm       TEXT,                      -- GetRealmName()
    guid        TEXT,
    class       TEXT,                      -- file token, e.g. WARRIOR
    race        TEXT,
    sex         INTEGER,
    faction     TEXT,
    level       INTEGER,
    guild       TEXT,
    guild_rank  TEXT,
    first_seen  INTEGER NOT NULL,          -- earliest snapshot `at`
    last_seen   INTEGER NOT NULL,          -- latest snapshot `at`
    UNIQUE (flavor, account, group_dir, char_dir)
) STRICT;

-- The character at each logout (or /reload). Same `at` = already ingested.
CREATE TABLE char_snapshots (
    character_id  INTEGER NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    at            INTEGER NOT NULL,
    money         INTEGER NOT NULL,        -- copper
    xp            INTEGER,
    xp_max        INTEGER,
    rested        INTEGER,
    rest_state    TEXT,                    -- GetRestState's name, e.g. 'Rested'
    level         INTEGER,
    ilvl_avg      REAL,
    ilvl_equipped REAL,
    played_total  INTEGER,                 -- seconds
    played_level  INTEGER,
    zone          TEXT,
    subzone       TEXT,
    map           INTEGER,                 -- uiMapID (C_Map.GetBestMapForUnit)
    UNIQUE (character_id, at)
) STRICT;

-- What a character carries, per location. Replaced per location on ingest;
-- bank and mail only when their `as_of` is newer than what's stored.
CREATE TABLE char_items (
    character_id INTEGER NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    location     TEXT NOT NULL CHECK (location IN ('equipped', 'bag', 'bank', 'mail')),
    container    INTEGER NOT NULL,         -- bag or bank tab index; mail message index; 0 for equipped
    slot         INTEGER NOT NULL,
    item_id      INTEGER NOT NULL,
    link         TEXT NOT NULL,
    count        INTEGER NOT NULL DEFAULT 1,
    as_of        INTEGER NOT NULL          -- when the addon last saw this location
) STRICT;

CREATE INDEX char_items_by_location ON char_items (character_id, location);

-- Mail messages, for the Mail tab. Sender and subject come from the user's
-- own mailbox and stay local: never in CSV exports, logs, ingest errors or
-- error messages (spec §4).
CREATE TABLE char_mail (
    character_id INTEGER NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    idx          INTEGER NOT NULL,         -- matches char_items.container for location 'mail'
    sender       TEXT,
    subject      TEXT,
    money        INTEGER NOT NULL DEFAULT 0,
    cod          INTEGER NOT NULL DEFAULT 0,
    days_left    REAL,
    as_of        INTEGER NOT NULL,
    PRIMARY KEY (character_id, idx)
) STRICT;

-- Static item info, so names and qualities show offline.
CREATE TABLE items (
    item_id      INTEGER PRIMARY KEY,
    name         TEXT,
    quality      INTEGER,
    ilvl         INTEGER,
    icon_file_id INTEGER,
    class_id     INTEGER,
    subclass_id  INTEGER,
    sell_price   INTEGER,
    seen_at      INTEGER NOT NULL
) STRICT;

-- Replaced per snapshot.
CREATE TABLE professions (
    character_id INTEGER NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    skill        INTEGER,
    max          INTEGER,
    line         INTEGER,                  -- skill line id
    spec         INTEGER,                  -- GetProfessionInfo's specialization index
    as_of        INTEGER NOT NULL,
    PRIMARY KEY (character_id, name)
) STRICT;

CREATE TABLE lockouts (
    character_id INTEGER NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    difficulty   TEXT NOT NULL DEFAULT '',
    reset_at     INTEGER,
    raid         INTEGER NOT NULL DEFAULT 0 CHECK (raid IN (0, 1)),
    as_of        INTEGER NOT NULL,
    PRIMARY KEY (character_id, name, difficulty)
) STRICT;

-- Gold over time: one point per snapshot and per session money event.
CREATE TABLE gold_points (
    character_id INTEGER NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    at           INTEGER NOT NULL,
    money        INTEGER NOT NULL,
    UNIQUE (character_id, at)
) STRICT;

-- One login to logout. `play_session_id` links to T14's process session by
-- time overlap, so Recent sessions can name the characters for certain.
CREATE TABLE adventures (
    id              INTEGER PRIMARY KEY,
    character_id    INTEGER NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    login           INTEGER NOT NULL,
    logout          INTEGER,
    play_session_id INTEGER REFERENCES play_sessions (id) ON DELETE SET NULL,
    start_money     INTEGER,
    end_money       INTEGER,
    start_xp        INTEGER,
    end_xp          INTEGER,
    start_level     INTEGER,
    end_level       INTEGER,
    note            TEXT,                  -- the user's own note; kept across re-ingest
    UNIQUE (character_id, login)
) STRICT;

CREATE INDEX adventures_by_login ON adventures (login);

-- A session's timeline. Replaced as a whole when its adventure is re-ingested.
CREATE TABLE adventure_events (
    adventure_id INTEGER NOT NULL REFERENCES adventures (id) ON DELETE CASCADE,
    seq          INTEGER NOT NULL,
    at           INTEGER NOT NULL,
    kind         TEXT NOT NULL,            -- zone | level | money | quest | death | repair | gain | lose | encounter
    data         TEXT NOT NULL DEFAULT '{}',  -- JSON, per kind
    PRIMARY KEY (adventure_id, seq)
) STRICT;

-- What ingest last did with each ForeverBuddy.lua, so unchanged files are
-- skipped and failures show as "will retry".
CREATE TABLE ingest_state (
    path        TEXT PRIMARY KEY,          -- relative to the flavor folder
    size        INTEGER NOT NULL,
    mtime_ns    INTEGER NOT NULL,
    ingested_at TEXT NOT NULL,             -- RFC 3339, UTC
    status      TEXT NOT NULL,             -- ok | skipped (parse) | skipped (integrity) | skipped (mismatch)
    error       TEXT                       -- names the file and field, never a value
) STRICT;
