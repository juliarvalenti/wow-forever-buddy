-- Play sessions from the process watcher (T14): when WoW ran, and which
-- characters' WTF folders changed during the run. A row with no ended_at is
-- the session in progress.

CREATE TABLE play_sessions (
    id         INTEGER PRIMARY KEY,
    flavor     TEXT NOT NULL,
    started_at TEXT NOT NULL,           -- RFC 3339, UTC
    ended_at   TEXT,                    -- RFC 3339, UTC; NULL while running
    characters TEXT NOT NULL DEFAULT '[]'  -- JSON [{account, realm, name}], last written first
) STRICT;

CREATE INDEX play_sessions_started ON play_sessions (started_at);
