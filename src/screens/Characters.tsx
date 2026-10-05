import { useMemo, useState } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type {
  BagView,
  CharacterCard,
  CharacterSheet,
  ItemRow,
} from "@/lib/bindings";
import {
  Button,
  Callout,
  Page,
  PageHeader,
  Panel,
  PanelBody,
  PanelHeader,
  Record as Parchment,
  Segmented,
} from "@/components/d";
import { useCharacterSheet, useCharacters } from "@/hooks/useCharacters";
import { ago, coins, plural, played, when } from "@/lib/format";

// design/mocks/round-3/characters.html and character.html, with
// IMPLEMENTING.md §7: no net worth or "worth carried" (AH numbers stay
// hidden), no search until satchels are indexed. Every name, zone, item and
// mail line is game text, rendered as React text and never as HTML.

/** Money: gold only for coins. */
export function Coins({ copper, silver = true }: { copper: number | null; silver?: boolean }) {
  const [g, s, c] = coins(copper ?? 0);
  return (
    <span className="ch-coins">
      <span className="g">{g.toLocaleString()}</span>
      {silver && s > 0 && <span className="s">{s}</span>}
      {silver && c > 0 && <span className="c">{c}</span>}
    </span>
  );
}

const CLASS: Record<string, string> = {
  warrior: "Warrior", paladin: "Paladin", hunter: "Hunter", rogue: "Rogue", priest: "Priest",
  shaman: "Shaman", mage: "Mage", warlock: "Warlock", druid: "Druid",
};

const fullName = (c: CharacterCard) => (c.surname ? `${c.name} ${c.surname}` : c.name);
const classStyle = (c: CharacterCard) =>
  ({ "--cc": c.class ? `var(--c-${c.class})` : undefined }) as React.CSSProperties;

/** "Level 30 Gnome Mage", from what's known. */
function classLine(c: CharacterCard): string {
  const parts = [c.level != null ? `Level ${c.level}` : null, c.race, c.class ? CLASS[c.class] ?? c.class : null];
  return parts.filter(Boolean).join(" ");
}

function Crest({ c, size = 64 }: { c: CharacterCard; size?: number }) {
  return (
    <span className="ch-crest" style={{ ...classStyle(c), "--w": `${size}px` } as React.CSSProperties} aria-hidden>
      {c.name.slice(0, 1)}
    </span>
  );
}

type Sort = "level" | "gold" | "seen";

/** Characters: every alt's card, or one character's sheet. */
export function Characters() {
  const { overview, error } = useCharacters();
  const [sort, setSort] = useState<Sort>("gold");
  const [open, setOpen] = useState<number | null>(null);

  const cards = useMemo(() => {
    const list = [...(overview?.characters ?? [])];
    const by: Record<Sort, (a: CharacterCard, b: CharacterCard) => number> = {
      level: (a, b) => (b.level ?? 0) - (a.level ?? 0),
      gold: (a, b) => (b.money ?? 0) - (a.money ?? 0),
      seen: (a, b) => b.last_seen.localeCompare(a.last_seen),
    };
    return list.sort(by[sort]);
  }, [overview, sort]);

  if (open != null) {
    return <Sheet id={open} cards={cards} onOpen={setOpen} onBack={() => setOpen(null)} />;
  }

  return (
    <Page>
      <PageHeader
        title="Characters"
        lede="Everything your alts carry, as of their last logout."
        actions={
          overview && overview.characters.length > 0 ? (
            <span className="ch-totals">
              <span>
                Gold <b><Coins copper={overview.gold} silver={false} /></b>
              </span>
              <span>
                Items <b>{overview.items.toLocaleString()}</b>
              </span>
            </span>
          ) : undefined
        }
      />
      {error && <Callout tone="bad">{error}</Callout>}
      {overview && overview.characters.length === 0 ? (
        <Panel>
          <PanelBody>
            <p className="d-muted">
              No character notes yet. Once the ForeverBuddy addon is installed and you log out of a
              character, its gold, gear and bags appear here.
            </p>
          </PanelBody>
        </Panel>
      ) : (
        <>
          <div className="ch-toolbar">
            Sort
            <Segmented<Sort>
              value={sort}
              onChange={setSort}
              options={[
                { value: "level", label: "Level" },
                { value: "gold", label: "Gold" },
                { value: "seen", label: "Last seen" },
              ]}
            />
          </div>
          <section className="ch-cards">
            {cards.map((c) => (
              <Card key={c.id} c={c} onOpen={() => setOpen(c.id)} />
            ))}
          </section>
        </>
      )}
    </Page>
  );
}

function Progress({ c }: { c: CharacterCard }) {
  // Levelling: XP and rested; at the cap: item level.
  if (c.xp != null && c.xp_max != null && c.xp_max > 0) {
    const pct = Math.min(100, (c.xp / c.xp_max) * 100);
    const rested = c.rested != null ? Math.min(100, ((c.xp + c.rested) / c.xp_max) * 100) : 0;
    return (
      <div className="ch-prog">
        <div className="lbl">
          <span>{c.level != null ? `${Math.floor(pct)}% to ${c.level + 1}` : `${Math.floor(pct)}%`}</span>
          {c.rested != null && c.rested > 0 && (
            <span className="rest">Rested {Math.round((c.rested / c.xp_max) * 100)}%</span>
          )}
        </div>
        <div className="ch-bar">
          <i className="rested" style={{ width: `${rested}%` }} />
          <i className="xp" style={{ width: `${pct}%` }} />
        </div>
      </div>
    );
  }
  if (c.ilvl != null) {
    return (
      <div className="ch-prog">
        <div className="lbl">
          <span>Item level</span>
          <b>{c.ilvl.toFixed(1)}</b>
        </div>
        <div className="ch-bar">
          <i className="ilvl" style={{ width: `${Math.min(100, (c.ilvl / 80) * 100)}%` }} />
        </div>
      </div>
    );
  }
  return <div className="ch-prog" />;
}

function Card({ c, onOpen }: { c: CharacterCard; onOpen: () => void }) {
  const where = c.subzone ?? c.zone;
  return (
    <button className="d-panel ch-card" onClick={onOpen} style={classStyle(c)}>
      <div className="ch-id">
        <Crest c={c} />
        <div>
          <div className="ch-nm ch-cc">{fullName(c)}</div>
          <div className="ch-cl">{classLine(c)}</div>
          <div className="ch-loc">
            {where ? `${where} · ` : ""}
            {ago(c.last_seen)}
          </div>
        </div>
      </div>
      <Progress c={c} />
      <div className="ch-facts">
        <div>
          Gold
          <b><Coins copper={c.money} silver={false} /></b>
        </div>
        <div>
          Satchels
          <b>{c.bag_free != null ? `${c.bag_free} free` : "Not seen"}</b>
        </div>
        <div>
          {c.mail > 0 ? "Mail" : "Played"}
          <b>{c.mail > 0 ? c.mail : c.played != null ? played(c.played) : "Not seen"}</b>
        </div>
      </div>
    </button>
  );
}

type Tab = "gear" | "satchels" | "bank" | "mail" | "professions";

const SLOTS: Record<number, string> = {
  1: "Head", 2: "Neck", 3: "Shoulder", 4: "Shirt", 5: "Chest", 6: "Waist", 7: "Legs", 8: "Feet",
  9: "Wrist", 10: "Hands", 11: "Finger", 12: "Finger", 13: "Trinket", 14: "Trinket", 15: "Back",
  16: "Main hand", 17: "Off hand", 18: "Ranged", 19: "Tabard",
};

/** "As of your last bank visit, 2 Oct"; ember past 7 days (IMPLEMENTING.md §7). */
function Freshness({ asOf, place }: { asOf: string | null; place: "bank" | "mailbox" }) {
  if (!asOf) {
    const what = place === "bank" ? "your bank" : "your mailbox";
    return (
      <p className="ch-fresh">
        Not seen yet. Open {what} once in-game and it appears here.
      </p>
    );
  }
  const old = Date.now() - new Date(asOf).getTime() > 7 * 86400000;
  const day = new Date(asOf).toLocaleDateString(undefined, { day: "numeric", month: "short" });
  return (
    <p className={`ch-fresh${old ? " old" : ""}`}>
      As of your last {place} visit, {day}
      {old ? `. Visit the ${place === "bank" ? "bank" : "mailbox"} in-game to refresh.` : ""}
    </p>
  );
}

function Slot({ item, label }: { item: ItemRow; label?: string }) {
  const q = item.quality != null ? `ch-q${item.quality}` : "";
  return (
    <div className="ch-slot">
      <span className={`ch-ico ${q}`} aria-hidden>
        <b>{item.name.slice(0, 1)}</b>
      </span>
      <div className="t">
        <div className={q}>{item.name}</div>
        <small>{label ?? (item.count > 1 ? `× ${item.count}` : "")}</small>
      </div>
      <span className="il">{item.ilvl ?? ""}</span>
    </div>
  );
}

function Bags({ bags, empty }: { bags: BagView[]; empty: string }) {
  if (bags.every((b) => b.items.length === 0)) return <p className="ch-empty">{empty}</p>;
  return (
    <>
      {bags.map((b) => (
        <div key={b.container}>
          <div className="ch-sec">
            {b.name ?? (b.container === 0 ? "Backpack" : `Bag ${b.container}`)}
            {b.size != null && ` · ${b.size - (b.free ?? 0)} of ${b.size} used`}
          </div>
          <div className="ch-doll">
            {b.items.map((i) => (
              <Slot key={`${i.container}-${i.slot}`} item={i} />
            ))}
          </div>
        </div>
      ))}
    </>
  );
}

function GoldSpark({ sheet }: { sheet: CharacterSheet }) {
  const pts = sheet.gold_30d;
  if (pts.length < 2) return <p className="d-dim">Not enough logouts yet for a trend.</p>;
  const xs = pts.map((p) => new Date(p.at).getTime());
  const ys = pts.map((p) => p.money ?? 0);
  const [x0, x1] = [Math.min(...xs), Math.max(...xs)];
  const [y0, y1] = [Math.min(...ys), Math.max(...ys)];
  const sx = (x: number) => (x1 === x0 ? 0 : ((x - x0) / (x1 - x0)) * 270);
  const sy = (y: number) => (y1 === y0 ? 28 : 50 - ((y - y0) / (y1 - y0)) * 44);
  const coords = pts.map((_, i) => `${sx(xs[i])},${sy(ys[i])}`);
  const line = coords.join(" ");
  return (
    <div className="ch-wallet">
      <svg viewBox="0 0 270 56" preserveAspectRatio="none" aria-hidden>
        <path d={`M0,56 L${coords.join(" L")} L270,56Z`} fill="rgba(255,140,50,.08)" />
        <polyline fill="none" stroke="#ffb15c" strokeWidth="1.5" vectorEffect="non-scaling-stroke" points={line} />
      </svg>
    </div>
  );
}

function Sheet({
  id,
  cards,
  onOpen,
  onBack,
}: {
  id: number;
  cards: CharacterCard[];
  onOpen: (id: number) => void;
  onBack: () => void;
}) {
  const { sheet, error } = useCharacterSheet(id);
  const [tab, setTab] = useState<Tab>("gear");
  const at = cards.findIndex((c) => c.id === id);
  const prev = at > 0 ? cards[at - 1] : null;
  const next = at >= 0 && at < cards.length - 1 ? cards[at + 1] : null;

  const c = sheet?.card;
  const satchelsUsed = sheet?.bags.reduce((n, b) => n + (b.size ?? 0) - (b.free ?? 0), 0) ?? 0;
  const satchelsSize = sheet?.bags.reduce((n, b) => n + (b.size ?? 0), 0) ?? 0;
  const bankCount = sheet?.bank.bags.reduce((n, b) => n + b.items.length, 0) ?? 0;
  const change =
    sheet && sheet.gold_30d.length > 1
      ? (sheet.gold_30d[sheet.gold_30d.length - 1].money ?? 0) - (sheet.gold_30d[0].money ?? 0)
      : null;

  return (
    <Page>
      <div className="ch-topbar">
        <div className="ch-crumbs">
          <button onClick={onBack}>Characters</button>
          <ChevronRight size={13} aria-hidden />
          <span>{c ? fullName(c) : "…"}</span>
        </div>
        <div style={{ display: "flex", gap: 6 }}>
          <Button variant="icon" title="Previous alt" disabled={!prev} onClick={() => prev && onOpen(prev.id)}>
            <ChevronLeft size={14} />
          </Button>
          <Button variant="icon" title="Next alt" disabled={!next} onClick={() => next && onOpen(next.id)}>
            <ChevronRight size={14} />
          </Button>
        </div>
      </div>
      {error && <Callout tone="bad">{error}</Callout>}
      {!sheet && !error && <p className="d-muted">Loading…</p>}
      {sheet && c && (
        <div className="ch-body">
          <Parchment tilt>
           <div className="ch-sheet">
            <div className="ch-ident" style={classStyle(c)}>
              <Crest c={c} size={92} />
              <div>
                <h1 className="ch-cc">{fullName(c)}</h1>
                <div className="ch-sub">
                  <span>
                    {classLine(c)}
                    {c.guild ? ` · <${c.guild}>` : ""}
                  </span>
                  <span className="ch-pill">
                    Last seen{c.subzone ?? c.zone ? ` in ${c.subzone ?? c.zone}` : ""} · {when(c.last_seen)}
                  </span>
                </div>
              </div>
            </div>
            <div className="ch-stats">
              <div>
                <div className="k">Gold</div>
                <div className="v"><Coins copper={c.money} /></div>
              </div>
              <div>
                <div className="k">Item level</div>
                <div className="v">{c.ilvl != null ? c.ilvl.toFixed(1) : "Not seen"}</div>
              </div>
              <div>
                <div className="k">Time played</div>
                <div className="v">{c.played != null ? played(c.played) : "Not seen"}</div>
              </div>
              <div>
                <div className="k">Rested</div>
                <div className="v">
                  {c.xp_max == null
                    ? "Max level"
                    : c.rested
                      ? `${Math.round((c.rested / c.xp_max) * 100)}%`
                      : "None"}
                </div>
              </div>
            </div>
            <nav className="ch-tabs" role="tablist">
              {(
                [
                  ["gear", "Gear", null],
                  ["satchels", "Satchels", satchelsSize > 0 ? `${satchelsUsed}/${satchelsSize}` : null],
                  ["bank", "Bank", bankCount > 0 ? bankCount : null],
                  ["mail", "Mail", sheet.mail.messages.length > 0 ? sheet.mail.messages.length : null],
                  ["professions", "Professions", null],
                ] as [Tab, string, string | number | null][]
              ).map(([key, label, n]) => (
                <button key={key} role="tab" aria-selected={tab === key} onClick={() => setTab(key)}>
                  {label}
                  {n != null && <span className="n">{n}</span>}
                </button>
              ))}
              <span className="stamp">{ago(c.last_seen)}</span>
            </nav>
            {tab === "gear" &&
              (sheet.equipped.length === 0 ? (
                <p className="ch-empty">No gear recorded yet.</p>
              ) : (
                <div className="ch-doll">
                  {sheet.equipped.map((i) => (
                    <Slot key={i.slot} item={i} label={SLOTS[i.slot] ?? `Slot ${i.slot}`} />
                  ))}
                </div>
              ))}
            {tab === "satchels" && <Bags bags={sheet.bags} empty="No satchels recorded yet." />}
            {tab === "bank" && (
              <>
                <Freshness asOf={sheet.bank.as_of} place="bank" />
                {sheet.bank.as_of && <Bags bags={sheet.bank.bags} empty="The bank was empty." />}
              </>
            )}
            {tab === "mail" && (
              <>
                <Freshness asOf={sheet.mail.as_of} place="mailbox" />
                {sheet.mail.as_of && sheet.mail.messages.length === 0 && (
                  <p className="ch-empty">The mailbox was empty.</p>
                )}
                {sheet.mail.messages.map((m, i) => (
                  <div key={i} className="ch-mail">
                    <div className="who">{m.sender ?? "Unknown sender"}</div>
                    {m.subject && <div className="what">{m.subject}</div>}
                    {(m.money ?? 0) > 0 && <Coins copper={m.money} />}
                    {m.items.map((it) => (
                      <Slot key={`${it.container}-${it.slot}`} item={it} />
                    ))}
                  </div>
                ))}
              </>
            )}
            {tab === "professions" &&
              (sheet.professions.length === 0 ? (
                <p className="ch-empty">No professions recorded yet.</p>
              ) : (
                sheet.professions.map((p) => (
                  <div key={p.name} className="ch-slot" style={{ gridTemplateColumns: "1fr auto" }}>
                    <div className="t">
                      <div>{p.name}</div>
                    </div>
                    <span className="il">
                      {p.skill ?? "?"} / {p.max ?? "?"}
                    </span>
                  </div>
                ))
              ))}
           </div>
          </Parchment>
          <div className="ch-side">
            <Panel>
              <PanelHeader title="Satchels">
                {satchelsSize > 0 && <span className="d-dim">{satchelsUsed} of {satchelsSize} used</span>}
              </PanelHeader>
              <PanelBody>
                {sheet.bags.length === 0 ? (
                  <p className="d-dim">Not seen yet.</p>
                ) : (
                  sheet.bags.map((b) => (
                    <div key={b.container} className="ch-bagrow">
                      <span>{b.name ?? (b.container === 0 ? "Backpack" : `Bag ${b.container}`)}</span>
                      <span className="free">
                        {b.free != null ? `${plural(b.free, "free slot", "free slots")}` : ""}
                      </span>
                    </div>
                  ))
                )}
              </PanelBody>
            </Panel>
            <Panel>
              <PanelHeader title="Gold, 30 days">
                {change != null && change !== 0 && (
                  <span className={change > 0 ? "ch-up" : "ch-down"}>
                    {change > 0 ? "▲ " : "▼ "}
                    <Coins copper={Math.abs(change)} silver={false} />
                  </span>
                )}
              </PanelHeader>
              <PanelBody>
                <GoldSpark sheet={sheet} />
              </PanelBody>
            </Panel>
            <Panel>
              <PanelHeader title="Professions" />
              <PanelBody>
                {sheet.professions.length === 0 ? (
                  <p className="d-dim">None recorded yet.</p>
                ) : (
                  <ul className="ch-prof">
                    {sheet.professions.map((p) => (
                      <li key={p.name}>
                        <div>
                          <span>{p.name}</span>
                          <span className="muted">
                            {p.skill ?? "?"} / {p.max ?? "?"}
                          </span>
                        </div>
                        <div className="ch-bar">
                          <i
                            className="fill"
                            style={{
                              width: `${p.skill != null && p.max ? Math.min(100, (p.skill / p.max) * 100) : 0}%`,
                            }}
                          />
                        </div>
                      </li>
                    ))}
                  </ul>
                )}
              </PanelBody>
            </Panel>
          </div>
        </div>
      )}
    </Page>
  );
}
