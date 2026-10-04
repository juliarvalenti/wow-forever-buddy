-- Key the hash cache on (flavor, path relative to the flavor folder) instead
-- of an absolute path: it survives the install moving, and NOCASE avoids
-- case-only duplicates on Windows (T3 review). The cache is disposable.

DROP TABLE file_hash_cache;

CREATE TABLE file_hash_cache (
    flavor   TEXT NOT NULL,
    path     TEXT NOT NULL COLLATE NOCASE,  -- e.g. WTF/Account/X/SavedVariables/Foo.lua
    size     INTEGER NOT NULL,
    mtime_ns INTEGER NOT NULL,
    blake3   TEXT NOT NULL,
    PRIMARY KEY (flavor, path)
) STRICT;
