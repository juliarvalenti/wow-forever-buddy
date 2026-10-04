import { useEffect, useState } from "react";
import { commands, type RecoveryStatus } from "@/lib/bindings";
import { Button, Callout, Dialog, PrimaryButton } from "@/components/d";
import { when } from "@/lib/format";
import type { useRecovery } from "@/hooks/useRestore";

/** When snapshot `id` was taken, or null until known (or if it's gone). */
function useTakenAt(id: string | null | undefined): string | null {
  const [at, setAt] = useState<string | null>(null);
  useEffect(() => {
    if (!id) return;
    commands.backupList().then(
      (list) => setAt(list.find((s) => s.id === id)?.created_at ?? null),
      () => setAt(null),
    );
  }, [id]);
  return at;
}

const LEAVE_WHY =
  "Nothing changes: files stay as they are now, part restored. The safety copy stays in Backups.";

/** Startup dialog for an interrupted restore (dashboard.html?recover and
 *  ?recover=unreadable). "Decide later" closes it; restores stay locked and a
 *  banner remains until it's resolved. */
export function RecoveryDialog({
  recovery,
  onLater,
  onOpenSafety,
}: {
  recovery: ReturnType<typeof useRecovery>;
  onLater: () => void;
  /** Opens that safety snapshot, or the Safety list when null. */
  onOpenSafety: (id: string | null) => void;
}) {
  const { status, busy, error, resolve } = recovery;
  const takenAt = useTakenAt(status?.kind === "pending" ? status.journal.original_pre_restore : null);
  if (!status || status.kind === "none") return null;

  if (status.kind === "unreadable") {
    const safety = status.latest_safety;
    return (
      <Dialog
        title="Your last restore didn't finish"
        onClose={onLater}
        footer={
          <>
            <span className="d-grow" />
            <Button variant="ghost" onClick={() => resolve("discard")} disabled={busy}>
              Clear notice
            </Button>
            <PrimaryButton onClick={() => onOpenSafety(safety)}>
              {safety ? "Open the safety copy" : "Open Backups"}
            </PrimaryButton>
          </>
        }
      >
        <p>
          We can't read its record, so it can't be rolled back or finished automatically.{" "}
          {safety
            ? "Your files from before the restore are in a safety copy in Backups."
            : "Check Backups for a safety copy from around then."}
        </p>
        <p className="d-muted">Restores stay locked until you clear this notice.</p>
        {error && <Callout tone="bad">{error}</Callout>}
      </Dialog>
    );
  }

  const { journal } = status;
  return (
    <Dialog
      title="Your last restore didn't finish"
      onClose={busy ? undefined : onLater}
      footer={
        <>
          <Button variant="ghost" onClick={onLater} disabled={busy}>
            Decide later
          </Button>
          <Button variant="ghost" onClick={() => resolve("discard")} disabled={busy} title={LEAVE_WHY}>
            Leave files as they are
          </Button>
          <span className="d-grow" />
          <Button onClick={() => resolve("finish")} disabled={busy}>
            Finish restore
          </Button>
          <PrimaryButton onClick={() => resolve("roll_back")} disabled={busy}>
            {busy ? "Working…" : "Roll back"}
          </PrimaryButton>
        </>
      }
    >
      <p>
        Restoring <b>{journal.summary}</b> stopped partway ({when(journal.started_at)}).
      </p>
      <p>
        <b>Roll back</b> (recommended) puts back exactly what was there before. <b>Finish restore</b>{" "}
        completes it.
      </p>
      {takenAt && <p className="d-muted">Safety copy: taken {when(takenAt)}</p>}
      <p className="d-dim">Either way, the safety copy stays in Backups.</p>
      {error && <Callout tone="bad">{error}</Callout>}
    </Dialog>
  );
}

/** The persistent notice after "Decide later". */
export function RecoveryBanner({
  status,
  onReview,
}: {
  status: RecoveryStatus | null;
  onReview: () => void;
}) {
  if (!status || status.kind === "none") return null;
  return (
    <Callout tone="stone">
      <span>
        <b>Your last restore didn't finish.</b> Restores are locked until you roll it back or
        finish it. Backups keep running.
      </span>
      <span className="d-grow" />
      <Button onClick={onReview}>Review</Button>
    </Callout>
  );
}
