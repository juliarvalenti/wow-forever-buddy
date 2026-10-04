# Spec: core file-system layer

Status: **PM-approved, awaiting Julia's sign-off** · Author: @coder · 2026-10-04 (rev 3: PM review notes + alignment with round-1/2 mocks)

This covers the Rust layer under every feature that touches the game folder: finding the install, knowing when WoW is running, reading and writing files safely, backups and restore, app config, the local database, and secrets. Later features (companion-addon ingest, characters, AH, addon/macro management) get all their file access through this layer.

**Ground rules.** The core never touches the network. The frontend never touches the file system directly; every file access goes through Tauri commands in Rust. Rust enforces safety; the UI only reflects it.

---

## 1. Game install detection

### Assumed layout (to be confirmed on Julia's machine, see §10)

```
<root>/                         e.g. C:\Program Files (x86)\World of Warcraft
  .build.info                   pipe-separated table; has a Product column per installed flavor
  _retail_/  _classic_/  _classic_era_/  _<forever?>_/     one folder per flavor
    Wow*.exe
    Interface/AddOns/<Addon>/<Addon>.toc
    WTF/
      Config.wtf
      Account/<ACCOUNT>/
        SavedVariables/<Addon>.lua (+ .lua.bak)         account-wide addon data
        bindings-cache.wtf  config-cache.wtf  macros-cache.txt
        <Realm>/<Character>/
          SavedVariables/<Addon>.lua (+ .lua.bak)       per-character addon data
          AddOns.txt  layout-local.txt  macros-cache.txt  bindings-cache.wtf  chat-cache.txt
    Cache/  Logs/  Screenshots/                          never backed up
```

### Detection order (first valid result wins; all candidates are returned to the UI)

1. **Saved path** from settings (re-validated on every start).
2. **Registry** (Windows, via the `winreg` crate):
   - `HKLM\SOFTWARE\WOW6432Node\Blizzard Entertainment\World of Warcraft` → `InstallPath`. It often points at a flavor folder, so normalize up to the root.
   - `HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\World of Warcraft` → `InstallLocation`.
3. **Common paths:** `%ProgramFiles(x86)%\World of Warcraft`, `%ProgramFiles%\World of Warcraft`, and `X:\World of Warcraft` and `X:\Games\World of Warcraft` on each fixed drive. On macOS: `/Applications/World of Warcraft`.
4. **Manual picker** (`tauri-plugin-dialog`, folder mode). The user may pick the root, a flavor folder, or a `WTF` folder. We normalize upward to the root.

We skip parsing Battle.net's `product.db` (protobuf, undocumented) for v0.1. Registry, common paths and the picker cover nearly everyone. We can add it later if needed.

### Validation and flavors

- A **root** is valid if it contains `.build.info` or at least one `_*_` folder that passes the flavor check.
- A **flavor** is valid if it contains a `Wow*.exe` (Windows) or `World of Warcraft*.app` (macOS) **or** a `WTF/` folder. A WTF folder without an exe still counts, so backups work from a copied folder.
- **Flavor discovery:** enumerate `_*_` dirs. Read product codes from `.build.info` (`Product` column, e.g. `wow`, `wow_classic`, `wow_classic_era`) for display labels.
- **Model:**
  - `Install { root, flavors: Vec<Flavor> }`
  - `Flavor { id: "_classic_", label, dir, exe: Option<PathBuf>, version: Option<String>, has_wtf, accounts: Vec<String> }`
  - `version` comes from the `.build.info` `Version` column, e.g. "1.15.4". The dashboard shows it under "Client detected".
- The user picks one **active flavor**, which is saved in settings. If only one flavor has a WTF folder, it is picked automatically.
- **Multiple roots** (e.g. a PTR copy on another drive) show up as separate candidates. For v0.1 only one is active at a time.

---

## 2. Game-running detection

- **Polling with `sysinfo`** every **2 s**. We refresh only process names and exe paths, which is cheap. WMI process events would be more "correct" but much more complex, and a 2 s delay doesn't matter here.
- **Match rule:** a process whose **exe path lives under the active install root** (case-insensitive). If the path can't be read (access denied), fall back to the file name: `Wow.exe`, `WowClassic.exe`, `WowB.exe`, `WowT.exe`, `Wow-64.exe`, plus whatever Forever's exe turns out to be. The name list is a constant plus a hidden setting.
- **State:** `GameStatus { running: bool, pids: Vec<u32>, since: Option<Timestamp> }`, held in `AppState` behind a `watch` channel.
- **UI notification:** emits the event `game://status-changed` on each transition. The frontend also calls `game_status()` on mount to get the initial state.
- **Exit hook:** a running→stopped transition triggers the *game-exit backup* (§5) **after** the WTF tree has been stable for 5 s. WoW writes SavedVariables while shutting down.
- **Trait seam for tests:** `trait ProcessProbe { fn wow_pids(&self, root: &Path) -> Vec<u32>; }` with `SysinfoProbe` and `FakeProbe`.

---

## 3. Safe read path

All reads of game files go through `fsx::read`. No other module opens game files.

| Rule | Implementation |
|---|---|
| Never block WoW's writes | `OpenOptions::new().read(true)` plus explicit `share_mode(FILE_SHARE_READ \| FILE_SHARE_WRITE \| FILE_SHARE_DELETE)` on Windows. Rust's std already defaults to this, but we set it explicitly and add a test so a refactor can't regress it. |
| Never hold handles | `safe_read(path) -> Result<Bytes>`: open, `read_to_end`, drop. Nothing streams from a game file. That includes zip/hash code, which reads the bytes `safe_read` returns. |
| Detect torn reads | `stat` → read → `stat`. If size or mtime changed, retry up to 3× with a 250 ms backoff, then return `AppError::Unstable` ("try later"). |
| Debounced watching | `notify` + `notify-debouncer-full`, watching `WTF/` recursively. A path is "settled" only when its size and mtime have been unchanged for **2 s**. Only then do we emit `wtf://changed { paths }`. If the notify backend fails (network drives etc.), fall back to polling every 30 s. |
| Bad parse means retry, never act | Consumers treat `Parse`/`Unstable` errors as "skip this cycle". Nothing destructive ever depends on a parse result. |
| Never execute Lua | SavedVariables are parsed as data by our own parser (below). We never use `mlua`/`rlua`. |

### SavedVariables parser (`sv` module)

**Decision: a hand-written recursive-descent parser for the SavedVariables subset of Lua.** WoW writes a very narrow grammar: `Name = <expr>` statements, where expressions are table constructors, strings, numbers, booleans and `nil`.

- **Supports:** `--` comments and `--[[ ]]` block comments; short strings with all Lua 5.1 escapes; long strings `[==[ ]==]`; decimal, hex and exponent numbers, plus `inf`/`nan` spellings some addons emit (`1/0`, `-nan(ind)`); `[k]=v`, `name=v` and positional fields; `,` and `;` separators.
- **Output:**
  - `LuaValue = Nil | Bool | Int(i64) | Num(f64) | Str(Vec<u8>) | Table(LuaTable)`
  - `LuaTable { array: Vec<LuaValue>, hash: Vec<(LuaValue, LuaValue)> }`, which keeps order so output is deterministic.
  - Strings stay as **bytes**: WoW does not guarantee UTF-8. We decode lossily only at display time.
- Anything outside the subset (function calls, operators, identifiers as values) → `Parse` error with line and column.
- **Why not `full_moon`:** it's a full Lua parser that keeps all whitespace and comments. That's roughly 10× slower and much heavier in memory on large SavedVariables (Auctionator/TSM DBs reach 50–100 MB). We use it **in tests only**, as a differential oracle on fixtures.
- **Target:** at least 100 MB/s, and memory no more than about 3× file size.
- The `sv` module is **not needed for v0.1 backups**. Its ticket can run in parallel and becomes a v0.2 prerequisite.

---

## 4. Safe write path

All writes to game files go through `fsx::write`, and the type system enforces the rules:

```rust
// The only way to get a guard: checks the game isn't running, snapshots the files, records an audit row.
let guard: MutationGuard = gate.begin_mutation(op_name, &[rel_path_a, rel_path_b])?;
fsx::write::atomic_write(&guard, rel_path_a, bytes)?;   // panics in debug / errors in release if path ∉ guard
fsx::write::remove(&guard, rel_path_b)?;
guard.commit()?;                                         // Drop without commit → logged as aborted
```

| Rule | Implementation |
|---|---|
| **No writes while WoW runs** | `begin_mutation` re-checks `ProcessProbe` synchronously (not the cached status) and returns `AppError::GameRunning`. There is **no setting that turns this off**. The "block writes while WoW is running" toggle in Settings is display-only and always on. `atomic_write` re-checks just before each rename. |
| **Snapshot before write** | `begin_mutation` takes a **partial snapshot** (`trigger: pre_write`) of exactly the listed paths, including "absent" markers for paths that don't exist yet, so undo can delete them. If the snapshot fails, the write doesn't happen. |
| **Atomic writes** | Write to `<dir>/.<name>.wfb-tmp-<rand>` in the **same directory** → `sync_all()` → rename over the target. On Windows, rename uses `MoveFileExW(REPLACE_EXISTING \| WRITE_THROUGH)` (`tempfile::NamedTempFile::persist`). A sharing violation (antivirus, indexer) is retried 5× with backoff 100 ms → 1.6 s. The temp file is always cleaned up on failure, and leftover `*.wfb-tmp-*` files are swept on startup. |
| **Minimal scope** | Paths are `RelPath`, a newtype validated against `..`, absolute paths, drive prefixes and ADS (`:`). They're joined to the flavor dir and checked with `dunce::canonicalize` to still be inside it (case-insensitive on Windows). No API offers "rewrite the tree". |
| **Audit** | Every mutation writes a `write_audit` row to SQLite: op, paths, pre-write snapshot id, result. This powers an "Undo last change" UI later. |

Known race: WoW could start in the milliseconds between our check and the rename. We accept that. The window is tiny, and the pre-write snapshot makes it recoverable.

---

## 5. Backups

### Format: content-addressed store (decision)

```
<backups_dir>/
  objects/ab/cdef0123….zst      blob = zstd(level 3) of file bytes, named by blake3 of the *uncompressed* bytes
  snapshots/<ULID>.json         manifest (source of truth)
```

**Manifest:**
```json
{ "version": 1, "id": "01J9…", "created_at": "…", "trigger": "manual|app_start|game_exit|scheduled|pre_write|pre_restore",
  "label": null, "pinned": false, "scope": "full|partial", "flavor": "_classic_", "game_running": false,
  "include_addons": false,
  "files": [ { "path": "WTF/Account/X/SavedVariables/Foo.lua", "size": 1234, "mtime": "…", "blake3": "…" } ],
  "absent": [ "WTF/…/new-file.lua" ] }
```

| Option | Pros | Cons |
|---|---|---|
| **Content-addressed (chosen)** | Dedup across snapshots. A WTF tree barely changes between snapshots, so auto and pre-write snapshots are nearly free and per-file restore is trivial. Integrity check comes from the hash. | Not openable by hand. Needs GC on prune. |
| Zip per snapshot | Portable, one file | No dedup (100 × 200 MB adds up fast), and per-file restore means seeking inside the zip |
| Folder copy | Dead simple, browsable | No dedup, slow to create, lots of small files |

We cover portability with **"Export snapshot as .zip"**, which builds a plain zip from the manifest.

- **Hash cache:** SQLite `file_hash_cache(path, size, mtime_ns, blake3)`. Unchanged files aren't re-read, so a no-change snapshot of a large WTF folder takes milliseconds.
- **Skip identical:** auto triggers skip creating a snapshot when the file set and hashes match the latest full snapshot. Manual snapshots are always created.
- **Writes:** blobs are written with the atomic-write helper (to app storage, not the game folder, so no guard is needed). Existing blobs are skipped. The manifest is written last, so a crash leaves only orphan blobs, which GC removes.
- **Snapshots while WoW is running:** only **manual** snapshots are taken mid-session, and they're flagged `game_running: true` in the UI ("taken mid-session"). Automatic triggers wait for the game to exit, so they never capture a half-written SavedVariables file from a `/reload`.

### What's included

- Always: `WTF/**`.
- Optional, off by default (setting): `Interface/AddOns/**`. It's large and mostly re-downloadable, but useful for hand-edited addons. Since the store dedups, turning it on mostly costs the first snapshot.
- Never: `Cache/`, `Logs/`, `Screenshots/`, `Errors/`, `*.wfb-tmp-*`.

### Triggers

| Trigger | When | Default |
|---|---|---|
| manual | "Back up now" | always available |
| app_start | on launch, if more than 6 h since the last full snapshot. If WoW is running, it waits for the game_exit backup instead | on |
| game_exit | running → stopped, after WTF has been stable for 5 s | on |
| scheduled | every N hours while the app is open (tokio interval). **Skipped while WoW is running**; game_exit covers the session | 24 h |
| pre_write | automatically, inside `begin_mutation` (partial) | always |
| pre_restore | automatically, before every restore (partial: the paths restore will touch) | always |

All backup and restore jobs run on one **serialized job queue** (one at a time), with progress reported via `backup://progress`.

### Retention (pure function, unit-tested)

- **manual** and **pinned**: kept until the user deletes them.
- **app_start / game_exit / scheduled**: keep everything from the last 48 h, then the newest snapshot per day for 14 days, then the newest per week for 8 weeks. Everything else is pruned.
- **pre_write / pre_restore**: keep 30 days, and always at least the last 20.
- **Storage budget** (the mock's "1.08 GB of 5 GB" meter): a soft cap, default **5 GB** of *actual* store size (after dedup and compression, not the sum of snapshot sizes). Over budget, the oldest auto snapshots, then the oldest safety snapshots, are pruned beyond the time rules above, but never the newest 3 of each. Manual and pinned snapshots are never auto-pruned. If they alone exceed the budget, the UI shows a warning instead.
- Pruning runs after each new snapshot. Then **GC** (mark blobs referenced by any manifest, sweep the rest) runs, throttled to once an hour.
- `backup_storage()` returns `{ used_bytes, budget_bytes, retention_summary }`. `retention_summary` is a short string generated from the policy constants (e.g. "Auto: 48 h, then daily for 2 weeks, weekly for 2 months · manual kept forever"). The UI shows it as is, so the copy can't drift from the policy. The mock's "Auto-prune after 30 days" text gets replaced by this string.
- Settings shows the total store size and a "Prune now" button.
- **UI type labels:** `manual` → Manual, `app_start`/`game_exit`/`scheduled` → Auto, `pre_write`/`pre_restore` → **Safety**. Safety snapshots get a generated label describing the operation, e.g. "Before restoring Velyra" or "Before macro edit".

### Restore

- **Scope = a selection.** The mock's restore panel multi-selects characters and, within a character, categories, so a restore takes a `RestoreSelection { items: Vec<ScopeItem> }`. The backend resolves it to a deduplicated set of paths:
  - `Everything`: the whole snapshot.
  - `Account { account, categories: Option<Vec<Category>> }`
  - `Character { account, realm, character, categories: Option<Vec<Category>> }`. `None` means the whole character folder.
  - `AddonData { addon, target: Account(account) | Character(…) | Everywhere }`, which covers `SavedVariables/<addon>.lua` and `.lua.bak` (the mock's "Addons" tab).
  - `Paths(Vec<RelPath>)`, which backs the pre-write undo.
- **Categories** (fixed mapping of files to categories in `install/layout.rs`, matching the mock):
  - `BindingsMacros`: `bindings-cache.wtf`, `macros-cache.txt`
  - `AddonSettings`: `SavedVariables/**`, `AddOns.txt`
  - `ChatLayout`: `chat-cache.txt`, `layout-local.txt`
  - `Other`: anything else in the folder, e.g. `config-cache.wtf`
- `backup_get(id)` returns the manifest as a tree with **aggregate sizes and file counts** per account, realm, character and category, plus per-addon entries. That's what the panel shows as "Thrandor 2.1 MB", "Addon settings (41) 1.9 MB" and so on.
- **Modes:**
  - **overlay (default for every scope):** write the snapshot's files, leave extra files alone.
  - **mirror (explicit opt-in):** also remove current files under the scope that aren't in the snapshot.
  - **Why overlay everywhere:** files that exist now but not in the snapshot are almost always newer, legitimate data, such as SavedVariables for an addon installed since then. Deleting them is the one restore action that can surprise you. Leftover extra files do no harm, because WoW ignores SavedVariables for addons that aren't loaded. Mirror is there for "put it back exactly" (e.g. undoing a broken UI setup). Even then, the deleted files go into the pre-restore snapshot, so it can be undone.
  - **Preview must show deletions prominently:** the `RestorePlan` lists `to_delete` separately, and the confirm dialog shows that list with its own count.
- **Flow:**
  1. `restore_preview` returns a plan: files to write (grouped per folder, so the dialog can show "…\Thrandor\SavedVariables\ (41 files)"), files to delete, unchanged count, bytes, and a one-line human summary ("keybindings, macros and 41 addon settings").
  2. The user confirms. While WoW is running, the dialog shows "Waiting for WoW to close…" with Restore disabled. It enables on `game://status-changed` → stopped, but **never runs on its own**: the user still clicks. The restore job queues behind the game_exit backup, so it always runs after WoW's exit writes have settled and been captured.
  3. `begin_mutation(all touched paths)`: this blocks if WoW is running and takes the pre-restore snapshot.
  4. Write a **restore journal** (`restore-journal.json` in app data).
  5. Verify each blob's blake3 → `atomic_write`, and remove files for mirror mode.
  6. Delete the journal and emit `restore://completed`.
- **Crash mid-restore:** on startup, a leftover journal means the restore was interrupted. The app offers to roll back to the pre-restore snapshot named in the journal, or to finish the restore.
- **Corrupt blob** (hash mismatch): abort before any write and report the files involved. `backup_verify(id)` runs the same check on demand.

---

## 6. App configuration and local database

### Locations (Tauri path resolver)

| What | Where | Why |
|---|---|---|
| `settings.json` | `app_config_dir()` | small, human-readable |
| `buddy.db` (SQLite) | `app_local_data_dir()` | `%LOCALAPPDATA%`, so it isn't synced by roaming profiles |
| `backups/` | `app_local_data_dir()/backups`, can be moved in settings (e.g. to another drive) | large |
| logs | `app_log_dir()` via `tauri-plugin-log` | |

### Settings

- A Rust struct with serde `#[serde(default)]` on every field, so missing keys get defaults and unknown keys are kept in a `#[serde(flatten)] extra` map so a downgrade doesn't wipe them.
- Saved with the atomic-write helper.

```jsonc
{ "schema_version": 1,
  "install": { "root": "C:\\…\\World of Warcraft", "flavor": "_classic_" },   // or null
  "backup": { "location": null, "include_addons": false,
              "on_app_start": true, "on_game_exit": true, "schedule_hours": 24 },
  "process_names_extra": [],
  "integrations": { "curseforge": { "enabled": false }, "wago": { "enabled": false },
                    "wago_io": { "enabled": false }, "github": { "enabled": false }, "battlenet": { "enabled": false } },
  "ui": {} }
```

- **Migrations:** `fn migrate(value: serde_json::Value) -> Value` runs a chain `v1→v2→…` keyed on `schema_version` before typed deserialization. Before migrating, the old file is copied to `settings.v<N>.bak.json`.
- The retention policy is a constant in v0.1. It's exposed in the UI later if wanted, and listed under open questions.

### SQLite

- **Crate:** `rusqlite` with the `bundled` feature, so no system SQLite is needed on Windows, plus `rusqlite_migration` for embedded, ordered SQL migrations.
- One connection behind a `Mutex`, used from `spawn_blocking`. WAL mode, `foreign_keys=ON`.
- **Alternative:** `sqlx` (async, compile-time-checked queries) is heavier and needs a DB at build time for its macros. That's not worth it at this size.
- **v1 schema:** `snapshots` (index of manifests: id, created_at, trigger, label, pinned, scope, flavor, file_count, total_bytes, new_bytes, **char_count, addon_count**, game_running), `file_hash_cache`, `write_audit`, `meta`.
  - `char_count` and `addon_count` back the mock's "7 chars · 52 addons" column. They're derived from manifest paths: character folders, and distinct `SavedVariables/<addon>.lua` names, or AddOns folders when those are included. They're computed at snapshot time so the list never opens manifests.
- Manifests on disk stay the source of truth for backups. `backup_reindex` rebuilds `snapshots` from them, so losing the DB never loses backups.
- Later features (characters, gold history, AH) add their own migrations to the same DB.

---

## 7. Secrets

- **Crate:** `keyring` v3 (features `windows-native`, `apple-native`). It uses Windows Credential Manager, and Keychain for dev on macOS.
- **Entry:** service `com.juliarvalenti.wowforeverbuddy`, user = integration id (`curseforge`, `wago`, `wago_io`, `github`, `battlenet_client_id`, `battlenet_client_secret`).
- **Interface** (`secrets.rs`):
  ```rust
  pub trait SecretStore { fn set(&self, id: IntegrationId, v: &str) -> Result<()>;
                          fn get(&self, id: IntegrationId) -> Result<Option<SecretString>>;  // Rust-only
                          fn delete(&self, id: IntegrationId) -> Result<()>;
                          fn is_set(&self, id: IntegrationId) -> Result<bool>; }
  ```
- **Values never cross to the frontend.** Commands expose only `set`, `delete` and status. "Test key" is a future Rust-side command run by each integration.
- Secrets are never logged, never written to settings or the DB, and can't end up in backups (they're not in the game folder).
- `IntegrationId` is a closed enum, so the frontend can't write arbitrary keychain entries.
- Tests use an in-memory `SecretStore`.

---

## 8. Rust/TS boundary

### Module layout (`src-tauri/src/`)

```
lib.rs            builder, plugin init, state, command registration, startup sequence
error.rs          AppError (thiserror) + Serialize
state.rs          AppState { settings, db, install, game_status, jobs, secrets }
commands/         thin #[tauri::command] wrappers, one file per area
  install.rs game.rs wtf.rs backup.rs settings.rs secrets.rs
install/          detect.rs (sources), validate.rs, layout.rs (Install/Flavor/WTF tree model)
game/             process.rs (ProcessProbe, poll loop), gate.rs (MutationGuard)
fsx/              read.rs (safe_read), write.rs (atomic_write/remove), relpath.rs, watch.rs (debounced watcher)
sv/               parse.rs, value.rs        (SavedVariables, data-only)
backup/           store.rs (blobs, GC), snapshot.rs, manifest.rs, restore.rs, journal.rs, retention.rs, export.rs
config/           settings.rs, migrate.rs, paths.rs
db/               mod.rs, migrations/*.sql
secrets.rs
jobs.rs           serialized background job queue + progress events
```

### Commands

| Command | Returns |
|---|---|
| `install_detect()` | `Vec<InstallCandidate>` |
| `install_get()` | `Option<Install>` (with active flavor) |
| `install_set(path, flavor?)` | `Install`; validates, normalizes, saves |
| `game_status()` | `GameStatus` |
| `wtf_tree()` | `WtfTree` (accounts → realms → characters → addons with SavedVariables), used by restore pickers |
| `backup_create(label?)` | `JobId` (result arrives via event) |
| `backup_list()` | `Vec<SnapshotSummary>` (incl. type label, char/addon counts, size) |
| `backup_storage()` | `{ used_bytes, budget_bytes, retention_summary }` |
| `backup_get(id)` | `SnapshotDetail` (manifest as a tree) |
| `backup_set_pinned(id, pinned)` / `backup_set_label(id, label)` | `()` |
| `backup_delete(id)` | `()` |
| `backup_restore_preview(id, selection, mode?)` | `RestorePlan` |
| `backup_restore(id, selection, mode?)` | `JobId` |
| `backup_verify(id)` | `VerifyReport` |
| `backup_export_zip(id, dest)` | `JobId` (`dest` comes from the save dialog) |
| `backup_prune_now()` | `PruneReport` |
| `restore_journal_status()` / `restore_journal_resolve(action)` | for the crash-recovery prompt |
| `settings_get()` / `settings_update(patch)` | `Settings` |
| `secrets_status()` | `Vec<{ id, is_set }>` |
| `secrets_set(id, value)` / `secrets_delete(id)` | `()` |
| `app_open_folder(which)` | reveals backups/logs/game folder via the opener plugin (fixed set of targets, not arbitrary paths) |

### Events

`game://status-changed`, `install://changed`, `wtf://changed {paths}`, `backup://progress {job, phase, done, total}`, `backup://created {summary}`, `backup://pruned`, `restore://completed {report}`, `job://failed {job, error}`.

### Error model

```rust
#[derive(thiserror::Error, Debug, Serialize, specta::Type)]
#[serde(tag = "kind", content = "detail")]
pub enum AppError { GameRunning, NoInstall, InvalidInstall(String), PathEscape(String),
                    NotFound(String), Io(String), Unstable(String), Parse { file: String, line: u32, col: u32, msg: String },
                    BackupCorrupt { files: Vec<String> }, Secret(String), Db(String), Busy /* job queue */ }
```

The frontend switches on `kind`. For example, `GameRunning` shows the "Close WoW first" dialog.

### Typed bindings

- `specta` + `tauri-specta` generate `src/lib/bindings.ts`, with command functions, payload types and event types, on debug builds.
- **Risk:** tauri-specta v2 is still an RC. **Fallback:** `ts-rs` for types plus a hand-written `invoke` wrapper.

### Capabilities and security config

- `capabilities/default.json`: `core:default`, `dialog:allow-open`, `dialog:allow-save`, `opener:allow-open-path`, scoped to the app data/log dirs and the install root (scope set at runtime). **No** `fs:*` or `shell:*` plugins.
- Set a real CSP in `tauri.conf.json` (it's currently `null`): `default-src 'self'; img-src 'self' asset: data:; style-src 'self' 'unsafe-inline'`.
- Commands accept `RelPath` or IDs and never absolute game paths, except `install_set` (picker output, validated) and the export `dest` (save-dialog output).

### Startup sequence

1. Load and migrate settings.
2. Open and migrate the DB.
3. Sweep stale `*.wfb-tmp-*` files.
4. Resolve the install (saved path → detect).
5. Start the process poller.
6. Start the WTF watcher.
7. Check for a restore journal.
8. Queue the app_start backup.
9. Arm the schedule.

The window shows right away. Steps 4–9 run in the background and report through events.

---

## 9. Testing

- **Fixture tree:** `src-tauri/tests/fixtures/wow/` holds a fake root with `.build.info`, two flavors (one with an exe stub, one WTF-only), 2 accounts × 2 realms × 3 characters, account- and character-level SavedVariables, `.lua.bak` files, and `Cache/` and `Logs/` (to confirm exclusion). It also has parser fixtures:
  - real-world-shaped SavedVariables (Details, WeakAuras, Auctionator-like)
  - long strings and escapes
  - a truncated file (simulated torn read)
  - non-UTF-8 bytes
  - a generated 50 MB file for the perf test (`#[ignore]` by default)
- **`.gitattributes`:** `src-tauri/tests/fixtures/** -text`, so git never rewrites CRLF and byte-exact restore tests stay honest.
- **Seams:** `ProcessProbe`, `SecretStore`, `Clock` (retention and schedule), and the install sources (registry source behind a trait). Integration tests build `AppCore` (everything except Tauri) against a `tempfile::TempDir` copy of the fixture.
- **Key tests:**
  - Snapshot → mutate → restore (each scope and mode) → byte-equal tree.
  - Pre-write snapshot exists and undo works.
  - `begin_mutation` with `FakeProbe(running)` returns `GameRunning`, and no file changes.
  - Retention table tests: given timestamps, which snapshots survive.
  - GC never deletes a referenced blob.
  - A restore journal left behind is detected and rolled back.
  - `RelPath` rejects `..`, absolute paths, `C:`, `\\?\` and `file:stream`.
  - Parser vs `full_moon` differential on all fixtures, plus a `proptest` round-trip (generate a `LuaValue` → serialize → parse → equal).
- **Windows-only (`#[cfg(windows)]`):**
  - While `safe_read` is running in a loop, another handle can open the same file for write/delete (proves the share flags).
  - `atomic_write` succeeds while a reader holds the target open with full sharing, and retries then fails cleanly while a handle holds it exclusively.
  - Long path (>260 chars) round-trip.
- **CI:** add `.github/workflows/ci.yml` on PRs, with a `windows-latest` + `macos-latest` matrix: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `npm ci && npm run build` (tsc). Cache with `swatinem/rust-cache`. The existing `release.yml` stays as is. On Windows, watch out for slower builds and antivirus scanning `target/`; the `ProcessProbe` fake keeps tests from depending on real processes.
- **Manual QA checklist (Julia's machine):** detection finds the Forever install, the status indicator flips when WoW launches and exits, a game-exit backup appears, and restoring a character while WoW is running is blocked.

---

## 10. Open questions

1. **Forever's folder layout:** what's the flavor folder name (`_forever_`?), the exe name, and the `.build.info` product code? Does it share a root with retail/classic or install separately? *Ask Julia for a screenshot or `dir` of the install root and the flavor folder.* This blocks finalizing detection and the process name list, but not building them, since both are data-driven.
2. **Retail vs classic client:** this doesn't affect this layer, but it does affect the companion addon (v0.2).
3. ~~Include `Interface/AddOns` in backups by default?~~ **Resolved (PM):** off, with a toggle.
4. ~~Retention defaults user-editable in v0.1?~~ **Resolved (PM):** constants for v0.1.
5. ~~Backup location default?~~ **Resolved (PM):** `%LOCALAPPDATA%`, movable in settings.
6. ~~tauri-specta RC?~~ **Resolved (PM):** yes, with ts-rs as the fallback.
7. **Mock data this layer can't provide in v0.1** (for @designer/@project-mgmt):
   - "WoW is running · **Thrandor**": process detection only knows the game is running, not which character is logged in. That needs the companion addon (v0.2). v0.1 shows "WoW is running" plus the session duration.
   - Class colors, level and ilvl on character rows (including the Backups restore list): the WTF folder only gives account, realm and character names. v0.1 shows names in a neutral color, and class data arrives with the addon.
   - The mocks assume Forever lives in `_classic_` with version 1.15.4. That's a placeholder until Julia's `dir` listing (Q1).

---

## 11. Proposed v0.1 tickets (ordered)

Each ticket is about 0.5–2 days, with tests, and passes CI on Windows and macOS.

1. **Rust skeleton + CI:** module layout, `AppError`, `AppState`/`AppCore`, tauri-specta bindings pipeline, `ci.yml` matrix, `.gitattributes`.
2. **Config + paths:** settings v1 schema, defaults, migrate chain, atomic save, `settings_get/update`.
3. **SQLite:** rusqlite bundled, migrations, v1 schema, `spawn_blocking` access helper.
4. **Safe IO:** `RelPath`, `safe_read` (share flags, torn-read retry), `atomic_write` (+ retry, tmp sweep), fixture tree, Windows share-mode tests.
5. **Install detection:** registry, common-path and manual sources; validation; flavors from `.build.info`; `install_*` commands; dialog plugin.
6. **Game watcher + write gate:** `ProcessProbe`/sysinfo poller, `game://status-changed`, `MutationGuard` with the synchronous running check, `write_audit`.
7. **Backup store:** CAS blobs, manifests, hash cache, `backup_create/list/get/delete/pin/label`, job queue + progress.
8. **Retention + GC + triggers:** retention fn, prune + GC, debounced WTF watcher, app_start / game_exit / scheduled triggers, skip-identical.
9. **Restore:** scopes, overlay/mirror, preview, pre-restore snapshot, journal + crash recovery, `backup_verify`.
10. **Secrets interface:** keyring store, `IntegrationId`, `secrets_*` commands.
11. **Frontend wiring:** generated bindings in `src/lib`, `useGameStatus`/`useInstall`/`useBackups`/`useRestore` hooks, and a Backups screen following the **round-1 mock structure** (list, back up now, restore flow with the "close WoW first" state and the deletions list). Use **plain, unstyled components** with all logic in the hooks. The visual direction is being redone, so don't polish visuals yet; the restyle should only touch markup and CSS.
12. **Export to zip** (nice-to-have for v0.1).
13. **SavedVariables parser** (parallel track, v0.2 prerequisite): parser, differential + proptest + perf tests.

Dependencies: 1 → {2, 3, 4}; 4 → {5, 6}; {3, 6} → 7 → {8, 9} → 11; 12 after 7. Tickets 10 and 13 can start any time after 1, so two coders can split the work: coder A takes 2–9, coder B takes 10, 13, then 11.
