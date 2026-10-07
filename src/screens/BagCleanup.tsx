import { useEffect, useRef, useState } from "react";
import { X } from "lucide-react";
import type { Cleanup, Delivery, ItemRow, Mark, Marked } from "@/lib/bindings";
import { Button, ItemIcon, LiveDot, Panel, PanelBody, PanelHeader, StatusDot } from "@/components/d";
import { coins, plural } from "@/lib/format";

// B3 bag cleanup on the character sheet (IMPLEMENTING §18): mark items to
// sell, or to send to another character; the addon shows the marks in game
// (INGAME §14). Marks are app data, so there's no write gate and no apply
// bar. Nothing here sells or sends.

export type Who = { id: number; name: string; class: string | null };

const cc = (cls: string | null | undefined) =>
  ({ "--cc": cls ? `var(--c-${cls})` : undefined }) as React.CSSProperties;

/** "1g 20s", "45s", "8c": a vendor price, never a market one. */
function vendor(copper: number): string {
  const [g, s, c] = coins(copper);
  return [g ? `${g}g` : null, s ? `${s}s` : null, !g && c ? `${c}c` : null].filter(Boolean).join(" ") || "0c";
}

/** The muted tag after a marked item's name: "sell" or "→ Sela". */
export function MarkTag({ mark }: { mark: Marked }) {
  return (
    <span className="bc-tag">
      {mark.to ? (
        <>
          →{" "}
          <span className="ch-cc" style={cc(mark.to.class)}>
            {mark.to.name}
          </span>
        </>
      ) : (
        "sell"
      )}
    </span>
  );
}

/** The ghost "Mark" menu on a bag row: Sell, Send to… (the other
 *  characters), Clear mark. */
export function MarkMenu({
  item,
  marked,
  others,
  onMark,
  onClear,
  onOpenChange,
}: {
  item: ItemRow;
  marked: Marked | undefined;
  others: Who[];
  onMark: (m: Mark) => void;
  onClear: () => void;
  /** So the row can hide its item tooltip while the menu is open. */
  onOpenChange?: (open: boolean) => void;
}) {
  const [open, setOpen] = useState<"menu" | "send" | null>(null);
  const box = useRef<HTMLSpanElement>(null);
  useEffect(() => onOpenChange?.(open != null), [open, onOpenChange]);
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (box.current && !box.current.contains(e.target as Node)) setOpen(null);
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [open]);
  const pick = (f: () => void) => () => {
    f();
    setOpen(null);
  };
  return (
    <span className={`bc-menu${open ? " open" : ""}`} ref={box}>
      <button className="bc-mark" aria-haspopup="menu" aria-expanded={open != null} onClick={() => setOpen(open ? null : "menu")}>
        Mark
      </button>
      {open === "menu" && (
        <span className="bc-pop" role="menu">
          <button role="menuitem" onClick={pick(() => onMark({ action: "sell" }))}>
            Sell
          </button>
          <button
            role="menuitem"
            disabled={item.bound || others.length === 0}
            title={item.bound ? "Soulbound, so it can't be mailed" : undefined}
            onClick={() => setOpen("send")}
          >
            Send to…
          </button>
          {marked && (
            <button role="menuitem" onClick={pick(onClear)}>
              Clear mark
            </button>
          )}
        </span>
      )}
      {open === "send" && (
        <span className="bc-pop" role="menu">
          {others.map((w) => (
            <button key={w.id} role="menuitem" onClick={pick(() => onMark({ action: "send", to: w.id }))}>
              <span className="ch-cc" style={cc(w.class)}>
                {w.name}
              </span>
            </button>
          ))}
        </span>
      )}
    </span>
  );
}

function deliveryLine(d: Delivery): { live: boolean; text: string } {
  switch (d.state) {
    case "synced":
      return {
        live: false,
        text: `In the game since ${new Date(d.since).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })}`,
      };
    case "pending":
      return { live: true, text: "Waiting for a sync: /reload or log in to see it" };
    case "waiting":
      return { live: true, text: "Goes to the game when WoW closes" };
    case "restart":
      return { live: true, text: "Needs the addon update, then restart WoW once" };
    case "failed":
      return { live: true, text: "Couldn't write it; the game keeps the last marks" };
  }
}

/** The side column's Bag cleanup panel: shown when something is marked or
 *  the bags hold greys. */
export function BagCleanup({
  cleanup,
  error,
  onClear,
  onMarkGreys,
}: {
  cleanup: Cleanup;
  error: string | null;
  onClear: (itemId: number) => void;
  onMarkGreys: () => void;
}) {
  const { marks, greys } = cleanup;
  if (marks.length === 0 && greys === 0) return null;
  const sell = marks.filter((m) => !m.to);
  const send = marks.length - sell.length;
  const sellCount = sell.reduce((n, m) => n + m.count, 0);
  const priced = sell.filter((m) => m.sell_price != null);
  const worth = priced.reduce((n, m) => n + (m.sell_price ?? 0) * m.count, 0);
  const footer = [
    sellCount > 0 ? `${sellCount} to sell${priced.length > 0 ? ` · ~${vendor(worth)} at a vendor` : ""}` : null,
    send > 0 ? `${send} to send` : null,
  ]
    .filter(Boolean)
    .join(" · ");
  const status = deliveryLine(cleanup.delivery);
  return (
    <Panel>
      <PanelHeader title="Bag cleanup">
        {marks.length > 0 && <span className="d-dim">{plural(marks.length, "marked", "marked")}</span>}
      </PanelHeader>
      <PanelBody>
        {marks.length === 0 && <p className="d-dim">Nothing marked.</p>}
        {marks.map((m) => {
          const q = m.quality != null ? `ch-q${m.quality}` : "";
          return (
            <div key={m.item_id} className="bc-row">
              <span className={`ch-ico ${q}`} aria-hidden>
                <b>{m.name.slice(0, 1)}</b>
                <ItemIcon id={m.icon} />
              </span>
              <span className={`nm ${q}`}>{m.name}</span>
              <span className="n">{m.count > 1 ? `× ${m.count}` : ""}</span>
              <MarkTag mark={m} />
              <button className="bc-x" title="Clear mark" aria-label={`Clear the mark on ${m.name}`} onClick={() => onClear(m.item_id)}>
                <X size={13} aria-hidden />
              </button>
            </div>
          );
        })}
        {footer && <p className="bc-foot">{footer}</p>}
        {greys > 0 && (
          <Button variant="ghost" onClick={onMarkGreys}>
            Mark all greys ({greys})
          </Button>
        )}
        {marks.length > 0 && (
          <div className="bc-sent">
            {status.live ? <LiveDot /> : <StatusDot />}
            {status.text}
          </div>
        )}
        {error && <p className="d-letter-bad">{error}</p>}
      </PanelBody>
    </Panel>
  );
}
