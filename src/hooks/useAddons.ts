import { useCallback, useEffect, useState } from "react";
import { commands, events, type AddonsList, type CharacterKey, type ToggleResult } from "@/lib/bindings";
import { errorText } from "@/lib/format";
import { useEvent } from "./useEvent";

/** The Addons screen's list (F4), read when the screen opens, when the game
 *  folder changes, and when WoW stops (it writes each character's AddOns.txt
 *  as you log out). `list` is null before a game folder is set.
 *
 *  F6: `setEnabled` turns an addon on or off for some characters (through
 *  the backend's write gate), and `undo` puts the AddOns.txt files back from
 *  that change's safety snapshot. Both re-read the list after. */
export function useAddons() {
  const [list, setList] = useState<AddonsList | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
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

  /** Throws the backend's error text, for the caller to show. */
  const setEnabled = useCallback(
    async (addon: string, characters: CharacterKey[], enabled: boolean): Promise<ToggleResult> => {
      setBusy(true);
      try {
        return await commands.addonsSetEnabled(addon, characters, enabled);
      } catch (e) {
        throw new Error(errorText(e));
      } finally {
        setBusy(false);
        refresh();
      }
    },
    [refresh],
  );

  const undo = useCallback(
    async (snapshotId: string) => {
      setBusy(true);
      try {
        await commands.addonsUndo(snapshotId);
      } catch (e) {
        throw new Error(errorText(e));
      } finally {
        setBusy(false);
        refresh();
      }
    },
    [refresh],
  );

  return { list, error, busy, refresh, setEnabled, undo };
}
