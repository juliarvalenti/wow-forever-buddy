import { useCallback, useEffect, useState } from "react";
import { commands, events, type MacrosList } from "@/lib/bindings";
import { errorText } from "@/lib/format";
import { useEvent } from "./useEvent";

/** The Macros screen's list (F7), read when the screen opens, when the game
 *  folder changes, and when WoW stops (it writes macros-cache.txt as you log
 *  out). `list` is null before a game folder is set. */
export function useMacros() {
  const [list, setList] = useState<MacrosList | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const refresh = useCallback(() => {
    commands.macrosList().then(
      (l) => {
        setList(l);
        setError(null);
      },
      (e) => setError(errorText(e)),
    );
  }, []);
  useEffect(refresh, [refresh]);
  useEvent(events.installChanged, refresh);
  useEvent(events.gameStatusChanged, (s) => {
    if (!s.running) refresh();
  });
  return { list, error };
}
