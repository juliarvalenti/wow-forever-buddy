import { useEffect, useState } from "react";
import { commands, events, type UpdateStatus } from "@/lib/bindings";
import { useEvent } from "./useEvent";

/** U2: the app's own update, asked once on mount and then followed via
 *  `update-status`. `null` until the first answer. */
export function useUpdate(): UpdateStatus | null {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  useEffect(() => {
    commands.updateStatus().then(setStatus, () => setStatus(null));
  }, []);
  useEvent(events.updateStatus, setStatus);
  return status;
}
