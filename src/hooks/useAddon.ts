import { useCallback, useEffect, useState } from "react";
import { commands, type AddonStatus } from "@/lib/bindings";
import { errorText } from "@/lib/format";

/** The ForeverBuddy addon in the active game folder (V4): its status, and
 *  install/update through the backend's write gate. `status` is null until
 *  known, or when there's no usable game folder. */
export function useAddon() {
  const [status, setStatus] = useState<AddonStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    commands.addonStatus().then(setStatus, () => setStatus(null));
  }, []);
  useEffect(refresh, [refresh]);

  const install = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await commands.addonInstall());
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }, []);

  return { status, busy, error, install, refresh };
}
