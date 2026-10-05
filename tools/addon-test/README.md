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
