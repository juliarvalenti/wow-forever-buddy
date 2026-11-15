# ForeverBuddy Lite

It has two parts:
- a small addon that captures your characters to SavedVariables, with a status window;
- a zero-dependency reader that Claude (or you) can query, as a CLI or as an MCP server.

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

```sh
node reader/fbl.mjs characters
node reader/fbl.mjs gear Mymage
node reader/fbl.mjs find "linen cloth"
node reader/fbl.mjs completed Myrogue 78676 78699
node reader/fbl.mjs talents
node reader/fbl.mjs help
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
| `raw` | The saved data as stored |

The character is optional: it defaults to the one saved most recently. Every result includes `saved`, which says when the game wrote that character.

**Where it reads from:**
- `$FBL_FILE` if you set it;
- otherwise every `WTF/Account/*/SavedVariables/ForeverBuddyLite.lua` under `$FBL_WTF`;
- by default, under `C:/Program Files (x86)/World of Warcraft/_forever_/WTF`.

**As an MCP server in Claude Code:**

```sh
claude mcp add forever-buddy-lite -- node /path/to/forever-buddy-lite/reader/fbl.mjs mcp
```

## Develop

```sh
node --test forever-buddy-lite/reader/test/fbl.test.mjs
```

CI (`.github/workflows/forever-buddy-lite.yml`) does three things:
1. compiles the addon on Lua 5.1;
2. runs it headless through WOWAPI with `tools/addon-smoke`, and fails on any Lua error or blocked action;
3. runs the reader tests.

The addon reads APIs that may be missing through `pcall`, and it prefers the ones other addons already use on Forever:
- `C_Traits` for talents, as Leatrix Plus and Questie do;
- `C_Reputation` and `C_SpellBook`, as Questie does;
- `C_Container` and `C_QuestLog`, as ForeverBuddy does.

The classic APIs are the fallback.
