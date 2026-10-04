import { useState } from "react";
import type { Flavor, Install, InstallCandidate } from "@/lib/bindings";
import {
  Button,
  Callout,
  Page,
  PageHeader,
  Panel,
  PanelBody,
  PanelHeader,
  Pill,
  PrimaryButton,
  StatusDot,
} from "@/components/d";
import { errorText, plural } from "@/lib/format";
import type { useInstall } from "@/hooks/useInstall";

const SOURCE: Record<InstallCandidate["source"], string> = {
  saved: "saved",
  registry: "via registry",
  common_path: "found on disk",
};

/** "7 characters on Ashenvale +2 realms" (count first). */
function roster(f: Flavor): string {
  if (f.characters === 0) return "no characters yet";
  const [first, ...rest] = f.realms;
  const more = rest.length > 0 ? ` +${plural(rest.length, "realm", "realms")}` : "";
  return `${plural(f.characters, "character", "characters")} on ${first}${more}`;
}

function FlavorLine({ f, active }: { f: Flavor; active: boolean }) {
  const exe = f.exe ? f.exe.split(/[\\/]/).pop() : null;
  return (
    <li style={{ display: "flex", gap: 8, alignItems: "baseline", padding: "4px 0" }}>
      <b style={{ color: active ? "var(--chalk-hi)" : undefined }}>{f.label}</b>
      <span className="d-mono d-dim">{f.id}</span>
      <span className="d-muted">
        {[f.version, exe ?? "settings only", roster(f)].filter(Boolean).join(" · ")}
      </span>
      {f.is_forever && <Pill kind="ok">Recommended</Pill>}
      {f.links.map((l) => (
        <span key={l.folder} className="d-dim" title={l.target}>
          {l.folder} is linked
        </span>
      ))}
    </li>
  );
}

function InstallSummary({ install }: { install: Install }) {
  return (
    <ul>
      {install.flavors.map((f) => (
        <FlavorLine key={f.id} f={f} active={f.id === install.active} />
      ))}
    </ul>
  );
}

/** Finding the game: the current folder, detection with sources, the
 *  picker, and where we looked when nothing was found. */
export function GameFolder({ install: inst }: { install: ReturnType<typeof useInstall> }) {
  const { state, report, detecting, detect, choose, pick } = inst;
  const [error, setError] = useState<string | null>(null);
  const [picked, setPicked] = useState<{ path: string; root: string } | null>(null);

  const use = async (path: string, flavor: string | null = null) => {
    setError(null);
    try {
      const install = await choose(path, flavor);
      setPicked({ path, root: install.root });
    } catch (e) {
      setError(errorText(e));
    }
  };

  const chooseFolder = async () => {
    const path = await pick();
    if (path) await use(path);
  };

  const looked = report?.looked_in ?? [];
  return (
    <Page>
      <PageHeader
        title="Game folder"
        lede="We only read here. Nothing is changed until you ask."
        actions={
          <>
            <Button onClick={chooseFolder}>Choose another folder…</Button>
            <PrimaryButton onClick={detect} disabled={detecting}>
              {detecting ? "Looking…" : "Look for the game"}
            </PrimaryButton>
          </>
        }
      />

      {error && <Callout tone="bad">{error}</Callout>}
      {state.kind === "invalid" && (
        <Callout tone="ember">
          <span>
            <b>The saved game folder needs checking.</b> {state.error}
          </span>
        </Callout>
      )}

      {state.kind === "ok" && (
        <Panel>
          <PanelHeader title="In use">
            <StatusDot />
          </PanelHeader>
          <PanelBody>
            <p className="d-mono">{state.install.root}</p>
            {picked && picked.path !== picked.root && (
              <p className="d-muted">
                You picked {picked.path}, so we use the game folder above it.
              </p>
            )}
            <InstallSummary install={state.install} />
          </PanelBody>
        </Panel>
      )}

      {report && report.candidates.length > 0 && (
        <Panel>
          <PanelHeader title="Found" />
          <PanelBody>
            {report.candidates.map((c) => (
              <div key={c.install.root} style={{ padding: "6px 0" }}>
                <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                  <span className="d-mono">{c.install.root}</span>
                  <span className="d-dim">{SOURCE[c.source]}</span>
                  <span className="d-grow" />
                  <Button onClick={() => use(c.install.root, c.install.active)}>
                    Use this folder
                  </Button>
                </div>
                <InstallSummary install={c.install} />
              </div>
            ))}
          </PanelBody>
        </Panel>
      )}

      {report && report.candidates.length === 0 && (
        <Panel>
          <PanelHeader title="We couldn't find World of Warcraft" />
          <PanelBody>
            <p>That's fine; point us at it and we'll take it from there.</p>
            <p className="d-muted">
              Looked in:{" "}
              {looked
                .slice(0, 4)
                .map((l) => l.path)
                .join(", ")}
              {looked.length > 4 && ` and ${looked.length - 4} more`}.
            </p>
            <p className="d-muted">
              Any of these works: the World of Warcraft folder, the _classic_beta_ folder, or even
              just its WTF folder.
            </p>
          </PanelBody>
        </Panel>
      )}
    </Page>
  );
}
