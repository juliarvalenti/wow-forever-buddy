import { useEffect, useState } from "react";
import { commands, events, type GameStatus } from "@/lib/bindings";
import { useEvent } from "./useEvent";

/** Whether WoW is running: asked once on mount, then followed via events.
 *  `null` until the first answer. */
export function useGameStatus(): GameStatus | null {
  const [status, setStatus] = useState<GameStatus | null>(null);
  useEffect(() => {
    commands.gameStatus().then(setStatus, () => setStatus(null));
  }, []);
  useEvent(events.gameStatusChanged, setStatus);
  return status;
}
