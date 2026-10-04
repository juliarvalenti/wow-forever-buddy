import type { RecoveryStatus } from "@/lib/bindings";
import { Button, Callout, Dialog, PrimaryButton } from "@/components/d";
import { when } from "@/lib/format";
import type { useRecovery } from "@/hooks/useRestore";

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
  onOpenSafety: (id: string) => void;
}) {
  const { status, busy, error, resolve } = recovery;
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
            {safety && (
              <PrimaryButton onClick={() => onOpenSafety(safety)}>Open the safety copy</PrimaryButton>
            )}
          </>
        }
      >
        <p>
          We can't read its record, so it can't be rolled back or finished automatically. Your
          files from before the restore are in a safety copy in Backups.
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
        Restoring <b>{journal.summary}</b> stopped partway on {when(journal.started_at)}.
      </p>
      <p>
        <b>Roll back</b> (recommended) puts back exactly what was there before. <b>Finish restore</b>{" "}
        completes it.
      </p>
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
