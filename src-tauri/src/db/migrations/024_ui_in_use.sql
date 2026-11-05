-- UI3: the UI documents in use, each for whom (`for`): everyone, some
-- classes or specs, or some characters by GUID. One document per `for`; a
-- new one for the same `for` replaces it. The UI slot carries them, most
-- specific first, and the addon draws the first that matches.
CREATE TABLE ui_in_use (
    flavor    TEXT NOT NULL,
    target    TEXT NOT NULL,          -- the `for` as canonical JSON; '{}' is everyone
    proposal  INTEGER NOT NULL,       -- an applied ui_doc proposal
    set_at    INTEGER NOT NULL,
    PRIMARY KEY (flavor, target)
) STRICT;

-- UI1's single document (meta `ui_doc:<flavor>`) was everyone's.
INSERT INTO ui_in_use (flavor, target, proposal, set_at)
    SELECT substr(key, 8), '{}', CAST(value AS INTEGER), 0
    FROM meta WHERE key LIKE 'ui_doc:%';
DELETE FROM meta WHERE key LIKE 'ui_doc:%';
