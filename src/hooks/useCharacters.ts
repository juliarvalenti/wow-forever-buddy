import { useCallback, useEffect, useState } from "react";
import {
  type AltLockout,
  commands,
  events,
  type CharacterSheet,
  type CharactersOverview,
  type SearchResults,
  type WtfCharacter,
} from "@/lib/bindings";
import { errorText } from "@/lib/format";
import { useEvent } from "./useEvent";

/** Every character's card and the totals. Follows `ingest-completed` (new
 *  notes read) and a changed game folder. */
export function useCharacters() {
  const [overview, setOverview] = useState<CharactersOverview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const refresh = useCallback(() => {
    commands.charactersOverview().then(
      (o) => {
        setOverview(o);
        setError(null);
      },
      (e) => setError(errorText(e)),
    );
  }, []);
  useEffect(refresh, [refresh]);
  useEvent(events.ingestCompleted, refresh);
  useEvent(events.installChanged, refresh);
  return { overview, error, refresh };
}

/** The search box (F2): results for `query`, a moment after typing stops,
 *  and again when new notes are read. `null` while the query is empty. A
 *  reply for an older query is dropped. */
export function useItemSearch(query: string) {
  const [results, setResults] = useState<SearchResults | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tick, setTick] = useState(0);
  useEvent(events.ingestCompleted, () => setTick((n) => n + 1));
  useEffect(() => {
    if (!query.trim()) {
      setResults(null);
      setError(null);
      return;
    }
    let live = true;
    const t = setTimeout(() => {
      commands.charactersSearch(query).then(
        (r) => {
          if (!live) return;
          setResults(r);
          setError(null);
        },
        (e) => live && setError(errorText(e)),
      );
    }, 150);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [query, tick]);
  return { results, error };
}

/** The characters found in the WTF folder (older settings folders left
 *  out), for cards the addon hasn't filled in yet. */
export function useRoster() {
  const [roster, setRoster] = useState<WtfCharacter[] | null>(null);
  const refresh = useCallback(() => {
    commands.charactersList().then(
      (list) => setRoster(list.filter((c) => !c.older)),
      () => setRoster(null),
    );
  }, []);
  useEffect(refresh, [refresh]);
  useEvent(events.installChanged, refresh);
  return roster;
}

/** One character's sheet, reloaded when that character's notes change. */
export function useCharacterSheet(id: number | null) {
  const [sheet, setSheet] = useState<CharacterSheet | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(() => {
    if (id == null) return;
    let live = true;
    commands.characterDetail(id).then(
      (s) => {
        if (!live) return;
        setSheet(s);
        setError(null);
      },
      (e) => live && setError(errorText(e)),
    );
    return () => {
      live = false;
    };
  }, [id]);
  useEffect(() => {
    setSheet(null);
    return load();
  }, [load]);
  useEvent(events.ingestCompleted, (e) => {
    if (id != null && e.characters.includes(id)) load();
  });
  return { sheet, error, reload: load };
}

/** Saves across every character that haven't reset (the Dashboard's
 *  "Lockouts this week"), followed as new notes are read. */
export function useLockouts() {
  const [lockouts, setLockouts] = useState<AltLockout[] | null>(null);
  const refresh = useCallback(() => {
    commands.lockoutsList().then(setLockouts, () => setLockouts(null));
  }, []);
  useEffect(refresh, [refresh]);
  useEvent(events.ingestCompleted, refresh);
  useEvent(events.installChanged, refresh);
  return lockouts;
}
