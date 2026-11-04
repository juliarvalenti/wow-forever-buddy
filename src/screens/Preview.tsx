import { useCallback, useEffect, useState } from "react";
import {
  Button,
  Callout,
  Meter,
  Page,
  PageHeader,
  Panel,
  PanelBody,
  PanelHeader,
  PrimaryButton,
  Segmented,
} from "@/components/d";
import { commands, events, type Moment, type PreviewStatus, type UiDraft } from "@/lib/bindings";
import { useEvent } from "@/hooks/useEvent";
import { useDrafts, usePreview } from "@/hooks/usePreview";
import { bytes, errorText } from "@/lib/format";

// ED1 (was PV1's Preview): Your UI. The player's own screen as the game
// draws it with Forever Buddy and their settings, in a moment they pick, and
// how to ask Claude to change it. No editing here: Claude makes drafts, the
// player tries each on their own screen, and Use this sends one to the game.

const MOMENTS: { value: Moment; label: string }[] = [
  { value: "standing", label: "Standing around" },
  { value: "combat", label: "In combat" },
  { value: "stealth", label: "In stealth" },
  { value: "low_health", label: "Low health" },
  { value: "no_target", label: "No target" },
  { value: "hidden", label: "Values hidden" },
];

const SKILL = "npx skills add juliarvalenti/wow-forever-buddy";

const ASKS = [
  "Stack my health over my target's, in blue.",
  "Make my bars a bit smaller and closer to my character.",
  "Show me two or three takes and I'll pick.",
];

export function Preview() {
  const { status, frame, blank, error, download, moment, show, draft, showDraft } = usePreview();
  const { drafts, reload } = useDrafts();
  const [copied, setCopied] = useState(false);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<string | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const current = drafts.find((d) => d.id === draft) ?? null;

  const pick = (id: number | null) => {
    setDone(null);
    setFailed(null);
    showDraft(id);
  };

  const decide = async (d: UiDraft, use: boolean) => {
    setBusy(true);
    setFailed(null);
    try {
      await commands.approvalsDecide(d.id, use ? "approve" : "decline");
      setDone(
        use ? `"${d.name}" is yours now. You'll see it in game the next time you log in.` : `Threw away "${d.name}".`,
      );
      showDraft(null);
    } catch (e) {
      setFailed(errorText(e));
    } finally {
      setBusy(false);
      reload();
    }
  };

  const copy = () => {
    navigator.clipboard.writeText(SKILL).then(
      () => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      },
      () => setCopied(false),
    );
  };

  const total = status?.download_bytes ?? 0;
  const size = bytes(total);
  const needsData = status?.data === "missing" || status?.data === "failed";
  const ready = status?.data === "ready";
  const label = MOMENTS.find((m) => m.value === moment)?.label ?? "";

  return (
    <Page>
      <PageHeader
        title="Your UI"
        lede="What your screen looks like in WoW: Forever with Forever Buddy. Claude makes the changes; you see them here."
      />
      {error && <Callout tone="bad">{error}</Callout>}

      <div className="yu">
        <div className="yu-main">
          {ready && (
            <div className="yu-tools">
              <span className="d-dim">Show</span>
              <Segmented options={MOMENTS} value={moment} onChange={show} />
            </div>
          )}

          {needsData && status && (
            <Panel>
              <PanelHeader title="One download first" />
              <PanelBody>
                <p className="d">
                  To draw your screen, Forever Buddy needs a small set of files describing the game, {size} from
                  GitHub. Each one is checked against the copy this app was built with, and nothing about you is sent.
                  You can remove them any time in Settings.
                </p>
                {status.data === "failed" && status.data_error && (
                  <div style={{ marginTop: 8 }}>
                    <Callout tone="bad">The download didn't work: {status.data_error}</Callout>
                  </div>
                )}
                <div className="ctl" style={{ marginTop: 10 }}>
                  <PrimaryButton onClick={download}>
                    {status.data === "failed" ? `Try again (${size})` : `Download (${size})`}
                  </PrimaryButton>
                </div>
              </PanelBody>
            </Panel>
          )}

          {status?.data === "downloading" && (
            <Panel>
              <PanelHeader title="Downloading" />
              <PanelBody>
                <Meter fraction={total ? (status.downloaded_bytes ?? 0) / total : 0} />
                <p className="d-dim" style={{ marginTop: 6 }}>
                  {bytes(status.downloaded_bytes ?? 0)} of {size}
                </p>
              </PanelBody>
            </Panel>
          )}

          {ready && status && (
            <>
              <div className="yu-screen">
                {frame && !blank ? (
                  <img
                    src={frame}
                    alt={`Your screen ${label.toLowerCase()}, as the game would draw it`}
                    style={{ opacity: status.drawing ? 0.6 : 1 }}
                  />
                ) : blank && !status.drawing ? (
                  <div className="yu-empty">
                    <h3>This is Blizzard's UI</h3>
                    {current ? (
                      <p>This draft doesn't turn on anything of ours.</p>
                    ) : (
                      <p>
                        Forever Buddy's HUD and action bars are off. Turn them on in game: type /fb, then open HUD
                        and Action bars. Or ask Claude to make you a look. Either way, it shows up here.
                      </p>
                    )}
                  </div>
                ) : (
                  <div className="yu-empty">
                    <p>Drawing your screen…</p>
                  </div>
                )}
                {frame && !blank && (
                  <span className="yu-tag">
                    {current ? current.name : "In game now"} · {label}
                  </span>
                )}
              </div>
              {status.draw_error && (
                <Callout tone="bad">That one didn't draw, so this is the last one that did. {status.draw_error}</Callout>
              )}
              {done && <Callout tone="stone">{done}</Callout>}
              {failed && <Callout tone="bad">{failed}</Callout>}
              {current && (
                <div className="yu-draft">
                  <div className="yu-draft-head">
                    <h3>{current.name}</h3>
                    <div className="ctl">
                      <Button variant="ghost" onClick={() => decide(current, false)} disabled={busy}>
                        Throw away
                      </Button>
                      <PrimaryButton onClick={() => decide(current, true)} disabled={busy}>
                        Use this
                      </PrimaryButton>
                    </div>
                  </div>
                  <p className="d">{current.why}</p>
                  {current.words && <p className="yu-words">You asked: “{current.words}”</p>}
                  <ul className="yu-changes">
                    {current.changes.map((c) => (
                      <li key={c.option}>
                        <span>{c.label}</span>
                        <span className="d-dim">
                          {c.same ? null : (
                            <>
                              <Value v={c.from} /> →{" "}
                            </>
                          )}
                          <Value v={c.to} />
                        </span>
                      </li>
                    ))}
                  </ul>
                  <p className="d-dim">Use this sends it to the game for your next login. Nothing changes before then.</p>
                </div>
              )}
              <p className="d-dim yu-note">
                Only your UI is drawn here, not the world behind it. Icons show as grey squares.
              </p>
            </>
          )}
        </div>

        <div className="yu-side">
          <Panel>
            <PanelHeader title="Drafts" />
            <PanelBody>
              <div className="yu-drafts">
                <button type="button" aria-pressed={!current} onClick={() => pick(null)}>
                  <b>In game now</b>
                  <span>What you have today.</span>
                </button>
                {drafts.map((d) => (
                  <button key={d.id} type="button" aria-pressed={current?.id === d.id} onClick={() => pick(d.id)}>
                    <b>{d.name}</b>
                    <span>{d.why}</span>
                  </button>
                ))}
              </div>
              {drafts.length === 0 && (
                <p className="d-dim" style={{ marginTop: 8 }}>
                  None yet. Ask Claude for two or three takes, and try each one on here.
                </p>
              )}
            </PanelBody>
          </Panel>
          <Panel>
            <PanelHeader title="Ask Claude" />
            <PanelBody>
              <p className="d">Once, in Claude Code:</p>
              <div className="yu-cmd">
                <code>{SKILL}</code>
                <Button variant="ghost" onClick={copy}>
                  {copied ? "Copied" : "Copy"}
                </Button>
              </div>
              <p className="d">Then say what you want, in your own words:</p>
              <ul className="yu-asks">
                {ASKS.map((a) => (
                  <li key={a}>{a}</li>
                ))}
              </ul>
              <p className="d-dim" style={{ marginTop: 10 }}>
                Claude's takes show up under Drafts. Nothing reaches the game until you pick one with Use this.
              </p>
            </PanelBody>
          </Panel>
        </div>
      </div>
    </Page>
  );
}

/** A setting's value in a draft: a colour gets its swatch. */
function Value({ v }: { v: string }) {
  if (!/^#[0-9a-f]{6}$/i.test(v)) return <>{v}</>;
  return (
    <>
      <i className="yu-swatch" style={{ background: v }} />
      {v}
    </>
  );
}

/** Settings: the downloaded files the Your UI tab draws with, and a way to
 *  remove them. Hidden until there is something to remove. */
export function PreviewDataRow() {
  const [status, setStatus] = useState<PreviewStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const load = useCallback(() => {
    commands.previewStatus().then(setStatus, () => setStatus(null));
  }, []);
  useEffect(load, [load]);
  useEvent(events.previewChanged, load);

  const remove = async () => {
    setBusy(true);
    try {
      await commands.previewRemoveData();
      setResult("Removed. The Your UI tab offers the download again.");
    } catch (e) {
      setResult(errorText(e));
    } finally {
      setBusy(false);
      load();
    }
  };

  if (!status || (status.data !== "ready" && !result)) return null;
  return (
    <div className="st-set full">
      <div className="t">Your UI files</div>
      <div className="d">
        The files the Your UI tab draws your screen with, {bytes(status.download_bytes ?? 0)}.
      </div>
      <div className="ctl" style={{ marginTop: 6 }}>
        {status.data === "ready" && (
          <Button variant="ghost" onClick={remove} disabled={busy}>
            {busy ? "Removing…" : "Remove them"}
          </Button>
        )}
        {result && <span className="d-dim">{result}</span>}
      </div>
    </div>
  );
}
