-- v1 schema (spec §6). Backup manifests on disk stay the source of truth;
-- `snapshots` is an index that backup_reindex can rebuild from them.

CREATE TABLE snapshots (
    id           TEXT PRIMARY KEY,          -- ULID, same as the manifest file name
    created_at   TEXT NOT NULL,             -- RFC 3339, UTC
    trigger      TEXT NOT NULL,             -- manual | app_start | game_exit | scheduled | pre_write | pre_restore
    label        TEXT,
    pinned       INTEGER NOT NULL DEFAULT 0,
    scope        TEXT NOT NULL,             -- full | partial
    flavor       TEXT NOT NULL,
    file_count   INTEGER NOT NULL,
    total_bytes  INTEGER NOT NULL,          -- logical size of the snapshot
    new_bytes    INTEGER NOT NULL,          -- bytes of blobs this snapshot added to the store
    char_count   INTEGER NOT NULL,
    addon_count  INTEGER NOT NULL,
    game_running INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE INDEX snapshots_created_at ON snapshots (created_at);
CREATE INDEX snapshots_trigger ON snapshots (trigger, created_at);

-- Lets unchanged files skip re-reading and re-hashing on every snapshot.
CREATE TABLE file_hash_cache (
    path     TEXT PRIMARY KEY,              -- absolute path, as given by the install
    size     INTEGER NOT NULL,
    mtime_ns INTEGER NOT NULL,
    blake3   TEXT NOT NULL
) STRICT;

-- One row per game-file mutation (spec §4), for auditing and "undo last change".
CREATE TABLE write_audit (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    at          TEXT NOT NULL,              -- RFC 3339, UTC
    op          TEXT NOT NULL,              -- e.g. "restore", "macro_edit"
    paths       TEXT NOT NULL,              -- JSON array of paths relative to the flavor dir
    snapshot_id TEXT,                       -- the pre-write snapshot
    result      TEXT NOT NULL,              -- started | committed | aborted | failed
    detail      TEXT
) STRICT;

CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;
