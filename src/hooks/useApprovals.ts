import { useCallback, useEffect, useState } from "react";
import { type Approvals, commands, type Decision, events } from "@/lib/bindings";
import { useEvent } from "@/hooks/useEvent";
import { errorText } from "@/lib/format";

// After a decision here, the sidebar's count reloads too.
const listeners = new Set<() => void>();
const changed = () => listeners.forEach((l) => l());
/** For Settings › Agents: the switch hides or shows the sidebar's count. */
export const approvalsChanged = changed;

function useReload(load: () => void) {
  useEffect(load, [load]);
  useEffect(() => {
    listeners.add(load);
    window.addEventListener("focus", load);
    return () => {
      listeners.delete(load);
      window.removeEventListener("focus", load);
    };
  }, [load]);
  useEvent(events.approvalsChanged, load);
}

/** P2b Approvals: what agents proposed, and deciding on it. */
export function useApprovals() {
  const [approvals, setApprovals] = useState<Approvals | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(() => {
    commands.approvalsList().then(setApprovals, (e) => setError(errorText(e)));
  }, []);
  useReload(load);

  /** Decides each in turn; stops at the first failure, which is shown. */
  const decide = useCallback(async (ids: number[], decision: Decision) => {
    setError(null);
    try {
      for (const id of ids) await commands.approvalsDecide(id, decision);
    } catch (e) {
      setError(errorText(e));
    } finally {
      changed();
    }
  }, []);
  return { approvals, error, decide };
}

/** The sidebar's ember count: 0 while nothing waits or access is off. */
export function useApprovalsWaiting(): number {
  const [n, setN] = useState(0);
  const load = useCallback(() => {
    commands.approvalsWaiting().then(setN, () => setN(0));
  }, []);
  useReload(load);
  return n;
}
