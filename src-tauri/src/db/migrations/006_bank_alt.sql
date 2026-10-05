-- F3: the user marks a character as a bank alt (a "Bank" tag on its card and
-- in the Dashboard roster). The user's, not the addon's: ingest upserts the
-- other columns of `characters` and never touches this one.

ALTER TABLE characters
    ADD COLUMN bank_alt INTEGER NOT NULL DEFAULT 0 CHECK (bank_alt IN (0, 1));
