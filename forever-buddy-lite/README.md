# ForeverBuddy Lite

It has two parts:
- a small addon that captures your characters to SavedVariables, with a status window;
- a reader (`reader-rs`, one Rust binary, `fbl-rs`) that Claude (or you) can query, as a CLI or as an MCP server, plus a live auction house dashboard.

It covers what Claude kept needing while helping in game: gear, bags, bank, mail, quest log and completed quests, skills (Lockpicking, weapon skills, professions), talents, spells, stats (spell power by school, crit, mana), reputation, lockouts, buffs and position; recipes and trainers; auction house gear; quest rewards and dungeon drops with their stats.

## In game

Run `node forever-buddy-lite/addon/install.mjs` (or copy `addon/ForeverBuddyLite` into `World of Warcraft/_forever_/Interface/AddOns/`), then enable it at character select. The window title shows the version and a build hash of the code, so you can tell which build you're on; `node forever-buddy-lite/addon/stamp.mjs` updates the hash after a change. After an update that adds files, restart the game once (a `/reload` doesn't load new files).

`/fbl` opens a window: what's been captured and when, and a button for each action below. `/fbl help` lists the commands:

- `/fbl r` (`save`) takes a snapshot and reloads, so it's written to disk. Use this when you want Claude to see fresh data.
- `/fbl snap` takes a snapshot and prints what it captured, without reloading.
- `/fbl status` prints the window's status lines.
- `/fbl ah`, with the auction house open, scans every weapon and armor listing (page by page, waiting out the AH throttle), then `/fbl r` to save. The scan is stored compressed (CBOR, deflated, base64); the reader unpacks it. Auctionator's full scan is picked up too.
- `/fbl items [level]` finds quest rewards you haven't earned and dungeon boss drops, from a few levels below you up to `[level]` (default: six above), and saves their stats. Needs Questie (it asks QuestieDB).
- `/fbl wa` opens a Musca Auras import of buff reminders (out of combat, per class); `/fbl wa bars` the rogue bars (Garrote, Rupture, Deadly Poison, Cheap Shot, Kidney Shot, Slice and Dice) on Aura (Modern) triggers, which keep running in combat.

It also captures by itself:
- a few seconds after login, and after bags, money, gear, skills, level, quests or talents change;
- at the bank (bank contents) and a mailbox (mail): as of your last visit;
- a profession window (recipes, colours, reagents) and a trainer's list (what it teaches at which skill).

The game writes the file only on `/reload` or logout. While it logs out it empties bags, gear and stats before addons save; a capture that comes back blank never replaces a good one.

### Layout

| File | Owns |
|---|---|
| `Core.lua` | helpers (safe API calls, item info, tooltip stat lines), the event hub, the command registry |
| `Data.lua` | the SavedVariables, schema and migrations, the blank-capture guard |
| `Snapshot.lua` | the character sections; `snap`, `r` |
| `Professions.lua` | profession windows and trainers |
| `AuctionHouse.lua` | the gear scan; `ah` |
| `QuestItems.lua` | quest rewards and dungeon drops via QuestieDB; `items` |
| `Auras.lua` | Musca Auras imports; `wa` |
| `UI.lua` | the window; `status` |
| `Commands.lua` | `/fbl` itself and `help` |

Saved data (`ForeverBuddyLiteDB`, schema 2): `characters["Name-Realm"]` per character; `ah`, `items`, `trainers` account-wide; `debug` for diagnostics; `ui` for the window position.

It never changes game state and sends nothing to the server.

## The reader

Build it once (needs Rust: https://rustup.rs):

```sh
cd forever-buddy-lite/reader-rs
cargo build --release          # the binary is target/release/fbl-rs (fbl-rs.exe on Windows)
```

Then (`cargo run --release -- <command>` works too):

```sh
fbl-rs characters
fbl-rs gear Mymage
fbl-rs find "linen cloth"
fbl-rs completed Myrogue 78676 78699
fbl-rs talents
fbl-rs help
```

Commands, each also an MCP tool:

| Command | What it returns |
|---|---|
| `characters` | Every character, newest first |
| `summary` | One character's overview |
| `stats` | The character's stats |
| `gear` | Worn gear |
| `items` | Bags, bank, mail and worn items, with an optional filter |
| `find` | An item across all characters |
| `quests` | The quest log with objectives |
| `completed` | Whether given quest ids are done |
| `skills` | Skill lines |
| `talents` | Talents by tree |
| `spells` | Known spells |
| `reputation` | Faction standings |
| `lockouts` | Instance lockouts |
| `buffs` | Active buffs |
| `mail` | Mailbox contents |
| `prices`, `flips`, `value` | Auction prices from Auctionator: an item's usual price, bargains, what your bags are worth |
| `upgrades` | Gear upgrades from the auction house scan (or quest rewards) |
| `recipes`, `trainers` | Profession recipes and what trainers teach |
| `raw` | The saved data as stored |

The character is optional: it defaults to the one saved most recently. Every result includes `saved`, which says when the game wrote that character.

More commands (not MCP tools):

| Command | What it does |
|---|---|
| `live [--port 4747] [--budget 5] [--min-listed 100] [--no-open] [--archive dir]` | The auction house dashboard (below) |
| `market [--budget 5] [--min-listed 100] [--top 12] [--json]` | The same analysis once, as text |
| `backtest` | Replays the market rules on past days against a control |
| `snipes [--cap 0.25] [--out file \| --dir folder]` | A Point Blank Sniper shopping list |
| `skillup [character] [profession] [--to 225]` | The cheapest way up a profession at today's prices |
| `questxp [character] [min] [max] [zone]`, `questxp [character] --chain "Quest"` | XP-dense zones, hubs and chains (needs QuestieDB) |
| `bis [character] [--levels 40,45,50,60]` | Best in slot by level for an agility melee (needs the WoWSims Forever database, `$WOWSIMS_DB`) |

**The dashboard.** Run `fbl-rs live` and leave it running; it opens http://localhost:4747 (this machine only). After every price scan or save that reaches disk (Auctionator's scan plus `/reload`, or `/fbl r`) it rebuilds, archives the scan to `Documents/FBL Market/scans`, prints new gaps and buys in the terminal, and pushes the update to the page. The page itself is `reader/web`; Hide on the page keeps an item out of every suggestion (`Documents/FBL Market/hidden.json`, or `$FBL_HIDDEN`).

**Where it reads from:**
- `$FBL_FILE` if you set it;
- otherwise every `WTF/Account/*/SavedVariables/ForeverBuddyLite.lua` under `$FBL_WTF`;
- by default, under `C:/Program Files (x86)/World of Warcraft/_forever_/WTF`.
- Auctionator's prices: `$FBL_AUCTIONATOR`, else the `Auctionator.lua` next to each addon file. QuestieDB: `$QUESTIEDB_DIR`, else the game's AddOns folder.

It only reads the game's files; it writes only its own (the scan archive, the hidden list, snipe lists).

**As an MCP server in Claude Code:**

```sh
claude mcp add forever-buddy-lite -- /path/to/forever-buddy-lite/reader-rs/target/release/fbl-rs mcp
```

## Develop

```sh
cd forever-buddy-lite/reader-rs
cargo test
cargo clippy --all-targets
```

The reader's modules: `lua` and `cbor` (the file formats), `sources` (typed data per file), `config` (every market threshold, with its reason), `model` (usual price, ladder gaps and verdicts), `strategies` (one function per market rule), `report`, `dashboard` and `catalog` (what the page reads), `live` (the watcher and server), `tools` (the commands), `mcp`. Tests run against `reader-rs/tests/fixture/ForeverBuddyLite.lua`.

CI (`.github/workflows/forever-buddy-lite.yml`) does three things:
1. compiles the addon on Lua 5.1;
2. runs it headless through WOWAPI with `tools/addon-smoke`, and fails on any Lua error or blocked action;
3. checks the reader: `cargo fmt --check`, `cargo clippy`, `cargo test`.

The addon reads APIs that may be missing through `pcall`, and it prefers the ones other addons already use on Forever:
- `C_Traits` for talents, as Leatrix Plus and Questie do;
- `C_Reputation` and `C_SpellBook`, as Questie does;
- `C_Container` and `C_QuestLog`, as ForeverBuddy does.

The classic APIs are the fallback.
