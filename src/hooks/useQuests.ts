import { useCallback, useEffect, useState } from "react";
import { commands, events, type QuestEntry, type QuestLog } from "@/lib/bindings";
import { useEvent } from "@/hooks/useEvent";

/** Whether any character has quest data yet (Q1b): the sheet's Quests tab
 *  ships dark until then. */
export function useQuestsAvailable() {
  const [available, setAvailable] = useState(false);
  const load = useCallback(() => {
    commands.questsAvailable().then(setAvailable, () => setAvailable(false));
  }, []);
  useEffect(load, [load]);
  useEvent(events.ingestCompleted, load);
  return available;
}

/** One character's quest log: the completed count, the quests still in its
 *  log (accepted, not handed in since) and its hand-ins, newest first. */
export function useQuestLog(id: number) {
  const [log, setLog] = useState<QuestLog | null>(null);
  const load = useCallback(() => {
    commands.characterQuests(id).then(setLog, () => setLog(null));
  }, [id]);
  useEffect(load, [load]);
  useEvent(events.ingestCompleted, load);
  if (!log) return null;
  // Entries come newest first, so a hand-in seen before an accept of the
  // same quest means that accept was handed in later.
  const handedIn = new Set<number>();
  const open: QuestEntry[] = [];
  const done: QuestEntry[] = [];
  for (const e of log.entries) {
    if (e.kind === "turned_in") {
      done.push(e);
      if (e.quest_id != null) handedIn.add(e.quest_id);
    } else if (e.quest_id == null || !handedIn.has(e.quest_id)) {
      open.push(e);
      if (e.quest_id != null) handedIn.add(e.quest_id); // one row per quest
    }
  }
  return { done: log.done, asOf: log.done_as_of, open, completed: done };
}
