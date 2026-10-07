// A fake backend for screenshots and design review: every command answers
// from canned data, picked by `?mock=<scenario>`. Only loaded when the app is
// built with VITE_MOCK=1 (`npm run build:mock`); the real build never
// contains it (see main.tsx). Names and numbers follow the round-3 mocks so
// app and mock screenshots line up.

import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type {
  Approvals,
  LoginNote,
  NoteView,
  Proposal,
  AddonChange,
  AddonInfo,
  AddonsList,
  AddonStatus,
  Adventure,
  AhHistory,
  AhStatus,
  AltLockout,
  CategoryNode,
  CharacterCard,
  CharacterSheet,
  Cleanup,
  GoodsWorth,
  IntegrationId,
  ItemRow,
  JournalEntry,
  Ledger,
  LedgerRange,
  List,
  ListItem,
  ListsView,
  Macro,
  Mark,
  Marked,
  MacrosList,
  Plan,
  PlaySession,
  QuestLog,
  Reason,
  RestorePlan,
  SearchResults,
  SecretStatus,
  SeenItem,
  Sellable,
  SnapshotDetail,
  SnapshotKind,
  SnapshotSummary,
  StorageInfo,
  Trigger,
  VerifyReport,
  WtfCharacter,
} from "@/lib/bindings";

/** The scenarios, for `scripts/shots.sh` and anyone poking around. */
export const SCENARIOS = [
  "dashboard", // WoW running, sessions, characters, the addon's data (V9's Dashboard)
  "noaddon", // the same before the addon has written anything (v0.1's Dashboard)
  "idle", // WoW not running
  "no-sessions",
  "dashboard-missing", // the saved game folder is gone
  "recover", // interrupted restore (Finish is refused once with DeletionsChanged)
  "recover-unreadable",
  "recover-unreadable-no-safety",
  "backups",
  "backups-corrupt", // restoring hits a damaged snapshot
  "backups-locked", // restoring hits copies another program holds; Try again then works
  "backups-deletions", // restore refused: more to delete than confirmed
  "backups-partial", // restore fails partway
  "backup-failed", // the last automatic backup failed
  "snapshot-unreadable",
  "nogame", // first run, nothing found
  "detect-failed",
  "startup-error",
  "characters", // V7: the alts' cards (open one for the sheet)
  "characters-empty", // no addon notes yet
  "ledger-empty", // the Ledger before the addon has written anything
  "adventure-empty", // Adventures before the addon has written anything
  "settings", // F1: a 24 h schedule and two keys saved (CurseForge, GitHub); Change… then confirms a move
  "settings-moving", // moving the backups, stuck part way so the progress shows
  "settings-pending", // moving is refused: an interrupted restore waits
  "addons-empty", // F4: no addons in Interface/AddOns yet (the Addons screen works in every scenario)
  "addons-linked", // F6: Velyra's settings folder is a link, so her row can't be switched
  "macros-empty", // F7: no macros-cache.txt anywhere yet (the Macros screen works in every scenario)
  "ah", // F5: ah.html's market (any scenario has it; this one opens on it in shots)
  "ah-empty", // no Auctionator prices yet: the AH is hidden, Settings says why (F5d)
  "ah-unreadable", // an Auctionator file this version can't read: hidden, Settings says so
  "lists-empty", // B2: no lists yet (the Lists screen has three in every other scenario)
  "lists-proposal", // P2: an agent's change to Tailoring 300, in place (lists.html?state=proposal)
  "approvals", // P2b: a new note, a conflict, and the Decided list
  "approvals-empty", // agent access on, nothing waiting
  "approvals-off", // agent access off
] as const;

type Args = Record<string, unknown>;
type Handler = (args: Args) => unknown;

export function installMockIpc(): void {
  const s = new URLSearchParams(location.search).get("mock") ?? "dashboard";
  const now = Date.now();
  const iso = (minsAgo: number) => new Date(now - minsAgo * 60000).toISOString();
  const running = s === "dashboard" || s === "noaddon" || s === "dashboard-missing";
  // No ForeverBuddy data yet: the v0.1 screens.
  const noAddon = s === "noaddon" || s === "dashboard-missing" || s === "characters-empty";
  // No Auctionator prices on this machine: no AH screen, no worth (F5d).
  const noPrices = s === "ah-empty" || s === "ah-unreadable";
  // F8: the game data cache in Settings. Empty until icons are turned on
  // ("settings-icons": on with a full cache; "-unreadable": on, can't read).
  let iconFiles = s === "settings-icons" ? 412 : 0;
  // B1 login notes.
  let loginNotes: LoginNote[] = [
    {
      id: 1,
      character_id: 2,
      text: "Hand in the Onyxia attunement before raid on Thursday.",
      once: false,
      until: iso(-60 * 24 * 3),
      author: "you",
      producer: null,
      created_at: iso(60 * 24),
      shown_at: null,
    },
  ];
  // P2b Approvals (approvals.html): Coinpurse's conflict, a new note for
  // Velyra, and the Decided list.
  const mine: LoginNote = {
    id: 7,
    character_id: 1,
    text: "Relist the Arcanite Bars if the price is still above 38g.",
    once: true,
    until: null,
    author: "you",
    producer: null,
    created_at: iso(30),
    shown_at: null,
  };
  const noteView = (character_id: number, character: string, cls: string, text: string): NoteView => ({
    character_id,
    character,
    class: cls,
    text,
    once: true,
    until: null,
    replaces: null,
  });
  const proposal = (id: number, mins: number, producer: string, extra: Partial<Proposal>): Proposal => ({
    id,
    kind: "login_note",
    producer,
    reason: null,
    created_at: iso(mins),
    status: "staged",
    status_reason: null,
    decided_at: null,
    note: null,
    plan: null,
    list: null,
    ...extra,
  });
  const step = (text: string, zone: string | null = null): Plan["steps"][number] => ({ text, quest_id: null, zone, kind: null });
  const felwood: Plan = {
    id: 4,
    character_id: 3,
    character: "Velyra",
    title: "Felwood",
    steps: [
      step('Accept "Cleansing Felwood"'),
      step("Kill Irontree Stompers", "Irontree Woods"),
      step('Turn in "Cleansing Felwood"', "Emerald Sanctuary"),
      step("Hearth to Ironforge"),
    ],
    producer: "app",
    created_at: iso(60 * 26),
    done: [1, 2],
    progress_at: iso(60 * 20),
    delivery: { state: "synced", since: iso(60 * 25) },
  };
  const approvals: Approvals = {
    // "lists-proposal": only the list change, shown in place on Lists (§15).
    waiting: (s === "approvals" || s === "lists-proposal"
      ? [
            proposal(3, 4, "Claude Desktop", {
              kind: "quest_plan",
              reason: "Felwood is nearly done; Winterspring next for the Everlook quests at 56.",
              plan: {
                character_id: 3,
                character: "Velyra Duskmane",
                class: "druid",
                title: "Winterspring",
                steps: [
                  step("Fly to Everlook"),
                  step('Accept "Are We There, Yeti?"', "Everlook"),
                  step("Collect 10 Thick Yeti Fur", "Owl Wing Thicket"),
                  step('Turn in "Are We There, Yeti?"', "Everlook"),
                ],
                replaces: felwood,
              },
            }),
            proposal(5, 60 * 22, "claude-code", {
              kind: "list",
              reason: "for the Mooncloth Robe at 300",
              list: {
                list_id: 1,
                name: "Tailoring 300",
                for_character: null,
                gone: false,
                changes: [
                  { item_id: 14342, name: "Mooncloth", quality: 2, icon_file_id: null, need: 4, was: null },
                  { item_id: 14047, name: "Runecloth", quality: 1, icon_file_id: null, need: 30, was: 20 },
                ],
              },
            }),
            proposal(2, 12, "Claude Desktop", {
              note: {
                ...noteView(1, "Coinpurse", "warrior", "Relist the Arcanite Bars above 36g, and post the Thorium Bars from the bank."),
                replaces: { id: 7, saw: "Relist the Arcanite Bars.", now: mine, conflict: true },
              },
            }),
          ]
      : []
    ).filter((p) => s !== "lists-proposal" || p.kind === "list"),
    decided:
      s === "approvals"
        ? [
            proposal(1, 60 * 20, "Claude Desktop", {
              kind: "quest_plan",
              status: "applied",
              decided_at: iso(60 * 20),
              plan: {
                character_id: 6,
                character: "Sela",
                class: "priest",
                title: "Desolace",
                steps: [step("Hearth")],
                replaces: null,
              },
            }),
            proposal(0, 60 * 44, "Claude Desktop", {
              status: "discarded",
              decided_at: iso(60 * 44),
              note: noteView(3, "Velyra Duskmane", "druid", "Farm Felwood."),
            }),
            proposal(-1, 60 * 46, "claude-code", {
              kind: "list",
              status: "rejected",
              status_reason: "item 99999 isn't one we know",
              decided_at: null,
            }),
            proposal(-2, 60 * 70, "Claude Desktop", {
              kind: "list",
              status: "applied",
              decided_at: iso(60 * 70),
              list: { list_id: null, name: "Raid consumables", for_character: null, gone: false, changes: [] },
            }),
          ]
        : [],
  };

  // V7's alts: id, name, surname, class, race, level, copper, zone, mins ago, extra.
  type Alt = [number, string, string | null, string, string, number, number, string, number, Partial<CharacterCard>?];
  const alts: Alt[] = [
    [1, "Coinpurse", null, "warrior", "Human", 12, 27790000, "Stormwind City", 60 * 46, { xp: 5800, xp_max: 10000, rested: 4200, bag_free: 3, bag_size: 60, mail: 14 }],
    [2, "Thrandor", null, "paladin", "Human", 60, 21401872, "Eastern Plaguelands", 60 * 20, { ilvl: 63.4, bag_free: 12, bag_size: 80, played: 9 * 86400 + 4 * 3600 }],
    [3, "Velyra", "Duskmane", "druid", "Night Elf", 60, 10660000, "Moonglade", 60 * 22, { ilvl: 58.1, bag_free: 21, bag_size: 80, played: 7 * 86400 + 19 * 3600 }],
    [4, "Brannic", null, "hunter", "Dwarf", 52, 4880000, "Ironforge", 60 * 50, { xp: 64, xp_max: 100, rested: 30, bag_free: 9, bag_size: 64, played: 4 * 86400 + 2 * 3600 }],
    [5, "Fizzwick", null, "mage", "Gnome", 44, 2120000, "Tanaris", 60 * 140, { xp: 22, xp_max: 100, rested: 78, bag_free: 15, bag_size: 56, played: 3 * 86400 + 11 * 3600 }],
    [6, "Sela", null, "priest", "Human", 38, 960000, "Desolace", 60 * 200, { xp: 81, xp_max: 100, rested: 19, bag_free: 6, bag_size: 48, played: 2 * 86400 + 8 * 3600 }],
    [7, "Kaelor", null, "rogue", "Night Elf", 27, 310000, "Ashenvale", 60 * 300, { xp: 10, xp_max: 100, rested: 90, bag_free: 4, bag_size: 40, played: 86400 + 6 * 3600 }],
  ];
  const card = (
    id: number,
    name: string,
    surname: string | null,
    cls: string,
    race: string,
    level: number,
    money: number,
    zone: string,
    mins: number,
    extra: Partial<CharacterCard> = {},
  ): CharacterCard => ({
    id,
    account: "ACCOUNT1",
    group_dir: "70",
    folder: surname ? `${name}-${surname}` : name,
    name,
    surname,
    class: cls,
    race,
    level,
    realm: "Classic Beta PvP 2",
    guild: id === 2 ? "Wardens of Dawn" : null,
    zone,
    subzone: null,
    last_seen: iso(mins),
    money,
    xp: null,
    xp_max: null,
    rested: null,
    ilvl: null,
    played: null,
    bag_free: null,
    bag_size: null,
    mail: 0,
    bank_items: 0,
    bank_alt: bankAlts.has(id),
    ...extra,
  });
  // F3: Coinpurse is the bank alt, and this week's saves (resets ahead).
  const bankAlts = new Set<number>([1]);
  // P1: Thrandor's plan from character.html?plan, 2 of 5 done, in the game.
  const approved = new Date(now - 20 * 60_000).toISOString();
  const planStep = (text: string, zone: string): Plan["steps"][number] => ({ text, quest_id: null, zone, kind: null });
  let plans: Plan[] = [
    {
      id: 1,
      character_id: 2,
      character: "Thrandor",
      title: "Stratholme run",
      steps: [
        planStep("Turn in: The Archivist", "Light's Hope Chapel"),
        planStep("Pick up: Dead Man's Plea", "Stratholme gate"),
        planStep("Stratholme: Ysida Harmon", "rescue her before the Baron"),
        planStep("Turn in: Dead Man's Plea", "Stratholme gate"),
        planStep("Hearth to Light's Hope", "3 turn-ins waiting"),
      ],
      producer: "agent:Claude Desktop",
      created_at: approved,
      done: [1, 2],
      progress_at: new Date(now - 5 * 60_000).toISOString(),
      delivery: { state: "synced", since: new Date(now - 16 * 60_000).toISOString() },
    },
  ];
  // B2: lists.html, Sela's Tailoring list with an errand from Coinpurse.
  const listWho = (id: number) => {
    const a = alts.find((x) => x[0] === id)!;
    return { id, name: a[1], class: a[3] };
  };
  const held = (id: number, bags: number, bank: number, mail = 0, days = 1) => ({
    character: listWho(id),
    bags,
    bank,
    mail,
    as_of: iso(60 * 24 * days),
  });
  let listItemId = 100;
  const listItem = (
    item_id: number | null,
    name: string,
    quality: number,
    need: number,
    holders: ListItem["holders"],
    price: number | null,
    errands: ListItem["errands"] = [],
  ): ListItem => ({
    id: ++listItemId,
    item_id,
    name,
    quality,
    icon_file_id: null,
    need,
    have: holders.reduce((n, h) => n + h.bags + h.bank + h.mail, 0),
    holders,
    price,
    errands,
  });
  let lists: List[] =
    s === "lists-empty"
      ? []
      : [
          {
            id: 1,
            name: "Tailoring 300",
            for_character: listWho(6),
            producer: "app",
            created_at: iso(60 * 72),
            items: [
              listItem(14341, "Rune Thread", 1, 6, [], null),
              listItem(8343, "Heavy Silken Thread", 1, 6, [held(6, 0, 4)], null),
              listItem(14047, "Runecloth", 1, 20, [held(1, 34, 306)], 11_200, [
                { from: listWho(1), count: 20, in_bags: 20 },
              ]),
              listItem(14256, "Felcloth", 2, 8, [held(6, 8, 0)], 41_000),
            ],
          },
          {
            id: 2,
            name: "Onyxia attunement",
            for_character: listWho(3),
            producer: "app",
            created_at: iso(60 * 200),
            items: [
              listItem(16309, "Drakefire Amulet", 3, 1, [], null),
              listItem(null, "Blackhand's Command", 1, 1, [], null),
            ],
          },
          {
            id: 3,
            name: "Raid consumables",
            for_character: null,
            producer: "agent:Claude Desktop",
            created_at: iso(60 * 30),
            items: [
              listItem(13446, "Major Healing Potion", 1, 20, [held(2, 6, 0), held(1, 0, 12, 0, 9)], 3_400),
              listItem(13510, "Flask of the Titans", 1, 2, [], 680_000),
              listItem(13461, "Greater Arcane Protection Potion", 1, 5, [held(3, 5, 0)], 21_500),
            ],
          },
        ];
  // B3: the sheet's bag items by id (character_detail's ids), and each
  // character's marks: item id -> { to (0 to sell), reason, producer }.
  const cleanupItems: Record<number, { name: string; quality: number; count: number; sell: number | null }> = {
    1001: { name: "Hearthstone", quality: 1, count: 1, sell: null },
    1002: { name: "Runecloth", quality: 1, count: 40, sell: 40 },
    1003: { name: "Broken Fang", quality: 0, count: 6, sell: 6 },
    1004: { name: "Torn Bear Pelt", quality: 0, count: 3, sell: 16 },
    1005: { name: "Truestrike Shoulders", quality: 3, count: 1, sell: 12_100 },
    1101: { name: "Major Healing Potion", quality: 1, count: 12, sell: 1_000 },
  };
  type MockMark = { to: number; reason: Reason | null; producer: string };
  // B3b: the Fangs from an accepted suggestion, the Runecloth from an
  // approved Claude proposal. Suggested: the pelts (grey) and the shoulders
  // (+9 for Kaelor).
  const allMarks = new Map<number, Map<number, MockMark>>([
    [
      2,
      new Map([
        [1003, { to: 0, reason: { code: "grey" }, producer: "app" }],
        [1002, { to: 6, reason: null, producer: "agent:Claude Desktop" }],
      ]),
    ],
  ]);
  const suggestable: Record<number, MockMark> = {
    1004: { to: 0, reason: { code: "grey" }, producer: "app" },
    1005: { to: 7, reason: { code: "upgrade", gain: 9 }, producer: "app" },
  };
  const dismissed = new Set<string>();
  const cleanupMarks = (id: number) => {
    if (!allMarks.has(id)) allMarks.set(id, new Map());
    return allMarks.get(id)!;
  };
  const cleanupRow = (item: number, m: MockMark): Marked => {
    const it = cleanupItems[item];
    return {
      item_id: item,
      name: it.name,
      quality: it.quality,
      icon: null,
      count: it.count,
      sell_price: it.sell,
      to: m.to ? listWho(m.to) : null,
      reason: m.reason,
      producer: m.producer,
    };
  };
  const cleanupSuggested = (id: number) =>
    id !== 2
      ? []
      : Object.entries(suggestable).filter(
          ([item]) => !cleanupMarks(id).has(Number(item)) && !dismissed.has(`${id}:${item}`),
        );
  const cleanupView = (id: number): Cleanup => ({
    marks: [...cleanupMarks(id).entries()].filter(([item]) => cleanupItems[item]).map(([item, m]) => cleanupRow(item, m)),
    suggestions: cleanupSuggested(id).map(([item, m]) => cleanupRow(Number(item), m)),
    delivery: { state: "synced", since: iso(18) },
  });
  const listsView = (): ListsView => ({
    lists: noAddon ? [] : lists,
    delivery: { state: "synced", since: iso(18) },
    briefing: { state: "pending", written_at: iso(4) },
    scan_at: iso(60 * 24 * 3),
  });
  const seen: SeenItem[] = [
    { item_id: 14047, name: "Runecloth", quality: 1, icon_file_id: null },
    { item_id: 14048, name: "Bolt of Runecloth", quality: 1, icon_file_id: null },
    { item_id: 14342, name: "Mooncloth", quality: 2, icon_file_id: null },
    { item_id: 12359, name: "Thorium Bar", quality: 1, icon_file_id: null },
    { item_id: 13446, name: "Major Healing Potion", quality: 1, icon_file_id: null },
  ];
  // F6: addon toggles made in this page load ("Addon/CharacterFolder" → on),
  // and what the last one replaced, for Undo.
  const addonToggles = new Map<string, boolean>();
  let lastToggle = new Map<string, boolean | undefined>();
  const save = (id: number, name: string, raid: boolean, minsAhead: number, difficulty = "Normal"): AltLockout => {
    const a = alts.find((x) => x[0] === id)!;
    return {
      character_id: id,
      character: a[1],
      class: a[3],
      lockout: { name, difficulty, raid, reset_at: iso(-minsAhead) },
    };
  };
  // F5: the round-3 ah.html items. id: [name, quality, price g, median g,
  // sightings, last seen days ago, listed].
  const ahItems: Record<number, { name: string | null; q: number; p: number; med: number; n: number; ago: number; listed: number }> = {
    12360: { name: "Arcanite Bar", q: 2, p: 36.4, med: 41.1, n: 12, ago: 3, listed: 84 },
    14047: { name: "Runecloth", q: 1, p: 1.12, med: 1.08, n: 31, ago: 3, listed: 900 },
    13468: { name: "Black Lotus", q: 2, p: 82, med: 75, n: 4, ago: 3, listed: 2 },
    13446: { name: "Major Healing Potion", q: 1, p: 1.85, med: 1.89, n: 18, ago: 3, listed: 140 },
    13510: { name: "Flask of the Titans", q: 1, p: 64, med: 62, n: 6, ago: 12, listed: 9 },
    12808: { name: "Essence of Undeath", q: 1, p: 3.4, med: 3.3, n: 8, ago: 3, listed: 60 },
    12811: { name: "Righteous Orb", q: 2, p: 4, med: 4.2, n: 5, ago: 12, listed: 11 },
  };
  const dayAgo = (d: number) => new Date(now - d * 86_400_000).toISOString().slice(0, 10);
  const watched = [12360, 14047, 13468, 13446, 13510];
  // Arcanite's scans over 30 days (ah.html's S array): [days ago, lowest g].
  const arcaniteScans: [number, number][] = [
    [29, 44], [27, 43.5], [24, 45], [22, 42], [20, 41], [17, 40.5], [15, 42.6], [12, 39.8], [10, 38.4], [8, 39.5], [6, 37.9], [3, 36.4],
  ];
  const ahPoints = (id: number) => {
    const it = ahItems[id];
    const scans: [number, number][] =
      id === 12360
        ? arcaniteScans
        : Array.from({ length: Math.min(it.n, 12) }, (_, k) => [it.ago + (Math.min(it.n, 12) - 1 - k) * 2, it.med * (0.92 + ((k * 37) % 17) / 100)]);
    return scans.map(([d, g]) => ({ day: dayAgo(d), low: Math.round(g * 10_000), high: Math.round(g * 10_600), available: it.listed }));
  };
  const ahItem = (id: number) => {
    const it = ahItems[id];
    return {
      item_id: id,
      name: it.name,
      quality: it.q,
      icon: null,
      price: Math.round(it.p * 10_000),
      last_seen: dayAgo(it.ago),
      sightings: it.n,
      median: Math.round(it.med * 10_000),
      recent: ahPoints(id).map((p) => p.low),
      listed: it.listed,
    };
  };
  type Held = [number, string, string, string, number];
  const sellable = (id: number, count: number, held: Held[], confidence: Sellable["confidence"], caution: string | null = null): Sellable => ({
    item: ahItem(id),
    count,
    holdings: held.map(([character_id, character, cls, location, n]) => ({ character_id, character, class: cls, location, count: n })),
    value: Math.round(ahItems[id].p * 10_000 * count),
    confidence,
    caution,
  });
  const saves: AltLockout[] = [
    save(4, "Scholomance", false, 60 * 17 + 20),
    save(2, "Molten Core", true, 60 * 52),
    save(3, "Molten Core", true, 60 * 52),
    save(2, "Onyxia's Lair", true, 60 * 24 * 4 + 60 * 6),
  ];

  const flavor = {
    id: "_classic_beta_",
    label: "WoW: Forever (Beta)",
    product: "wow_classic_beta",
    version: "1.60.1.70009",
    is_forever: true,
    dir: "C:\\Program Files (x86)\\World of Warcraft\\_classic_beta_",
    exe: "C:\\Program Files (x86)\\World of Warcraft\\_classic_beta_\\WowB.exe",
    has_wtf: true,
    accounts: ["ACCOUNT1"],
    characters: 7,
    links: [],
  };
  const install = {
    root: "C:\\Program Files (x86)\\World of Warcraft",
    flavors: [flavor],
    active: "_classic_beta_",
  };

  // Typed against the bindings, so a new field on the Rust side fails tsc
  // here instead of crashing a screen in the harness.
  const sum = (
    id: string,
    trigger: Trigger,
    kind: SnapshotKind,
    mins: number,
    extra: Partial<SnapshotSummary> = {},
  ): SnapshotSummary => ({
    id,
    created_at: iso(mins),
    trigger,
    kind,
    label: null,
    pinned: false,
    scope: "full",
    flavor: "_classic_beta_",
    game_running: false,
    file_count: 1912,
    char_count: 7,
    addon_count: 52,
    total_bytes: 48.2e6,
    new_bytes: 1e5,
    ...extra,
  });
  const list = [
    sum("S1", "game_exit", "Auto", 120),
    sum("S2", "pre_restore", "Safety", 60 * 3, {
      label: "Before restoring macros and 2 addon settings",
      scope: "partial",
      file_count: 3,
      char_count: 1,
      addon_count: 2,
      total_bytes: 2.1e5,
    }),
    sum("S3", "manual", "Manual", 60 * 26, { label: "Before the raid UI rework", game_running: true }),
    sum("S4", "app_start", "Auto", 60 * 50),
    sum("S5", "game_exit", "Auto", 60 * 52),
    sum("S6", "scheduled", "Auto", 60 * 74),
  ];

  const cats = (n: number): CategoryNode[] => [
    { category: "BindingsMacros", totals: { files: 2, bytes: 3.1e4 } },
    { category: "AddonSettings", totals: { files: n, bytes: 1.9e6 } },
    { category: "ChatLayout", totals: { files: 2, bytes: 1.2e4 } },
  ];
  const detail: SnapshotDetail = {
    summary: list[0],
    accounts: [
      {
        name: "ACCOUNT1",
        totals: { files: 1900, bytes: 4.8e7 },
        categories: [
          { category: "AddonSettings", totals: { files: 18, bytes: 9e6 } },
          { category: "BindingsMacros", totals: { files: 2, bytes: 2e4 } },
        ],
        characters: [
          // Forever's layout (probe run 1): an opaque group id, and the
          // surname in the character folder, plus Velyra's older folder from
          // before surnames (W1b).
          { realm: "70", name: "Thrandor", older: false, totals: { files: 45, bytes: 2.1e6 }, categories: cats(41) },
          {
            realm: "70",
            name: "Velyra-Duskmane",
            older: false,
            totals: { files: 30, bytes: 1.4e6 },
            categories: cats(26),
          },
          {
            realm: "Classic Beta PvP 2",
            name: "Velyra",
            older: true,
            totals: { files: 12, bytes: 3.1e5 },
            categories: cats(8),
          },
        ],
      },
    ],
    addons: [
      { name: "Details", totals: { files: 4, bytes: 3e6 } },
      { name: "Questie", totals: { files: 2, bytes: 1e6 } },
    ],
    other: { files: 2, bytes: 4000 },
    skipped: [],
  };

  const plan: RestorePlan = {
    snapshot_id: "S1",
    mode: "mirror",
    write_count: 3,
    bytes: 1.9e6,
    unchanged: 40,
    read_only: [],
    write: [
      { folder: "WTF/Account/ACCOUNT1/Ashenvale/Thrandor", files: ["macros-cache.txt"], bytes: 2e4 },
      {
        folder: "WTF/Account/ACCOUNT1/Ashenvale/Thrandor/SavedVariables",
        files: ["Details.lua", "Questie.lua"],
        bytes: 1.8e6,
      },
    ],
    delete: ["WTF/Account/ACCOUNT1/Ashenvale/Thrandor/SavedVariables/NewAddon.lua"],
    not_backed_up: [],
    summary: "macros and 2 addon settings; removes 1 file",
  };
  const journal = {
    source_snapshot: "S1",
    original_pre_restore: "S2",
    pre_restore_snapshot: "S2",
    selection: { items: [{ kind: "Everything" }] },
    mode: "overlay",
    flavor: "_classic_beta_",
    started_at: iso(60 * 3 + 10),
    summary: "macros and 2 addon settings",
  };
  const failure = {
    problem: "database",
    message: "database error: schema version 4 is newer than this build (3)",
    paths: {
      config_dir: "C:\\Users\\julia\\AppData\\Roaming\\com.juliarvalenti.wowforeverbuddy",
      local_data_dir: "C:\\Users\\julia\\AppData\\Local\\com.juliarvalenti.wowforeverbuddy",
      log_dir: "C:\\Users\\julia\\AppData\\Local\\com.juliarvalenti.wowforeverbuddy\\logs",
    },
    at_fault: "C:\\Users\\julia\\AppData\\Local\\com.juliarvalenti.wowforeverbuddy\\buddy.db",
  };
  const detectReport = {
    candidates: [],
    looked_in: [
      "C:\\Program Files (x86)\\World of Warcraft\\_retail_",
      "C:\\Program Files (x86)\\World of Warcraft",
      "C:\\Program Files\\World of Warcraft",
      "C:\\World of Warcraft",
      "C:\\Games\\World of Warcraft",
      "D:\\World of Warcraft",
    ].map((path, i) => ({ source: i === 0 ? "registry" : "common_path", path })),
  };
  const who = (...names: string[]) => names.map((name) => ({ account: "ACCOUNT1", realm: "70", name }));
  const session = (
    id: number,
    startMins: number,
    endMins: number | null,
    characters = who(),
    adventures: number[] = [],
  ): PlaySession => ({
    id,
    flavor: "_classic_beta_",
    started_at: iso(startMins),
    ended_at: endMins == null ? null : iso(endMins),
    characters,
    adventures,
    crashed: false,
  });

  // backups-corrupt: as in backups.html?error=corrupt, two copies that don't
  // match and one that's gone.
  const thrandor = "WTF/Account/ACCOUNT1/Ashenvale/Thrandor";
  const damaged = {
    files: [
      `${thrandor}/SavedVariables/Details.lua`,
      `${thrandor}/SavedVariables/Bartender4.lua`,
      `${thrandor}/macros-cache.txt`,
    ],
    missing: [`${thrandor}/macros-cache.txt`],
    unreadable: [] as string[],
  };
  // backups-locked: two copies another program holds open; nothing damaged.
  const locked = {
    files: [`${thrandor}/SavedVariables/Details.lua`, `${thrandor}/SavedVariables/Questie.lua`],
    missing: [] as string[],
    unreadable: [`${thrandor}/SavedVariables/Details.lua`, `${thrandor}/SavedVariables/Questie.lua`],
  };

  // Scenario state that changes as you click through.
  let unlocked = false; // backups-locked: the lock is gone by the first "Try again"
  // Settings (F1): toggles and keys stick for the page's life.
  const settings = {
    backup: {
      location: null as string | null,
      include_addons: false,
      on_app_start: true,
      on_game_exit: true,
      schedule_hours: s.startsWith("settings") ? 24 : 0,
    },
    install:
      s === "nogame"
        ? null
        : { root: "C:\\Program Files (x86)\\World of Warcraft", flavor: "_classic_beta_", links: [] },
    // F8c: off by default, as in the app ("settings-icons": already on).
    item_icons: s.startsWith("settings-icons"),
    // P2a: off by default ("settings-agents": on, with some activity).
    agent_access: ["settings-agents", "approvals", "approvals-empty", "lists-proposal"].includes(s),
    ui: {} as Record<string, string>,
  };
  const secrets = new Map<IntegrationId, boolean>([
    ["curseforge", s.startsWith("settings")],
    ["github", s.startsWith("settings")],
  ]);
  let refused = false;
  let recoveryRefused = false;
  let resolved = false;

  // V4: `addon-ready` (WoW closed, not installed; Install works),
  // `addon-installed`, `addon-update` (an older version installed).
  // The default dashboard has WoW running, so Install is locked.
  const addon: AddonStatus = {
    installed_version:
      s === "addon-installed" || s === "characters" || s === "dashboard"
        ? "0.2.0"
        : s === "addon-update"
          ? "0.1.0"
          : null,
    bundled_version: "0.2.0",
    update_available: s === "addon-update",
    enabled_on: ["Brannic", "Coinpurse", "Fizzwick", "Kaelor", "Sela", "Thrandor"],
    disabled_on: ["Velyra-Duskmane"],
  };

  // V8: the Ledger, from gold.html's numbers. `ledger-empty` has no data yet.
  const ledgerFor = (range: LedgerRange): Ledger => {
    const n = range === "week" ? 7 : range === "month" ? 30 : range === "quarter" ? 90 : 31;
    const day = (i: number) => new Date(now - (n - 1 - i) * 86_400_000).toISOString().slice(0, 10);
    // From `a` gold to `end` copper, ending exactly on `end` so the chart's last
    // day, the Account gold tile and the cards all agree (as ledger.rs does).
    const walk = (a: number, end: number, seed: number) =>
      Array.from({ length: n }, (_, i) =>
        i === n - 1
          ? end
          : Math.round((a + ((end / 10_000 - a) * i) / Math.max(1, n - 1) + Math.sin(i * seed) * 30) * 10_000),
      );
    const money = (id: number) => alts.find((r) => r[0] === id)![6];
    const series = [
      { character_id: 1, name: "Coinpurse", count: 1, values: walk(2140, money(1), 1.3) },
      { character_id: 2, name: "Thrandor Vargur", count: 1, values: walk(1650, money(2), 0.7) },
      { character_id: 3, name: "Velyra Duskmane", count: 1, values: walk(1080, money(3), 2.1) },
      { character_id: 4, name: "Brannic", count: 1, values: walk(300, money(4), 0.4) },
      { character_id: null, name: "3 others", count: 3, values: walk(280, money(5) + money(6) + money(7), 1.1) },
    ];
    const account = series[0].values.map((_, i) => series.reduce((a, s) => a + s.values[i], 0));
    const at = (minsAgo: number) => iso(minsAgo);
    const entry = (
      id: number,
      characterId: number,
      name: string,
      startMinsAgo: number,
      mins: number,
      delta: number,
      note: { text: string; quality: number | null } | null,
      level: number | null = null,
    ): JournalEntry => ({
      adventure_id: id,
      character_id: characterId,
      name,
      login: at(startMinsAgo),
      logout: at(startMinsAgo - mins),
      played_secs: mins * 60,
      gold_delta: delta * 10_000,
      level,
      of_note: note,
    });
    return {
      since: iso(60 * 24 * 33),
      tiles: {
        account_gold: account[n - 1],
        characters: 7,
        last_30_days: 12_020_000,
        this_week: 4_120_000,
        best_earner: { character_id: 2, name: "Thrandor Vargur", gained: 4_900_000, sessions: 9 },
      },
      chart: {
        days: Array.from({ length: n }, (_, i) => day(i)),
        series,
        account,
      },
      journal: [
        entry(1, 2, "Thrandor Vargur", 60 * 22, 192, 312, { text: "Reached level 60", quality: null }, 60),
        entry(2, 1, "Coinpurse", 60 * 27, 22, 640, { text: "Arcanite Bar ×12", quality: 2 }),
        entry(3, 3, "Velyra Duskmane", 60 * 48, 125, -86, { text: "Stormwind City", quality: null }),
        entry(4, 2, "Thrandor Vargur", 60 * 51, 160, 178, { text: "Runecloth ×60", quality: 1 }),
        entry(5, 4, "Brannic", 60 * 74, 115, 41, { text: "Feralas · 14 quests", quality: null }, 52),
      ],
    };
  };

  // V9: session.html's Stratholme evening (192 minutes, 22 hours ago).
  const adventureFor = (id: number): Adventure => {
    const start = 60 * 22;
    const t = (mins: number) => iso(start - mins);
    const line = (mins: number, kind: string, text: string, extra: Partial<Adventure["timeline"][0]> = {}) => ({
      at: t(mins),
      kind,
      text,
      detail: null,
      quality: null,
      withheld: false,
      ...extra,
    });
    return {
      id,
      character_id: 2,
      name: "Thrandor Vargur",
      class: "PALADIN",
      race: "Human",
      login: t(0),
      logout: t(192),
      played_secs: 192 * 60,
      title: "Stratholme & Eastern Plaguelands",
      level_start: 59,
      level_end: 60,
      last_zone: "Eastern Plaguelands",
      travelled: ["Eastern Plaguelands", "Stratholme"],
      tally: { gold: 3_124_000, xp: null, quest_xp: 148_210, loot: 47, deaths: 1, repairs: 180_000 },
      money: [
        { at: t(0), money: 18_280_000 },
        { at: t(40), money: 18_910_000 },
        { at: t(60), money: 18_850_000 },
        { at: t(126), money: 19_640_000 },
        { at: t(150), money: 21_020_000 },
        { at: t(192), money: 21_404_000 },
      ],
      markers: [
        { at: t(58), kind: "death", label: "Died in Stratholme" },
        { at: t(126), kind: "encounter", label: "Defeated Baron Rivendare" },
        { at: t(135), kind: "level", label: "Reached level 60" },
        { at: t(150), kind: "quest", label: "Turned in The Archivist" },
      ],
      timeline: [
        line(0, "login", "Logged in", { detail: "1,828g" }),
        line(0, "zone", "Travelled to Eastern Plaguelands"),
        line(12, "zone", "Entered Stratholme"),
        line(58, "death", "Died in Stratholme", { withheld: true }),
        line(60, "repair", "Repaired for 6g"),
        line(126, "encounter", "Defeated Baron Rivendare"),
        line(127, "loot", "Gained Truestrike Shoulders", { quality: 3, withheld: true }),
        line(135, "level", "Reached level 60"),
        line(140, "zone", "Travelled to Eastern Plaguelands"),
        line(150, "quest", "Turned in The Archivist", { detail: "+62g · +38400 XP" }),
        line(192, "logout", "Logged out in Eastern Plaguelands", { detail: "2,140g" }),
      ],
      gained: [
        // Thrandor wears the shoulders now (the recap's Worth: "equipped").
        { item_id: 16_000, name: "Truestrike Shoulders", quality: 3, count: 1, how: null, equipped: true },
        { item_id: 14_047, name: "Runecloth", quality: 1, count: 40, how: null, equipped: false },
        { item_id: 13_446, name: "Major Healing Potion", quality: 1, count: 6, how: null, equipped: false },
      ],
      spent: [
        { item_id: 13_510, name: "Flask of the Titans", quality: 1, count: 1, how: "used", equipped: false },
        { item_id: 13_446, name: "Major Healing Potion", quality: 1, count: 4, how: "used", equipped: false },
        { item_id: 999, name: "Vendor junk", quality: 0, count: 22, how: "sold", equipped: false },
      ],
      quests: [
        { title: "The Archivist", zone: "Eastern Plaguelands" },
        { title: "Dead Man's Plea", zone: "Stratholme" },
      ],
      note: "Ding at last. The Baron dropped the shoulders on the second run.",
      prev: { id: id + 1, name: "Velyra Duskmane", login: iso(60 * 48) },
      next: id > 1 ? { id: id - 1, name: "Coinpurse", login: iso(60 * 20) } : null,
    };
  };

  const handlers: Record<string, Handler> = {
    ledger_get: ({ range }) =>
      s === "ledger-empty"
        ? {
            since: null,
            tiles: { account_gold: 0, characters: 0, last_30_days: 0, this_week: 0, best_earner: null },
            chart: { days: [], series: [], account: [] },
            journal: [],
          }
        : ledgerFor(range as LedgerRange),
    ledger_export_csv: () => "C:\\Users\\Julia\\Documents\\forever-buddy-gold.csv",
    addon_status: () => addon,
    addon_install: () => {
      addon.installed_version = addon.bundled_version;
      addon.update_available = false;
      return addon;
    },
    startup_failure: () => (s === "startup-error" ? failure : null),
    game_status: () => ({ running, pids: running ? [4242] : [], since: running ? iso(102) : null, unknown: false }),
    install_get: () => {
      if (s === "dashboard-missing")
        throw { kind: "InvalidInstall", detail: "D:\\World of Warcraft\\_classic_beta_ has no WTF folder" };
      return s === "nogame" || s === "detect-failed" ? null : install;
    },
    install_detect: () => {
      if (s === "detect-failed") throw { kind: "Io", detail: "registry access denied" };
      return detectReport;
    },
    install_set: () => install,
    // F8: icons don't load outside the app (no icon://), so screens show
    // their letter tiles; Settings shows a cache as if they had.
    // As the backend: while icons are off, not even the build is read.
    icons_cache_status: () => ({
      files: iconFiles,
      bytes: iconFiles * 9_800,
      build: settings.item_icons ? "1.60.1.70205" : null,
      unreadable: settings.item_icons && s === "settings-icons-unreadable",
    }),
    icons_cache_rebuild: () => {
      iconFiles = 412;
      return { read: 410, failed: 2 };
    },
    icons_cache_clear: () => {
      iconFiles = 0;
      return null;
    },
    settings_get: () => structuredClone(settings),
    settings_update: ({ patch }) => {
      // null leaves a field as is, except location, where it means the default.
      const b = (patch as { backup?: Record<string, unknown> }).backup ?? {};
      for (const [k, v] of Object.entries(b))
        if (v !== null || k === "location") Object.assign(settings.backup, { [k]: v });
      const p = patch as {
        item_icons?: boolean | null;
        agent_access?: boolean | null;
        ui?: Record<string, string | null>;
      };
      if (p.item_icons != null) settings.item_icons = p.item_icons;
      if (p.agent_access != null) settings.agent_access = p.agent_access;
      for (const [k, v] of Object.entries(p.ui ?? {}))
        if (v === null) delete settings.ui[k];
        else settings.ui[k] = v;
      // A copy, as the real backend sends: React skips a re-render for the
      // same object.
      return structuredClone(settings);
    },
    secrets_status: (): SecretStatus[] =>
      (["curseforge", "wago", "github", "battlenet_client_id", "battlenet_client_secret"] as const).map((id) => ({
        id,
        is_set: secrets.get(id) ?? false,
        error: null,
      })),
    secrets_set: ({ id }) => {
      secrets.set(id as IntegrationId, true);
      return null;
    },
    secrets_delete: ({ id }) => {
      secrets.delete(id as IntegrationId);
      return null;
    },
    agent_status: () => ({
      command: "C:\\Program Files\\WoW Forever Buddy\\wow-forever-buddy.exe",
      flag: "--mcp",
      activity:
        s === "settings-agents"
          ? [
              { at: new Date(Date.now() - 2 * 60_000).toISOString(), client: "Claude Desktop", tool: "get_quests", ok: true },
              { at: new Date(Date.now() - 3 * 60_000).toISOString(), client: "Claude Desktop", tool: "list_characters", ok: true },
              { at: new Date(Date.now() - 26 * 3600_000).toISOString(), client: "claude-code", tool: "get_character", ok: false },
            ]
          : [],
    }),
    app_info: () => ({
      version: "0.2.0",
      paths: {
        config_dir: "C:\\Users\\Julia\\AppData\\Roaming\\com.juliarvalenti.wowforeverbuddy",
        local_data_dir: "C:\\Users\\Julia\\AppData\\Local\\com.juliarvalenti.wowforeverbuddy",
        log_dir: "C:\\Users\\Julia\\AppData\\Local\\com.juliarvalenti.wowforeverbuddy\\logs",
      },
    }),
    backup_prune_now: () => ({
      pruned: [],
      blobs_removed: 0,
      freed_bytes: 0,
      used_bytes: 1.16e9,
      budget_bytes: 5.37e9,
      over_budget: false,
    }),
    "plugin:dialog|open": () => "D:\\Backups",
    backup_move_location: ({ location }) => {
      if (s === "settings-pending") throw { kind: "RestorePending" };
      if (s === "settings-moving") {
        // Part way through the copy, and it stays there.
        setTimeout(() => emit("move-progress", { done: 412, total: 1843 }), 50);
        return new Promise(() => {});
      }
      settings.backup.location = (location as string | null) ?? null;
      return {
        dir: location ? `${location}\\WoW Forever Buddy backups` : "C:\\…\\backups",
        files: 1843,
        bytes: 1.16e9,
        left_behind: null,
      };
    },
    app_open_folder: () => null,
    // F7: macros-readonly.html's macros for Thrandor, plus account macros.
    macros_list: (): MacrosList => {
      const m = (name: string, body: string): Macro => ({
        name,
        icon: "INV_Misc_QuestionMark",
        body,
        length: new TextEncoder().encode(body).length,
      });
      const filler = (n: number) => `/run print("${"x".repeat(Math.max(0, n - 13))}")`;
      const thrandor = [
        m(
          "Judge + Seal",
          "#showtooltip Judgement\n/cast [mod:shift] Seal of Light; [mod:ctrl] Seal of Wisdom\n/cast [@target,harm,nodead] Judgement\n/use 13\n/startattack\n/cast [nomod] Seal of Command" +
            " ".repeat(61), // 231 bytes, as in the mock: the warn state
        ),
        m("Holy Light @mouseover", "#showtooltip\n/cast [@mouseover,help,nodead][] Holy Light\n/stopmacro [nomod]\n/say Healing!"),
        m("BoP focus", "#showtooltip Blessing of Protection\n/cast [@focus,help] Blessing of Protection"),
        m("Cleanse self", "#showtooltip Cleanse\n/cast [@player] Cleanse"),
        m("Mount", "#showtooltip\n/cast Summon Warhorse"),
        m("Trinket + Wings", "#showtooltip\n/use 13\n/use 14\n/cast Avenging Wrath"),
        m("Divine Shield + Hearth", filler(262)),
        m("Seal twist", filler(143)),
        m("Righteous Fury", "/cast Righteous Fury"),
        m("Lay on Hands focus", "#showtooltip\n/cast [@focus] Lay on Hands"),
        m("Wisdom party", filler(76)),
      ];
      const none = s === "macros-empty";
      return {
        flavor: "_classic_beta_",
        accounts: [
          {
            account: "ACCOUNT1",
            macros: none ? [] : [m("Assist main tank", "/assist [@focus]"), m("Ready check", "/readycheck")],
            modified: none ? null : iso(60 * 24 * 2),
          },
        ],
        characters: ["Thrandor", "Velyra-Duskmane", "Brannic", "Fizzwick", "Sela"].map((folder) => ({
          account: "ACCOUNT1",
          group: "70",
          folder,
          macros: folder === "Thrandor" && !none ? thrandor : [],
          modified: folder === "Thrandor" && !none ? iso(60 * 24 * 2) : null,
        })),
        max: 255,
      };
    },
    // F6: a toggle remembers itself until undone (one level, like the app's
    // last-change Undo).
    addons_apply: ({ changes }) => {
      const list = changes as AddonChange[];
      const keys = list.map((c) => `${c.addon}/${c.character.folder}`);
      lastToggle = new Map(keys.map((k) => [k, addonToggles.get(k)]));
      list.forEach((c, i) => addonToggles.set(keys[i], c.enabled));
      return { snapshot_id: "S9", applied: list.length };
    },
    addons_undo: () => {
      for (const [k, v] of lastToggle) {
        if (v === undefined) addonToggles.delete(k);
        else addonToggles.set(k, v);
      }
      lastToggle = new Map();
      return null;
    },
    // F4: addons-readonly.html's ten addons and five characters.
    addons_list: (): AddonsList => {
      const folders = ["Thrandor", "Velyra-Duskmane", "Brannic", "Fizzwick", "Sela"];
      const dir = "C:\\Program Files (x86)\\World of Warcraft\\_classic_beta_\\Interface\\AddOns";
      const a = (
        name: string,
        title: string,
        version: string,
        author: string,
        iface: number,
        enabled: number[],
        notes: string | null = null,
        needs: string[] = [],
      ): AddonInfo => ({
        name,
        title,
        version,
        author,
        notes,
        interfaces: [iface],
        out_of_date: iface < 16001,
        needs,
        path: `${dir}\\${name}`,
        // F6: toggles made in this page load win over the canned states.
        enabled: enabled.map((on, i) => addonToggles.get(`${name}/${folders[i]}`) ?? Boolean(on)),
      });
      return {
        flavor: "_classic_beta_",
        game: "WoW: Forever (Beta)",
        folder: dir,
        interface: 16001,
        // In addons-linked, Velyra's settings folder is a link the gate refuses.
        characters: folders.map((folder) => ({
          account: "ACCOUNT1",
          group: "70",
          folder,
          linked: s === "addons-linked" && folder === "Velyra-Duskmane",
        })),
        read_at: iso(4),
        addons: s === "addons-empty"
          ? []
          : [
              a("AtlasLootClassic", "AtlasLoot Classic", "v2.4.6", "Hoizame", 11502, [1, 0, 1, 0, 0]),
              a("Auctionator", "Auctionator", "11.1.4", "plusmouse", 16001, [1, 0, 0, 0, 0]),
              a("Bagnon", "Bagnon", "10.2.5", "Jaliborc", 16001, [1, 1, 1, 1, 1]),
              a("ClassicCastbars", "ClassicCastbars", "1.7.9", "wardz", 11504, [1, 1, 1, 1, 1]),
              a("Details", "Details! Damage Meter", "v16001.27", "Tercio", 16001, [1, 1, 1, 1, 1]),
              a("ForeverBuddy", "ForeverBuddy", "0.2.0", "Forever Buddy", 16001, [1, 1, 1, 1, 1]),
              a("HealBot", "HealBot Continued", "10.2.0", "Strife", 16001, [0, 1, 0, 0, 1]),
              a("MoveAnything", "MoveAnything", "2.1.0", "Vika", 16001, [0, 0, 0, 0, 0]),
              a("OmniCC", "OmniCC", "10.2.3", "Tuller", 16001, [1, 1, 1, 1, 1]),
              a(
                "Questie",
                "Questie",
                "10.3.0",
                "Aero, Logon",
                16001,
                [0, 0, 1, 1, 1],
                "Shows quests on the map and minimap, with objectives and turn-ins.",
              ),
            ],
      };
    },
    startup_open_data_folder: () => null,
    backup_list: () => list,
    backup_storage: (): StorageInfo => ({
      used_bytes: 1.16e9,
      budget_bytes: 5.37e9,
      over_budget: false,
      cleanup_blocked: null,
      // retention.rs Policy::summary with the default POLICY.
      retention_summary:
        "Automatic: everything from the last 48 h, then one a day for 14 days and one a week for 8 weeks. Safety: 30 days (at least the last 20). Manual and pinned: kept until you delete them.",
    }),
    backup_auto_status: () =>
      s === "backup-failed"
        ? {
            at: iso(3),
            trigger: "game_exit",
            error: "WTF\\Account\\ACCOUNT1\\SavedVariables\\Details.lua is locked by another program",
            skipped: 0,
          }
        : null,
    backup_create: () => sum("S0", "manual", "Manual", 0),
    // In backups-corrupt only the newest snapshot (S1) is damaged. In
    // backups-locked, checking again finds the lock released.
    backup_verify: ({ id }): VerifyReport => {
      if (s === "backups-locked") unlocked = true;
      const bad = s === "backups-corrupt" && id === "S1";
      return {
        snapshot_id: id as string,
        files: 1912,
        corrupt: bad ? damaged.files : [],
        missing: bad ? damaged.missing : [],
        unreadable: [],
      };
    },
    backup_get: () => {
      if (s === "snapshot-unreadable") throw { kind: "Io", detail: "manifest for S1 is unreadable" };
      return detail;
    },
    backup_restore_preview: () =>
      s === "backups-deletions" && refused
        ? { ...plan, delete: [...plan.delete, "WTF/Account/ACCOUNT1/Ashenvale/Thrandor/SavedVariables/FromLogout.lua"] }
        : plan,
    backup_restore: () => {
      if (s === "backups-deletions") {
        refused = true;
        throw { kind: "DeletionsChanged", detail: { paths: ["x"] } };
      }
      if (s === "backups-partial") {
        refused = true;
        throw { kind: "Io", detail: "Details.lua is locked by another program" };
      }
      if (s === "backups-corrupt") throw { kind: "BackupCorrupt", detail: damaged };
      if (s === "backups-locked" && !unlocked) throw { kind: "BackupCorrupt", detail: locked };
      return { snapshot_id: "S1", pre_restore_snapshot: "S9", written: 3, deleted: 1, summary: plan.summary };
    },
    restore_journal_status: () =>
      resolved
        ? { kind: "none" }
        : s === "recover" || (s === "backups-partial" && refused)
          ? { kind: "pending", journal }
          : s === "recover-unreadable"
            ? { kind: "unreadable", error: "restore journal is unreadable", latest_safety: "S2" }
            : s === "recover-unreadable-no-safety"
              ? { kind: "unreadable", error: "restore journal is unreadable", latest_safety: null }
              : { kind: "none" },
    restore_journal_preview: ({ action }) =>
      action === "roll_back"
        ? { ...plan, mode: "overlay", delete: ["WTF/Account/ACCOUNT1/Ashenvale/Thrandor/SavedVariables/WeakAuras.lua"] }
        : {
            ...plan,
            delete: recoveryRefused
              ? [...plan.delete, "WTF/Account/ACCOUNT1/SavedVariables/FromPlaying.lua"]
              : plan.delete,
          },
    restore_journal_resolve: ({ action }) => {
      if (action === "finish" && !recoveryRefused) {
        recoveryRefused = true;
        throw { kind: "DeletionsChanged", detail: { paths: ["x"] } };
      }
      resolved = true;
      return { snapshot_id: "S1", pre_restore_snapshot: "S9", written: 3, deleted: 1, summary: plan.summary };
    },
    // V7: the round-3 characters.html alts (no net worth, §7).
    characters_overview: () => {
      const characters = alts.map(([id, name, surname, cls, race, level, money, zone, mins, extra]) =>
        card(id, name, surname, cls, race, level, money, zone, mins, extra),
      );
      return {
        gold: characters.reduce((n, c) => n + (c.money ?? 0), 0),
        items: 1284,
        characters: noAddon ? [] : characters,
      };
    },
    lockouts_list: (): AltLockout[] => (noAddon ? [] : saves),
    // B1: login notes; Thrandor (id 2) has one waiting.
    notes_list: () => loginNotes,
    notes_add: ({ note }) => {
      const n = note as { character_id: number; text: string; once: boolean; until: number | null };
      const id = loginNotes.length + 10;
      loginNotes.unshift({
        id,
        character_id: n.character_id,
        text: n.text,
        once: n.once,
        until: n.until ? new Date(n.until * 1000).toISOString() : null,
        author: "you",
        producer: null,
        created_at: iso(0),
        shown_at: null,
      });
      return id;
    },
    // P2b: approvals.html. "approvals": a note replacing one, a conflict,
    // and some history; "approvals-empty"; "approvals-off" (access off).
    approvals_list: () => approvals,
    approvals_waiting: () => (settings.agent_access ? approvals.waiting.length : 0),
    approvals_decide: ({ id, decision }) => {
      const p = approvals.waiting.find((w) => w.id === id);
      if (!p) throw { kind: "NotFound", message: "that suggestion, or it was already decided" };
      approvals.waiting = approvals.waiting.filter((w) => w.id !== id);
      const status = decision === "decline" ? "discarded" : "applied";
      approvals.decided.unshift({ ...p, status, decided_at: iso(0) });
      return null;
    },
    notes_delete: ({ id }) => {
      loginNotes = loginNotes.filter((n) => n.id !== id);
      return null;
    },
    // Q1b: character.html?tab=quests for Thrandor (id 2); the other alts have
    // none yet. Dark (no tab) before the addon has written anything.
    quests_available: () => !noAddon,
    // P1: character.html?plan, Thrandor's active plan (IMPLEMENTING §16).
    plans_list: (): Plan[] => (noAddon ? [] : plans),
    plan_clear: ({ characterId }): Plan[] => {
      plans = plans.filter((p) => p.character_id !== characterId);
      return plans;
    },
    // B3/B3b: Thrandor's bag cleanup (character.html, IMPLEMENTING §18).
    cleanup_get: ({ characterId }): Cleanup => cleanupView(Number(characterId)),
    cleanup_mark: ({ characterId, itemId, mark }): Cleanup => {
      const m = mark as Mark;
      cleanupMarks(Number(characterId)).set(Number(itemId), {
        to: m.action === "send" ? m.to : 0,
        reason: null,
        producer: "app",
      });
      return cleanupView(Number(characterId));
    },
    cleanup_clear: ({ characterId, itemId }): Cleanup => {
      cleanupMarks(Number(characterId)).delete(Number(itemId));
      return cleanupView(Number(characterId));
    },
    cleanup_accept: ({ characterId, itemId }): Cleanup => {
      const id = Number(characterId);
      for (const [item, m] of cleanupSuggested(id))
        if (itemId == null || Number(item) === itemId) cleanupMarks(id).set(Number(item), m);
      return cleanupView(id);
    },
    cleanup_dismiss: ({ characterId, itemId }): Cleanup => {
      dismissed.add(`${characterId}:${itemId}`);
      return cleanupView(Number(characterId));
    },
    lists_get: listsView,
    list_create: ({ name, forCharacter }) => {
      lists = [
        ...lists,
        {
          id: Math.max(0, ...lists.map((l) => l.id)) + 1,
          name: String(name),
          for_character: forCharacter == null ? null : listWho(Number(forCharacter)),
          producer: "app",
          created_at: new Date().toISOString(),
          items: [],
        },
      ];
      return listsView();
    },
    list_update: ({ id, name, forCharacter }) => {
      lists = lists.map((l) =>
        l.id === id
          ? { ...l, name: String(name), for_character: forCharacter == null ? null : listWho(Number(forCharacter)) }
          : l,
      );
      return listsView();
    },
    list_delete: ({ id }) => {
      lists = lists.filter((l) => l.id !== id);
      return listsView();
    },
    list_item_add: ({ listId, item, need }) => {
      const it = item as { id?: number; name?: string };
      const known = seen.find((x) => x.item_id === it.id);
      lists = lists.map((l) =>
        l.id === listId
          ? { ...l, items: [...l.items, listItem(it.id ?? null, known?.name ?? it.name ?? "", known?.quality ?? 1, Number(need), [], null)] }
          : l,
      );
      return listsView();
    },
    list_item_need: ({ id, need }) => {
      lists = lists.map((l) => ({ ...l, items: l.items.map((i) => (i.id === id ? { ...i, need: Number(need) } : i)) }));
      return listsView();
    },
    list_item_remove: ({ id }) => {
      lists = lists.map((l) => ({ ...l, items: l.items.filter((i) => i.id !== id) }));
      return listsView();
    },
    items_seen_search: ({ query }): SeenItem[] =>
      seen.filter((x) => x.name.toLowerCase().includes(String(query).toLowerCase())),
    character_quests: ({ id }): QuestLog => {
      if (id !== 2) return { done: 0, done_as_of: null, entries: [] };
      const day = (d: number, hh: number, mm: number) => {
        const t = new Date(now - d * 86_400_000);
        t.setHours(hh, mm, 0, 0);
        return t.toISOString();
      };
      const q = (
        at: string,
        kind: QuestLog["entries"][number]["kind"],
        title: string,
        zone: string,
        giver: string,
      ): QuestLog["entries"][number] => ({ at, kind, quest_id: null, title, zone, giver, map: null, x: null, y: null });
      return {
        done: 214,
        done_as_of: day(1, 23, 59),
        entries: [
          q(day(1, 22, 10), "turned_in", "The Archivist", "Eastern Plaguelands", "Duke Nicholas Zverenhoff"),
          q(day(1, 21, 58), "turned_in", "Ramstein", "Stratholme", "Duke Nicholas Zverenhoff"),
          q(day(1, 21, 41), "turned_in", "The Truth Comes Crashing Down", "Light's Hope Chapel", "Fiona"),
          q(day(2, 20, 30), "accepted", "Dead Man's Plea", "Stratholme", "Anthion Harmon"),
          q(day(2, 20, 25), "accepted", "The Active Agent", "Eastern Plaguelands", "Betina Bigglezink"),
          q(day(3, 20, 12), "turned_in", "Houses of the Holy", "Stratholme", "Leonid Barthalomew"),
          q(day(3, 19, 47), "turned_in", "The Corruptor", "Eastern Plaguelands", "Betina Bigglezink"),
          q(day(4, 19, 0), "accepted", "Mission Accomplished!", "Eastern Plaguelands", "Betina Bigglezink"),
          q(day(8, 18, 0), "accepted", "Of Love and Family", "Eastern Plaguelands", "Tirion Fordring"),
        ],
      };
    },
    character_set_bank_alt: ({ id, bankAlt }) => {
      if (bankAlt) bankAlts.add(id as number);
      else bankAlts.delete(id as number);
      return null;
    },
    character_detail: ({ id }): CharacterSheet => {
      const row = alts.find((a) => a[0] === id) ?? alts[1];
      const c = card(...row);
      const item = (slot: number, name: string, quality: number, ilvl: number, container = 0, count = 1): ItemRow => ({
        container,
        slot,
        item_id: 1000 + container * 100 + slot,
        // B3: the Hearthstone is soulbound, so it can't be marked to send.
        bound: name === "Hearthstone",
        name,
        quality,
        ilvl,
        icon: null,
        count,
        // The journal saw the shoulders and the potions drop.
        looted_at: name === "Truestrike Shoulders" || name === "Major Healing Potion" ? iso(60 * 26) : null,
        looted_in: name === "Truestrike Shoulders" || name === "Major Healing Potion" ? "Stratholme" : null,
      });
      return {
        card: c,
        equipped: [
          item(1, "Lionheart Helm", 4, 67),
          item(2, "Mark of Fordring", 3, 63),
          item(3, "Truestrike Shoulders", 3, 63),
          item(15, "Cape of the Black Baron", 3, 63),
          item(5, "Lawbringer Chestguard", 4, 66),
          item(9, "Vambraces of the Sadist", 3, 63),
          item(10, "Lawbringer Gauntlets", 4, 66),
          item(6, "Onslaught Girdle", 4, 71),
          item(7, "Legplates of the Chromatic Defier", 3, 63),
          item(8, "Lawbringer Boots", 4, 66),
          item(11, "Don Julio's Band", 3, 65),
          item(12, "Painweaver Band", 3, 63),
          item(13, "Hand of Justice", 3, 58),
          item(14, "Drake Fang Talisman", 4, 75),
          item(16, "Ashkandi, Greatsword of the Brotherhood", 4, 77),
          item(18, "Libram of Hope", 3, 60),
        ],
        bags: [
          {
            container: 0,
            name: "Backpack",
            size: 16,
            free: 0,
            items: [
              item(1, "Hearthstone", 1, 1),
              item(2, "Runecloth", 1, 50, 0, 40),
              item(3, "Broken Fang", 0, 1, 0, 6),
              item(4, "Torn Bear Pelt", 0, 1, 0, 3),
              item(5, "Truestrike Shoulders", 3, 63),
            ],
          },
          { container: 1, name: "Mooncloth Bag", size: 20, free: 4, items: [item(1, "Major Healing Potion", 1, 55, 1, 12)] },
          { container: 2, name: "Mooncloth Bag", size: 20, free: 4, items: [] },
          { container: 3, name: "Runecloth Bag", size: 24, free: 4, items: [] },
        ],
        bank: { as_of: iso(60 * 24 * 2), bags: [{ container: 1, name: "Bank", size: 28, free: 6, items: [item(1, "Arcanite Bar", 2, 60, 1, 8)] }] },
        mail: {
          as_of: iso(60 * 24 * 9),
          messages: [{ sender: "Coinpurse", subject: "Runecloth", money: 0, cod: 0, days_left: 27.5, items: [item(1, "Runecloth", 1, 50, 1, 20)] }],
        },
        professions: [
          { name: "Blacksmithing", skill: 300, max: 300 },
          { name: "Mining", skill: 285, max: 300 },
        ],
        lockouts: saves.filter((a) => a.character_id === c.id).map((a) => a.lockout),
        // Read at the last login; Kaelor's never have been.
        lockouts_as_of: c.id === 7 ? null : c.last_seen,
        gold_30d: [46, 44, 45, 38, 40, 34, 36, 28, 31, 24, 26, 18, 20, 8].map((y, i) => ({
          at: iso(60 * 24 * (28 - i * 2)),
          money: (2140 - y * 14) * 10000,
        })),
      };
    },
    // F2: the characters-search mock's Runecloth rows, plus a few others to
    // find. Every word must be in the name, like the backend.
    characters_search: ({ query }): SearchResults => {
      const words = String(query).toLowerCase().split(/\s+/).filter((w) => w && !w.startsWith("ilvl"));
      const stock: [number, string, string, string, string, number, number][] = [
        [1, "Coinpurse", "warrior", "bank", "Runecloth", 1, 340],
        [3, "Velyra Duskmane", "druid", "bank", "Runecloth", 1, 60],
        [2, "Thrandor", "paladin", "bag", "Runecloth", 1, 40],
        [6, "Sela", "priest", "bag", "Runecloth", 1, 12],
        [1, "Coinpurse", "warrior", "bank", "Arcanite Bar", 2, 12],
        [2, "Thrandor", "paladin", "bag", "Major Healing Potion", 1, 12],
        [3, "Velyra Duskmane", "druid", "mail", "Major Healing Potion", 1, 5],
      ];
      const hits = stock
        .filter(([, , , , name]) => words.every((w) => name.toLowerCase().includes(w)))
        .map(([character_id, character, cls, location, name, quality, count], i) => ({
          character_id,
          character,
          class: cls,
          location,
          item_id: 14000 + i,
          name,
          quality,
          ilvl: 50,
          icon: null,
          count,
          // Velyra's bank visit is 12 days old, so its Where shows ember.
          as_of: location === "bag" ? null : iso(60 * 24 * (character_id === 3 ? 12 : 2)),
        }));
      return {
        hits,
        total: hits.reduce((n, h) => n + h.count, 0),
        characters: [...new Set(hits.map((h) => h.character_id))],
        more: false,
      };
    },
    characters_list: () =>
      (
        [
          ["Thrandor", 20],
          ["Velyra-Duskmane", 60 * 22],
          ["Coinpurse", 60 * 46],
          ["Brannic", 60 * 50],
          ["Fizzwick", 60 * 140],
          ["Sela", 60 * 200],
          ["Kaelor", 60 * 300],
          // Not logged in since the addon went in: a neutral card on Characters.
          ["Ashwyn", 60 * 24 * 20],
        ] as const
      )
        .map(
          ([name, mins]): WtfCharacter => ({ account: "ACCOUNT1", realm: "70", name, last_played: iso(mins), older: false }),
        )
        // Velyra's folder from before surnames (W1b): listed, not counted.
        .concat({
          account: "ACCOUNT1",
          realm: "Classic Beta PvP 2",
          name: "Velyra",
          last_played: iso(60 * 24 * 12),
          older: true,
        }),
    sessions_list: () =>
      s === "no-sessions"
        ? []
        : [
            ...(running ? [session(9, 102, null)] : []),
            // The first two have addon adventures (V9): they open the recap.
            session(8, 60 * 24 + 100, 60 * 24 - 92, who("Thrandor"), [1]),
            session(7, 60 * 29, 60 * 29 - 22, who("Coinpurse"), [2]),
            session(6, 60 * 47, 60 * 47 - 125, who("Velyra-Duskmane")),
          ],
    // V9: session.html's evening, condensed; `adventure-empty` has none.
    adventure_get: ({ id }) =>
      s === "adventure-empty" || noAddon ? null : adventureFor((id as number | null) ?? 1),
    adventure_set_note: () => null,
    // F5: ah.html's market; `ah-empty` before Auctionator has saved prices,
    // `ah-unreadable` with a file this version can't read (F5d hides the AH).
    ah_status: (): AhStatus =>
      noPrices
        ? {
            has_prices: false,
            market: null,
            items: 0,
            last_scan_at: null,
            newest_day: null,
            file: s === "ah-empty" ? "none" : "unreadable",
          }
        : {
            has_prices: true,
            market: "Forever",
            items: 4812,
            last_scan_at: iso(60 * 24 * 3 + 30),
            newest_day: dayAgo(3),
            file: "read",
          },
    ah_search: ({ query }) =>
      Object.keys(ahItems)
        .map(Number)
        .filter((id) => ahItems[id].name!.toLowerCase().includes(String(query).toLowerCase()))
        .map(ahItem),
    ah_history: ({ itemId }): AhHistory => ({ item: ahItem(itemId as number), points: ahPoints(itemId as number) }),
    ah_watchlist: () => (s === "ah-empty" ? [] : watched.map(ahItem)),
    ah_set_watched: ({ itemId, watched: on }) => {
      const id = itemId as number;
      if (on && !watched.includes(id)) watched.push(id);
      if (!on) watched.splice(watched.indexOf(id), 1);
      return null;
    },
    ah_worth_selling: (): Sellable[] =>
      s === "ah-empty"
        ? []
        : [
            sellable(12360, 24, [[1, "Coinpurse", "warrior", "bank", 24]], "sure"),
            sellable(14047, 452, [[1, "Coinpurse", "warrior", "bank", 340], [3, "Velyra", "druid", "bank", 60], [4, "Brannic", "hunter", "bag", 40], [2, "Thrandor", "paladin", "bag", 12]], "sure"),
            sellable(13468, 1, [[3, "Velyra", "druid", "bank", 1]], "rough", "few"),
            sellable(12808, 18, [[2, "Thrandor", "paladin", "bag", 18]], "fair"),
            sellable(12811, 14, [[2, "Thrandor", "paladin", "bank", 14]], "rough", "stale"),
          ],
    // F5c: gold.html's net worth (goods 2,618g, 611 of 1,284 items priced);
    // `ledger-empty` and the no-addon scenarios have no prices.
    ah_goods_worth: (): GoodsWorth =>
      noAddon || noPrices || s === "ledger-empty"
        ? { value: 0, items: 0, priced: 0, by_character: [], top: [], as_of: null }
        : {
            value: 26_180_000,
            items: 1284,
            priced: 611,
            by_character: [[1, 13_120_000], [2, 6_400_000], [3, 3_900_000], [4, 1_700_000], [5, 1_060_000]],
            top: (
              [
                [12360, "Arcanite Bar", 2, 24, 9_120_000],
                [14047, "Runecloth", 1, 452, 5_060_000],
                [13510, "Flask of the Titans", 1, 6, 3_600_000],
                [13468, "Black Lotus", 2, 2, 1_640_000],
                [12808, "Essence of Undeath", 1, 18, 612_000],
              ] as [number, string, number, number, number][]
            ).map(([item_id, name, quality, count, value]) => ({
              count,
              value,
              item: {
                item_id,
                name,
                quality,
                icon: null,
                price: value / count,
                last_seen: iso(60 * 24 * 3).slice(0, 10),
                sightings: 12,
                median: value / count,
                recent: [],
                listed: null,
              },
            })),
            as_of: iso(60 * 24 * 3 + 30),
          },
    // A price for most items asked about (the recap's "≈ worth" cells).
    ah_prices: ({ itemIds }) =>
      noAddon || noPrices ? [] : (itemIds as number[]).filter((id) => id % 5 !== 0).map((id) => [id, 2_000 + (id % 97) * 1_100]),
  };

  mockWindows("main");
  mockIPC(
    (cmd, args) => {
      const handler = handlers[cmd];
      if (!handler) throw { kind: "NotFound", detail: `mock: no handler for ${cmd}` };
      return handler((args ?? {}) as Args);
    },
    { shouldMockEvents: true },
  );
}
