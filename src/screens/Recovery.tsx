import { useEffect, useState } from "react";
import { commands, type RecoveryStatus } from "@/lib/bindings";
import { Button, Callout, Dialog, PrimaryButton } from "@/components/d";
import { when } from "@/lib/format";
import type { RecoveryAction, RecoveryPreview, useRecovery } from "@/hooks/useRestore";
import { PlanDetails } from "@/screens/PlanDetails";

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
  const { status, busy, error, confirming, preview, resolve } = recovery;
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

  if (confirming) return <ConfirmRecovery recovery={recovery} preview={confirming} />;

  const { journal } = status;
  return (
    <Dialog
      title="Your last restore didn't finish"
      onClose={busy ? undefined : onLater}
      footer={
        <>
          <span className="d-dim" style={{ fontSize: 11.5 }}>
            Either way, the safety copy stays in Backups.
          </span>
          <span className="d-grow" />
          <Button variant="ghost" onClick={() => resolve("discard")} disabled={busy} title={LEAVE_WHY}>
            Leave files as they are
          </Button>
          <Button variant="ghost" onClick={onLater} disabled={busy}>
            Decide later
          </Button>
        </>
      }
    >
      <p>
        The app closed while restoring. Some files may be from the backup and some from before.
        Pick one way to make them consistent again.
      </p>
      <dl className="d-facts">
        <dt>Restoring</dt>
        <dd>{journal.summary}</dd>
        <dt>Started</dt>
        <dd>{when(journal.started_at)}</dd>
        {takenAt && (
          <>
            <dt>Safety copy</dt>
            <dd>Taken {when(takenAt)}, before anything changed</dd>
          </>
        )}
      </dl>
      <div className="d-choices">
        <div className="pick">
          <div className="t">
            Roll back <span className="d-pill manual">Recommended</span>
          </div>
          <div className="d">Put the files back exactly as they were before the restore, using the safety copy.</div>
          <PrimaryButton onClick={() => preview("roll_back")} disabled={busy}>
            Roll back…
          </PrimaryButton>
        </div>
        <div>
          <div className="t">Finish restore</div>
          <div className="d">Write the remaining files from the snapshot, as you originally asked.</div>
          <Button onClick={() => preview("finish")} disabled={busy}>
            Finish restore…
          </Button>
        </div>
      </div>
      {error && <Callout tone="bad">{error}</Callout>}
    </Dialog>
  );
}

const RECOVERY_COPY: Record<
  RecoveryAction,
  { title: string; button: string; removedBecause: (one: boolean) => string }
> = {
  roll_back: {
    title: "Roll back your last restore?",
    button: "Roll back",
    removedBecause: (one) => (one ? "your restore added it" : "your restore added them"),
  },
  finish: {
    title: "Finish your last restore?",
    button: "Finish restore",
    removedBecause: (one) => (one ? "it isn't in that snapshot" : "they aren't in that snapshot"),
  },
};

/** Roll back and finish change files, so they get the same confirm step as a
 *  restore: every file to remove is listed, and the backend refuses to
 *  remove anything not on this list. */
function ConfirmRecovery({
  recovery,
  preview,
}: {
  recovery: ReturnType<typeof useRecovery>;
  preview: RecoveryPreview;
}) {
  const { busy, error, cancel, resolve } = recovery;
  const { action, plan } = preview;
  const copy = RECOVERY_COPY[action];
  // Unlike a new restore, an empty plan still goes ahead: it clears the
  // notice once there's nothing left to change.
  const blocked = !plan || plan.read_only.length > 0;
  const empty = plan != null && plan.write_count + plan.delete.length === 0;
  return (
    <Dialog
      title={copy.title}
      onClose={busy ? undefined : cancel}
      footer={
        <>
          <span className="d-grow" />
          <Button variant="ghost" onClick={cancel} disabled={busy}>
            Back
          </Button>
          <PrimaryButton onClick={() => plan && resolve(action, plan)} disabled={busy || blocked}>
            {busy ? "Working…" : copy.button}
          </PrimaryButton>
        </>
      }
    >
      {preview.changed && (
        <Callout tone="bad">
          Stopped before changing anything. More files would be removed than you confirmed.
        </Callout>
      )}
      {preview.error && <Callout tone="bad">{preview.error}</Callout>}
      {error && <Callout tone="bad">{error}</Callout>}
      {!plan && !preview.error && <p className="d-muted">Working out what changes…</p>}
      {plan && empty && <p>Nothing left to change. This clears the notice.</p>}
      {plan && !empty && <PlanDetails plan={plan} removedBecause={copy.removedBecause} />}
      {plan && !empty && (
        <p className="d-muted">
          A safety snapshot of the current files is taken before anything changes.
        </p>
      )}
      {preview.changed && (
        <p style={{ color: "var(--ember-2)" }}>The list changed. Please check it again.</p>
      )}
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
