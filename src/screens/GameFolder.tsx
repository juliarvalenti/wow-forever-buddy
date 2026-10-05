import { useEffect, useRef, useState } from "react";
import type { Flavor, Install, InstallCandidate } from "@/lib/bindings";
import {
  Button,
  Callout,
  LiveDot,
  Page,
  PageHeader,
  Panel,
  PanelBody,
  PanelHeader,
  Pill,
  PrimaryButton,
  Record,
  StatusDot,
} from "@/components/d";
import { errorText, plural } from "@/lib/format";
import type { useInstall } from "@/hooks/useInstall";

const SOURCE: Record<InstallCandidate["source"], string> = {
  saved: "saved",
  registry: "via registry",
  common_path: "found on disk",
};

/** "the Windows registry, Program Files, and \Games and \World of Warcraft on
 *  drives C, D and E": where detection looked, in a sentence. The full list
 *  goes in the tooltip. */
function lookedInSummary(looked: { source: string; path: string }[]): string {
  const parts: string[] = [];
  if (looked.some((l) => l.source === "registry")) parts.push("the Windows registry");
  const paths = looked.filter((l) => l.source !== "registry").map((l) => l.path);
  if (paths.some((p) => /program files/i.test(p))) parts.push("Program Files");
  const loose = paths.filter((p) => !/program files/i.test(p));
  const drives = [...new Set(loose.map((p) => p.match(/^([A-Za-z]):/)?.[1]?.toUpperCase()).filter(Boolean))];
  if (drives.length > 0) {
    const games = loose.some((p) => /\\games\\/i.test(p)) ? "\\Games and " : "";
    const list = drives.length > 1 ? `${drives.slice(0, -1).join(", ")} and ${drives[drives.length - 1]}` : drives[0];
    parts.push(`${games}\\World of Warcraft on ${drives.length > 1 ? "drives" : "drive"} ${list}`);
  }
  if (parts.length === 0) return paths.join(", ");
  return parts.length > 1 ? `${parts.slice(0, -1).join(", ")}, and ${parts[parts.length - 1]}` : parts[0];
}

/** "7 characters". No realm: Forever's WTF folders don't name one. */
function roster(f: Flavor): string {
  if (f.characters === 0) return "no characters yet";
  return plural(f.characters, "character", "characters");
}

function FlavorLine({ f, active }: { f: Flavor; active: boolean }) {
  const exe = f.exe ? f.exe.split(/[\\/]/).pop() : null;
  return (
    <li className={active ? "active" : undefined}>
      <b>{f.label}</b>
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
    <ul className="d-flavors">
      {install.flavors.map((f) => (
        <FlavorLine key={f.id} f={f} active={f.id === install.active} />
      ))}
    </ul>
  );
}

/** Finding the game: the current folder, detection with sources, the
 *  picker, and where we looked when nothing was found. */
export function GameFolder({ install: inst }: { install: ReturnType<typeof useInstall> }) {
  const { state, report, detecting, detectError, detect, choose, pick } = inst;
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
    setError(null);
    try {
      const path = await pick();
      if (path) await use(path);
    } catch (e) {
      setError(errorText(e));
    }
  };

  // First run: look straight away rather than show an empty screen.
  const autoDetected = useRef(false);
  useEffect(() => {
    if (state.kind === "none" && !autoDetected.current) {
      autoDetected.current = true;
      detect();
    }
  }, [state.kind, detect]);

  const looked = report?.looked_in ?? [];
  // A failed search leaves the same way out as finding nothing: pick by hand.
  const notFound =
    state.kind !== "ok" &&
    ((report != null && report.candidates.length === 0) || (detectError != null && !detecting));
  const looking = detecting || (state.kind === "none" && !report && !detectError);

  // First run: a letter on parchment (onboarding.html), with the actions inside it.
  if (state.kind === "none") {
    return (
      <Page>
        <div className="d-letter-wrap">
          <ol className="d-stepper" aria-label="Setup">
            <li className="on">
              <span>1</span>Find the game
            </li>
            <li>
              <span>2</span>First backup
            </li>
            <li>
              <span>3</span>Companion addon <i>optional</i>
            </li>
          </ol>
          <Record>
            <div className="d-letter">
              <h1>Well met.</h1>
              <p className="d-muted">
                Forever Buddy keeps your WoW: Forever settings backed up, and later keeps a ledger
                of your characters. First, let's find the game.
              </p>

              {looking && !report && (
                <p className="d-letter-status">
                  <LiveDot /> Looking for World of Warcraft…
                </p>
              )}
              {detectError && !detecting && (
                <p className="d-letter-bad">
                  Looking for the game didn't work, but you can still choose the folder yourself.{" "}
                  <span className="d-dim">({detectError})</span>
                </p>
              )}
              {error && <p className="d-letter-bad">{error}</p>}

              {report?.candidates.map((c) => (
                <div key={c.install.root} className="d-found">
                  <div className="d-letter-label">Found World of Warcraft</div>
                  <div className="d-found-path">
                    <span className="d-mono">{c.install.root}</span>
                    <span className="d-grow" />
                    <span className="d-dim">{SOURCE[c.source]}</span>
                  </div>
                  <InstallSummary install={c.install} />
                  <div className="d-letter-acts">
                    <PrimaryButton onClick={() => use(c.install.root, c.install.active)}>
                      Use this folder
                    </PrimaryButton>
                    <Button variant="ghost" onClick={chooseFolder}>
                      Choose another folder…
                    </Button>
                  </div>
                </div>
              ))}

              {notFound && (
                <div className="d-found">
                  <div className="d-letter-head">We couldn't find World of Warcraft</div>
                  <p className="d-muted">That's fine; point us at it and we'll take it from there.</p>
                  {looked.length > 0 && (
                    <p className="d-dim" title={looked.map((l) => l.path).join("\n")}>
                      Looked in: {lookedInSummary(looked)}.
                    </p>
                  )}
                  <div className="d-hints">
                    <div><span className="d-mono">World of Warcraft\</span>the main folder</div>
                    <div><span className="d-mono">_classic_beta_\</span>the Forever folder</div>
                    <div><span className="d-mono">WTF\</span>even just this works</div>
                  </div>
                  <div className="d-letter-acts">
                    <PrimaryButton onClick={chooseFolder}>Choose folder…</PrimaryButton>
                    <Button variant="ghost" onClick={detect} disabled={detecting}>
                      {detecting ? "Looking…" : "Look again"}
                    </Button>
                  </div>
                </div>
              )}

              <p className="d-letter-note">We only read here. Nothing is changed until you ask.</p>
            </div>
          </Record>
          <p className="d-dim d-letter-after">
            Next, Forever Buddy takes a first backup of your settings, before anything else happens.
          </p>
        </div>
      </Page>
    );
  }

  return (
    <Page>
      <PageHeader
        title="Game folder"
        lede="We only read here. Nothing is changed until you ask."
        actions={
          notFound ? (
            <>
              <Button onClick={detect} disabled={detecting}>
                {detecting ? "Looking…" : "Look again"}
              </Button>
              <PrimaryButton onClick={chooseFolder}>Choose folder…</PrimaryButton>
            </>
          ) : (
            <>
              <Button onClick={chooseFolder}>Choose another folder…</Button>
              <PrimaryButton onClick={detect} disabled={detecting}>
                {detecting ? "Looking…" : "Look for the game"}
              </PrimaryButton>
            </>
          )
        }
      />

      {looking && !report && (
        <Panel>
          <PanelBody>
            <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
              <LiveDot />
              <span>Looking for World of Warcraft…</span>
            </div>
          </PanelBody>
        </Panel>
      )}

      {detectError && !detecting && (
        <Callout tone="bad">
          <span>
            <b>Looking for the game didn't work.</b> You can still choose the folder yourself.{" "}
            <span className="d-dim">({detectError})</span>
          </span>
        </Callout>
      )}
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
