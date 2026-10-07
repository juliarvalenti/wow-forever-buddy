import { useCallback, useEffect, useState } from "react";
import { commands, events, type Plan, type QuestEntry, type QuestLog } from "@/lib/bindings";
import { useEvent } from "@/hooks/useEvent";

/** P1: this character's active quest plan (or none), with Clear. Progress
 *  and the Bridge state change after an ingest, so it reloads then. */
export function useQuestPlan(characterId: number) {
  const [plans, setPlans] = useState<Plan[]>([]);
  const load = useCallback(() => {
    commands.plansList().then(setPlans, () => setPlans([]));
  }, []);
  useEffect(load, [load]);
  useEvent(events.ingestCompleted, load);
  const clear = useCallback(() => {
    commands.planClear(characterId).then(setPlans, () => load());
  }, [characterId, load]);
  return { plan: plans.find((p) => p.character_id === characterId) ?? null, clear };
}

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
