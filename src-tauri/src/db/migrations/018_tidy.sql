-- O2: tidying old characters (IMPLEMENTING §22).

-- Hidden characters leave every list, total and agent tool; nothing is
-- deleted, and Unhide brings them back. When it was hidden, for Settings.
ALTER TABLE characters ADD COLUMN hidden_at INTEGER;

-- What the screens read: every character that isn't hidden. Ingest and the
-- per-character writes keep using `characters`.
CREATE VIEW visible_characters AS
    SELECT * FROM characters WHERE hidden_at IS NULL;

-- Characters whose app history was forgotten. Ingest skips these folders,
-- in WTF and in backups, until Remember again deletes the row; the name and
-- class are only for Settings' "Forgotten" list.
CREATE TABLE forgotten (
    flavor       TEXT NOT NULL,
    account      TEXT NOT NULL COLLATE NOCASE,
    group_dir    TEXT NOT NULL COLLATE NOCASE,
    char_dir     TEXT NOT NULL COLLATE NOCASE,
    name         TEXT NOT NULL,
    class        TEXT,
    forgotten_at INTEGER NOT NULL,
    PRIMARY KEY (flavor, account, group_dir, char_dir)
) STRICT;
