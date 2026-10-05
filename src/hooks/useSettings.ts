import { useCallback, useEffect, useState } from "react";
import {
  commands,
  type AppInfo,
  type IntegrationId,
  type SecretStatus,
  type Settings,
  type SettingsPatch_Deserialize,
} from "@/lib/bindings";
import { errorText } from "@/lib/format";

/** The settings, saved as they change: each change is a patch, so a stale
 *  copy here can't overwrite newer values (spec §8). */
export function useSettings() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    commands.settingsGet().then(setSettings, (e) => setError(errorText(e)));
  }, []);

  /** Applies a patch; the backend's answer is what's shown. Errors (a
   *  backup location inside the game folder, say) are kept for the UI. */
  const update = useCallback(async (patch: SettingsPatch_Deserialize) => {
    setError(null);
    try {
      setSettings(await commands.settingsUpdate(patch));
      return true;
    } catch (e) {
      setError(errorText(e));
      return false;
    }
  }, []);

  return { settings, error, update };
}

/** Which integration keys are saved in the OS credential store. The values
 *  never come back to the UI; only whether each one is set. */
export function useSecrets() {
  const [status, setStatus] = useState<SecretStatus[] | null>(null);
  const refresh = useCallback(() => {
    commands.secretsStatus().then(setStatus, () => setStatus(null));
  }, []);
  useEffect(refresh, [refresh]);

  const set = useCallback(
    async (id: IntegrationId, value: string) => {
      await commands.secretsSet(id, value);
      refresh();
    },
    [refresh],
  );
  const remove = useCallback(
    async (id: IntegrationId) => {
      await commands.secretsDelete(id);
      refresh();
    },
    [refresh],
  );

  const isSet = (id: IntegrationId) => status?.find((s) => s.id === id)?.is_set ?? false;
  const errorOf = (id: IntegrationId) => status?.find((s) => s.id === id)?.error ?? null;
  return { status, isSet, errorOf, set, remove };
}

/** The app's own folders (for the default backup location). */
export function useAppInfo() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  useEffect(() => {
    commands.appInfo().then(setInfo, () => setInfo(null));
  }, []);
  return info;
}
