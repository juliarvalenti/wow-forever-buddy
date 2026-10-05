// A fake backend for screenshots and design review: every command answers
// from canned data, picked by `?mock=<scenario>`. Only loaded when the app is
// built with VITE_MOCK=1 (`npm run build:mock`); the real build never
// contains it (see main.tsx). Names and numbers follow the round-3 mocks so
// app and mock screenshots line up.

import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type {
  AddonStatus,
  Adventure,
  CategoryNode,
  JournalEntry,
  Ledger,
  LedgerRange,
  PlaySession,
  RestorePlan,
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
  "dashboard", // WoW running, sessions, characters
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
  "ledger-empty", // the Ledger before the addon has written anything
  "adventure-empty", // Adventures before the addon has written anything
] as const;

type Args = Record<string, unknown>;
type Handler = (args: Args) => unknown;

export function installMockIpc(): void {
  const s = new URLSearchParams(location.search).get("mock") ?? "dashboard";
  const now = Date.now();
  const iso = (minsAgo: number) => new Date(now - minsAgo * 60000).toISOString();
  const running = s === "dashboard" || s === "dashboard-missing";

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
  let refused = false;
  let recoveryRefused = false;
  let resolved = false;

  // V4: `addon-ready` (WoW closed, not installed; Install works),
  // `addon-installed`, `addon-update` (an older version installed).
  // The default dashboard has WoW running, so Install is locked.
  const addon: AddonStatus = {
    installed_version:
      s === "addon-installed" ? "0.2.0" : s === "addon-update" ? "0.1.0" : null,
    bundled_version: "0.2.0",
    update_available: s === "addon-update",
    enabled_on: ["Brannic", "Coinpurse", "Fizzwick", "Kaelor", "Sela", "Thrandor"],
    disabled_on: ["Velyra-Duskmane"],
  };

  // V8: the Ledger, from gold.html's numbers. `ledger-empty` has no data yet.
  const ledgerFor = (range: LedgerRange): Ledger => {
    const n = range === "week" ? 7 : range === "month" ? 30 : range === "quarter" ? 90 : 31;
    const day = (i: number) => new Date(now - (n - 1 - i) * 86_400_000).toISOString().slice(0, 10);
    const walk = (a: number, b: number, seed: number) =>
      Array.from({ length: n }, (_, i) =>
        Math.round((a + ((b - a) * i) / Math.max(1, n - 1) + Math.sin(i * seed) * 30) * 10_000),
      );
    const series = [
      { character_id: 1, name: "Coinpurse", count: 1, values: walk(2140, 2779, 1.3) },
      { character_id: 2, name: "Thrandor Vargur", count: 1, values: walk(1650, 2140, 0.7) },
      { character_id: 3, name: "Velyra Duskmane", count: 1, values: walk(1080, 1066, 2.1) },
      { character_id: 4, name: "Brannic", count: 1, values: walk(300, 488, 0.4) },
      { character_id: null, name: "3 others", count: 3, values: walk(280, 339, 1.1) },
    ];
    const at = (minsAgo: number) => iso(minsAgo);
    const entry = (
      id: number,
      name: string,
      startMinsAgo: number,
      mins: number,
      delta: number,
      note: { text: string; quality: number | null } | null,
      level: number | null = null,
    ): JournalEntry => ({
      adventure_id: id,
      character_id: id,
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
        account_gold: 68_124_709,
        characters: 7,
        last_30_days: 12_020_000,
        this_week: 4_120_000,
        best_earner: { character_id: 2, name: "Thrandor Vargur", gained: 4_900_000, sessions: 9 },
      },
      chart: {
        days: Array.from({ length: n }, (_, i) => day(i)),
        series,
        account: series[0].values.map((_, i) => series.reduce((a, s) => a + s.values[i], 0)),
      },
      journal: [
        entry(1, "Thrandor Vargur", 60 * 22, 192, 312, { text: "Reached level 60", quality: null }, 60),
        entry(2, "Coinpurse", 60 * 27, 22, 640, { text: "Arcanite Bar ×12", quality: 2 }),
        entry(3, "Velyra Duskmane", 60 * 48, 125, -86, { text: "Stormwind City", quality: null }),
        entry(4, "Thrandor Vargur", 60 * 51, 160, 178, { text: "Runecloth ×60", quality: 1 }),
        entry(5, "Brannic", 60 * 74, 115, 41, { text: "Feralas · 14 quests", quality: null }, 52),
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
      tally: { gold: 3_124_000, xp: null, loot: 47, deaths: 1, repairs: 180_000 },
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
        line(127, "loot", "Looted Truestrike Shoulders", { quality: 3, withheld: true }),
        line(135, "level", "Reached level 60"),
        line(140, "zone", "Travelled to Eastern Plaguelands"),
        line(150, "quest", "Turned in The Archivist", { detail: "+62g · +38400 XP" }),
        line(192, "logout", "Logged out in Eastern Plaguelands", { detail: "2,140g" }),
      ],
      gained: [
        { item_id: 16_000, name: "Truestrike Shoulders", quality: 3, count: 1, how: null },
        { item_id: 14_047, name: "Runecloth", quality: 1, count: 40, how: null },
        { item_id: 13_446, name: "Major Healing Potion", quality: 1, count: 6, how: null },
      ],
      spent: [
        { item_id: 13_510, name: "Flask of the Titans", quality: 1, count: 1, how: "used" },
        { item_id: 13_446, name: "Major Healing Potion", quality: 1, count: 4, how: "used" },
        { item_id: 999, name: "Vendor junk", quality: 0, count: 22, how: "sold" },
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
    settings_get: () => ({ backup: { on_game_exit: true, on_app_start: true, schedule_hours: 0 } }),
    app_open_folder: () => null,
    startup_open_data_folder: () => null,
    backup_list: () => list,
    backup_storage: (): StorageInfo => ({
      used_bytes: 1.16e9,
      budget_bytes: 5.37e9,
      over_budget: false,
      cleanup_blocked: null,
      retention_summary:
        "Keeps everything from the last 48 hours, one a day for 2 weeks and one a week for 8 weeks. Manual and pinned backups are kept until you delete them.",
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
    adventure_get: ({ id }) => (s === "adventure-empty" ? null : adventureFor((id as number | null) ?? 1)),
    adventure_set_note: () => null,
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
