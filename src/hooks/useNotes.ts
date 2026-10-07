import { useCallback, useEffect, useState } from "react";
import { commands, events, type LoginNote, type NewNote } from "@/lib/bindings";
import { useEvent } from "@/hooks/useEvent";
import { errorText } from "@/lib/format";

/** Login notes (B1): waiting ones, and ones shown in the last week. An
 *  ingest can archive a once note (the game showed it), so it reloads then. */
export function useNotes() {
  const [notes, setNotes] = useState<LoginNote[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(() => {
    commands.notesList().then(setNotes, (e) => setError(errorText(e)));
  }, []);
  useEffect(load, [load]);
  useEvent(events.ingestCompleted, load);

  const add = useCallback(
    async (note: NewNote) => {
      setError(null);
      try {
        await commands.notesAdd(note);
        load();
        return true;
      } catch (e) {
        setError(errorText(e));
        return false;
      }
    },
    [load],
  );
  const remove = useCallback(
    async (id: number) => {
      setError(null);
      try {
        await commands.notesDelete(id);
        load();
      } catch (e) {
        setError(errorText(e));
      }
    },
    [load],
  );
  return { notes, error, add, remove };
}
