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
import { commands, events, type Moment, type PreviewStatus } from "@/lib/bindings";
import { useEvent } from "@/hooks/useEvent";
import { usePreview } from "@/hooks/usePreview";
import { bytes, errorText } from "@/lib/format";

// ED1 (was PV1's Preview): Your UI. The player's own screen as the game
// draws it with Forever Buddy and their settings, in a moment they pick, and
// how to ask Claude to change it. No editing here: Claude makes the changes.

const MOMENTS: { value: Moment; label: string }[] = [
  { value: "standing", label: "Standing around" },
  { value: "combat", label: "In combat" },
  { value: "stealth", label: "In stealth" },
  { value: "low_health", label: "Low health" },
];

const SKILL = "npx skills add juliarvalenti/wow-forever-buddy";

const ASKS = [
  "Stack my health over my target's, in blue.",
  "Make my bars a bit smaller and closer to my character.",
  "Show me two or three takes and I'll pick.",
];

export function Preview() {
  const { status, frame, blank, error, download, moment, show } = usePreview();
  const [copied, setCopied] = useState(false);

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
                    <p>
                      Forever Buddy's HUD and action bars are off. Turn them on in game: type /fb, then open HUD and
                      Action bars. Or ask Claude to make you a look. Either way, it shows up here.
                    </p>
                  </div>
                ) : (
                  <div className="yu-empty">
                    <p>Drawing your screen…</p>
                  </div>
                )}
                {frame && !blank && <span className="yu-tag">{label}</span>}
              </div>
              {status.draw_error && (
                <Callout tone="bad">That one didn't draw, so this is the last one that did. {status.draw_error}</Callout>
              )}
              <p className="d-dim yu-note">
                Only your UI is drawn here, not the world behind it. Icons show as grey squares.
              </p>
            </>
          )}
        </div>

        <div className="yu-side">
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
                Claude's changes wait in Approvals. Once you approve, you'll see them in game the next time you log in.
              </p>
            </PanelBody>
          </Panel>
        </div>
      </div>
    </Page>
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
