import { useEffect, useRef } from "react";
import type { EventCallback, UnlistenFn } from "@tauri-apps/api/event";

type Listenable<T> = { listen: (cb: EventCallback<T>) => Promise<UnlistenFn> };

/** Subscribes to a typed backend event for the component's lifetime. */
export function useEvent<T>(event: Listenable<T>, handler: (payload: T) => void) {
  const latest = useRef(handler);
  latest.current = handler;
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;
    event
      .listen((e) => latest.current(e.payload))
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [event]);
}
