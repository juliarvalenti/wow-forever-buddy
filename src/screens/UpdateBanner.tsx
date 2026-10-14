import { Check, Sparkles, TriangleAlert } from "lucide-react";
import { commands, type UpdateStatus } from "@/lib/bindings";
import { Button, LiveDot, LockedAction, Meter } from "@/components/d";

// Copy and states from IMPLEMENTING §23 (dashboard.html?update=…).

const openPage = () => void commands.updateOpenReleasePage();

function WhatsNew() {
  return (
    <button className="link" onClick={openPage}>
      What's new
    </button>
  );
}

function Hide({ onClick }: { onClick: () => void }) {
  return (
    <button className="x" title="Hide until the next version" aria-label="Hide until the next version" onClick={onClick}>
      ×
    </button>
  );
}

/** The slim line above every screen when a newer version is out. `dismissed`
 *  is the version the × last hid; `later` hides "ready" for this session. */
export function UpdateBanner({
  status,
  dismissed,
  onDismiss,
  later,
  onLater,
}: {
  status: UpdateStatus | null;
  dismissed: string | null;
  onDismiss: (version: string) => void;
  later: boolean;
  onLater: () => void;
}) {
  const phase = status?.phase;
  if (!status || !phase || phase.kind === "none") return null;
  const v = phase.version;
  const hide = () => onDismiss(v);
  const strip = (children: React.ReactNode, role: "status" | "alert" = "status") => (
    <div className="d-callout stone d-upd" role={role}>
      {children}
    </div>
  );

  switch (phase.kind) {
    case "available":
      if (dismissed === v) return null;
      return strip(
        <>
          <Sparkles size={14} aria-hidden style={{ color: "var(--ember-2)" }} />
          <span className="d-grow">
            <b>Forever Buddy {v} is out.</b>{" "}
            {!status.installable && <span className="muted">This copy can't update itself. </span>}
            <WhatsNew />
          </span>
          {status.installable ? (
            <Button onClick={() => void commands.updateDownload()}>Update now</Button>
          ) : (
            <Button onClick={openPage}>Download from GitHub</Button>
          )}
          <Hide onClick={hide} />
        </>,
      );
    case "downloading": {
      const pct = phase.percent ?? 0;
      return strip(
        <>
          <LiveDot />
          <span className="d-grow">
            <b>Downloading {v}…</b> <span className="muted">You can keep using the app.</span>
          </span>
          <span className="bar">
            <Meter fraction={pct / 100} />
          </span>
          <span className="pct">{pct}%</span>
        </>,
      );
    }
    case "ready":
      if (later) return null;
      return strip(
        <>
          <Check size={14} aria-hidden style={{ color: "var(--ok)" }} />
          <span className="d-grow">
            <b>{v} is ready.</b> Restart Forever Buddy to finish. It takes a few seconds.
          </span>
          <Button variant="ghost" onClick={onLater}>
            Later
          </Button>
          {phase.waiting ? (
            <LockedAction why="Restarts after the backup finishes">Restart now</LockedAction>
          ) : (
            <Button onClick={() => void commands.updateRestart()}>Restart now</Button>
          )}
        </>,
      );
    case "failed":
      if (dismissed === v) return null;
      return strip(
        <>
          <TriangleAlert size={14} aria-hidden style={{ color: "var(--bad)" }} />
          <span className="d-grow">
            <b>The update didn't install.</b>{" "}
            <span className="muted">
              {phase.reason === "signature"
                ? "The download didn't pass its signature check, so nothing changed."
                : phase.reason === "offline"
                  ? "Couldn't reach GitHub. You're offline or it's down."
                  : "Nothing changed. Try again, or download it from GitHub."}
            </span>
          </span>
          <Button variant="ghost" onClick={openPage}>
            Download from GitHub
          </Button>
          <Button onClick={() => void commands.updateDownload()}>Try again</Button>
          <Hide onClick={hide} />
        </>,
        "alert",
      );
  }
}

/** Once, on the first start after an update. */
export function UpdatedBanner({ version }: { version: string }) {
  return (
    <div className="d-callout stone d-upd ok" role="status">
      <Check size={14} aria-hidden style={{ color: "var(--ok)" }} />
      <span className="d-grow">
        <b>Updated to {version}.</b> <WhatsNew />
      </span>
    </div>
  );
}
