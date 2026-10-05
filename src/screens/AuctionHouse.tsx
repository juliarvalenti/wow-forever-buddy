import { useEffect, useMemo, useRef, useState } from "react";
import { AlertTriangle, Clock, RefreshCw, Search, Star } from "lucide-react";
import type { AhStatus } from "@/lib/bindings";
import { Callout, Page, PageHeader, Panel, PanelBody, PanelHeader, Segmented } from "@/components/d";
import {
  type History,
  type Item,
  type Point,
  type Sell,
  useAhHistory,
  useAhSearch,
  useAhStatus,
  useWatchlist,
  useWorthSelling,
} from "@/hooks/useAh";
import { ago, gold, plural } from "@/lib/format";
import { Coins } from "@/screens/Characters";

// design/mocks/round-3/ah.html (F5), from Auctionator's saved prices. The
// mock's Sales ledger needs mail invoices (matrix row 43) and isn't here
// yet. Item names come from what our addon has seen; others read "Item N".

/** Worth selling's floor: 50g, as the mock's "over 50g". */
const MIN_VALUE = 50 * 10_000;

const nameOf = (i: Item) => i.name ?? `Item ${i.item_id}`;
const quality = (i: Item) => (i.quality != null ? `ch-q${i.quality}` : "");

function ItemIcon({ item, lg }: { item: Item; lg?: boolean }) {
  return (
    <span className={`ah-ico${lg ? " lg" : ""} ${quality(item)}`} aria-hidden>
      <b>{nameOf(item).slice(0, 1)}</b>
    </span>
  );
}

const daysAgo = (day: string) =>
  Math.max(0, Math.round((Date.now() - new Date(`${day}T12:00:00`).getTime()) / 86_400_000));

function seen(day: string): string {
  const d = daysAgo(day);
  return d === 0 ? "today" : d === 1 ? "yesterday" : `${d} days ago`;
}

function shortDay(day: string): string {
  return new Date(`${day}T12:00:00`).toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

/** "−6%" against the 30-day median, or null without one. */
function vsMedian(i: Item): number | null {
  return i.median ? Math.round(((i.price - i.median) / i.median) * 100) : null;
}

export function AuctionHouse() {
  const status = useAhStatus();
  const watch = useWatchlist();
  const selling = useWorthSelling(MIN_VALUE);
  const [picked, setPicked] = useState<number | null>(null);
  // Until one is picked: the first watched item, else the most valuable to sell.
  const shown = picked ?? watch.items?.[0]?.item_id ?? selling?.[0]?.item.item_id ?? null;

  return (
    <Page>
      <PageHeader title="Auction House" lede="Prices from your own in-game scans. Nothing is fetched online." />
      {status && !status.has_prices ? (
        <NoPrices />
      ) : (
        <>
          {status && <ScanBar status={status} />}
          {watch.error && <Callout tone="bad">{watch.error}</Callout>}
          <section className="ah-cols">
            <PriceHistory
              itemId={shown}
              onPick={setPicked}
              watched={(watch.items ?? []).some((w) => w.item_id === shown)}
              onWatch={(id, on) => watch.setWatched(id, on)}
            />
            <div className="d-stack">
              <Watchlist items={watch.items} shown={shown} onPick={setPicked} />
              <WorthSelling rows={selling} onPick={setPicked} />
            </div>
          </section>
        </>
      )}
    </Page>
  );
}

function NoPrices() {
  return (
    <Panel>
      <PanelHeader title="No auction prices yet" />
      <PanelBody>
        <p className="d-muted" style={{ margin: 0, lineHeight: 1.6 }}>
          Forever Buddy reads the prices the Auctionator addon saves. With Auctionator installed,
          open the Auction House in-game and run a scan. The prices appear here after you log out
          or type <span className="d-mono">/reload</span>, and their history builds up with every
          scan.
        </p>
      </PanelBody>
    </Panel>
  );
}

function ScanBar({ status }: { status: AhStatus }) {
  const at = status.last_scan_at;
  const stale = at ? Date.now() - new Date(at).getTime() > 2 * 86_400_000 : true;
  return (
    <div className="ah-scanbar">
      <span className={`dot${stale ? " stale" : ""}`} aria-hidden />
      <span>
        <b>Last scan: {at ? ago(at) : status.newest_day ? seen(status.newest_day) : "unknown"}</b>
        {at && (
          <>
            {" "}
            <span className="sep">·</span>{" "}
            {new Date(at).toLocaleString(undefined, { day: "numeric", month: "short", hour: "numeric", minute: "2-digit" })}
          </>
        )}{" "}
        <span className="sep">·</span> {plural(status.items, "price", "prices")}
      </span>
      <span className="how">
        <RefreshCw size={12} aria-hidden />
        <span>
          Scan in-game to refresh: press <b>Scan</b> at the Auction House; prices land here on logout.
        </span>
      </span>
    </div>
  );
}

type Range = "7" | "30" | "all";

/** The rolling median of the daily lows over the 30 days up to each day. */
function rollingMedian(points: Point[]): number[] {
  return points.map((p, i) => {
    const end = new Date(`${p.day}T12:00:00`).getTime();
    const lows = points
      .slice(0, i + 1)
      .filter((q) => end - new Date(`${q.day}T12:00:00`).getTime() < 30 * 86_400_000)
      .map((q) => q.low)
      .sort((a, b) => a - b);
    const n = lows.length;
    return n % 2 ? lows[(n - 1) / 2] : (lows[n / 2 - 1] + lows[n / 2]) / 2;
  });
}

function PriceHistory({
  itemId,
  onPick,
  watched,
  onWatch,
}: {
  itemId: number | null;
  onPick: (id: number) => void;
  watched: boolean;
  onWatch: (id: number, on: boolean) => void;
}) {
  const { history, error } = useAhHistory(itemId);
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(false);
  const found = useAhSearch(query);
  const [range, setRange] = useState<Range>("30");
  const item = history?.item;

  const pick = (id: number) => {
    onPick(id);
    setQuery("");
    setOpen(false);
  };

  return (
    <Panel>
      <PanelHeader title="Price history">
        <span className="d-grow" />
        <span className="d-dim">from your scans only</span>
      </PanelHeader>
      <div className="ah-pick">
        <label className="d-field">
          <Search size={14} aria-hidden style={{ color: "var(--soot)" }} />
          <input
            value={query}
            placeholder={item ? nameOf(item) : "Find an item…"}
            aria-label="Item"
            onChange={(e) => {
              setQuery(e.target.value);
              setOpen(true);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && found[0]) pick(found[0].item_id);
              if (e.key === "Escape") setOpen(false);
            }}
          />
        </label>
        {open && query.trim() && (
          <ul className="ah-found" role="listbox">
            {found.length === 0 ? (
              <li className="d-dim" style={{ padding: "6px 10px", fontSize: 12 }}>
                No scanned price for that. Items appear here once an alt has carried them.
              </li>
            ) : (
              found.map((f) => (
                <li key={f.item_id}>
                  <button role="option" aria-selected={f.item_id === itemId} onClick={() => pick(f.item_id)}>
                    <ItemIcon item={f} />
                    <span className={quality(f)}>{nameOf(f)}</span>
                    <span className="p">{gold(f.price)}</span>
                  </button>
                </li>
              ))
            )}
          </ul>
        )}
        <Segmented
          options={[
            { value: "7", label: "7 days" },
            { value: "30", label: "30 days" },
            { value: "all", label: "All" },
          ]}
          value={range}
          onChange={setRange}
        />
      </div>
      {error && (
        <PanelBody>
          <p className="d-muted">{error}</p>
        </PanelBody>
      )}
      {itemId == null && (
        <PanelBody>
          <p className="d-muted">Find an item to see its price over time.</p>
        </PanelBody>
      )}
      {history && item && <ItemHistory history={history} range={range} watched={watched} onWatch={onWatch} />}
    </Panel>
  );
}

function ItemHistory({
  history,
  range,
  watched,
  onWatch,
}: {
  history: History;
  range: Range;
  watched: boolean;
  onWatch: (id: number, on: boolean) => void;
}) {
  const item = history.item;
  const diff = vsMedian(item);
  const verdict =
    diff == null
      ? null
      : diff <= -10
        ? { kind: "ok", text: `Cheap · ${-diff}% under usual` }
        : diff >= 10
          ? { kind: "warn", text: `Pricey · ${diff}% over usual` }
          : { kind: "", text: "About usual" };
  return (
    <>
      <div className="ah-head">
        <div className="nm">
          <ItemIcon item={item} lg />
          <div className={quality(item)}>
            {nameOf(item)}
            <small>Item {item.item_id}</small>
          </div>
          <button
            className="ah-star"
            aria-pressed={watched}
            title={watched ? "Remove from the watchlist" : "Add to the watchlist"}
            onClick={() => onWatch(item.item_id, !watched)}
          >
            <Star size={15} fill={watched ? "currentColor" : "none"} aria-hidden />
          </button>
        </div>
        <div className="ah-fig">
          <span className="k">Lowest buyout</span>
          <span className="v">
            <Coins copper={item.price} />
          </span>
        </div>
        {item.median != null && (
          <div className="ah-fig">
            <span className="k">30-day median</span>
            <span className="v">
              <Coins copper={item.median} />
            </span>
          </div>
        )}
        {verdict && (
          <div className="ah-verdict">
            <span className={`d-pill ${verdict.kind}`}>{verdict.text}</span>
            <small>vs your 30-day median</small>
          </div>
        )}
      </div>
      <div className="ah-legend">
        <span>
          <i />
          Lowest buyout
        </span>
        <span>
          <i className="med" />
          Median
        </span>
        <span>
          <span className="dotk" />
          Each dot is a day you scanned it
        </span>
      </div>
      <PriceChart history={history} range={range} />
      <div className="ah-foot">
        <span>
          Seen <b>{plural(item.sightings, "time", "times")}</b> in 30 days
        </span>
        <span>
          Last seen <b>{seen(item.last_seen)}</b>
        </span>
        {item.listed != null && (
          <span>
            Typical listing <b>{plural(Math.round(item.listed), "auction", "auctions")}</b>
          </span>
        )}
      </div>
    </>
  );
}

function PriceChart({ history, range }: { history: History; range: Range }) {
  const svg = useRef<SVGSVGElement>(null);
  const [width, setWidth] = useState(640);
  const [hover, setHover] = useState<number | null>(null);
  useEffect(() => {
    const el = svg.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth || 640));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const medians = useMemo(() => rollingMedian(history.points), [history]);
  const from = range === "all" ? 0 : Date.now() - Number(range) * 86_400_000;
  const shown = history.points
    .map((p, i) => ({ ...p, med: medians[i], t: new Date(`${p.day}T12:00:00`).getTime() }))
    .filter((p) => p.t >= from);
  if (shown.length === 0) {
    return <p className="d-muted" style={{ padding: "12px 14px" }}>Not seen in this range.</p>;
  }

  const H = 190, L = 40, R = 104, T = 10, B = 24;
  const t0 = range === "all" ? shown[0].t : from;
  const t1 = Math.max(Date.now(), shown[shown.length - 1].t);
  const vals = shown.flatMap((p) => [p.low, p.med]);
  const lo = Math.min(...vals) * 0.96;
  const hi = Math.max(...vals) * 1.04;
  const x = (t: number) => L + ((width - L - R) * (t - t0)) / Math.max(1, t1 - t0);
  const y = (v: number) => T + (H - T - B) * (1 - (v - lo) / Math.max(1, hi - lo));
  const ticks = [0, 1, 2, 3].map((k) => lo + ((hi - lo) * k) / 3);
  const xTicks = [0, 0.25, 0.5, 0.75, 1].map((f) => t0 + (t1 - t0) * f);
  const last = shown[shown.length - 1];
  const line = (key: "low" | "med") => shown.map((p) => `${x(p.t).toFixed(1)},${y(p[key]).toFixed(1)}`).join(" ");
  const onMove = (e: React.MouseEvent<SVGRectElement>) => {
    const r = svg.current!.getBoundingClientRect();
    const px = e.clientX - r.left;
    let best = 0;
    shown.forEach((p, i) => {
      if (Math.abs(x(p.t) - px) < Math.abs(x(shown[best].t) - px)) best = i;
    });
    setHover(best);
  };
  const hp = hover != null ? shown[hover] : null;

  return (
    <div className="ah-chartwrap">
      <svg ref={svg} className="ah-chart" role="img" aria-label="Price over time">
        <g className="grid">
          {ticks.map((v) => (
            <line key={v} x1={L} x2={width - R + 4} y1={y(v)} y2={y(v)} />
          ))}
        </g>
        <g className="axis">
          {ticks.map((v) => (
            <text key={v} x={L - 6} y={y(v) + 3.5} textAnchor="end">
              {gold(v)}
            </text>
          ))}
          {xTicks.map((t) => (
            <text key={t} x={x(t)} y={H - 6} textAnchor="middle">
              {new Date(t).toLocaleDateString(undefined, { day: "numeric", month: "short" })}
            </text>
          ))}
        </g>
        <polyline className="lmed" points={line("med")} />
        <polyline className="lmin" points={line("low")} />
        {shown.map((p) => (
          <circle key={p.day} className="dmin" cx={x(p.t)} cy={y(p.low)} r={3.5} />
        ))}
        <text className="lab" x={x(last.t) + 8} y={y(last.med) + 4}>
          Median <tspan className="m">{gold(last.med)}</tspan>
        </text>
        <text className="lab" x={x(last.t) + 8} y={y(last.low) + (Math.abs(y(last.low) - y(last.med)) < 13 ? 17 : 4)}>
          Lowest <tspan className="m">{gold(last.low)}</tspan>
        </text>
        {hp && <line className="xh" x1={x(hp.t)} x2={x(hp.t)} y1={T} y2={H - B} />}
        <rect
          x={L}
          y={T}
          width={Math.max(0, width - L - R)}
          height={H - T - B}
          fill="transparent"
          onMouseMove={onMove}
          onMouseLeave={() => setHover(null)}
        />
      </svg>
      {hp && (
        <div className="ah-tip" style={{ left: Math.min(x(hp.t) + 22, width - 150), top: 10 }}>
          <div className="d">{shortDay(hp.day)}</div>
          <div>
            <span className="sw" style={{ background: "#c9772a" }} />
            Lowest {gold(hp.low)}
          </div>
          <div>
            <span className="sw" style={{ background: "#5a8bd4" }} />
            Median {gold(hp.med)}
          </div>
          {hp.available != null && <div className="d">{plural(hp.available, "listed", "listed")}</div>}
        </div>
      )}
    </div>
  );
}

function Spark({ values }: { values: number[] }) {
  if (values.length < 2) return <span />;
  const lo = Math.min(...values);
  const hi = Math.max(...values);
  const pts = values.map((v, i) => `${((i / (values.length - 1)) * 54).toFixed(1)},${(hi === lo ? 10 : 18 - ((v - lo) / (hi - lo)) * 16).toFixed(1)}`);
  return (
    <svg className="sp" viewBox="0 0 54 20" aria-hidden>
      <polyline points={pts.join(" ")} />
    </svg>
  );
}

function Watchlist({
  items,
  shown,
  onPick,
}: {
  items: Item[] | null;
  shown: number | null;
  onPick: (id: number) => void;
}) {
  return (
    <Panel>
      <PanelHeader title="Watchlist" />
      {items && items.length === 0 ? (
        <PanelBody>
          <p className="d-muted" style={{ margin: 0 }}>
            Star an item in Price history to keep an eye on it here.
          </p>
        </PanelBody>
      ) : (
        <ul className="ah-watch">
          {(items ?? []).map((i) => {
            const diff = vsMedian(i);
            const old = daysAgo(i.last_seen) > 7;
            return (
              <li key={i.item_id} className={i.item_id === shown ? "on" : undefined} onClick={() => onPick(i.item_id)}>
                <ItemIcon item={i} />
                <div className="wn">
                  <div className={quality(i)}>{nameOf(i)}</div>
                  <small className={old ? "old" : undefined}>
                    {plural(i.sightings, "scan", "scans")} · last {seen(i.last_seen)}
                  </small>
                </div>
                <Spark values={i.recent} />
                <div className="wp">
                  <Coins copper={i.price} />
                  {diff != null && (
                    <small className={diff <= -5 ? "cheap" : undefined}>
                      {diff > 0 ? "+" : diff < 0 ? "−" : ""}
                      {Math.abs(diff)}%{diff <= -5 ? " · cheap" : ""}
                    </small>
                  )}
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </Panel>
  );
}

const CONF_LABEL = { sure: "sure", fair: "fair", rough: "rough" } as const;
const PLACE = { bag: "satchels", bank: "bank", mail: "mail" } as Record<string, string>;

/** "Coinpurse · bank · +112 on 3 alts". */
function whereLine(s: Sell): string {
  const [first, ...rest] = s.holdings;
  if (!first) return "";
  const head = `${first.character} · ${PLACE[first.location] ?? first.location}`;
  if (rest.length === 0) return head;
  const more = rest.reduce((n, h) => n + h.count, 0);
  const alts = new Set(rest.map((h) => h.character_id)).size;
  return `${head} · +${more.toLocaleString()} ${alts > 1 || rest[0].character_id !== first.character_id ? `on ${plural(alts, "alt", "alts")}` : "elsewhere"}`;
}

function WorthSelling({ rows, onPick }: { rows: Sell[] | null; onPick: (id: number) => void }) {
  return (
    <Panel>
      <PanelHeader title="Worth selling">
        <span className="d-grow" />
        <span className="d-dim">on your alts · over 50g</span>
      </PanelHeader>
      {rows && rows.length === 0 ? (
        <PanelBody>
          <p className="d-muted" style={{ margin: 0 }}>
            Nothing your alts carry is worth over 50g at your last scan.
          </p>
        </PanelBody>
      ) : (
        <ul className="ah-sugg">
          {(rows ?? []).slice(0, 8).map((s) => {
            const rough = s.confidence === "rough";
            return (
              <li key={s.item.item_id} className={rough ? "hedge" : undefined} onClick={() => onPick(s.item.item_id)}>
                <ItemIcon item={s.item} />
                <div className="sn">
                  <div className={rough ? undefined : quality(s.item)}>
                    {nameOf(s.item)} ×{s.count.toLocaleString()}
                  </div>
                  <small>{whereLine(s)}</small>
                </div>
                <div className="sv">
                  {rough ? `~${gold(s.value)}?` : `≈ ${gold(s.value)}`}
                  <small className={`ah-conf ${s.confidence}`}>
                    <i />
                    <i />
                    <i />
                    {CONF_LABEL[s.confidence]}
                  </small>
                </div>
                {s.caution && (
                  <div className="why">
                    {s.caution === "stale" ? <Clock size={12} aria-hidden /> : <AlertTriangle size={12} aria-hidden />}
                    {s.caution === "stale"
                      ? `Last seen ${seen(s.item.last_seen)}, so the price may be stale.`
                      : `Only ${plural(s.item.sightings, "sighting", "sightings")}, so the price swings a lot. Check before listing.`}
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}
      <div className="ah-sugg-foot">
        Value = what you carry × your last scanned lowest buyout. Soulbound items are skipped.
      </div>
    </Panel>
  );
}
