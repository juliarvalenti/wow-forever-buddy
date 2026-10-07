-- P1: quest plans (INGAME §7, IMPLEMENTING §16). One active plan per
-- character, set when the player approves a proposal (P2's queue and
-- Approvals panel, never here) and sent to the game in the bridge's Plan
-- slot. Earlier and cleared plans stay as 'replaced'; Approvals keeps the
-- history.
CREATE TABLE quest_plans (
    id           INTEGER PRIMARY KEY,
    character_id INTEGER NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    title        TEXT NOT NULL,
    steps        TEXT NOT NULL,            -- JSON array of steps (plans.rs Step)
    producer     TEXT NOT NULL,            -- 'app' or 'agent:<client name>'
    status       TEXT NOT NULL CHECK (status IN ('active', 'replaced')),
    created_at   TEXT NOT NULL,            -- RFC 3339, UTC: when it was approved
    -- Progress as of logout, from the addon's ForeverBuddyDB.plan: the
    -- 1-based steps done, and the file time it came from (newer wins).
    done         TEXT NOT NULL DEFAULT '[]',
    progress_at  INTEGER
) STRICT;
CREATE INDEX quest_plans_active ON quest_plans (character_id, status);
