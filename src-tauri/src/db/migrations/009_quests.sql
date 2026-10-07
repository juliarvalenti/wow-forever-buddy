-- Q1b: each character's completed quests, from the addon's
-- snapshot.quests_done (0.4.0 on): the whole list as of its newest
-- snapshot, replaced only by a newer one, so a replayed older backup can't
-- shrink it. Accepts and turn-ins are already kept as adventure_events
-- (kinds quest_accepted and quest).
CREATE TABLE char_quests_done (
    character_id INTEGER NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    quest_id     INTEGER NOT NULL,
    as_of        INTEGER NOT NULL,         -- the snapshot's `at`
    PRIMARY KEY (character_id, quest_id)
) STRICT;
