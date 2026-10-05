import { useCallback, useEffect, useState } from "react";
import { commands, events, type AddonsList } from "@/lib/bindings";
import { errorText } from "@/lib/format";
import { useEvent } from "./useEvent";

/** The Addons screen's list (F4), read when the screen opens, when the game
 *  folder changes, and when WoW stops (it writes each character's AddOns.txt
 *  as you log out). `list` is null before a game folder is set. */
export function useAddons() {
  const [list, setList] = useState<AddonsList | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const refresh = useCallback(() => {
    commands.addonsList().then(
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
  return { list, error, refresh };
}
