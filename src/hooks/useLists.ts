import { useCallback, useEffect, useState } from "react";
import { commands, events, type ListsView, type NewItem } from "@/lib/bindings";
import { errorText } from "@/lib/format";
import { useEvent } from "@/hooks/useEvent";

/** B2: every list with what the characters hold, and the Lists slot's way to
 *  the game. Holdings change after an ingest, so it reloads then. Each change
 *  returns the fresh view; a refused one leaves it and says why. */
export function useLists() {
  const [view, setView] = useState<ListsView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(() => {
    commands.listsGet().then(setView, (e) => setError(errorText(e)));
  }, []);
  useEffect(load, [load]);
  useEvent(events.ingestCompleted, load);

  const run = useCallback((p: Promise<ListsView>) => {
    setError(null);
    return p.then(
      (v) => {
        setView(v);
        return true;
      },
      (e) => {
        setError(errorText(e));
        return false;
      },
    );
  }, []);

  return {
    view,
    error,
    create: (name: string, forCharacter: number | null) => run(commands.listCreate(name, forCharacter)),
    update: (id: number, name: string, forCharacter: number | null) =>
      run(commands.listUpdate(id, name, forCharacter)),
    remove: (id: number) => run(commands.listDelete(id)),
    addItem: (listId: number, item: NewItem, need: number) => run(commands.listItemAdd(listId, item, need)),
    setNeed: (id: number, need: number) => run(commands.listItemNeed(id, need)),
    removeItem: (id: number) => run(commands.listItemRemove(id)),
  };
}
