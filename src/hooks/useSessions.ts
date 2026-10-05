import { useCallback, useEffect, useState } from "react";
import { commands, events, type PlaySession, type WtfCharacter } from "@/lib/bindings";
import { errorText } from "@/lib/format";
import { useEvent } from "./useEvent";

/** Enough history for "this week" on a Sunday, plus a few recent rows. */
const DAYS = 14;

/** Recent play sessions (from the process watcher) and the characters in the
 *  WTF folder. Both follow `sessions-changed`: a session's end also changes
 *  who was played last. */
export function useSessions() {
  const [sessions, setSessions] = useState<PlaySession[] | null>(null);
  const [characters, setCharacters] = useState<WtfCharacter[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    commands.sessionsList(DAYS).then(
      (s) => {
        setSessions(s);
        setError(null);
      },
      (e) => setError(errorText(e)),
    );
    commands.charactersList().then(setCharacters, () => setCharacters(null));
  }, []);

  useEffect(refresh, [refresh]);
  useEvent(events.sessionsChanged, refresh);
  useEvent(events.installChanged, refresh);

  return { sessions, characters, error, refresh };
}

/** Monday 00:00 local time of the week containing `now`. */
export function weekStart(now: Date): Date {
  const d = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  d.setDate(d.getDate() - ((d.getDay() + 6) % 7));
  return d;
}

/** Time played since Monday (a running session counts up to `now`; one that
 *  started before Monday counts from Monday) and how many characters. */
export function thisWeek(sessions: PlaySession[], now = new Date()) {
  const from = weekStart(now).getTime();
  let ms = 0;
  const who = new Set<string>();
  for (const s of sessions) {
    const end = s.ended_at ? new Date(s.ended_at).getTime() : now.getTime();
    const start = Math.max(new Date(s.started_at).getTime(), from);
    if (end <= start) continue;
    ms += end - start;
    for (const c of s.characters) who.add(`${c.account}|${c.realm}|${c.name}`.toLowerCase());
  }
  return { ms, characters: who.size };
}
