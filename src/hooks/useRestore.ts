import { useCallback, useEffect, useState } from "react";
import {
  commands,
  events,
  type JournalAction,
  type RecoveryStatus,
  type RestoreMode,
  type RestorePlan,
  type RestoreReport,
  type RestoreSelection,
} from "@/lib/bindings";
import { errorText, isAppError } from "@/lib/format";
import { useEvent } from "./useEvent";

export type RestoreRun =
  | { kind: "idle" }
  | { kind: "running"; done: number; total: number }
  | { kind: "done"; report: RestoreReport }
  /** Refused or failed; `corrupt` lists damaged files when that's the reason. */
  | { kind: "error"; message: string; corrupt?: string[]; gameRunning?: boolean };

/** Preview and run a restore. The restore never runs by itself: `run` is
 *  only called from the user's confirm click. */
export function useRestore() {
  const [plan, setPlan] = useState<RestorePlan | null>(null);
  const [planError, setPlanError] = useState<string | null>(null);
  const [run, setRun] = useState<RestoreRun>({ kind: "idle" });

  useEvent(events.restoreProgress, ({ done, total }) =>
    setRun((r) => (r.kind === "running" ? { kind: "running", done, total } : r)),
  );

  const preview = useCallback(
    async (id: string, selection: RestoreSelection, mode: RestoreMode = "overlay") => {
      setPlan(null);
      setPlanError(null);
      try {
        setPlan(await commands.backupRestorePreview(id, selection, mode));
      } catch (e) {
        setPlanError(errorText(e));
      }
    },
    [],
  );

  const start = useCallback(
    async (id: string, selection: RestoreSelection, mode: RestoreMode = "overlay") => {
      setRun({ kind: "running", done: 0, total: 0 });
      try {
        setRun({ kind: "done", report: await commands.backupRestore(id, selection, mode) });
      } catch (e) {
        setRun({
          kind: "error",
          message: errorText(e),
          corrupt: isAppError(e) && e.kind === "BackupCorrupt" ? e.detail.files : undefined,
          gameRunning: isAppError(e) && e.kind === "GameRunning",
        });
      }
    },
    [],
  );

  const reset = useCallback(() => {
    setPlan(null);
    setPlanError(null);
    setRun({ kind: "idle" });
  }, []);

  return { plan, planError, run, preview, start, reset };
}

/** Interrupted-restore state. While anything but `none`, restores stay locked. */
export function useRecovery() {
  const [status, setStatus] = useState<RecoveryStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    commands.restoreJournalStatus().then(setStatus, () => setStatus(null));
  }, []);
  useEffect(refresh, [refresh]);
  useEvent(events.restoreCompleted, refresh);

  const resolve = useCallback(
    async (action: JournalAction) => {
      setBusy(true);
      setError(null);
      try {
        await commands.restoreJournalResolve(action);
      } catch (e) {
        setError(errorText(e));
      } finally {
        setBusy(false);
        refresh();
      }
    },
    [refresh],
  );

  const pending = status != null && status.kind !== "none";
  return { status, pending, busy, error, resolve, refresh };
}
