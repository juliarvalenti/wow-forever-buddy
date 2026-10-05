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

/** Fired when a restore fails. A restore that fails partway leaves its
 *  journal pending without a `restore-completed` event, so `useRecovery`
 *  re-checks on this to show the recovery dialog straight away. */
const RESTORE_FAILED = "forever-buddy:restore-failed";

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
        window.dispatchEvent(new Event(RESTORE_FAILED));
      }
    },
    [],
  );

  /** Back to a fresh preview, e.g. to restore from another snapshot after
   *  this one turned out damaged. Nothing the user saw carries over. */
  const reset = useCallback(() => {
    shown.current = null;
    setPlan(null);
    setPlanError(null);
    setChanged(false);
    setRun({ kind: "idle" });
  }, []);

  return { plan, planError, loading, changed, run, preview, start, reset };
}

/** Roll back or finish: the recovery actions that change files. */
export type RecoveryAction = Exclude<JournalAction, "discard">;

/** What the recovery dialog is confirming: the action and its plan. */
export type RecoveryPreview = {
  action: RecoveryAction;
  plan: RestorePlan | null;
  error: string | null;
  /** The backend refused because the plan grew; this is the new one. */
  changed: boolean;
};

/** Interrupted-restore state. While anything but `none`, restores stay locked.
 *  Roll back and finish are previewed and confirmed like a normal restore:
 *  `preview(action)`, then `resolve(action, plan)` sends that plan's
 *  deletions, and the backend refuses if it would remove anything else. */
export function useRecovery() {
  const [status, setStatus] = useState<RecoveryStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirming, setConfirming] = useState<RecoveryPreview | null>(null);

  const refresh = useCallback(() => {
    commands.restoreJournalStatus().then(setStatus, () => setStatus(null));
  }, []);
  useEffect(refresh, [refresh]);
  useEvent(events.restoreCompleted, refresh);
  useEffect(() => {
    window.addEventListener(RESTORE_FAILED, refresh);
    return () => window.removeEventListener(RESTORE_FAILED, refresh);
  }, [refresh]);

  const preview = useCallback(async (action: RecoveryAction, changed = false) => {
    setError(null);
    setConfirming({ action, plan: null, error: null, changed });
    try {
      const plan = await commands.restoreJournalPreview(action);
      setConfirming({ action, plan, error: null, changed });
    } catch (e) {
      setConfirming({ action, plan: null, error: errorText(e), changed });
    }
  }, []);

  /** Back from the confirm step to the choice. */
  const cancel = useCallback(() => setConfirming(null), []);

  /** Discard needs no plan; roll back and finish need the one confirmed. */
  const resolve = useCallback(
    async (action: JournalAction, confirmed?: RestorePlan) => {
      setBusy(true);
      setError(null);
      try {
        await commands.restoreJournalResolve(action, confirmed?.delete ?? []);
        setConfirming(null);
      } catch (e) {
        if (action !== "discard" && isAppError(e) && e.kind === "DeletionsChanged") {
          // Nothing changed; show the new list to confirm again.
          await preview(action, true);
        } else {
          setError(errorText(e));
        }
      } finally {
        setBusy(false);
        refresh();
      }
    },
    [refresh, preview],
  );

  const pending = status != null && status.kind !== "none";
  return { status, pending, busy, error, confirming, preview, cancel, resolve, refresh };
}
