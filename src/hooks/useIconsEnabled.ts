import { useSyncExternalStore } from "react";
import { commands } from "@/lib/bindings";

// "Show item icons from my game files" (F8c), shared by every item tile.
// Read once from settings; useSettings keeps it current, so turning icons
// on in Settings or from the Characters nudge updates every screen at once.

let enabled = false;
let loaded = false;
const listeners = new Set<() => void>();

export function setIconsEnabled(on: boolean) {
  loaded = true;
  if (on === enabled) return;
  enabled = on;
  listeners.forEach((l) => l());
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  if (!loaded) {
    loaded = true;
    commands.settingsGet().then(
      (s) => setIconsEnabled(s.item_icons ?? false),
      () => {},
    );
  }
  return () => listeners.delete(listener);
}

/** Whether item tiles should ask for real icons. Off until settings say so. */
export function useIconsEnabled(): boolean {
  return useSyncExternalStore(subscribe, () => enabled);
}
