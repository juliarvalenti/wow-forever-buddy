import { useCallback, useEffect, useState } from "react";
import { commands, events, type Goal, type NewGoal } from "@/lib/bindings";
import { useEvent } from "@/hooks/useEvent";
import { errorText } from "@/lib/format";

/** Goals (G1): open ones, and ones reached in the last 3 days. Progress comes
 *  from ingest, and an approved proposal adds one, so it reloads on both. */
export function useGoals() {
  const [goals, setGoals] = useState<Goal[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(() => {
    commands.goalsList().then(setGoals, (e) => setError(errorText(e)));
  }, []);
  useEffect(load, [load]);
  useEvent(events.ingestCompleted, load);
  useEvent(events.approvalsChanged, load);

  /** The refusal ("Brannic is already level 55.") on failure, else null. */
  const add = useCallback(
    async (goal: NewGoal): Promise<string | null> => {
      try {
        await commands.goalsAdd(goal);
        load();
        return null;
      } catch (e) {
        return errorText(e);
      }
    },
    [load],
  );
  const remove = useCallback(
    async (id: number) => {
      setError(null);
      try {
        await commands.goalsDelete(id);
        load();
      } catch (e) {
        setError(errorText(e));
      }
    },
    [load],
  );
  return { goals, error, add, remove };
}
