import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { commands, events, type DetectReport, type Install } from "@/lib/bindings";
import { errorText } from "@/lib/format";
import { useEvent } from "./useEvent";

export type InstallState =
  | { kind: "loading" }
  | { kind: "none" }
  | { kind: "ok"; install: Install }
  /** Saved, but no longer valid (folder moved, link re-pointed, drive unplugged). */
  | { kind: "invalid"; error: string };

/** The active game folder, kept current via `install-changed`, plus the
 *  actions onboarding and Settings need. */
export function useInstall() {
  const [state, setState] = useState<InstallState>({ kind: "loading" });
  const [report, setReport] = useState<DetectReport | null>(null);
  const [detecting, setDetecting] = useState(false);

  const refresh = useCallback(() => {
    commands.installGet().then(
      (install) => setState(install ? { kind: "ok", install } : { kind: "none" }),
      (e) => setState({ kind: "invalid", error: errorText(e) }),
    );
  }, []);

  useEffect(refresh, [refresh]);
  useEvent(events.installChanged, ({ install }) =>
    setState(install ? { kind: "ok", install } : { kind: "none" }),
  );

  const detect = useCallback(async () => {
    setDetecting(true);
    try {
      setReport(await commands.installDetect());
    } finally {
      setDetecting(false);
    }
  }, []);

  /** Saves a folder (root, flavor folder or WTF; the backend walks up). */
  const choose = useCallback(async (path: string, flavor: string | null = null) => {
    const install = await commands.installSet(path, flavor);
    setState({ kind: "ok", install });
    return install;
  }, []);

  /** Opens the folder picker; resolves to the picked path or null. */
  const pick = useCallback(async () => {
    const path = await open({ directory: true, multiple: false });
    return typeof path === "string" ? path : null;
  }, []);

  return { state, report, detecting, detect, choose, pick, refresh };
}
