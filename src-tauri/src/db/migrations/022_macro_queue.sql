-- MC1 (issue #262): macros the player approved, waiting for the game. The
-- app never writes the game's macro file; the Macros slot carries each
-- waiting macro to the addon, and the player's click in the game applies it.
-- The addon reports what it did at logout, which closes the row.
CREATE TABLE macro_queue (
    id           INTEGER PRIMARY KEY,
    flavor       TEXT NOT NULL,
    proposal_id  INTEGER NOT NULL REFERENCES proposals (id),
    body         TEXT NOT NULL,       -- JSON: proposals::macros::MacroBody
    approved_at  INTEGER NOT NULL,
    done_at      INTEGER,             -- the game applied or skipped it
    result       TEXT                 -- what the addon said it did
) STRICT;

CREATE INDEX macro_queue_waiting ON macro_queue (flavor, done_at);
