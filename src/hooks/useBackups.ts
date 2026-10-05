import { useCallback, useEffect, useState } from "react";
import {
  commands,
  events,
  type AutoBackupFailure,
  type SnapshotDetail,
  type SnapshotSummary,
  type StorageInfo,
} from "@/lib/bindings";
import { errorText } from "@/lib/format";
import { useEvent } from "./useEvent";

/** The snapshot list, the storage meter and retention sentence, "Back up
 *  now" with progress, the last failure, and the latest automatic backup
 *  failure (kept by the backend until an automatic backup succeeds). */
export function useBackups() {
  const [list, setList] = useState<SnapshotSummary[] | null>(null);
  const [storage, setStorage] = useState<StorageInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [autoFailed, setAutoFailed] = useState<AutoBackupFailure | null>(null);

  const refresh = useCallback(() => {
    commands.backupList().then(
      (l) => {
        setList(l);
        setError(null);
      },
      (e) => setError(errorText(e)),
    );
    // Pruning runs after automatic backups, so this changes with the list.
    commands.backupStorage().then(setStorage, () => setStorage(null));
    // Asked, not only heard: a failure can happen before this screen listens.
    commands.backupAutoStatus().then(setAutoFailed, () => setAutoFailed(null));
  }, []);

  useEffect(refresh, [refresh]);
  useEvent(events.backupCreated, refresh);
  useEvent(events.backupFailed, setAutoFailed);
  useEvent(events.restoreCompleted, refresh); // a restore adds a safety snapshot
  useEvent(events.backupProgress, setProgress);

  const backUpNow = useCallback(
    async (label: string | null = null) => {
      setFailed(null);
      setProgress({ done: 0, total: 0 });
      try {
        await commands.backupCreate(label);
      } catch (e) {
        setFailed(errorText(e));
      } finally {
        setProgress(null);
        refresh();
      }
    },
    [refresh],
  );

  return { list, storage, error, progress, failed, autoFailed, backUpNow, refresh };
}

/** One snapshot grouped by account, character and category, for the panel. */
export function useSnapshot(id: string | null) {
  const [detail, setDetail] = useState<SnapshotDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    setDetail(null);
    setError(null);
    if (!id) return;
    let live = true;
    commands.backupGet(id).then(
      (d) => live && setDetail(d),
      (e) => live && setError(errorText(e)),
    );
    return () => {
      live = false;
    };
  }, [id]);
  return { detail, error };
}
