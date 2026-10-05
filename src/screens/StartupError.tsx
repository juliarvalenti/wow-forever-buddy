import { useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { commands, type StartupFailure } from "@/lib/bindings";
import { Check } from "lucide-react";
import { Button, Panel, PanelBody, PanelHeader, PrimaryButton } from "@/components/d";
import { errorText } from "@/lib/format";

const TITLES: Record<StartupFailure["problem"], string> = {
  database: "Your data is from a newer version of Forever Buddy",
  settings: "Your settings file can't be read",
  other: "Forever Buddy couldn't open its data",
};

const WHY: Record<StartupFailure["problem"], string> = {
  database:
    "This usually means a newer version ran on this PC and this one is older. Install the newer version again to keep going.",
  settings:
    "Something changed settings.json so it can't be read. Fix or remove it, and Forever Buddy will start with default settings.",
  other: "Something stopped Forever Buddy from opening its own files.",
};

/** Shown instead of the app when its own data can't be opened at startup.
 *  No sidebar, one panel (design/mocks/round-3/startup-error.html). */
export function StartupError({ failure }: { failure: StartupFailure }) {
  const [note, setNote] = useState<string | null>(null);
  const { paths, at_fault } = failure;
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
              <p style={{ fontSize: 13.5, lineHeight: 1.55 }}>{WHY[failure.problem]}</p>
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
              <div style={{ display: "flex", gap: 8 }}>
                <PrimaryButton onClick={openFolder}>Open data folder</PrimaryButton>
                <Button onClick={copy}>Copy error details</Button>
                <Button variant="ghost" onClick={() => getCurrentWindow().close()}>
                  Quit
                </Button>
              </div>
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
    </div>
  );
}
