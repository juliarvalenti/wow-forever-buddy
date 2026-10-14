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
