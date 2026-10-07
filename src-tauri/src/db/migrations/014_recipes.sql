-- C1: what each character can make, from its own profession window (addon
-- 0.7.0's snapshot.recipes). One row per profession and crafted item id.
-- `scanned_at` is when the addon last read that profession's window: a
-- profession not opened since keeps its rows, and the tooltip greys a scan
-- older than a week.
CREATE TABLE char_recipes (
    character_id INTEGER NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    profession   TEXT NOT NULL,
    item_id      INTEGER NOT NULL,
    scanned_at   INTEGER NOT NULL,
    PRIMARY KEY (character_id, profession, item_id)
) STRICT;
CREATE INDEX char_recipes_item ON char_recipes (item_id);
