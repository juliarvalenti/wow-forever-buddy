import { useCallback, useEffect, useRef, useState } from "react";
import { commands, events, type Moment, type PreviewStatus } from "@/lib/bindings";
import { useEvent } from "@/hooks/useEvent";
import { errorText } from "@/lib/format";

/** PV1 and ED1: the player's screen as the game draws it. Opening it asks
 *  for a frame, and from then on the app redraws whenever a setting changes;
 *  `preview-changed` says when. `show(moment)` redraws in another moment
 *  (standing, combat, stealth, low health). The frame is an SVG as a data:
 *  URL (the one kind the CSP lets an <img> load; BUG-PREVIEW-IMG), shown in
 *  an <img>, so nothing in it can run. */
export function usePreview() {
  const [status, setStatus] = useState<PreviewStatus | null>(null);
  const [frame, setFrame] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [moment, setMoment] = useState<Moment | null>(null);
  const shown = useRef(-1);

  const load = useCallback(async () => {
    try {
      const s = await commands.previewStatus();
      setStatus(s);
      if (s.generation !== shown.current) {
        shown.current = s.generation;
        setFrame(await commands.previewFrame());
      }
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  useEffect(() => {
    commands.previewOpen(null).catch((e) => setError(errorText(e)));
    load();
  }, [load]);
  useEvent(events.previewChanged, load);

  const show = useCallback((m: Moment) => {
    setMoment(m);
    setError(null);
    commands.previewOpen(m).catch((e) => setError(errorText(e)));
  }, []);

  const download = useCallback(() => {
    setError(null);
    commands.previewDownload().catch((e) => setError(errorText(e)));
  }, []);

  return {
    status,
    frame,
    blank: status?.blank ?? false,
    error,
    download,
    // The moment picked here, until the frame for it arrives.
    moment: moment ?? status?.moment ?? "standing",
    show,
  };
}
