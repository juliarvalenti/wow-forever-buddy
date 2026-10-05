import { useEffect, useRef, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import {
  commands,
  type Chart,
  type JournalEntry,
  type LedgerExport,
  type LedgerRange,
} from "@/lib/bindings";
import {
  Button,
  Callout,
  DataTable,
  Page,
  PageHeader,
  PanelBody,
  PanelHeader,
  Record,
  Segmented,
  Tile,
} from "@/components/d";
import { useCharacters } from "@/hooks/useCharacters";
import { useLedger } from "@/hooks/useLedger";
import { classStyle } from "@/screens/Characters";
import { coins, errorText, gold, plural, sessionWhen, span } from "@/lib/format";

// Copy and layout from design/mocks/round-3/gold.html, with the v0.2 rules in
// IMPLEMENTING.md §7: three tiles, no Net worth, the chart at full width.

const RANGES: { value: LedgerRange; label: string; days: string }[] = [
  { value: "week", label: "7 days", days: "7 days" },
  { value: "month", label: "30 days", days: "30 days" },
  { value: "quarter", label: "90 days", days: "90 days" },
  { value: "all", label: "All", days: "all time" },
];

/** The mock's validated paper palette: top four, then "others" dashed. */
const COLORS = ["#8a4e0e", "#b0306e", "#e09a2a", "#5f8f1a"];
/** End-label text: the line colour, darkened where it's too light to read on parchment. */
const LABELS = ["#8a4e0e", "#b0306e", "#b8761a", "#5f8f1a"];
const OTHERS = "#8a7a62";
const TOTAL = "#3a2616";

function shortDate(day: string | Date): string {
  const d = typeof day === "string" ? new Date(`${day}T12:00:00`) : day;
  return d.toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

/** "6,812 47 09" in coins, for the Account gold tile (here and on the
 *  Dashboard). */
export function Coins({ copper }: { copper: number }) {
  const [g, s, c] = coins(copper);
  return (
    <span className="d-coins">
      <span className="g">{g.toLocaleString()}</span>
      <span className="s">{String(s).padStart(2, "0")}</span>
      <span className="c">{String(c).padStart(2, "0")}</span>
    </span>
  );
}

function Delta({ copper }: { copper: number }) {
  return <span className={copper < 0 ? "d-down" : copper > 0 ? "d-up" : undefined}>{gold(copper, true)}</span>;
}

/** Rounds up to a step that gives about four gridlines. */
function niceMax(v: number): number {
  if (v <= 0) return 1;
  const step = 10 ** Math.floor(Math.log10(v / 4));
  const nice = [1, 2, 2.5, 5, 10].map((m) => m * step).find((s) => s * 4 >= v) ?? 10 * step;
  return nice * 4;
}

function GoldChart({ chart }: { chart: Chart }) {
  const svg = useRef<SVGSVGElement>(null);
  const [width, setWidth] = useState(760);
  const [hover, setHover] = useState<number | null>(null);
  useEffect(() => {
    const el = svg.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth || 760));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const n = chart.days.length;
  // R leaves room for the end labels ("Velyra Duskmane 1,066").
  const H = 250, L = 44, R = 168, T = 8, B = 22;
  const toGold = (c: number) => c / 10_000;
  const max = niceMax(Math.max(...chart.account.map((c) => toGold(c ?? 0)), 1) * 1.04);
  const x = (i: number) => L + ((width - L - R) * i) / Math.max(1, n - 1);
  const y = (g: number) => T + (H - T - B) * (1 - g / max);
  const path = (values: (number | null)[]) => {
    let d = "";
    let pen = false;
    values.forEach((v, i) => {
      if (v == null) {
        pen = false;
        return;
      }
      d += `${pen ? "L" : "M"}${x(i).toFixed(1)},${y(toGold(v)).toFixed(1)}`;
      pen = true;
    });
    return d;
  };
  const lines = chart.series.map((s, k) => ({
    name: s.name,
    color: s.character_id == null ? OTHERS : COLORS[k % COLORS.length],
    text: s.character_id == null ? OTHERS : LABELS[k % LABELS.length],
    dash: s.character_id == null,
    values: s.values,
  }));
  const ticks = [0, 1, 2, 3, 4].map((k) => (max * k) / 4);
  const xTicks = n <= 1 ? [0] : [0, 0.25, 0.5, 0.75, 1].map((f) => Math.round(f * (n - 1)));
  // Direct labels at the line ends, nudged apart.
  const ends = [
    { name: "Total", color: TOTAL, v: chart.account[n - 1] ?? 0 },
    ...lines.map((l) => ({ name: l.name, color: l.text, v: l.values[n - 1] ?? 0 })),
  ]
    .map((e) => ({ ...e, ty: y(toGold(e.v)) }))
    .sort((a, b) => a.ty - b.ty);
  for (let k = 1; k < ends.length; k++) {
    if (ends[k].ty - ends[k - 1].ty < 13) ends[k].ty = ends[k - 1].ty + 13;
  }

  const onMove = (ev: React.MouseEvent<SVGRectElement>) => {
    const r = svg.current!.getBoundingClientRect();
    const i = Math.round(((ev.clientX - r.left - L) / (width - L - R)) * (n - 1));
    setHover(Math.max(0, Math.min(n - 1, i)));
  };
  const tipLeft = hover == null ? 0 : x(hover) + 14 + 168 > width ? x(hover) - 184 : x(hover) + 14;

  return (
    <div className="d-chartwrap">
      <div className="d-legend">
        <span>
          <i className="tot" style={{ borderColor: TOTAL }} />
          Account total
        </span>
        {lines.map((l) => (
          <span key={l.name}>
            <i className={l.dash ? "dash" : undefined} style={{ borderColor: l.color }} />
            {l.name}
          </span>
        ))}
      </div>
      <svg ref={svg} className="d-chart" role="img" aria-label="Gold over time">
        <g className="grid">
          {ticks.map((t) => (
            <line key={t} x1={L} x2={width - R} y1={y(t)} y2={y(t)} />
          ))}
        </g>
        <g className="axis">
          {ticks.map((t) => (
            <text key={t} x={L - 6} y={y(t) + 3.5} textAnchor="end">
              {t >= 1000 ? `${Math.round(t / 100) / 10}k` : Math.round(t)}
            </text>
          ))}
          {xTicks.map((i) => (
            <text key={i} x={x(i)} y={H - 4} textAnchor="middle">
              {shortDate(chart.days[i])}
            </text>
          ))}
        </g>
        {lines.map((l) => (
          <path
            key={l.name}
            d={path(l.values)}
            fill="none"
            stroke={l.color}
            strokeWidth={2}
            strokeDasharray={l.dash ? "5 4" : undefined}
            strokeLinejoin="round"
          />
        ))}
        <path d={path(chart.account)} fill="none" stroke={TOTAL} strokeWidth={3} strokeLinejoin="round" />
        {ends.map((e) => (
          <text key={e.name} className="lbl" x={width - R + 8} y={e.ty + 4} fill={e.color}>
            {e.name} <tspan className="v">{Math.round(toGold(e.v)).toLocaleString()}</tspan>
          </text>
        ))}
        {hover != null && <line className="cross" x1={x(hover)} x2={x(hover)} y1={T} y2={H - B} />}
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
      {hover != null && (
        <div className="d-ctip" style={{ left: tipLeft + 14, top: 40 }}>
          <div className="d">{shortDate(chart.days[hover])}</div>
          {lines.map((l) => (
            <div key={l.name} className="r">
              <span>
                <i style={{ borderColor: l.color }} />
                {l.name}
              </span>
              <span>{l.values[hover] == null ? "" : gold(l.values[hover]!)}</span>
            </div>
          ))}
          <div className="r tot">
            <span>Account</span>
            <span>{gold(chart.account[hover] ?? 0)}</span>
          </div>
        </div>
      )}
    </div>
  );
}

function GoldTable({ chart }: { chart: Chart }) {
  const rows = chart.days.map((d, i) => ({ d, i })).reverse();
  return (
    <DataTable
      head={
        <tr>
          <th>Day</th>
          {chart.series.map((s) => (
            <th key={s.name} className="r">
              {s.name}
            </th>
          ))}
          <th className="r">Account</th>
        </tr>
      }
      className="d-journal"
    >
      {rows.map(({ d, i }) => (
        <tr key={d}>
          <td>{shortDate(d)}</td>
          {chart.series.map((s) => (
            <td key={s.name} className="r">
              {s.values[i] == null ? "" : gold(s.values[i]!)}
            </td>
          ))}
          <td className="r">
            <b>{gold(chart.account[i] ?? 0)}</b>
          </td>
        </tr>
      ))}
    </DataTable>
  );
}

function JournalRow({
  e,
  cls,
  onOpen,
}: {
  e: JournalEntry;
  cls: string | null;
  onOpen: (id: number) => void;
}) {
  const when = sessionWhen(e.login, e.logout);
  const cut = when.indexOf(", ");
  const note = e.of_note;
  return (
    <tr className="d-open" title="Open this adventure" onClick={() => onOpen(e.adventure_id)}>
      <td className="when">
        <b>{cut > 0 ? when.slice(0, cut) : when}</b>
        <small>{cut > 0 ? when.slice(cut + 2) : ""}</small>
      </td>
      <td>
        <span className="who">
          <span className="ch-cc" style={classStyle({ class: cls })}>
            {e.name}
          </span>
          {e.level != null && (
            <span className="d-wax" title={`Reached level ${e.level}`}>
              {e.level}
            </span>
          )}
        </span>
      </td>
      <td>{e.played_secs != null ? span(e.played_secs * 1000) : ""}</td>
      <td className="r gold-d">{e.gold_delta != null && <Delta copper={e.gold_delta} />}</td>
      <td>
        {note &&
          (note.quality != null ? (
            <span className="d-loot">
              <span className={`d-ico d-q${note.quality}`}>
                <b>{note.text.charAt(0)}</b>
              </span>
              <span className={`d-q${note.quality}`}>{note.text}</span>
            </span>
          ) : (
            note.text
          ))}
      </td>
    </tr>
  );
}

/** Gold and history across characters (V8). */
export function Ledger({
  onOpenDashboard,
  onOpenAdventure,
}: {
  onOpenDashboard: () => void;
  onOpenAdventure: (id: number) => void;
}) {
  const [range, setRange] = useState<LedgerRange>("month");
  const [view, setView] = useState<"chart" | "table">("chart");
  const { ledger, error } = useLedger(range);
  // The ledger rows carry ids, not classes; the overview has each character's class.
  const { overview } = useCharacters();
  const classOf = (id: number | null | undefined) =>
    overview?.characters.find((c) => c.id === id)?.class ?? null;
  const [saved, setSaved] = useState<string | null>(null);
  const [exportError, setExportError] = useState<string | null>(null);

  const exportCsv = async (kind: LedgerExport) => {
    setSaved(null);
    setExportError(null);
    try {
      const today = new Date().toISOString().slice(0, 10);
      const dest = await save({
        defaultPath: `forever-buddy-${kind}-${today}.csv`,
        filters: [{ name: "CSV", extensions: ["csv"] }],
      });
      if (!dest) return;
      setSaved(await commands.ledgerExportCsv(range, kind, dest));
    } catch (e) {
      setExportError(errorText(e));
    }
  };

  const empty = ledger != null && ledger.since == null;
  const tiles = ledger?.tiles;
  const chart = ledger?.chart;
  const rangeDays = RANGES.find((r) => r.value === range)!.days;

  return (
    <Page>
      <PageHeader
        title="Ledger"
        lede={
          ledger?.since
            ? `Gold & history across your characters, from every logout since ${shortDate(new Date(ledger.since))}.`
            : "Gold & history across your characters."
        }
        actions={
          !empty && (
            <Button onClick={() => exportCsv("gold")} disabled={!ledger}>
              Export CSV
            </Button>
          )
        }
      />
      {error && <Callout tone="bad">{error}</Callout>}
      {exportError && <Callout tone="bad">{exportError}</Callout>}
      {saved && <p className="d-dim">Saved to {saved}</p>}

      {empty ? (
        <Record>
          <PanelHeader title="Your ledger is blank" />
          <PanelBody>
            <p>
              Gold is noted each time a character logs out with the ForeverBuddy addon. Install it from
              the Dashboard, then log in and out once (or type <span className="d-mono">/reload</span>).
            </p>
            <p style={{ marginTop: 10 }}>
              <Button onClick={onOpenDashboard}>Go to the Dashboard</Button>
            </p>
          </PanelBody>
        </Record>
      ) : (
        <>
          <section className="d-strip" style={{ gridTemplateColumns: "repeat(3, minmax(0, 1fr))" }}>
            <Tile
              label="Account gold"
              value={tiles ? <Coins copper={tiles.account_gold ?? 0} /> : "…"}
              sub={tiles && `across ${plural(tiles.characters, "character", "characters")}`}
            />
            <Tile
              label="Last 30 days"
              value={tiles ? gold(tiles.last_30_days ?? 0, true) : "…"}
              sub={tiles && <>{gold(tiles.this_week ?? 0, true)} this week</>}
            />
            <Tile
              label="Best earner"
              value={
                tiles?.best_earner ? (
                  <span className="ch-cc" style={classStyle({ class: classOf(tiles.best_earner.character_id) })}>
                    {tiles.best_earner.name}
                  </span>
                ) : tiles ? (
                  "None yet"
                ) : (
                  "…"
                )
              }
              sub={
                tiles?.best_earner &&
                `${gold(tiles.best_earner.gained ?? 0, true)} in 30 days · ${plural(tiles.best_earner.sessions, "session", "sessions")}`
              }
            />
          </section>

          <div className="d-toolbar" style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <Segmented options={RANGES} value={range} onChange={setRange} />
            <Segmented
              options={[
                { value: "chart", label: "Chart" },
                { value: "table", label: "Table" },
              ]}
              value={view}
              onChange={setView}
            />
            <span className="d-grow" />
            <span className="d-dim">Gold is read from SavedVariables at each logout</span>
          </div>

          <Record>
            <PanelHeader title="Gold over time">
              <span className="d-grow" />
              <span>
                {chart && chart.days.length > 0
                  ? `${shortDate(chart.days[0])} – ${shortDate(chart.days[chart.days.length - 1])} · per character`
                  : ""}
              </span>
            </PanelHeader>
            {chart && chart.days.length > 0 && (view === "chart" ? <GoldChart chart={chart} /> : <GoldTable chart={chart} />)}
          </Record>

          <Record ruled>
            <PanelHeader title="Journal">
              <span className="d-grow" />
              <span>
                {ledger &&
                  `${plural(ledger.journal.length, "adventure", "adventures")} in ${rangeDays} · newest first`}
              </span>
              {ledger && ledger.journal.length > 0 && (
                <button className="d-link" style={{ marginLeft: 12 }} onClick={() => exportCsv("journal")}>
                  Export journal
                </button>
              )}
            </PanelHeader>
            {ledger && ledger.journal.length > 0 ? (
              <DataTable
                className="d-journal"
                head={
                  <tr>
                    <th>When</th>
                    <th>Character</th>
                    <th>Played</th>
                    <th className="r">Gold</th>
                    <th>Of note</th>
                  </tr>
                }
              >
                {ledger.journal.map((e) => (
                  <JournalRow key={e.adventure_id} e={e} cls={classOf(e.character_id)} onOpen={onOpenAdventure} />
                ))}
              </DataTable>
            ) : (
              <PanelBody>
                <p className="d-dim">No adventures in {rangeDays}.</p>
              </PanelBody>
            )}
          </Record>
        </>
      )}
    </Page>
  );
}
