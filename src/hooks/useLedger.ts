import { useCallback, useEffect, useState } from "react";
import { commands, type Ledger, type LedgerRange } from "@/lib/bindings";
import { errorText } from "@/lib/format";

/** The Ledger over `range` (V8). `ledger` is null while loading. */
export function useLedger(range: LedgerRange) {
  const [ledger, setLedger] = useState<Ledger | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    commands.ledgerGet(range).then(
      (l) => {
        setLedger(l);
        setError(null);
      },
      (e) => setError(errorText(e)),
    );
  }, [range]);
  useEffect(refresh, [refresh]);

  return { ledger, error, refresh };
}
