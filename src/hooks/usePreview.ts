import { useCallback, useEffect, useRef, useState } from "react";
import { commands, events, type PreviewStatus } from "@/lib/bindings";
import { useEvent } from "@/hooks/useEvent";
import { errorText } from "@/lib/format";

/** PV1: the live preview. Opening it asks for a frame, and from then on the
 *  app redraws whenever a setting changes; `preview-changed` says when.
 *  The frame is an SVG shown through a blob URL in an <img>, so nothing in
 *  it can run. */
export function usePreview() {
  const [status, setStatus] = useState<PreviewStatus | null>(null);
  const [frame, setFrame] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const shown = useRef(-1);
  const url = useRef<string | null>(null);

  const load = useCallback(async () => {
    try {
      const s = await commands.previewStatus();
      setStatus(s);
      if (s.generation !== shown.current) {
        shown.current = s.generation;
        const svg = await commands.previewFrame();
        if (url.current) URL.revokeObjectURL(url.current);
        url.current = svg ? URL.createObjectURL(new Blob([svg], { type: "image/svg+xml" })) : null;
        setFrame(url.current);
      }
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  useEffect(() => {
    commands.previewOpen().catch((e) => setError(errorText(e)));
    load();
    return () => {
      if (url.current) URL.revokeObjectURL(url.current);
    };
  }, [load]);
  useEvent(events.previewChanged, load);

  const download = useCallback(() => {
    setError(null);
    commands.previewDownload().catch((e) => setError(errorText(e)));
  }, []);

  return { status, frame, error, download };
}
