import { useCallback, useEffect, useState } from "react";
import {
  type AhHistory,
  type AhItem,
  type AhStatus,
  commands,
  events,
  type Sellable,
} from "@/lib/bindings";
import { errorText } from "@/lib/format";
import { useEvent } from "./useEvent";

// specta sends Rust's f64 as `number | null`; the backend never sends null
// for these, so they're made plain numbers once, here.

export type Item = Omit<AhItem, "price" | "recent"> & { price: number; recent: number[] };
export type Point = { day: string; low: number; high: number; available: number | null };
export type History = { item: Item; points: Point[] };
export type Sell = Omit<Sellable, "item" | "value"> & { item: Item; value: number };

const item = (i: AhItem): Item => ({ ...i, price: i.price ?? 0, recent: i.recent.map((v) => v ?? 0) });
const history = (h: AhHistory): History => ({
  item: item(h.item),
  points: h.points.map((p) => ({ day: p.day, low: p.low ?? 0, high: p.high ?? 0, available: p.available })),
});
const sell = (s: Sellable): Sell => ({ ...s, item: item(s.item), value: s.value ?? 0 });

/** Reloads `load` on start and whenever new prices or characters' notes are
 *  read (worth selling depends on both). */
function useLive(load: () => void) {
  useEffect(load, [load]);
  useEvent(events.pricesUpdated, load);
  useEvent(events.ingestCompleted, load);
  useEvent(events.installChanged, load);
}

/** The scan bar: whether there are prices at all, and how fresh. */
export function useAhStatus() {
  const [status, setStatus] = useState<AhStatus | null>(null);
  const load = useCallback(() => {
    commands.ahStatus().then(setStatus, () => setStatus(null));
  }, []);
  useLive(load);
  return status;
}

/** The watchlist, with add/remove. */
export function useWatchlist() {
  const [items, setItems] = useState<Item[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(() => {
    commands.ahWatchlist().then(
      (l) => setItems(l.map(item)),
      (e) => setError(errorText(e)),
    );
  }, []);
  useLive(load);
  const setWatched = useCallback(
    async (itemId: number, watched: boolean) => {
      setError(null);
      try {
        await commands.ahSetWatched(itemId, watched);
        load();
      } catch (e) {
        setError(errorText(e));
      }
    },
    [load],
  );
  return { items, error, setWatched };
}

/** What the alts carry that's worth selling (at least `minValue` copper). */
export function useWorthSelling(minValue: number) {
  const [rows, setRows] = useState<Sell[] | null>(null);
  const load = useCallback(() => {
    commands.ahWorthSelling(minValue).then(
      (r) => setRows(r.map(sell)),
      () => setRows(null),
    );
  }, [minValue]);
  useLive(load);
  return rows;
}

/** One item's whole history (the screen slices the range itself, so the
 *  median line has its earlier days to work from). */
export function useAhHistory(itemId: number | null) {
  const [data, setData] = useState<History | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(() => {
    if (itemId == null) {
      setData(null);
      return;
    }
    let live = true;
    commands.ahHistory(itemId, null).then(
      (h) => {
        if (!live) return;
        setData(history(h));
        setError(null);
      },
      (e) => live && setError(errorText(e)),
    );
    return () => {
      live = false;
    };
  }, [itemId]);
  useEffect(load, [load]);
  useEvent(events.pricesUpdated, load);
  return { history: data, error };
}

/** Priced items matching `query`, a moment after typing stops. */
export function useAhSearch(query: string) {
  const [results, setResults] = useState<Item[]>([]);
  useEffect(() => {
    if (!query.trim()) {
      setResults([]);
      return;
    }
    let live = true;
    const t = setTimeout(() => {
      commands.ahSearch(query).then(
        (r) => live && setResults(r.map(item)),
        () => live && setResults([]),
      );
    }, 150);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [query]);
  return results;
}
