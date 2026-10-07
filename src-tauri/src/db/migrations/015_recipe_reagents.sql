-- C2: what a craftable item takes, from the recipe's required reagents
-- (addon 0.7.0's snapshot.recipes[...].mats). Game data, not a character's:
-- one row per item and reagent, replaced when a newer scan lists the item.
-- The tooltip adds up the alts' holdings of each reagent against `qty`.
CREATE TABLE recipe_reagents (
    item_id    INTEGER NOT NULL,
    reagent_id INTEGER NOT NULL,
    qty        INTEGER NOT NULL,
    seen_at    INTEGER NOT NULL,
    PRIMARY KEY (item_id, reagent_id)
) STRICT;
