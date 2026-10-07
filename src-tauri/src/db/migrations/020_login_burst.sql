-- BUG-ADV: addons before 0.9.0 took their bag baseline before a fresh
-- login's bags had loaded, then logged everything carried as gained in one
-- go, so every adventure showed the whole inventory as loot. Drops those
-- from the adventures already stored, by the same rule ingest now applies
-- (`ingest::apply::login_burst`): plain gains (no `how`) within 60 seconds of
-- login that share a second with at least 5 different items.
DELETE FROM adventure_events
WHERE rowid IN (
    SELECT e.rowid
    FROM adventure_events e
    JOIN adventures a ON a.id = e.adventure_id
    WHERE e.kind = 'gain'
      AND json_extract(e.data, '$.how') IS NULL
      AND e.at - a.login <= 60
      AND (
          SELECT count(DISTINCT json_extract(b.data, '$.item'))
          FROM adventure_events b
          WHERE b.adventure_id = e.adventure_id
            AND b.at = e.at
            AND b.kind = 'gain'
            AND json_extract(b.data, '$.how') IS NULL
      ) >= 5
);
