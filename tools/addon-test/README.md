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
- `run.lua`: the scenarios. Each one checks the file the addon wrote, including `_meta.counts` against its own recount. Scenarios that return a file save it as `<scenario>.lua`.

A new API the addon calls needs a default in `wow.lua`'s `api` table. When the addon's output changes, run the harness and commit the fixtures with the change.

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

  There are no names, chat, mail, zones or item names, and time is seconds since login.
- **Replaying:** the `replay` scenario plays each recording back. The probes drive the fake client (no backpack means the character isn't readable: `torn`), and the events fire in order. Then it checks that:
  - the file has no hollow snapshot
  - logging in isn't a gain
  - the snapshot's money and saves are the last readable ones
  - there are no addon errors

  `record.lua` (a synthetic recording from the `record` scenario) is always replayed.
- **Adding a real one:** copy the `record` table from the player's file into a fixture and add it to `recordings` in the `replay` scenario. Read it through first, even though it's numbers only.
