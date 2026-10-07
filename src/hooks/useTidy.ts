import { useCallback, useEffect, useState } from "react";
import { commands, type DataSize, events, type Tidy } from "@/lib/bindings";
import { errorText } from "@/lib/format";
import { useEvent } from "@/hooks/useEvent";

/** O2 (IMPLEMENTING §22): gone, hidden and forgotten characters. An ingest
 *  can bring a gone folder back, so it reloads then. Each change returns the
 *  fresh lists. */
export function useTidy() {
  const [tidy, setTidy] = useState<Tidy | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(() => {
    commands.tidyGet().then(setTidy, (e) => setError(errorText(e)));
  }, []);
  useEffect(load, [load]);
  useEvent(events.ingestCompleted, load);

  const run = useCallback(async (p: Promise<Tidy>): Promise<boolean> => {
    setError(null);
    try {
      setTidy(await p);
      return true;
    } catch (e) {
      setError(errorText(e));
      return false;
    }
  }, []);

  return {
    tidy,
    error,
    reload: load,
    hide: (id: number, hidden: boolean) => run(commands.tidyHide(id, hidden)),
    forget: (id: number) => run(commands.tidyForget(id)),
    remember: (forgottenId: number) => run(commands.tidyRemember(forgottenId)),
  };
}

/** "Forever Buddy's own data": the database's size and counts, and Compact. */
export function useDataSize() {
  const [data, setData] = useState<DataSize | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(() => {
    commands.tidyData().then(setData, (e) => setError(errorText(e)));
  }, []);
  useEffect(load, [load]);

  /** The size before and after, or null when it was refused. */
  const compact = useCallback(async (): Promise<[number, number] | null> => {
    setError(null);
    const before = data?.bytes ?? 0;
    try {
      const after = await commands.tidyCompact();
      setData(after);
      return [before, after.bytes ?? 0];
    } catch (e) {
      setError(errorText(e));
      return null;
    }
  }, [data]);

  return { data, error, reload: load, compact };
}
