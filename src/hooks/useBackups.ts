import { useCallback, useEffect, useState } from "react";
import {
  commands,
  events,
  type SnapshotDetail,
  type SnapshotSummary,
} from "@/lib/bindings";
import { errorText } from "@/lib/format";
import { useEvent } from "./useEvent";

/** The snapshot list, "Back up now" with progress, and the last failure. */
export function useBackups() {
  const [list, setList] = useState<SnapshotSummary[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [failed, setFailed] = useState<string | null>(null);

  const refresh = useCallback(() => {
    commands.backupList().then(
      (l) => {
        setList(l);
        setError(null);
      },
      (e) => setError(errorText(e)),
    );
  }, []);

  useEffect(refresh, [refresh]);
  useEvent(events.backupCreated, refresh);
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

  return { list, error, progress, failed, backUpNow, refresh };
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
