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

/** The muted tag after a marked item's name: "sell" or "→ Sela", then
 *  (B3b) its reason in grey. On the parchment satchel rows names are plain
 *  ink. */
export function MarkTag({ mark, parchment }: { mark: Marked; parchment?: boolean }) {
  const why = reasonText(mark, parchment);
  return (
    <span className="bc-tag">
      {mark.to ? (
        <>
          →{" "}
          {parchment ? (
            mark.to.name
          ) : (
            <span className="ch-cc" style={cc(mark.to.class)}>
              {mark.to.name}
            </span>
          )}
        </>
      ) : (
        "sell"
      )}
      {why && <span className="bc-why"> · {why}</span>}
    </span>
  );
}

/** "grey", "outgrown", "+9 item level for Kaelor", then for an agent's mark
 *  `from "Claude Desktop", approved`. Null with neither. */
function reasonText(m: Marked, parchment?: boolean): React.ReactNode {
  const r = m.reason;
  const why =
    r?.code === "upgrade" && m.to ? (
      <>
        +{r.gain} item level for{" "}
        {parchment ? (
          m.to.name
        ) : (
          <span className="ch-cc" style={cc(m.to.class)}>
            {m.to.name}
          </span>
        )}
      </>
    ) : r?.code === "grey" || r?.code === "outgrown" ? (
      r.code
    ) : null;
  const agent = m.producer.startsWith("agent:") ? `from "${m.producer.slice("agent:".length)}", approved` : null;
  if (!why && !agent) return null;
  return (
    <>
      {why}
      {why && agent && " · "}
      {agent}
    </>
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

function Tile({ m }: { m: Marked }) {
  const q = m.quality != null ? `ch-q${m.quality}` : "";
  return (
    <>
      <span className={`ch-ico ${q}`} aria-hidden>
        <b>{m.name.slice(0, 1)}</b>
        <ItemIcon id={m.icon} />
      </span>
      <span className={`nm ${q}`}>{m.name}</span>
      <span className="n">{m.count > 1 ? `× ${m.count}` : ""}</span>
    </>
  );
}

/** The side column's Bag cleanup panel: shown when something is marked or
 *  suggested. */
export function BagCleanup({
  cleanup,
  error,
  onClear,
  onAccept,
  onDismiss,
}: {
  cleanup: Cleanup;
  error: string | null;
  onClear: (itemId: number) => void;
  /** Mark one suggestion, or all with null. */
  onAccept: (itemId: number | null) => void;
  onDismiss: (itemId: number) => void;
}) {
  const { marks, suggestions } = cleanup;
  if (marks.length === 0 && suggestions.length === 0) return null;
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
        {marks.map((m) => (
          <div key={m.item_id} className="bc-row">
            <Tile m={m} />
            <MarkTag mark={m} />
            <button className="bc-x" title="Clear mark" aria-label={`Clear the mark on ${m.name}`} onClick={() => onClear(m.item_id)}>
              <X size={13} aria-hidden />
            </button>
          </div>
        ))}
        {footer && <p className="bc-foot">{footer}</p>}
        {suggestions.length > 0 && (
          <div className="bc-sugg">
            <div className="bc-subhead">
              <span>Suggested</span>
              <Button variant="ghost" onClick={() => onAccept(null)}>
                Mark all {suggestions.length}
              </Button>
            </div>
            {suggestions.map((m) => (
              <div key={m.item_id} className="bc-row suggested">
                <Tile m={m} />
                <span className="bc-tag">
                  <MarkTag mark={m} /> · suggested
                </span>
                <span className="bc-acts">
                  <Button variant="ghost" onClick={() => onAccept(m.item_id)}>
                    Mark
                  </Button>
                  <button className="bc-x" title="Dismiss" aria-label={`Dismiss the suggestion for ${m.name}`} onClick={() => onDismiss(m.item_id)}>
                    <X size={13} aria-hidden />
                  </button>
                </span>
              </div>
            ))}
          </div>
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
