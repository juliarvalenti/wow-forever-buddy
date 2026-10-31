# Addon test harness

Runs the ForeverBuddy addon (`src-tauri/resources/addon/ForeverBuddy/`) outside the game, against a fake client, and writes what it saves as fixtures for the Rust ingest tests (`src-tauri/tests/fixtures/addon/`). See spec v0.2-addon §7.

```sh
lua5.1 tools/addon-test/run.lua           # run every scenario, rewrite the fixtures
lua5.1 tools/addon-test/run.lua --check   # what CI runs: fail if a fixture is stale
```

On macOS, `brew install luajit` and run it with `luajit` instead (Homebrew has no Lua 5.1; LuaJIT is the same language).

- `wow.lua`: the fake client. It covers the APIs the addon calls, frames and events, the clock, `C_Timer`, secret values, and SavedVariables kept as text.
  - It also models a small game world (money, XP, zone, bags, gear, repair cost). Helpers such as `loot`, `sell`, `die`, `turnIn` and `bank` change that world and fire the events the client would.
  - Any API can be made secret, throwing or missing. `RegisterEvent` can refuse named events, and chosen events can deliver secret arguments.
  - The client's settings and Edit Mode (U1d):
    - `Settings`'s vertical layout: categories, proxy settings, checkbox rows with their parent and shown predicates, and section headers.
    - Edit Mode through `EventRegistry`, opened and closed with `client.editMode(true/false)`.
    - `opts.canvasOnly` makes an older client that only has the canvas page, and `opts.noEditMode` a client without Edit Mode.
- `serialize.lua`: writes a table the way the client writes SavedVariables, with keys sorted.
- `run.lua`: the scenarios. Each one checks the file the addon wrote, including `_meta.counts` against its own recount. After every scenario, each client's file must have no caught errors (`_meta.errors` and the `*_errors` counters). Scenarios that return a file save it as `<scenario>.lua`.

- `forever.lua`: how WoW: Forever answers each API the addon calls, by phase (H2). The phases are `loading` (PLAYER_LOGIN until the bags load), `live`, and `torn` (the logout teardown). Each row says what the API returns instead of its live answer in each phase (`0`, `"nil"`, `"{}"`, or nothing for "as live"), and where we know that from (`recording`, `report` with the ticket, or `assumed`). Outside `live`, every mocked API answers from its row, so the phase behaviour lives in this one table.

A new API the addon calls needs a mock in `wow.lua`'s `api` table and a row in `forever.lua`. The run fails without either, and its last lines print the coverage:
- how many APIs are mocked and have a row;
- how many of those rows come from a report or recording, and how many are assumed;
- how many are confirmed by a real recording;
- which APIs and handled events no scenario reaches.

When the addon's output changes, run the harness and commit the fixtures with the change.

**The bridge contract (H5):** the `contract` scenario loads the slot files in `src-tauri/tests/fixtures/bridge/` as the game would and checks the addon uses each one. Those files aren't written by hand: `src-tauri/src/contract.rs` ingests this harness's `lists.lua`, adds a list, a plan, a cleanup mark and a login note, and builds every slot the way the app's `send_to_game` does. It then runs this harness (Lua 5.1, through mlua) inside `cargo test`, so CI checks the chain on Windows and macOS, and fails if the slots changed without the fixtures being committed. If you change a slot's format, run `cargo test contract` and commit the fixtures.

**The settings schema (P4a):** the addon's settings are declared once, in `ForeverBuddy/Settings.lua`: id, module, section, type, default, label, help, parent and acts. The window's Settings tab and Esc › Options read that list; what each option does is in `ForeverBuddy.lua`, by id, and the addon refuses to load if a declared option does nothing.
- `settings_schema` checks every declaration and generates `src-tauri/resources/settings-schema.json` for the app. Under `--check`, a stale file fails, so commit it with the change.
- `settings_matrix` runs every declared value. Each option is set on and then off from the Options page, with a session played through and the file it writes checked. The value must be kept at the next login. Then every option off, and every option on.
- A new option needs its declaration, its get and set, then a run of the harness and a commit of the schema.

## The Forever census (SIM5a)

Our fakes, this harness and WOWAPI, say what the client defines. The census records what the real one does: every global's name and type, one level into each table (`C_Item.GetItemInfo`), two under `Enum`, and each widget type's methods. It holds names and types only, never values, and skips any name containing the character's own name.

1. **In game,** with the probe addon (`tools/probe-addon/ForeverBuddyProbe`, version 4) installed: `/fbprobe census`. It loads the auction house UI first, so its frames are counted, then walks the globals a slice per frame and prints "census done". Then `/reload` to save it.
2. **Copy** `WTF/Account/<ACCOUNT>/SavedVariables/ForeverBuddyProbe.lua` to `tools/probe-addon/results/census.lua` and commit it.
3. **The report:** `luajit tools/addon-test/census.lua tools/probe-addon/results/census.lua` (add `--all` for every name). It lists:
   - the APIs ForeverBuddy calls that Forever doesn't have (each one is a bug);
   - the names and widget methods our fake gives the addon that Forever doesn't have, where a test could pass on a lie;
   - WOWAPI against Forever, both ways.

Once `results/census.lua` exists, the H2 fidelity check also fails the run if the addon calls anything the census doesn't list. The `census` scenario tests the lookups on a made-up census.

## A strict fake (SIM4)

The fake is as strict as the client where a mistake would otherwise pass quietly:

- **Widget methods:**
  - A widget method `wow.lua` doesn't model raises, naming the method.
  - The miss is also recorded on the client, so the scenario fails even when the addon's own `pcall` swallowed the error.
  - A new widget method the addon calls needs a line in `wow.lua`. If it's only layout, add it to the recorded no-ops.
- **Anchors:** `SetPoint` refuses a misspelled point or relative point. Every anchor is kept on `w.points`, one per point, the same way the client keeps them.
- **Fields:** an unset lower-case field is `nil`, as in the client.
- **Secret values** error on indexing, concatenation, arithmetic and ordering. Lua 5.1 can't make `==` or `#` error on a table, so those rules stay by inspection.

The `strict_fake` scenario proves each of these holds.

## From the addon bible (R3a)

Scenarios for rules in the addon bible (room memory `procedures/addon-bible/*`; "2.18" is rule 2.18):

- `forever_globals` (2.18): fails if `ForeverBuddy.lua` names a global Forever doesn't have (bare `GetItemInfo`, the old spec functions and others), so use the `C_` form. The list comes from EllesmereUI's CI guardrails.
- `unreadable_table` (2.7): a bag slot Forever hands back as a table holding secrets. `client.unreadable()` makes one: `canaccesstable` says false, and indexing it raises.
- `keyring` (2.19): bag -2 counts as carried.
- `forbidden_tooltip` (3.19): `client.hover(id, "tooltip" | "owner")` shows a forbidden tooltip, and writing to it fails the run.
- `eui_skin`: with EllesmereUI installed (`opts.globals` puts a fake one in), the window takes its look through `EllesmereUI.RegisterSkin`. `ButtonFrameTemplate` and `UIPanelScrollFrameTemplate` now give their `CloseButton` and `ScrollBar`.

## Chaos (H4)

`chaos_1` … `chaos_200` are seeded random sessions. Each seed picks a setup, plays about 30 random steps, logs out, and checks what must always hold.

- **Setup:** whether Forever's loading and teardown apply, and a few APIs made secret.
- **Login (H4b):** plain, or by hand in another order: money before `PLAYER_LOGIN`, the bags before `PLAYER_ENTERING_WORLD`, or the bags late with a `BAG_UPDATE_DELAYED` before they've loaded and two after.
- **Steps:**
  - time passing, loot, use, a vendor sale and money;
  - combat on or off, `/reload` and the logout countdown;
  - level-ups, XP, deaths and zone changes;
  - now and then a burst of 2,001 to 2,500 loots, past the event cap on its own (H4b).
- **Logins in a row (H4b):** one in ten seeds logs in 11 to 13 times, past the session cap. Each login logs out one of three ways (the countdown, the teardown before the countdown, or instantly), and every file written is checked.
- **What it checks:**
  - the file parses and `_meta.counts` adds up;
  - sessions and events stay within the addon's caps;
  - every plain gain is an item the session actually gave;
  - a snapshot is never hollow.
- **Errors:** the runner's after-each checks (escaped, unmodelled or caught errors) apply to each seed.

A failure names its seed and lists the steps. The same seed always plays the same steps.

- **More seeds:** `CHAOS_SEEDS=1000 luajit tools/addon-test/run.lua` runs that many.
- **One seed:** `CHAOS_ONLY=96 luajit tools/addon-test/run.lua` runs just that seed and prints its steps.

The checks do bite:
- Re-breaking the login-baseline fix (BUG-ADV) makes 85 of the 200 seeds fail with "a gain the session never gave".
- Raising the addon's session cap fails 27 seeds, and raising its event cap fails 41. Before H4b no seed reached either cap.

## Coverage (H3)

`lua5.1 tools/addon-test/run.lua --check --coverage` (what CI runs) also prints what the scenarios ran of `ForeverBuddy.lua`. On a PR, it's also on the job's summary page. `coverage.lua` collects it with a debug hook; it has no dependencies.

- **Lines:** lines run against an estimate of the code lines. Lua 5.1 can't list a chunk's executable lines, so blank lines, comments, closing keywords, bare `local` declarations and table-constructor entries don't count.
- **Event handlers and other functions never entered.** This list is exact, and it's the part to act on.
- **Unrun stretches of 3 or more code lines** inside functions that did run: branches, roughly.

When a handler or a branch that matters shows up there, add it to a scenario. `coverage_gaps` holds the ones that didn't fit anywhere else. Window drags and a button's hover tooltip are left out on purpose.

## Time budgets (H6)

Every call into the addon is counted in Lua VM instructions, with a debug hook. A call is an event's handler, a timer, a slash command or a click. Instructions, not milliseconds, so a busy CI runner can't make it flaky. The fake's own API functions don't count: the real client's are C. JIT is off while counting, since LuaJIT's compiled code doesn't call hooks.

- **Per handler:** at most `HANDLER_BUDGET` (150,000, about 1.5 ms in a plain Lua 5.1 interpreter like the game's).
- **At logout:** `PLAYER_CAMPING`, `PLAYER_LEAVING_WORLD` and `PLAYER_LOGOUT` get `LOGOUT_BUDGET` (220,000). They run once, while the player is leaving, and do the session's whole bookkeeping.
- **Per frame:** what a scenario runs inside `client.frame(fn)` counts as one frame, at most `FRAME_BUDGET` (400,000, about 4 ms of a 16 ms frame). The `bursts` scenario plays what the client delivers in one frame: a 500-row AH page with a list docked, 300 item loads, 20 quest log updates with a plan, a 50-letter mailbox, and full bags changing. It ends with a session at the event cap and the logout countdown, so every run meets the costliest logout, not only the chaos seeds that reach the cap.

Any scenario over budget fails. The run ends with the eight costliest handlers and the costliest frame, which also go on the CI job's summary page. The counts depend on the interpreter: Lua 5.1's run about 1.6 times LuaJIT's, and the budgets fit both. Under `--coverage` nothing is counted (coverage's hook is set), so CI runs the harness a second time without it.

## Layout snapshots

`window_layout` opens the Forever Buddy window on each tab with the test slots and writes its frame tree to `snapshots/window.txt`. Each line holds a widget's kind, name, anchors, size, text and whether it's hidden.

- **Without `--check`:** the file is rewritten.
- **With `--check`** (CI): a difference fails the run.

A change to the window's layout therefore shows up as a diff in the PR to review and commit. Snapshots live here, not in the Rust fixtures folder, because the ingest tests read that folder whole.

## Recordings (SIM1 b)

The fake client only knows what we told it. The real client's timing (what it answers at login, during the logout countdown, after the teardown) comes from **recordings**.

- **Recording:** in game, tick "Record my next session" in the Forever Buddy window's Settings tab (or type `/fb record`). The next login session is recorded into that character's `ForeverBuddy.lua` as `record`, then recording turns itself off.
- **What's in it:** numbers only. Events by name, their number and boolean arguments, and after each event a probe:
  - `b`: backpack slots
  - `m`: money, as the change since the first probe
  - `l`, `x`, `xm`: level and XP
  - `w`: worn items
  - `q`: quests done
  - `sv`: saves
  - `c`: combat
  - `bk`, `ml`: bank and mailbox open

  There are no names, chat, mail, zones or item names, and time is seconds since login. `ITEM_DATA_LOAD_RESULT`, which fires for every item the UI loads, is only counted (`counts`), not logged.
- **Replaying:** the `replay` scenario plays each recording back. The probes drive the fake client's phase (no backpack means the character isn't up: `loading` before the first live probe, `torn` after), and the events fire in order. For a real recording, each probe taken outside `live` is also checked against `forever.lua`: a row the recording contradicts fails the run, and one it agrees with counts as confirmed. Then it checks that:
  - the file has no hollow snapshot
  - logging in isn't a gain
  - the snapshot's money and saves are the last readable ones
  - there are no addon errors

  `record.lua` (a synthetic recording from the `record` scenario) is always replayed.
- **Adding a real one:** copy the `record` table from the player's file into a fixture (`ForeverBuddyDB = { record = ... }`, nothing else from the file) and add its name to `REAL` in `run.lua`. Read it through first, even though it's numbers only. Its first run is where `assumed` rows become confirmed or get corrected.
