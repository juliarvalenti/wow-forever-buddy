import { useCallback, useEffect, useState } from "react";
import { commands, events, type GoodsWorth, type Holdings } from "@/lib/bindings";
import { useEvent } from "./useEvent";

// F5c: the worth §7 hid until AH prices existed. Shown only when something
// has a price; never as zeros or dashes (IMPLEMENTING.md §7).

export type Worth = Omit<GoodsWorth, "value" | "by_character" | "top"> & {
  value: number;
  byCharacter: Map<number, number>;
  top: (Omit<Holdings, "value" | "item"> & { value: number; item: Holdings["item"] & { price: number } })[];
};

/** What the alts' goods are worth at scan prices; `null` until known, and
 *  while nothing is priced. */
export function useGoodsWorth() {
  const [worth, setWorth] = useState<Worth | null>(null);
  const load = useCallback(() => {
    commands.ahGoodsWorth().then(
      (w) =>
        setWorth(
          w.priced > 0
            ? {
                ...w,
                value: w.value ?? 0,
                byCharacter: new Map(w.by_character.map(([id, v]) => [id, v ?? 0])),
                top: w.top.map((t) => ({ ...t, value: t.value ?? 0, item: { ...t.item, price: t.item.price ?? 0 } })),
              }
            : null,
        ),
      () => setWorth(null),
    );
  }, []);
  useEffect(load, [load]);
  useEvent(events.pricesUpdated, load);
  useEvent(events.ingestCompleted, load);
  return worth;
}

/** The last lowest buyout of each of `ids` that has one (copper). */
export function usePrices(ids: number[]) {
  const [prices, setPrices] = useState<Map<number, number>>(new Map());
  const key = ids.join(",");
  useEffect(() => {
    if (!key) {
      setPrices(new Map());
      return;
    }
    let live = true;
    commands.ahPrices(key.split(",").map(Number)).then(
      (p) => live && setPrices(new Map(p.map(([id, v]) => [id, v ?? 0]))),
      () => live && setPrices(new Map()),
    );
    return () => {
      live = false;
    };
  }, [key]);
  return prices;
}
