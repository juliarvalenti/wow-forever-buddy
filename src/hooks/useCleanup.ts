import { useCallback, useEffect, useState } from "react";
import { type Cleanup, commands, events, type Mark } from "@/lib/bindings";
import { errorText } from "@/lib/format";
import { useEvent } from "@/hooks/useEvent";

/** B3: one character's marks (sell, or send to another character) and the
 *  Cleanup slot's way to the game. An ingest can clear marks (the item left
 *  the character), so it reloads then. Each change returns the fresh view. */
export function useCleanup(characterId: number) {
  const [cleanup, setCleanup] = useState<Cleanup | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(() => {
    commands.cleanupGet(characterId).then(setCleanup, (e) => setError(errorText(e)));
  }, [characterId]);
  useEffect(load, [load]);
  useEvent(events.ingestCompleted, load);

  const run = useCallback((p: Promise<Cleanup>) => {
    setError(null);
    p.then(setCleanup, (e) => setError(errorText(e)));
  }, []);

  return {
    cleanup,
    error,
    mark: (itemId: number, mark: Mark) => run(commands.cleanupMark(characterId, itemId, mark)),
    clear: (itemId: number) => run(commands.cleanupClear(characterId, itemId)),
    markGreys: () => run(commands.cleanupMarkGreys(characterId)),
  };
}
