import { useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { commands, type CopyFailure, type StartupFailure } from "@/lib/bindings";
import { Check } from "lucide-react";
import { Button, Dialog, Meter, Panel, PanelBody, PanelHeader, PrimaryButton } from "@/components/d";
import { bytes, errorText } from "@/lib/format";

const TITLES: Record<StartupFailure["problem"], string> = {
  database: "Your data is from a newer version of Forever Buddy",
  upgrade_copy: "Forever Buddy couldn't make a safety copy before updating",
  settings: "Your settings file can't be read",
  other: "Forever Buddy couldn't open its data",
};

const WHY: Record<StartupFailure["problem"], string> = {
  database:
    "This usually means a newer version ran on this PC and this one is older. Install the newer version again to keep going.",
  // Built from the reason instead (see copyWhy).
  upgrade_copy: "",
  settings:
    "Something changed settings.json so it can't be read. Fix or remove it, and Forever Buddy will start with default settings.",
  other: "Something stopped Forever Buddy from opening its own files.",
};

/** "The copy didn't work because …", in plain words (IMPLEMENTING.md §6). */
function copyWhy(why: CopyFailure | null): string {
  const lead = "This version needs to update your data, and it always copies it first.";
  let because: string;
  if (why?.kind === "disk_full") {
    because =
      why.free_bytes != null
        ? `the drive is full: about ${bytes(why.needed_bytes)} is needed and ${bytes(why.free_bytes)} is free. Nothing was changed. Free up some space and try again.`
        : `the drive is full: about ${bytes(why.needed_bytes)} is needed. Nothing was changed. Free up some space and try again.`;
  } else if (why?.kind === "locked") {
    because =
      "another program is using the file, often antivirus or a sync app. Nothing was changed. Trying again usually works.";
  } else {
    because = "of an unexpected error (see Error details). Nothing was changed.";
  }
  return `${lead} The copy didn't work because ${because}`;
}

/** "4 Oct" from YYYY-MM-DD, for the confirm's fallback line. */
function day(iso: string): string {
  return new Date(`${iso}T00:00:00`).toLocaleDateString(undefined, {
    day: "numeric",
    month: "short",
  });
}

/** Shown instead of the app when its own data can't be opened at startup.
 *  No sidebar, one panel (design/mocks/round-3/startup-error.html). */
export function StartupError({ failure: first }: { failure: StartupFailure }) {
  const [failure, setFailure] = useState(first);
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const { paths, at_fault } = failure;
  const copyCase = failure.problem === "upgrade_copy";
  // A full drive shows free against needed, with a small meter.
  const why = failure.copy_failure;
  const space =
    why?.kind === "disk_full" && why.free_bytes != null
      ? { free: why.free_bytes, needed: why.needed_bytes ?? 0 }
      : null;

  // Re-runs startup. Started: reload into the app. Still failing: show the
  // new failure. `skip` is the one-shot override; the backend refuses it
  // from any other state and never saves it.
  const retry = async (skip: boolean) => {
    setBusy(true);
    setNote(null);
    try {
      const again = await commands.startupRetry(skip);
      if (again == null) {
        window.location.reload();
        return;
      }
      setFailure(again);
      setNote("It still didn't work.");
    } catch (e) {
      setNote(errorText(e));
    } finally {
      setBusy(false);
      setConfirming(false);
    }
  };
  // Join with the separator the OS uses, so paths read naturally on Windows.
  const sep = paths.local_data_dir.includes("\\") ? "\\" : "/";
  const files = [
    `${paths.local_data_dir}${sep}buddy.db`,
    `${paths.config_dir}${sep}settings.json`,
    `${paths.local_data_dir}${sep}backups`,
  ];
  const norm = (p: string) => p.replace(/\\/g, "/").toLowerCase();
  const isFault = (f: string) => at_fault != null && norm(f) === norm(at_fault);

  const copy = async () => {
    const text = `${TITLES[failure.problem]}\n${failure.message}\n${at_fault ?? ""}`;
    try {
      await navigator.clipboard.writeText(text);
      setNote("Copied.");
    } catch {
      setNote("Couldn't copy. Select the details below instead.");
    }
  };

  const openFolder = () =>
    commands.startupOpenDataFolder().catch((e) => setNote(errorText(e)));

  return (
    <div className="d-center">
      <div style={{ width: "min(620px, 100%)" }}>
        <Panel>
          <PanelHeader title="Something stopped Forever Buddy from starting" />
          <PanelBody>
            <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
              <h1 className="d-display" style={{ fontSize: 26, lineHeight: 1.15, color: "#d8cfc0" }}>
                {TITLES[failure.problem]}
              </h1>
              <p style={{ fontSize: 13.5, lineHeight: 1.55 }}>
                {copyCase ? copyWhy(failure.copy_failure) : WHY[failure.problem]}
              </p>
              {space && (
                <div style={{ display: "flex", flexDirection: "column", gap: 4, maxWidth: 360 }}>
                  <Meter fraction={1 - space.free / Math.max(space.needed, 1)} over />
                  <span className="d-dim">
                    {bytes(space.free)} free of {bytes(space.needed)} needed
                  </span>
                </div>
              )}
              <p style={{ display: "flex", gap: 8, alignItems: "center", color: "var(--ok)" }}>
                <Check size={14} aria-hidden /> Your backups and your game files are untouched.
              </p>
              <ul className="d-files d-mono">
                {files.map((f) => (
                  <li key={f} style={isFault(f) ? { color: "var(--bad)" } : undefined}>
                    {f}
                  </li>
                ))}
              </ul>
              {copyCase ? (
                // Try again first (most causes pass); the way out is quiet and
                // behind a confirm.
                <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                  <PrimaryButton onClick={() => retry(false)} disabled={busy}>
                    {busy ? "Trying…" : "Try again"}
                  </PrimaryButton>
                  <Button onClick={openFolder}>Open data folder</Button>
                  <Button variant="ghost" onClick={() => setConfirming(true)} disabled={busy}>
                    Update without a safety copy…
                  </Button>
                  <Button variant="ghost" onClick={() => getCurrentWindow().close()}>
                    Quit
                  </Button>
                </div>
              ) : (
                <div style={{ display: "flex", gap: 8 }}>
                  <PrimaryButton onClick={openFolder}>Open data folder</PrimaryButton>
                  <Button onClick={copy}>Copy error details</Button>
                  <Button variant="ghost" onClick={() => getCurrentWindow().close()}>
                    Quit
                  </Button>
                </div>
              )}
              {note && <p className="d-muted">{note}</p>}
              <details>
                <summary className="d-muted">Error details</summary>
                <p className="d-mono" style={{ marginTop: 6, whiteSpace: "pre-wrap" }}>
                  {failure.message}
                </p>
              </details>
            </div>
          </PanelBody>
        </Panel>
      </div>
      {confirming && (
        <Dialog
          title="Update without a safety copy?"
          onClose={() => !busy && setConfirming(false)}
          footer={
            <>
              {/* Cancel is the default; the risky action is plain danger
                  text, never the bronze primary. */}
              <Button onClick={() => setConfirming(false)} disabled={busy}>
                Cancel
              </Button>
              <button
                className="d-btn ghost"
                style={{ color: "var(--bad)" }}
                onClick={() => retry(true)}
                disabled={busy}
              >
                {busy ? "Updating…" : "Update anyway"}
              </button>
            </>
          }
        >
          <p>
            If the update fails partway, your gold history and adventures may be lost. Forever Buddy
            would fall back to{" "}
            {failure.fallback_copy ? (
              <>
                the daily copy from <b>{day(failure.fallback_copy)}</b>, or start fresh if there isn't
                one.
              </>
            ) : (
              "starting fresh, since there's no daily copy yet."
            )}
          </p>
          <p>Your game backups and your game files aren't affected.</p>
          <p className="d-muted" style={{ fontSize: 12 }}>
            This only applies to this update. If a later one can't make a copy, you'll be asked again.
          </p>
        </Dialog>
      )}
    </div>
  );
}
