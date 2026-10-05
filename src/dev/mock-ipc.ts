// A fake backend for screenshots and design review: every command answers
// from canned data, picked by `?mock=<scenario>`. Only loaded when the app is
// built with VITE_MOCK=1 (`npm run build:mock`); the real build never
// contains it (see main.tsx). Names and numbers follow the round-3 mocks so
// app and mock screenshots line up.

import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type {
  CategoryNode,
  RestorePlan,
  SnapshotDetail,
  SnapshotKind,
  SnapshotSummary,
  StorageInfo,
  Trigger,
  VerifyReport,
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
  "backups-deletions", // restore refused: more to delete than confirmed
  "backups-partial", // restore fails partway
  "backup-failed", // the last automatic backup failed
  "snapshot-unreadable",
  "nogame", // first run, nothing found
  "detect-failed",
  "startup-error",
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
    realms: ["Ashenvale"],
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
          { realm: "Ashenvale", name: "Thrandor", totals: { files: 45, bytes: 2.1e6 }, categories: cats(41) },
          { realm: "Ashenvale", name: "Velyra", totals: { files: 30, bytes: 1.4e6 }, categories: cats(26) },
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
  const who = (...names: string[]) => names.map((name) => ({ account: "ACCOUNT1", realm: "Ashenvale", name }));
  const session = (id: number, startMins: number, endMins: number | null, characters = who(), crashed = false) => ({
    id,
    flavor: "_classic_beta_",
    started_at: iso(startMins),
    ended_at: endMins == null ? null : iso(endMins),
    characters,
    crashed,
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
  };

  // Scenario state that changes as you click through.
  let refused = false;
  let recoveryRefused = false;
  let resolved = false;

  const handlers: Record<string, Handler> = {
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
    // In backups-corrupt only the newest snapshot (S1) is damaged.
    backup_verify: ({ id }): VerifyReport => {
      const bad = s === "backups-corrupt" && id === "S1";
      return {
        snapshot_id: id as string,
        files: 1912,
        corrupt: bad ? damaged.files : [],
        missing: bad ? damaged.missing : [],
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
          ["Velyra", 60 * 22],
          ["Coinpurse", 60 * 46],
          ["Brannic", 60 * 50],
          ["Fizzwick", 60 * 140],
          ["Sela", 60 * 200],
          ["Kaelor", 60 * 300],
        ] as const
      ).map(([name, mins]) => ({ account: "ACCOUNT1", realm: "Ashenvale", name, last_played: iso(mins) })),
    sessions_list: () =>
      s === "no-sessions"
        ? []
        : [
            ...(running ? [session(9, 102, null)] : []),
            session(8, 60 * 24 + 100, 60 * 24 - 92, who("Thrandor")),
            session(7, 60 * 29, 60 * 29 - 22, who("Coinpurse")),
            session(6, 60 * 47, 60 * 47 - 125, who("Velyra")),
          ],
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
