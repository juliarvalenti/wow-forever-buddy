import { useCallback, useEffect, useRef, useState } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { commands, type Adventure as AdventureData, type ItemLine } from "@/lib/bindings";
import {
  Button,
  Callout,
  DataTable,
  Page,
  PageHeader,
  PanelBody,
  PanelHeader,
  Record,
} from "@/components/d";
import { useGoodsWorth, usePrices } from "@/hooks/useWorth";
import { ago, errorText, gold, span } from "@/lib/format";
import { Crest, classStyle } from "@/screens/Characters";

// Copy and layout from design/mocks/round-3/session.html. The Gained table's
// "≈ worth" cells show for items with an AH price (F5c), blank otherwise.

const WITHHELD =
  "WoW: Forever keeps some combat details from addons, such as who killed you or which mob dropped an item, so those lines are shorter.";

const MULTI_WORD: Record<string, string> = { DEATHKNIGHT: "Death Knight", DEMONHUNTER: "Demon Hunter" };

/** "PALADIN" → "Paladin"; "NightElf" → "Night Elf". */
export function label(token: string | null): string {
  if (!token) return "";
  if (MULTI_WORD[token]) return MULTI_WORD[token];
  const spaced = token.replace(/([a-z])([A-Z])/g, "$1 $2");
  return spaced === spaced.toUpperCase()
    ? spaced.charAt(0) + spaced.slice(1).toLowerCase()
    : spaced;
}

function clock(iso: string): string {
  return new Date(iso).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
}

function longDay(iso: string): string {
  return new Date(iso).toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" });
}

function shortDay(iso: string): string {
  return new Date(iso).toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

function useAdventure(id: number | null) {
  const [adventure, setAdventure] = useState<AdventureData | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(() => {
    commands.adventureGet(id).then(
      (a) => {
        setAdventure(a);
        setError(null);
      },
      (e) => setError(errorText(e)),
    );
  }, [id]);
  useEffect(load, [load]);
  return { adventure, error, reload: load };
}

/** Which label wins when two crowd each other: a ding over a death over a
 *  boss over a quest. */
const PRIORITY: Record<string, number> = { level: 4, death: 3, encounter: 2, quest: 1 };

/** The chart markers' short labels, as in the mock: "died", the boss's first
 *  word ("Baron"), "ding", "quest". A label sits centred on its line, or
 *  starts just right of it, or pushes the label before it to end just left
 *  of its own line, whichever fits first. When none fit they merge: quests
 *  into "quests", anything else keeps the more important one. */
type Anchor = "start" | "middle" | "end";
type MarkerLabel = { x: number; text: string; kind: string; anchor: Anchor };
const NUDGE = 3;
const GAP = 6;
/** 10px italic Georgia, near enough. */
const textWidth = (s: string) => s.length * 5.6;
function edges(l: MarkerLabel): [number, number] {
  const w = textWidth(l.text);
  if (l.anchor === "start") return [l.x + NUDGE, l.x + NUDGE + w];
  if (l.anchor === "end") return [l.x - NUDGE - w, l.x - NUDGE];
  return [l.x - w / 2, l.x + w / 2];
}
function markerLabels(a: AdventureData, x: (t: number) => number): MarkerLabel[] {
  const out: MarkerLabel[] = [];
  const clear = (l: MarkerLabel, before: MarkerLabel | undefined) =>
    !before || edges(l)[0] >= edges(before)[1] + GAP;
  for (const m of a.markers) {
    const mx = x(new Date(m.at).getTime());
    const text =
      m.kind === "death"
        ? "died"
        : m.kind === "level"
          ? "ding"
          : m.kind === "quest"
            ? "quest"
            : (m.label.replace(/^Defeated /, "").split(" ")[0] ?? "boss");
    const last = out[out.length - 1];
    const placed = (["middle", "start"] as const)
      .map((anchor) => ({ x: mx, text, kind: m.kind, anchor }))
      .find((l) => clear(l, last));
    if (placed) {
      out.push(placed);
      continue;
    }
    // Make room by ending the previous label at its line.
    if (last && last.anchor !== "end") {
      const moved = { ...last, anchor: "end" as const };
      const next = (["middle", "start"] as const)
        .map((anchor) => ({ x: mx, text, kind: m.kind, anchor }))
        .find((l) => clear(l, moved));
      if (next && clear(moved, out[out.length - 2])) {
        out[out.length - 1] = moved;
        out.push(next);
        continue;
      }
    }
    if (last.kind === "quest" && m.kind === "quest") last.text = "quests";
    else if ((PRIORITY[m.kind] ?? 0) > (PRIORITY[last.kind] ?? 0))
      Object.assign(last, { x: mx, text, kind: m.kind, anchor: "middle" });
  }
  return out;
}

/** Gold through the evening, with deaths, bosses, dings and quests marked. */
function MoneyChart({ a }: { a: AdventureData }) {
  const svg = useRef<SVGSVGElement>(null);
  const [width, setWidth] = useState(420);
  useEffect(() => {
    const el = svg.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth || 420));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const pts = a.money.map((p) => ({ t: new Date(p.at).getTime(), g: (p.money ?? 0) / 10_000 }));
  if (pts.length < 2) return null;
  // T leaves room for the marker labels above the plot.
  const H = 104, L = 38, R = 8, T = 18, B = 16;
  const t0 = pts[0].t, t1 = Math.max(pts[pts.length - 1].t, t0 + 1);
  const lo = Math.min(...pts.map((p) => p.g)), hi = Math.max(...pts.map((p) => p.g));
  const pad = Math.max(1, (hi - lo) * 0.15);
  const x = (t: number) => L + ((width - L - R) * (t - t0)) / (t1 - t0);
  const y = (g: number) => T + (H - T - B) * (1 - (g - (lo - pad)) / (hi + pad - (lo - pad)));
  // Gold only changes at a point, so draw steps.
  let d = `M${x(pts[0].t).toFixed(1)},${y(pts[0].g).toFixed(1)}`;
  for (const p of pts.slice(1)) d += `H${x(p.t).toFixed(1)}V${y(p.g).toFixed(1)}`;
  const colour: Record<string, string> = { death: "#b5412e", encounter: "#3a2616", level: "#8e1a16", quest: "#6e5537" };
  return (
    <svg ref={svg} className="d-sg" role="img" aria-label="Gold through the session">
      <g className="grid">
        {[lo, hi].map((g) => (
          <line key={g} x1={L} x2={width - R} y1={y(g)} y2={y(g)} />
        ))}
      </g>
      <g className="axis">
        {[lo, hi].map((g) => (
          <text key={g} x={L - 5} y={y(g) + 3.5} textAnchor="end">
            {Math.round(g).toLocaleString()}g
          </text>
        ))}
        <text x={L} y={H - 3}>
          {clock(a.money[0].at)}
        </text>
        <text x={width - R} y={H - 3} textAnchor="end">
          {clock(a.money[a.money.length - 1].at)}
        </text>
      </g>
      {a.markers.map((m, i) => {
        const mx = x(new Date(m.at).getTime());
        return (
          <line key={i} x1={mx} x2={mx} y1={T} y2={H - B} stroke={colour[m.kind] ?? "#6e5537"} strokeDasharray="2 3">
            <title>
              {clock(m.at)} · {m.label}
            </title>
          </line>
        );
      })}
      <g className="ev">
        {markerLabels(a, x).map((l) => (
          <text
            key={l.x}
            x={l.x + (l.anchor === "end" ? -NUDGE : l.anchor === "start" ? NUDGE : 0)}
            y={T - 5}
            textAnchor={l.anchor}
          >
            {l.text}
          </text>
        ))}
      </g>
      <path d={d} fill="none" stroke="#3a2616" strokeWidth={2} />
    </svg>
  );
}

export function ItemName({ i }: { i: ItemLine }) {
  const q = i.quality;
  return (
    <span className="d-loot">
      <span className={`d-ico${q != null ? ` d-q${q}` : ""}`}>
        <b>{i.name.charAt(0)}</b>
      </span>
      <span className={q != null ? `d-q${q}` : undefined}>{i.name}</span>
    </span>
  );
}

function Note({ a, onSaved }: { a: AdventureData; onSaved: () => void }) {
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(a.note ?? "");
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    setText(a.note ?? "");
    setEditing(false);
  }, [a.id, a.note]);
  const save = async () => {
    try {
      await commands.adventureSetNote(a.id, text);
      setError(null);
      setEditing(false);
      onSaved();
    } catch (e) {
      setError(errorText(e));
    }
  };
  return (
    <div className="d-note">
      <div className="who">
        Your note
        {!editing && (
          <button className="d-link" onClick={() => setEditing(true)}>
            {a.note ? "Edit" : "Add a note"}
          </button>
        )}
      </div>
      {editing ? (
        <>
          <textarea value={text} maxLength={2000} onChange={(e) => setText(e.target.value)} autoFocus />
          <div style={{ display: "flex", gap: 8, marginTop: 6, justifyContent: "flex-end" }}>
            <Button variant="ghost" onClick={() => setEditing(false)}>
              Cancel
            </Button>
            <Button onClick={save}>Save note</Button>
          </div>
          {error && <p className="d-letter-bad">{error}</p>}
        </>
      ) : (
        a.note ?? <span style={{ color: "var(--ink-3)" }}>Nothing yet.</span>
      )}
    </div>
  );
}

const HOW: Record<string, string> = { sold: "sold", used: "used", mailed: "mailed", bought: "bought", mail: "from mail" };

/** One adventure, recapped (V9). `id` null shows the newest. */
export function Adventure({
  id,
  onOpen,
  onOpenDashboard,
  onOpenJournal,
}: {
  id: number | null;
  onOpen: (id: number) => void;
  onOpenDashboard: () => void;
  onOpenJournal: () => void;
}) {
  const { adventure: a, error, reload } = useAdventure(id);
  // F5c: what the gains fetch at the last AH scan (before any early return).
  const prices = usePrices(a ? a.gained.map((i) => i.item_id) : []);
  const scanAt = useGoodsWorth()?.as_of ?? null;

  if (error) {
    return (
      <Page>
        <PageHeader title="Adventures" />
        <Callout tone="bad">{error}</Callout>
      </Page>
    );
  }
  if (a === undefined) return <Page>{null}</Page>;
  if (a === null) {
    return (
      <Page>
        <PageHeader title="Adventures" lede="What happened in each session, written from your logins and logouts." />
        <Record>
          <PanelHeader title="No adventures yet" />
          <PanelBody>
            <p>
              Each time a character logs out with the ForeverBuddy addon, the session is written up here. Install it
              from the Dashboard, then play and log out (or type <span className="d-mono">/reload</span>).
            </p>
            <p style={{ marginTop: 10 }}>
              <Button onClick={onOpenDashboard}>Go to the Dashboard</Button>
            </p>
          </PanelBody>
        </Record>
      </Page>
    );
  }

  const levelled = a.level_start != null && a.level_end != null && a.level_end > a.level_start;
  const withheld = a.timeline.some((l) => l.withheld);
  const repairs = a.tally.repairs ?? 0;
  // Adventures carry file tokens ("PALADIN", "NightElf"); the crest wants the card's form.
  const kin = { class: a.class?.toLowerCase() ?? null, race: label(a.race) || null };

  return (
    <Page>
      <p className="d-crumb">
        {/* The list of adventures is the Ledger's journal. */}
        <button onClick={onOpenJournal} title="All adventures, in the Ledger's journal">
          Adventures
        </button>{" "}
        › {a.name} · {shortDay(a.login)}
      </p>
      <PageHeader
        title={a.title}
        lede={`${longDay(a.login)} · ${clock(a.login)}${a.logout ? ` – ${clock(a.logout)}` : ""} · written from your login and logout snapshots`}
        actions={
          <div style={{ display: "flex", gap: 6 }}>
            <Button variant="ghost" disabled={!a.prev} onClick={() => a.prev && onOpen(a.prev.id)}>
              <ChevronLeft size={14} aria-hidden />
              {a.prev ? `${shortDay(a.prev.login)} · ${a.prev.name}` : "Earliest"}
            </Button>
            <Button variant="ghost" disabled={!a.next} onClick={() => a.next && onOpen(a.next.id)}>
              {a.next ? `${shortDay(a.next.login)} · ${a.next.name}` : "Latest"}
              <ChevronRight size={14} aria-hidden />
            </Button>
          </div>
        }
      />

      <Record>
        <div className="d-adv">
          <div className="col">
            <div className="d-adv-head" style={classStyle(kin)}>
              <Crest c={kin} size={58} />
              <div>
                <div className="nm ch-cc">{a.name}</div>
                <div className="sub">
                  {[label(a.race), label(a.class)].filter(Boolean).join(" ")}
                  {a.level_start != null &&
                    ` · Level ${a.level_start}${levelled ? ` → ${a.level_end}` : ""}`}
                </div>
                <div className="when">
                  {a.played_secs != null && `${span(a.played_secs * 1000)} played`}
                  {a.last_zone && ` · logged out in ${a.last_zone}`}
                </div>
              </div>
              {levelled && (
                <div className="seal" title={`Reached level ${a.level_end}`}>
                  <span>
                    Ding!
                    <b>{a.level_end}</b>
                  </span>
                </div>
              )}
            </div>
            {a.travelled.length > 0 && (
              <div className="d-zones">
                Travelled
                {a.travelled.map((z) => (
                  <span key={z} className="d-chips">
                    <span>{z}</span>
                  </span>
                ))}
              </div>
            )}

            <div className="d-tally">
              <div>
                <div className="k">Gold</div>
                <div className={`v ${(a.tally.gold ?? 0) < 0 ? "d-down" : "d-up"}`}>
                  {a.tally.gold != null ? gold(a.tally.gold, true) : ""}
                </div>
              </div>
              {a.tally.xp != null ? (
                <div>
                  <div className="k">Experience</div>
                  <div className="v">+{a.tally.xp.toLocaleString()}</div>
                </div>
              ) : (a.tally.quest_xp ?? 0) > 0 ? (
                // Across a level-up only the quest rewards are known.
                <div>
                  <div className="k">Quest XP</div>
                  <div className="v">+{(a.tally.quest_xp ?? 0).toLocaleString()}</div>
                </div>
              ) : null}
              <div>
                <div className="k">Loot</div>
                <div className="v">
                  {a.tally.loot.toLocaleString()} <small>{a.tally.loot === 1 ? "item" : "items"}</small>
                </div>
              </div>
              <div>
                <div className="k">Deaths</div>
                <div className="v">
                  {a.tally.deaths}
                  {repairs > 0 && <small> · {gold(-repairs)} repairs</small>}
                </div>
              </div>
            </div>

            {a.money.length > 1 && (
              <>
                <div className="d-sechead">
                  Gold through the session
                  <span>
                    {gold(a.money[0].money ?? 0)} → {gold(a.money[a.money.length - 1].money ?? 0)}
                  </span>
                </div>
                <MoneyChart a={a} />
              </>
            )}

            <div className="d-sechead">How it went</div>
            <ul className="d-tl">
              {a.timeline.map((l, i) => (
                <li
                  key={i}
                  className={
                    l.kind === "death"
                      ? "bad"
                      : l.kind === "loot"
                        ? "loot"
                        : l.kind === "level"
                          ? "ding"
                          : undefined
                  }
                >
                  <span className="t">{clock(l.at)}</span>
                  <span className="d">
                    <span className={l.quality != null ? `d-q${l.quality}` : undefined}>{l.text}</span>
                    {l.withheld && (
                      <i className="d-veil" title={WITHHELD}>
                        ◌
                      </i>
                    )}
                    {l.detail && <small>{l.detail}</small>}
                  </span>
                </li>
              ))}
            </ul>
            {withheld && (
              <p className="d-veil-foot">
                <i className="d-veil">◌</i>
                {WITHHELD}
              </p>
            )}
          </div>

          <div className="col">
            <div className="d-sechead">
              Gained
              <span>{a.gained.reduce((n, i) => n + i.count, 0).toLocaleString()} items</span>
            </div>
            {a.gained.length > 0 ? (
              <DataTable
                className="d-loot-t"
                head={
                  <tr>
                    <th>Item</th>
                    <th className="r">Qty</th>
                    <th />
                    {prices.size > 0 && <th className="r">Worth</th>}
                  </tr>
                }
              >
                {a.gained.map((i) => {
                  const each = prices.get(i.item_id);
                  return (
                    <tr key={`${i.item_id}|${i.how}`}>
                      <td>
                        <ItemName i={i} />
                      </td>
                      <td className="r">{i.count}</td>
                      <td className="x">{i.how ? HOW[i.how] ?? i.how : ""}</td>
                      {prices.size > 0 && (
                        <td
                          className="r"
                          title={!i.equipped && each != null ? `${gold(each)} each at your last scan` : undefined}
                        >
                          {i.equipped ? "equipped" : each != null ? `≈ ${gold(each * i.count)}` : ""}
                        </td>
                      )}
                    </tr>
                  );
                })}
              </DataTable>
            ) : (
              <p className="d-dim">Nothing new in the bags.</p>
            )}
            {prices.size > 0 && scanAt && (
              <p className="d-dim" style={{ marginTop: 6, fontSize: 11.5 }}>
                Worth from your AH scan {ago(scanAt)}.
              </p>
            )}

            <div className="d-sechead">Spent &amp; lost</div>
            {a.spent.length > 0 || repairs > 0 ? (
              <DataTable
                className="d-loot-t"
                head={
                  <tr>
                    <th>Item</th>
                    <th className="r">Qty</th>
                    <th />
                  </tr>
                }
              >
                {a.spent.map((i) => (
                  <tr key={`${i.item_id}|${i.how}`}>
                    <td>
                      <ItemName i={i} />
                    </td>
                    <td className="r">{i.count}</td>
                    <td className="x">{i.how ? HOW[i.how] ?? i.how : ""}</td>
                  </tr>
                ))}
                {repairs > 0 && (
                  <tr>
                    <td>Repairs</td>
                    <td className="r">{gold(-repairs)}</td>
                    <td />
                  </tr>
                )}
              </DataTable>
            ) : (
              <p className="d-dim">Nothing used, sold or mailed.</p>
            )}

            {a.quests.length > 0 && (
              <>
                <div className="d-sechead">
                  Quests completed <span>{a.quests.length}</span>
                </div>
                <ul className="d-quests">
                  {a.quests.map((q, i) => (
                    <li key={i}>
                      {q.title}
                      {q.zone && <span className="r">{q.zone}</span>}
                    </li>
                  ))}
                </ul>
              </>
            )}

            <Note a={a} onSaved={reload} />
          </div>
        </div>
      </Record>
    </Page>
  );
}
