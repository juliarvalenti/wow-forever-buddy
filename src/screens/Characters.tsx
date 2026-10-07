import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  Backpack,
  ChevronLeft,
  ChevronRight,
  Image as ImageIcon,
  Landmark,
  type LucideIcon,
  Mail,
  Puzzle,
  Search,
} from "lucide-react";
import {
  type BagView,
  type CharacterCard,
  type CharacterSheet,
  commands,
  type ItemRow,
  type Lockout,
  type Mark,
  type Marked,
  type Plan,
  type QuestEntry,
  type SearchResults,
  type WtfCharacter,
} from "@/lib/bindings";
import {
  Button,
  Callout,
  LiveDot,
  Page,
  PageHeader,
  Panel,
  PanelBody,
  PanelHeader,
  Record as Parchment,
  Segmented,
  StatusDot,
  Switch,
  ItemIcon,
} from "@/components/d";
import { useAddon } from "@/hooks/useAddon";
import {
  type Filters,
  NO_FILTERS,
  useCharacterSheet,
  useCharacters,
  useItemSearch,
  useRoster,
} from "@/hooks/useCharacters";
import { useCleanup } from "@/hooks/useCleanup";
import { useQuestLog, useQuestPlan, useQuestsAvailable } from "@/hooks/useQuests";
import { useGoodsWorth } from "@/hooks/useWorth";
import { useSettings } from "@/hooks/useSettings";
import { BagCleanup, MarkMenu, MarkTag, type Who } from "@/screens/BagCleanup";
import { LoginNotes } from "@/screens/LoginNotes";
import {
  ago,
  characterName,
  coins,
  errorText,
  plural,
  played,
  resetDay,
  resetsIn,
  when,
} from "@/lib/format";

// design/mocks/round-3/characters.html and character.html. Net worth and
// "Worth carried" show only once AH prices exist (F5c), from priced items
// only (IMPLEMENTING.md §7: never zeros). Search shows once the addon has filled in a
// character, since that's what indexes satchels. Every name, zone, item and
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
/** Who a crest or a class-coloured name is for: lower-case class token, race label. */
type Kin = { class: string | null; race?: string | null };

/** `--cc`, the class colour, for `.ch-cc` names and the crest. */
export const classStyle = (c: Kin) =>
  ({ "--cc": c.class ? `var(--c-${c.class})` : undefined }) as React.CSSProperties;

/** "Level 30 Gnome Mage", from what's known. */
function classLine(c: CharacterCard): string {
  const parts = [c.level != null ? `Level ${c.level}` : null, c.race, c.class ? CLASS[c.class] ?? c.class : null];
  return parts.filter(Boolean).join(" ");
}

// The class crest (design/mocks/round-3/_shell.js `GLYPH` and `crest()`):
// a shield in the class tint with the class glyph. Classes without a glyph
// yet show the shield alone.
const GLYPH: Record<string, React.ReactNode> = {
  paladin: (
    <>
      <path d="M8 5h8v5H8z" />
      <path d="M11 10h2v10h-2z" />
    </>
  ),
  druid: (
    <>
      <circle cx="8" cy="8" r="1.8" />
      <circle cx="12" cy="6.5" r="1.8" />
      <circle cx="16" cy="8" r="1.8" />
      <path d="M12 11c3 0 5 3 5 5.5 0 1.7-1.5 2.5-3 2-1.3-.4-2.7-.4-4 0-1.5.5-3-.3-3-2C7 14 9 11 12 11z" />
    </>
  ),
  hunter: (
    <>
      <path d="M6 18L17 7" strokeWidth="2" />
      <path d="M13 6h5v5z" />
      <path d="M5 16l3 3-3 1z" />
    </>
  ),
  mage: <path d="M12 3l2 6.5L20.5 12 14 14.5 12 21l-2-6.5L3.5 12 10 9.5z" />,
  priest: (
    <>
      <circle cx="12" cy="12" r="4.5" />
      <path d="M12 3v3M12 18v3M3 12h3M18 12h3" strokeWidth="2" />
    </>
  ),
  rogue: (
    <>
      <path d="M12 3l2 3v9h-4V6z" />
      <path d="M8 15h8v2H8z" />
      <path d="M11 17h2v4h-2z" />
    </>
  ),
  warrior: (
    <>
      <path d="M5 5l14 14M19 5L5 19" strokeWidth="2.2" />
      <path d="M4 8l4-4M16 4l4 4" />
    </>
  ),
};

/** "Hu" for Human, "NE" for Night Elf: the crest's race badge. */
function raceBadge(race: string): string {
  const words = race.split(/\s+/).filter(Boolean);
  return words.length > 1 ? words.map((w) => w[0].toUpperCase()).join("") : race.slice(0, 2);
}

export function Crest({ c, size = 64 }: { c: Kin; size?: number }) {
  return (
    <span
      className={`ch-pslot${c.class ? "" : " plain"}`}
      style={{ ...classStyle(c), "--w": `${size}px` } as React.CSSProperties}
      aria-hidden
    >
      <span className="pi">
        <svg className="crest" viewBox="0 0 24 24">
          <path d="M12 1.5l9 3.3v6.6c0 6.2-4.3 10.3-9 12.1-4.7-1.8-9-5.9-9-12.1V4.8z" />
          <g transform="translate(4.2 4.2) scale(.65)">{c.class ? GLYPH[c.class] : null}</g>
        </svg>
      </span>
      {c.race && <span className="race">{raceBadge(c.race)}</span>}
    </span>
  );
}

/** Before the addon has seen a character: a stone slot with a silhouette. */
function Blank({ size = 64 }: { size?: number }) {
  return (
    <span className="ch-pslot blank" style={{ "--w": `${size}px` } as React.CSSProperties} aria-hidden>
      <span className="pi">
        <svg viewBox="0 0 40 50">
          <circle cx="20" cy="18" r="8" />
          <path d="M4 50c0-13 7-19 16-19s16 6 16 19z" />
        </svg>
      </span>
    </span>
  );
}

type Sort = "level" | "gold" | "seen";

/** settings.ui key: the item-icons offer was dismissed ("Not now"). */
const ICON_NUDGE = "characters.icon_nudge";

const same = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: "accent" }) === 0;

/** Characters: every alt's card, or one character's sheet. Characters in
 *  the WTF folder the addon hasn't seen yet get a neutral card
 *  (characters-noaddon.html) and fill in as each one is seen. */
export function Characters({ onOpenDashboard }: { onOpenDashboard: () => void }) {
  const { overview, error, refresh } = useCharacters();
  const worth = useGoodsWorth();
  const roster = useRoster();
  const addon = useAddon();
  const { settings, update } = useSettings();
  const [sort, setSort] = useState<Sort>("gold");
  const [open, setOpen] = useState<number | null>(null);
  // Kept while a sheet opened from a result is on show, so Back returns to it.
  const [query, setQuery] = useState("");
  // TIP3 (b): kept while the text changes, reset when the search is cleared.
  const [filters, setFilters] = useState<Filters>(NO_FILTERS);
  useEffect(() => {
    if (!query.trim()) setFilters(NO_FILTERS);
  }, [query]);
  const search = useItemSearch(query, filters);
  const field = useRef<HTMLInputElement>(null);
  // Ctrl K (Cmd K on a Mac) jumps to the search box, as the mock's hint says.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        field.current?.focus();
        field.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const unseen = useMemo(() => {
    const seen = overview?.characters ?? [];
    return (roster ?? [])
      .filter(
        (r) =>
          !seen.some(
            (c) => same(c.account, r.account) && same(c.group_dir, r.realm) && same(c.folder, r.name),
          ),
      )
      .sort((a, b) => (b.last_played ?? "").localeCompare(a.last_played ?? ""));
  }, [overview, roster]);
  const addonMissing = addon.status != null && addon.status.installed_version == null;
  // F8c: item icons are opt-in; offer them here, where the letters are, once
  // there's a game folder to read them from (IMPLEMENTING §13).
  const iconNudge =
    settings != null &&
    settings.install != null &&
    !settings.item_icons &&
    settings.ui?.[ICON_NUDGE] !== "dismissed" &&
    !addonMissing;

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
    return (
      <Sheet id={open} cards={cards} onOpen={setOpen} onBack={() => setOpen(null)} onTagged={refresh} />
    );
  }

  const anySeen = cards.length > 0;
  const results = search.results;
  const match = (id: number | null): "hit" | "miss" | undefined =>
    results ? (id != null && results.characters.includes(id) ? "hit" : "miss") : undefined;
  return (
    <Page>
      <PageHeader
        title="Characters"
        lede={
          anySeen
            ? "Everything your alts carry, as of their last logout."
            : "Found in your WTF folder. Class, level and gold appear once the addon has seen each character."
        }
        actions={
          anySeen && overview ? (
            <span className="ch-totals">
              <span>
                Gold <b><Coins copper={overview.gold} silver={false} /></b>
              </span>
              <span>
                Items <b>{overview.items.toLocaleString()}</b>
              </span>
              {worth && (
                <span title={`Gold plus ${worth.priced.toLocaleString()} of ${worth.items.toLocaleString()} items at your last AH scan`}>
                  Net worth <b><Coins copper={(overview.gold ?? 0) + worth.value} silver={false} /></b>
                </span>
              )}
            </span>
          ) : unseen.length > 0 ? (
            <span className="ch-totals">
              <span>
                <b>{unseen.length}</b> {unseen.length === 1 ? "character" : "characters"}
              </span>
            </span>
          ) : undefined
        }
      />
      {error && <Callout tone="bad">{error}</Callout>}
      {addonMissing && (
        <div className="ch-banner">
          <Puzzle size={16} aria-hidden />
          <span className="grow">
            <b>Install the ForeverBuddy addon</b> to see class, level, gold and satchels for every alt.
          </span>
          <button onClick={onOpenDashboard}>
            How to install <ChevronRight size={13} aria-hidden />
          </button>
        </div>
      )}
      {iconNudge && anySeen && (
        <div className="ch-banner">
          <ImageIcon size={16} aria-hidden />
          <span className="grow">
            Items show letters. Turn on icons to see the real pictures, read from your own game files.
          </span>
          <Button onClick={() => update({ item_icons: true })}>Show icons</Button>
          <Button variant="ghost" onClick={() => update({ ui: { [ICON_NUDGE]: "dismissed" } })}>
            Not now
          </Button>
        </div>
      )}
      {overview && !anySeen && unseen.length === 0 ? (
        <Panel>
          <PanelBody>
            <p className="d-muted">
              No characters found yet. Log in to a character once and it appears here.
            </p>
          </PanelBody>
        </Panel>
      ) : (
        <>
          {anySeen && (
            <div className="ch-toolbar">
              <label className="ch-search">
                <Search size={14} aria-hidden />
                <input
                  ref={field}
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  onKeyDown={(e) => e.key === "Escape" && setQuery("")}
                  placeholder="Search every satchel, bank and mailbox…  e.g. Runecloth, Arcanite"
                  aria-label="Search every satchel, bank and mailbox"
                  spellCheck={false}
                />
                <kbd>{/Mac/.test(navigator.platform) ? "⌘ K" : "Ctrl K"}</kbd>
              </label>
              <span className="right">
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
              </span>
            </div>
          )}
          {search.error && <Callout tone="bad">{search.error}</Callout>}
          {results && (
            <Results
              query={query}
              results={results}
              cards={cards}
              onOpen={setOpen}
              filters={filters}
              onFilters={setFilters}
              filteredOut={search.filteredOut}
            />
          )}
          <section className={`ch-cards${results ? " searching" : ""}`}>
            {cards.map((c) => (
              <Card key={c.id} c={c} match={match(c.id)} onOpen={() => setOpen(c.id)} />
            ))}
            {unseen.map((r) => (
              <UnseenCard
                key={`${r.account}/${r.realm}/${r.name}`}
                r={r}
                addonMissing={addonMissing}
                match={match(null)}
              />
            ))}
          </section>
        </>
      )}
    </Page>
  );
}

const WHERE: Record<string, { label: string; icon: LucideIcon }> = {
  bag: { label: "Satchels", icon: Backpack },
  bank: { label: "Bank", icon: Landmark },
  mail: { label: "Mail", icon: Mail },
};

/** The search results (characters-search mock), without the Value column
 *  and "≈ at last scan": AH numbers stay hidden until v0.4 (IMPLEMENTING.md
 *  §7). A row opens that character's sheet. */
/** TIP3 (b): the quality choices, by the lowest quality each keeps. */
const QUALITY = [
  { value: "any", label: "Any", min: null },
  { value: "2", label: "Uncommon+", min: 2 },
  { value: "3", label: "Rare+", min: 3 },
  { value: "4", label: "Epic", min: 4 },
] as const;
type QualityChoice = (typeof QUALITY)[number]["value"];

/** "Quality" and "Item level at least", inside the results (IMPLEMENTING §19). */
function FilterRow({ filters, onFilters }: { filters: Filters; onFilters: (f: Filters) => void }) {
  const [ilvl, setIlvl] = useState(filters.minIlvl?.toString() ?? "");
  useEffect(() => setIlvl(filters.minIlvl?.toString() ?? ""), [filters.minIlvl]);
  const chosen = (QUALITY.find((q) => q.min === filters.minQuality)?.value ?? "any") as QualityChoice;
  return (
    <div className="ch-filters">
      <span>Quality</span>
      <Segmented<QualityChoice>
        value={chosen}
        onChange={(v) => onFilters({ ...filters, minQuality: QUALITY.find((q) => q.value === v)?.min ?? null })}
        options={QUALITY.map((q) => ({ value: q.value, label: q.label }))}
      />
      <span className="gap">Item level at least</span>
      <label className="d-field ch-ilvl">
        <input
          inputMode="numeric"
          value={ilvl}
          placeholder="any"
          aria-label="Minimum item level"
          onChange={(e) => {
            const text = e.target.value.replace(/\D/g, "").slice(0, 3);
            setIlvl(text);
            const n = Number(text);
            onFilters({ ...filters, minIlvl: text && n >= 1 && n <= 300 ? n : null });
          }}
        />
      </label>
    </div>
  );
}

function Results({
  query,
  results,
  cards,
  onOpen,
  filters,
  onFilters,
  filteredOut,
}: {
  query: string;
  results: SearchResults;
  cards: CharacterCard[];
  onOpen: (id: number) => void;
  filters: Filters;
  onFilters: (f: Filters) => void;
  filteredOut: boolean;
}) {
  const names = new Set(results.hits.map((h) => h.name));
  // One item found: name it, as the mock does. Several: the query.
  const title = names.size === 1 ? [...names][0] : query.trim();
  const kin = (id: number) => cards.find((c) => c.id === id);
  return (
    <Panel>
      <PanelHeader title={title}>
        {results.hits.length > 0 && (
          <span className="d-dim ch-meta">
            <b className="ch-strong">{results.total.toLocaleString()}</b> on{" "}
            {plural(results.characters.length, "character", "characters")}
          </span>
        )}
      </PanelHeader>
      <FilterRow filters={filters} onFilters={onFilters} />
      {results.hits.length === 0 ? (
        <PanelBody>
          {filteredOut ? (
            <div className="ch-nofilter">
              <p className="d-muted">Nothing matches these filters.</p>
              <Button variant="ghost" onClick={() => onFilters(NO_FILTERS)}>
                Clear filters
              </Button>
            </div>
          ) : (
            <p className="d-muted">Nothing matches in any satchel, bank or mailbox.</p>
          )}
        </PanelBody>
      ) : (
        <table className="d-table ch-results">
          <thead>
            <tr>
              <th>Item</th>
              <th>Character</th>
              <th>Where</th>
              <th className="num">Count</th>
            </tr>
          </thead>
          <tbody>
            {results.hits.map((h) => {
              const where = WHERE[h.location] ?? { label: h.location, icon: Backpack };
              const Icon = where.icon;
              // Bank and mail are as of the last visit: say when, ember past 7 days.
              const visit =
                h.location === "bank" || h.location === "mail"
                  ? visitLine(h.as_of, h.location === "bank" ? "bank" : "mailbox")
                  : null;
              const q = h.quality != null ? `ch-q${h.quality}` : "";
              const c = kin(h.character_id);
              return (
                <tr key={`${h.character_id}-${h.location}-${h.item_id}`} onClick={() => onOpen(h.character_id)}>
                  <td>
                    <span className="ch-item">
                      <span className={`ch-ico sm ${q}`} aria-hidden>
                        <b>{h.name.slice(0, 1)}</b>
                        <ItemIcon id={h.icon} />
                      </span>
                      <span className={q}>{h.name}</span>
                    </span>
                  </td>
                  <td className="ch-cc" style={classStyle({ class: c?.class ?? h.class })}>
                    {h.character}
                  </td>
                  <td>
                    <span className={`ch-where${visit?.old ? " old" : ""}`} title={visit?.text}>
                      <Icon size={13} aria-hidden />
                      {where.label}
                    </span>
                  </td>
                  <td className="num">{h.count.toLocaleString()}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
      {results.more && (
        <PanelBody>
          <p className="d-dim">Showing the {results.hits.length} largest. Add a word to narrow it down.</p>
        </PanelBody>
      )}
    </Panel>
  );
}

/** A character the addon hasn't written about yet: name and last played
 *  from the folder, quiet placeholders for the rest, never zeros. */
function UnseenCard({
  r,
  addonMissing,
  match,
}: {
  r: WtfCharacter;
  addonMissing: boolean;
  match?: "hit" | "miss";
}) {
  const need = addonMissing ? "needs addon" : "log in once";
  return (
    <div className={`d-panel ch-card unseen${match ? ` ${match}` : ""}`}>
      <div className="ch-id">
        <Blank />
        <div>
          <div className="ch-nm">{characterName(r.name)}</div>
          <div className="ch-loc">{r.last_played ? `Last played ${ago(r.last_played)}` : "Not played yet"}</div>
        </div>
      </div>
      <div className="ch-prog">
        <div className="lbl">
          <span>Level &amp; experience</span>
          <i>{need}</i>
        </div>
        <div className="ch-nobar" />
      </div>
      <div className="ch-facts">
        <div>
          Gold<b>n/a</b>
        </div>
        <div>
          Satchels<b>n/a</b>
        </div>
        <div>
          Played<b>n/a</b>
        </div>
      </div>
    </div>
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

function Card({
  c,
  match,
  onOpen,
}: {
  c: CharacterCard;
  match?: "hit" | "miss";
  onOpen: () => void;
}) {
  const where = c.subzone ?? c.zone;
  return (
    <button className={`d-panel ch-card${match ? ` ${match}` : ""}`} onClick={onOpen} style={classStyle(c)}>
      <div className="ch-id">
        <Crest c={c} />
        <div>
          <div className="ch-nm ch-cc">
            {fullName(c)}
            {c.bank_alt && <span className="ch-tag">Bank</span>}
          </div>
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

type Tab = "gear" | "satchels" | "bank" | "mail" | "professions" | "quests";

const SLOTS: Record<number, string> = {
  1: "Head", 2: "Neck", 3: "Shoulder", 4: "Shirt", 5: "Chest", 6: "Waist", 7: "Legs", 8: "Feet",
  9: "Wrist", 10: "Hands", 11: "Finger", 12: "Finger", 13: "Trinket", 14: "Trinket", 15: "Back",
  16: "Main hand", 17: "Off hand", 18: "Ranged", 19: "Tabard",
};

/** "As of your last bank visit, 2 Oct"; ember past 7 days (IMPLEMENTING.md §7). */
function visitLine(asOf: string | null, place: "bank" | "mailbox"): { text: string; old: boolean } {
  if (!asOf) {
    const what = place === "bank" ? "your bank" : "your mailbox";
    return { text: `Not seen yet. Open ${what} once in-game and it appears here.`, old: false };
  }
  const old = Date.now() - new Date(asOf).getTime() > 7 * 86400000;
  const day = new Date(asOf).toLocaleDateString(undefined, { day: "numeric", month: "short" });
  return {
    text: `As of your last ${place} visit, ${day}${old ? `. Visit the ${place} in-game to refresh.` : ""}`,
    old,
  };
}

/** "Yesterday", "Sat 3 Oct": the day heads of "Recently completed". */
function questDay(iso: string, now = new Date()): string {
  const d = new Date(iso);
  const midnight = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((midnight(now) - midnight(d)) / 86400000);
  if (days === 0) return "Today";
  if (days === 1) return "Yesterday";
  return d.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });
}

const shortDate = (iso: string) => new Date(iso).toLocaleDateString(undefined, { day: "numeric", month: "short" });
const clock = (iso: string) => new Date(iso).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });

/** "Zone · from Giver" / "Zone · to Giver"; the giver is left out when the
 *  addon didn't record one (an item-started quest, or an older addon). */
function questWhere(e: QuestEntry, prep: "from" | "to"): string | null {
  const parts = [e.zone, e.giver && `${prep} ${e.giver}`].filter(Boolean);
  return parts.length ? parts.join(" · ") : null;
}

const COMPLETED_PAGE = 50;

/** Q1b: IMPLEMENTING §14. Read-only; no coordinates (those are for the
 *  planner), and never another player's name (the addon drops them). */
function Quests({ name, log }: { name: string; log: NonNullable<ReturnType<typeof useQuestLog>> }) {
  const [all, setAll] = useState(false);
  if (log.done === 0 && log.open.length === 0 && log.completed.length === 0) {
    return (
      <p className="ch-empty">
        No quests noted for {name} yet. They appear after you accept or hand one in with the addon running, then log
        out.
      </p>
    );
  }
  const shown = all ? log.completed : log.completed.slice(0, COMPLETED_PAGE);
  const days: [string, QuestEntry[]][] = [];
  for (const e of shown) {
    const day = questDay(e.at);
    const last = days[days.length - 1];
    if (last && last[0] === day) last[1].push(e);
    else days.push([day, [e]]);
  }
  return (
    <div className="qv">
      {log.done > 0 && (
        <div className="qfresh">
          {plural(log.done, "quest", "quests")} completed{log.asOf ? ` · as of logout, ${shortDate(log.asOf)}` : ""}
        </div>
      )}
      {log.open.length > 0 && (
        <>
          <div className="qsec">
            In your log <span>{log.open.length}</span>
          </div>
          <ul className="qlist">
            {log.open.map((e, i) => (
              <li key={`${e.at}-${i}`}>
                <span className="qt">{e.title ?? (e.quest_id != null ? `Quest ${e.quest_id}` : "A quest")}</span>
                {questWhere(e, "from") && <span className="qw">{questWhere(e, "from")}</span>}
                <span className="qd">accepted {shortDate(e.at)}</span>
              </li>
            ))}
          </ul>
        </>
      )}
      {days.length > 0 && (
        <>
          <div className="qsec">
            Recently completed <span>newest first</span>
          </div>
          {days.map(([day, entries]) => (
            <div key={day}>
              <div className="qday">{day}</div>
              <ul className="qlist done">
                {entries.map((e, i) => (
                  <li key={`${e.at}-${i}`}>
                    <span className="qt">{e.title ?? (e.quest_id != null ? `Quest ${e.quest_id}` : "A quest")}</span>
                    {questWhere(e, "to") && <span className="qw">{questWhere(e, "to")}</span>}
                    <span className="qd">{clock(e.at)}</span>
                  </li>
                ))}
              </ul>
            </div>
          ))}
          {!all && log.completed.length > COMPLETED_PAGE && (
            <button className="qmore" onClick={() => setAll(true)}>
              Show older
            </button>
          )}
        </>
      )}
      <div className="qnote">
        Each quest you accept or hand in is noted at logout. Completed quests from before the addon count in the total,
        without dates.
      </div>
    </div>
  );
}

/** "21:02" today, else "5 Oct, 21:02". */
function approvedAt(iso: string): string {
  const d = new Date(iso);
  const time = d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
  return d.toDateString() === new Date().toDateString()
    ? time
    : `${d.toLocaleDateString(undefined, { day: "numeric", month: "short" })}, ${time}`;
}

/** The plan's Bridge state, in bridge.html's words (IMPLEMENTING §16). */
function planDelivery(p: Plan): { live: boolean; text: string } {
  const d = p.delivery;
  switch (d.state) {
    case "synced":
      return { live: false, text: `In the game since ${approvedAt(d.since)} · progress as of logout` };
    case "pending":
      return { live: true, text: "Waiting for a sync: /reload or log in to see it" };
    case "waiting":
      return { live: true, text: "Goes to the game when WoW closes" };
    case "restart":
      return { live: true, text: "Needs the addon update, then restart WoW once" };
    case "failed":
      return { live: true, text: "Couldn't write it; the game keeps the last plan" };
  }
}

/** P1: the active quest plan (IMPLEMENTING §16). Approval happens in P2's
 *  Approvals panel, never here; this only shows it and can clear it. */
function QuestPlan({ plan, onClear }: { plan: Plan; onClear: () => void }) {
  const done = new Set(plan.done);
  const current = plan.steps.findIndex((_, i) => !done.has(i + 1));
  const zone = plan.steps.find((s) => s.zone)?.zone;
  const producer = plan.producer.startsWith("agent:")
    ? `from "${plan.producer.slice("agent:".length)}"`
    : "made in Forever Buddy";
  const status = planDelivery(plan);
  return (
    <Panel>
      <PanelHeader title="Quest plan">
        <span className="d-dim">
          {done.size} of {plan.steps.length} done
        </span>
      </PanelHeader>
      <div className="pmeta">
        {[zone, `${producer}, approved ${approvedAt(plan.created_at)}`].filter(Boolean).join(" · ")}
      </div>
      <ol className="psteps">
        {plan.steps.map((s, i) => (
          <li key={i} className={done.has(i + 1) ? "done" : i === current ? "now" : undefined}>
            <span className="no">{i + 1}</span>
            <span>
              <b>{s.text}</b>
              {s.zone && <small>{s.zone}</small>}
            </span>
          </li>
        ))}
      </ol>
      <div className="psent">
        {status.live ? <LiveDot /> : <StatusDot />}
        {status.text}
      </div>
      <div className="pact">
        <Button variant="ghost" onClick={onClear}>
          Clear plan
        </Button>
      </div>
    </Panel>
  );
}

function Freshness({ asOf, place }: { asOf: string | null; place: "bank" | "mailbox" }) {
  const { text, old } = visitLine(asOf, place);
  return <p className={`ch-fresh${old ? " old" : ""}`}>{text}</p>;
}

/** The tooltip glass, with only what the addon captured: name in its
 *  quality colour, slot, item level, stack, and "Gained 2 Oct · Westfall"
 *  when the journal has it. "Gained", never "Looted": the addon can't tell
 *  loot from a quest reward, crafting or a trade, and never a source
 *  (IMPLEMENTING.md §7). Rendered into <body> so the parchment's tilt
 *  doesn't move it. */
function Tooltip({ item, slot, at }: { item: ItemRow; slot?: string; at: DOMRect }) {
  const width = 256;
  const left = at.right + 10 + width > window.innerWidth ? at.left - 10 - width : at.right + 10;
  const top = Math.max(8, Math.min(at.top, window.innerHeight - 160));
  const looted = item.looted_at
    ? `Gained ${new Date(item.looted_at).toLocaleDateString(undefined, { day: "numeric", month: "short" })}${
        item.looted_in ? ` · ${item.looted_in}` : ""
      }`
    : null;
  return createPortal(
    <div className="ch-tt" style={{ left, top, width }} role="tooltip">
      <div className={`t tq${item.quality ?? 1}`}>{item.name}</div>
      {slot && <div>{slot}</div>}
      {item.ilvl != null && <div className="y">Item Level {item.ilvl}</div>}
      {item.count > 1 && <div>Stack of {item.count}</div>}
      {looted && <div className="src">{looted}</div>}
    </div>,
    document.body,
  );
}

/** B3: what the satchels tab needs to mark items. */
type Marking = {
  marks: Map<number, Marked>;
  others: Who[];
  onMark: (itemId: number, mark: Mark) => void;
  onClear: (itemId: number) => void;
};

function Slot({
  item,
  label,
  slot,
  marking,
}: {
  item: ItemRow;
  label?: string;
  slot?: string;
  marking?: Marking | null;
}) {
  const q = item.quality != null ? `ch-q${item.quality}` : "";
  const [hover, setHover] = useState<DOMRect | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const marked = marking?.marks.get(item.item_id);
  return (
    <div
      className={`ch-slot${marking ? " markable" : ""}`}
      onMouseEnter={(e) => setHover(e.currentTarget.getBoundingClientRect())}
      onMouseLeave={() => setHover(null)}
    >
      <span className={`ch-ico ${q}`} aria-hidden>
        <b>{item.name.slice(0, 1)}</b>
        <ItemIcon id={item.icon} />
      </span>
      <div className="t">
        <div className={q}>{item.name}</div>
        <small>
          {label ?? (item.count > 1 ? `× ${item.count}` : "")}
          {marked && <MarkTag mark={marked} />}
        </small>
      </div>
      <span className="il">
        {marking ? (
          <MarkMenu
            item={item}
            marked={marked}
            others={marking.others}
            onMark={(m) => marking.onMark(item.item_id, m)}
            onClear={() => marking.onClear(item.item_id)}
            onOpenChange={setMenuOpen}
          />
        ) : null}
        {item.ilvl ?? ""}
      </span>
      {hover && !menuOpen && <Tooltip item={item} slot={slot} at={hover} />}
    </div>
  );
}

/** Main hand, off hand and ranged/relic: their own row under the gear. */
const WEAPON_SLOTS = new Set([16, 17, 18]);

function Gear({ items }: { items: ItemRow[] }) {
  if (items.length === 0) return <p className="ch-empty">No gear recorded yet.</p>;
  const slot = (i: ItemRow) => SLOTS[i.slot] ?? `Slot ${i.slot}`;
  const armor = items.filter((i) => !WEAPON_SLOTS.has(i.slot));
  const weapons = items.filter((i) => WEAPON_SLOTS.has(i.slot));
  return (
    <>
      <div className="ch-doll">
        {armor.map((i) => (
          <Slot key={i.slot} item={i} label={slot(i)} slot={slot(i)} />
        ))}
      </div>
      {weapons.length > 0 && (
        <>
          <div className="ch-sec">Weapons</div>
          <div className="ch-doll">
            {weapons.map((i) => (
              <Slot key={i.slot} item={i} label={slot(i)} slot={slot(i)} />
            ))}
          </div>
        </>
      )}
    </>
  );
}

function Bags({ bags, empty, marking }: { bags: BagView[]; empty: string; marking?: Marking | null }) {
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
              <Slot key={`${i.container}-${i.slot}`} item={i} marking={marking} />
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

/** Raid and dungeon saves that haven't reset, with the countdown (F3). */
/** IMPLEMENTING.md §8: one row per save, soonest reset first; the full
 *  reset date in the row's tooltip. `asOf` null: never read yet. */
function LockoutList({ lockouts, asOf }: { lockouts: Lockout[]; asOf: string | null }) {
  if (asOf == null) return <p className="d-dim">Lockouts appear after your next login.</p>;
  if (lockouts.length === 0) return <p className="d-dim">Not saved anywhere this week.</p>;
  return (
    <ul className="ch-locks">
      {lockouts.map((l) => (
        <li key={`${l.name}|${l.difficulty}`} title={l.reset_at ? resetDay(l.reset_at) : undefined}>
          <span>
            {l.name}
            {/* Only when it tells something: the game's default reads "Normal". */}
            {l.difficulty && l.difficulty !== "Normal" && <span className="dif"> {l.difficulty}</span>}
          </span>
          <span className="in">{l.reset_at ? resetsIn(l.reset_at) : ""}</span>
        </li>
      ))}
    </ul>
  );
}

function Sheet({
  id,
  cards,
  onOpen,
  onBack,
  onTagged,
}: {
  id: number;
  cards: CharacterCard[];
  onOpen: (id: number) => void;
  onBack: () => void;
  /** The bank-alt tag changed: the cards reload. */
  onTagged: () => void;
}) {
  const { sheet, error, reload } = useCharacterSheet(id);
  // F5c: what this character's goods fetch at scan prices, when any are priced.
  const goods = useGoodsWorth();
  const carried = goods?.byCharacter.get(id) || null;
  const scanAt = goods?.as_of ?? null;
  const unpriced = goods ? goods.items - goods.priced : 0;
  const [tagError, setTagError] = useState<string | null>(null);
  const setBankAlt = (on: boolean) => {
    setTagError(null);
    commands.characterSetBankAlt(id, on).then(
      () => {
        reload();
        onTagged();
      },
      (e) => setTagError(errorText(e)),
    );
  };
  const [tab, setTab] = useState<Tab>("gear");
  // Q1b ships dark: the tab appears once any character has quest data.
  const questsOn = useQuestsAvailable();
  const quests = useQuestLog(id);
  // P1: this character's active quest plan, if it has one.
  const { plan, clear: clearPlan } = useQuestPlan(id);
  // B3: marks to sell or send, and who else could receive.
  const cleanup = useCleanup(id);
  const marking: Marking | null = cleanup.cleanup
    ? {
        marks: new Map(cleanup.cleanup.marks.map((m) => [m.item_id, m])),
        others: cards.filter((k) => k.id !== id).map((k) => ({ id: k.id, name: k.name, class: k.class })),
        onMark: (item, mark) => cleanup.mark(item, mark),
        onClear: (item) => cleanup.clear(item),
      }
    : null;
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
          {c && (
            <label className="ch-bankalt" title="Shows a Bank tag on this character's card">
              Bank alt
              <Switch checked={c.bank_alt} onChange={setBankAlt} label="Bank alt" />
            </label>
          )}
          <Button variant="icon" title="Previous alt" disabled={!prev} onClick={() => prev && onOpen(prev.id)}>
            <ChevronLeft size={14} />
          </Button>
          <Button variant="icon" title="Next alt" disabled={!next} onClick={() => next && onOpen(next.id)}>
            <ChevronRight size={14} />
          </Button>
        </div>
      </div>
      {error && <Callout tone="bad">{error}</Callout>}
      {tagError && <Callout tone="bad">{tagError}</Callout>}
      {!sheet && !error && <p className="d-muted">Loading…</p>}
      {sheet && c && (
        <div className="ch-body">
          <Parchment tilt>
           <div className="ch-sheet">
            <div className="ch-ident" style={classStyle(c)}>
              <Crest c={c} size={92} />
              <div>
                <h1 className="ch-cc">
                  {fullName(c)}
                  {c.bank_alt && <span className="ch-tag">Bank</span>}
                </h1>
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
            <div className={`ch-stats${carried ? " five" : ""}`}>
              <div>
                <div className="k">Gold</div>
                <div className="v"><Coins copper={c.money} /></div>
              </div>
              {carried != null && (
                <div
                  title={`Bags, bank and mail at your AH scan${scanAt ? ` ${ago(scanAt)}` : ""}. ${
                    unpriced > 0 ? `${plural(unpriced, "item", "items")} across your alts have no price yet and aren't counted.` : ""
                  }`}
                >
                  <div className="k">Worth carried</div>
                  <div className="v">
                    ≈ <Coins copper={carried} silver={false} />
                  </div>
                </div>
              )}
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
                  ...(questsOn ? [["quests", "Quests", quests && quests.done > 0 ? quests.done : null]] : []),
                ] as [Tab, string, string | number | null][]
              ).map(([key, label, n]) => (
                <button key={key} role="tab" aria-selected={tab === key} onClick={() => setTab(key)}>
                  {label}
                  {n != null && <span className="n">{n}</span>}
                </button>
              ))}
              <span className="stamp">{ago(c.last_seen)}</span>
            </nav>
            {tab === "gear" && <Gear items={sheet.equipped} />}
            {tab === "satchels" && (
              <Bags bags={sheet.bags} empty="No satchels recorded yet." marking={marking} />
            )}
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
            {tab === "quests" && questsOn && quests && <Quests name={c.name} log={quests} />}
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
            {cleanup.cleanup && (
              <BagCleanup
                cleanup={cleanup.cleanup}
                error={cleanup.error}
                onClear={cleanup.clear}
                onMarkGreys={cleanup.markGreys}
              />
            )}
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
            {plan && <QuestPlan plan={plan} onClear={clearPlan} />}
            <LoginNotes characterId={id} />
            <Panel>
              <PanelHeader title="Lockouts">
                {sheet.lockouts_as_of && (
                  <span className="d-dim">
                    as of login,{" "}
                    {new Date(sheet.lockouts_as_of).toLocaleDateString(undefined, { day: "numeric", month: "short" })}
                  </span>
                )}
              </PanelHeader>
              <PanelBody>
                <LockoutList lockouts={sheet.lockouts} asOf={sheet.lockouts_as_of} />
              </PanelBody>
            </Panel>
          </div>
        </div>
      )}
    </Page>
  );
}
