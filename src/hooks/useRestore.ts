import { useCallback, useEffect, useRef, useState } from "react";
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
  /** Refused or failed. `corrupt` lists damaged files when that's the reason;
   *  `deletionsChanged` means the folder changed after the confirmed preview. */
  | {
      kind: "error";
      message: string;
      corrupt?: string[];
      gameRunning?: boolean;
      deletionsChanged?: boolean;
    };

/** What the user would be confirming: the same plan means the same files. */
function samePlan(a: RestorePlan, b: RestorePlan): boolean {
  const key = (p: RestorePlan) => JSON.stringify([p.write, p.delete, p.read_only]);
  return key(a) === key(b);
}

/** Preview and run a restore. The restore never runs by itself: `start` is
 *  only called from the user's confirm click, and it sends the deletions the
 *  user saw so the backend refuses if there would be more. */
export function useRestore() {
  const [plan, setPlan] = useState<RestorePlan | null>(null);
  const [planError, setPlanError] = useState<string | null>(null);
  /** A preview is in flight; the plan shown may be about to change. */
  const [loading, setLoading] = useState(false);
  /** A re-preview came back different from what was shown before. */
  const [changed, setChanged] = useState(false);
  const [run, setRun] = useState<RestoreRun>({ kind: "idle" });
  const shown = useRef<RestorePlan | null>(null);

  useEvent(events.restoreProgress, ({ done, total }) =>
    setRun((r) => (r.kind === "running" ? { kind: "running", done, total } : r)),
  );

  /** Fetches a plan. The previous one stays on screen until the new one
   *  arrives, and `changed` is set if they differ. */
  const preview = useCallback(
    async (id: string, selection: RestoreSelection, mode: RestoreMode = "overlay") => {
      setLoading(true);
      try {
        const next = await commands.backupRestorePreview(id, selection, mode);
        if (shown.current && !samePlan(shown.current, next)) setChanged(true);
        shown.current = next;
        setPlan(next);
        setPlanError(null);
      } catch (e) {
        setPlanError(errorText(e));
      } finally {
        setLoading(false);
      }
    },
    [],
  );

  const start = useCallback(
    async (id: string, selection: RestoreSelection, mode: RestoreMode, confirmed: RestorePlan) => {
      setRun({ kind: "running", done: 0, total: 0 });
      try {
        const report = await commands.backupRestore(id, selection, mode, confirmed.delete);
        setRun({ kind: "done", report });
      } catch (e) {
        setRun({
          kind: "error",
          message: errorText(e),
          corrupt: isAppError(e) && e.kind === "BackupCorrupt" ? e.detail.files : undefined,
          gameRunning: isAppError(e) && e.kind === "GameRunning",
          deletionsChanged: isAppError(e) && e.kind === "DeletionsChanged",
        });
      }
    },
    [],
  );

  return { plan, planError, loading, changed, run, preview, start };
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
