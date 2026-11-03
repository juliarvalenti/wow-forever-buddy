import { useCallback, useEffect, useState } from "react";
import { Button, Callout, Meter, Page, PageHeader, Panel, PanelBody, PanelHeader, PrimaryButton } from "@/components/d";
import { commands, events, type PreviewStatus } from "@/lib/bindings";
import { useEvent } from "@/hooks/useEvent";
import { usePreview } from "@/hooks/usePreview";
import { bytes, errorText } from "@/lib/format";

// PV1: the player's screen as the game draws it with Forever Buddy and
// their settings, redrawn when a setting changes (from Settings, an
// approved proposal, or the agent). The look is a first cut for the
// designer to take over.

const CLASS_NAMES: Record<string, string> = { rogue: "Rogue" };

const LEDE = "Your screen as WoW draws it with Forever Buddy and your settings. It redraws when a setting changes.";

function drewIn(ms: number): string {
  return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`;
}

export function Preview() {
  const { status, frame, error, download } = usePreview();

  if (!status) {
    return (
      <Page>
        <PageHeader title="Preview" lede={LEDE} />
        {error && <Callout tone="bad">{error}</Callout>}
      </Page>
    );
  }

  const needsData = status.data === "missing" || status.data === "failed";
  const total = status.download_bytes ?? 0;
  const size = bytes(total);

  return (
    <Page>
      <PageHeader title="Preview" lede={LEDE} />
      {error && <Callout tone="bad">{error}</Callout>}

      {needsData && (
        <Panel>
          <PanelHeader title="One download first" />
          <PanelBody>
            <p className="d">
              The preview runs a small World of Warcraft emulator, which needs its data files: the game's API lists
              and interface strings, {size} from GitHub. Each file is checked against the copy this app was built
              with, and anything that doesn't match is thrown away. Nothing is sent. You can remove it any time in
              Settings.
            </p>
            {status.data === "failed" && status.data_error && (
              <div style={{ marginTop: 8 }}>
                <Callout tone="bad">The download didn't work: {status.data_error}</Callout>
              </div>
            )}
            <div className="ctl" style={{ marginTop: 10 }}>
              <PrimaryButton onClick={download}>
                {status.data === "failed" ? `Try again (${size})` : `Download preview data (${size})`}
              </PrimaryButton>
            </div>
          </PanelBody>
        </Panel>
      )}

      {status.data === "downloading" && (
        <Panel>
          <PanelHeader title="Downloading preview data" />
          <PanelBody>
            <Meter fraction={total ? (status.downloaded_bytes ?? 0) / total : 0} />
            <p className="d-dim" style={{ marginTop: 6 }}>
              {bytes(status.downloaded_bytes ?? 0)} of {size}
            </p>
          </PanelBody>
        </Panel>
      )}

      {status.data === "ready" && (
        <Panel>
          <PanelHeader title={status.class ? `${CLASS_NAMES[status.class] ?? status.class} at login` : "At login"}>
            <span className="d-dim">
              {status.drawing ? "Drawing…" : status.drew_ms != null ? `Drawn in ${drewIn(status.drew_ms)}` : ""}
            </span>
          </PanelHeader>
          <PanelBody>
            {status.draw_error && (
              <Callout tone="bad">
                The last redraw didn't work, so this is the one before it. {status.draw_error}
              </Callout>
            )}
            {frame ? (
              <img
                src={frame}
                alt="Your screen with Forever Buddy, as the game would draw it"
                style={{
                  display: "block",
                  width: "100%",
                  aspectRatio: "16 / 9",
                  background: "#1b2226",
                  borderRadius: 6,
                  opacity: status.drawing ? 0.7 : 1,
                }}
              />
            ) : (
              <div className="d-dim" style={{ aspectRatio: "16 / 9", display: "grid", placeItems: "center" }}>
                {status.drawing ? "Drawing your screen…" : "Nothing drawn yet."}
              </div>
            )}
            <p className="d-dim" style={{ marginTop: 8 }}>
              Only Forever Buddy's frames are drawn; Blizzard's own aren't here yet. Classes without a sample
              character show a stand-in.
            </p>
          </PanelBody>
        </Panel>
      )}
    </Page>
  );
}

/** Settings: the preview's downloaded data, and a way to remove it. Hidden
 *  until there is something to remove. */
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
      setResult("Removed. The Preview tab offers the download again.");
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
      <div className="t">Preview data</div>
      <div className="d">
        The emulator's data files the Preview tab draws with, {bytes(status.download_bytes ?? 0)}.
      </div>
      <div className="ctl" style={{ marginTop: 6 }}>
        {status.data === "ready" && (
          <Button variant="ghost" onClick={remove} disabled={busy}>
            {busy ? "Removing…" : "Remove preview data"}
          </Button>
        )}
        {result && <span className="d-dim">{result}</span>}
      </div>
    </div>
  );
}
