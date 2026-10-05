# Spec: Bridge v0.4, app → addon data slots, in-game Sync, alt-aware tooltips

Status: **draft, reload route pending probe run 4** · Author: @coder2 · 2026-10-05 (rev 2: tooltips are the first slot, Julia's pick)

Today data flows one way: the addon writes SavedVariables at logout or `/reload`, and the app ingests them. The bridge adds the other half. The app writes generated data files into **our own addon folder**, the addon loads them at login or `/reload`, and a Sync click in game does both halves in one reload. The first thing sent over it is a **tooltip index**: hover any item in game and see which alts hold it, where, and what it last sold for (§5). The game alone can't show that, because it only knows the character you're on. The **weekly checklist** (§5a) follows as the next slot.

Builds on `docs/specs/core-fs.md` (write gate, `RelPath`, `atomic_replace`, `sv`) and `docs/specs/v0.2-addon.md` (the ForeverBuddy addon and ingest). Mocks: `design/mocks/round-3/bridge.html` (#83). Probe: `tools/probe-addon` v3, run 4 (#84).

**Ground rules.** No network, in the app or the addon. The app writes nothing a player made while WoW runs. Slot files are data, never code. Nothing an agent writes reaches the game without the player's approval.

---

## 1. Scope

| In v0.4 | Later | Not planned |
|---|---|---|
| Data slots: fixed files, schema, data-only writer with a runtime check | More slots (shopping list, quest plan) | Any path chosen by the UI or an agent |
| The while-running write exception for those slots only (§3) | Agents staging slot content through the change queue (§7) | Live data mid-session (impossible, matrix fact 4) |
| Delivery receipts in the addon's SavedVariables | Quest planner (§6, after its go/no-go) | Automatic reloads |
| In-game Sync on a click (§4) | The weekly checklist slot and `/fb` frame (§5a) | |
| The tooltip index slots and the read-only tooltip hook (§5) | Profession cooldowns (for the checklist) | |
| App: "Sent to the game" panel on the Dashboard | | |

---

## 2. Data slots

### Files

A slot is one Lua file in `Interface/AddOns/ForeverBuddy/Data/`, listed in the TOC **before** `ForeverBuddy.lua`, so its global exists when our code runs:

```
## SavedVariablesPerCharacter: ForeverBuddyDB

Data/Tooltip1.lua
Data/Tooltip2.lua
ForeverBuddy.lua
```

- **Addon 0.4.0 has two slots, `Tooltip1` and `Tooltip2`:** the tooltip index, split in two by item id (§5). The list is a Rust constant (`bridge::SLOTS`); each entry is a name, a `RelPath`, a global name and a schema version. `Checklist` joins in a later addon release.
- **WoW reads the TOC once, at client start.** A changed slot shows after `/reload`; a new slot needs an addon update and a full restart. So slots are added only with an addon release, never at runtime.
- **The bundled slot files are stubs** (`ForeverBuddyData_Tooltip1 = nil`). `addon::install` writes them like any bundled file (WoW closed, behind the gate), then the bridge regenerates every slot in the same guard, so an update never leaves the game an empty index.

### Shape

Each file sets exactly one global, `ForeverBuddyData_<Slot>`, to one table:

```lua
ForeverBuddyData_Tooltip1 = {
	["schema"] = 1,
	["stamp"] = 1759698240,      -- when the app generated it (unix s); the delivery receipt echoes it
	["app"] = "0.4.0",
	...                           -- slot-specific body, §5
}
```

- The addon reads only `schema` values it knows. An unknown one shows a single quiet line where the data would be ("From a newer Forever Buddy. Update the addon from the app."), never a Lua error.
- Strings are display text only. The addon never passes a slot value to `RunScript`, `loadstring`, a macro, a slash command, a secure attribute or a frame name.
- **No markup from a slot.** `SetText` interprets WoW escape codes, so a slot string with `|Hitem:…|h[Fake Epic]|h`, `|T…|t` or `|c` could fake an item link, embed a texture or recolour text. The addon passes every slot string through one helper, `plain(s)`, that doubles each `|` to `||` before any `SetText` or tooltip line. The colours and icons the frame needs come from our own Lua (class colour from the `class` token), never from slot text. The escaping happens in the addon, at display, so the slot keeps the raw string and the check covers every producer, the app's and agents'.

### Writing a slot (`bridge::write_slot`)

1. Build the `LuaValue` table and serialize it with `sv::write_globals`. Strings use quoted escapes (`\"`, `\\`, `\n`, `\ddd`), never long brackets, so text containing `]]` can't end a string early.
2. **Check it at runtime, before every write:** `sv::parse` the bytes back and refuse unless the result is exactly one global, the slot's own name, and a table of strings, numbers and booleans, at most 8 deep, equal to what was serialized. `sv::parse` accepts data only, so code can't get through.
3. **Size cap:** 1 MB per slot, checked on the bytes.
4. Write with `fsx::atomic_replace`, so WoW never reads half a file.
5. Record the write in `bridge_slots` (§5, migration 008).

A unit test round-trips hostile strings (`]]`, `"`, `\`, newlines, control bytes, invalid UTF-8, `--[[`, `|Hitem:19019|h[Fake]|h`, `|TInterface\\Icons\\X:0|t`) through write and parse, and a property test checks that every generated table passes step 2. The addon test harness (`tools/addon-test`) loads a slot with the `|H` and `|T` strings and asserts that `plain()` returns them with every `|` doubled.

**Several slots in one write.** When more than one slot changes, every file is built and passes steps 2 and 3 **before any is written**, so one bad slot can't leave a half-updated set. Then each is replaced in turn.

---

## 3. Writing while WoW runs

The write gate refuses every game-folder write while WoW runs. Slots are the one exception, under @security-sanity-reviewer's four conditions (S1 thread, 20:51), all built in from the start:

1. **An exact list, not a prefix.** Only the `RelPath`s in `bridge::SLOTS`. No path comes from the UI or an agent. If `Interface/AddOns/ForeverBuddy` or `Data` is a link (symlink or junction), the write is refused. `WTF/` stays fully gated: WoW overwrites SavedVariables at logout, so writing them while it runs would lose data anyway.
2. **Atomic replace only,** through `atomic_replace`. No safety snapshot: the content is our generated data, never the player's.
3. **Data-only, checked at runtime** (§2, step 2), not just in tests.
4. **1 MB cap per slot.**

The exception lives in its own small function (`WriteGate::write_slots(&[(Slot, bytes)])`), which takes `Slot`s from the constant list, not paths. It can't go through `gate.begin`, since that refuses exactly the running case. Instead it:

- resolves every path through `RelPath` and `GameRoot`, where the escape check lives, so a linked `ForeverBuddy` or `Data` folder is refused with `PathEscape` (test modelled on `remove_never_follows_a_linked_folder_out`);
- writes with `atomic_replace`, and skips only the running check and the snapshot;
- takes the jobs lock, so it can't race a restore or an addon install.

Everything else keeps today's rule.

If the addon on disk is older than 0.4.0 (its TOC doesn't list the slot), the app doesn't write, and the panel shows the "restart" state ("needs the addon update below").

---

## 4. In-game Sync

A reload does both halves at once: WoW saves SavedVariables first (game → app, our watcher already ingests after `/reload`), then re-reads the slot files (app → game).

**Route, decided by probe run 4** (#84, `/fbprobe reload`):

| Probe result | Sync in v0.4 |
|---|---|
| (A) a plain button calling `ReloadUI()` reloads | A **Sync** button that calls `ReloadUI()` on click. Preferred by security: no macro text at all. |
| A blocked, (B) the secure `/reload` button reloads | A `SecureActionButtonTemplate` with `type = "macro"` and the **constant** `macrotext = "/reload"` in our own Lua. It is never read from a slot or anything the app or an agent writes, since secure macro text runs slash commands as the player. |
| Both blocked | No button. The frame says "Type /reload to sync" (the mock's `?sync=typed`). |

Either way:

- **Only on a click,** never on a timer or event (`ReloadUI` needs a hardware event anyway).
- **Not in combat.** The button is disabled under `InCombatLockdown()`, with the mock's tooltip: "Reloads your UI so Forever Buddy and the game swap the latest notes. Not in combat."
- `/fb sync` does the same as the button where the route allows it (a slash command is a hardware event; a secure button can't be clicked from Lua, so on route B `/fb sync` prints "Type /reload to sync").

**Probe run 4 also checks** that `/reload` re-reads a slot whose contents changed while logged in (`Data.lua` stamp). If it doesn't, Sync can't deliver while playing, the panel copy becomes "picked up next login", and the while-running exception (§3) isn't needed for v0.4.

### Delivery receipts

On `ADDON_LOADED`, the addon copies each slot's `stamp` into `ForeverBuddyDB.bridge[<slot>] = { stamp, seenAt }`. That's per character and reaches disk at the next reload or logout, when ingest stores it. The app compares it with the last stamp it wrote:

| App state (mock `?app=`) | When |
|---|---|
| `pending` · "waiting for a sync" | written, no receipt with that stamp yet |
| `synced` · "in the game since 21:06" | a receipt with the newest stamp |
| `restart` · "restart WoW once" | the addon on disk is older than 0.4.0 |
| `failed` · "couldn't write" | the last write was refused (link, cap, check) or failed; the game keeps the last file it saw |

The "how to" line uses the route the probe picked: "press Sync on the Forever Buddy frame in-game (or type /fb sync)" or "type /reload in-game".

---

## 5. Alt-aware tooltips: the tooltip index (first slots)

Hover an item anywhere in game and the tooltip adds which alts hold it, where, and its last scan price (INGAME.md §8, `ingame.html`):

```
Forever Buddy
Coinpurse                 340 · bank
Velyra                     60 · bags
All alts                   400
Last scan            ≈ 1g 12s each
As of each alt's last logout · scan 3 days ago
```

### What the app sends

**Ids and counts only.** The only strings from our side are the account's own character names and class tokens, in a small header. Item names come from the game's own cache (the tooltip is already showing the item), so no item text is ever sent.

```lua
ForeverBuddyData_Tooltip1 = {
	["schema"] = 1,
	["stamp"] = 1759698240,
	["app"] = "0.4.0",
	["scanAt"] = 1759437240,          -- last Auctionator scan (unix s), absent without prices
	["alts"] = {                       -- index → character; same list in both slots
		{ ["name"] = "Coinpurse", ["surname"] = "", ["class"] = "WARRIOR", ["seen"] = 1759530000, ["bank"] = 1759530000, ["mail"] = 1759100000 },
		...
	},
	["items"] = {                      -- this slot's half: item ids with id % 2 == 0 (Tooltip1) or 1 (Tooltip2)
		[12360] = { 36400, 1, 0, 24, 0, 0, 3, 2, 0, 0, 0 },
		...
	},
}
```

- An `items` entry is a flat list: the AH price in copper (`0` if none), then for each alt holding it, five numbers: the alt's index in `alts`, then its count in bags, bank, mail and equipped. A flat list of numbers is both the smallest shape `sv::write_globals` emits and the simplest to read in Lua.
- `seen`, `bank` and `mail` in `alts` are when that alt's bags, bank and mailbox were last read (its last logout, last bank visit, last mailbox visit). Shift shows them per alt.
- Sources: `char_items` grouped by `(item_id, character, location)`; prices from `ah_latest` for the current market, only when `ah_status.has_prices` (F5d). Every character the app knows for this flavor is included, across WTF accounts, since the AddOns folder is shared by them all.

### Size, against the 1 MB cap

Measured shape: each number is one line (tabs, digits, comma, newline), about 7 bytes. An (item, alt) pair is five numbers, about 35 bytes; each item adds about 30 bytes of its own (key, braces, price).

| Account | Pairs | Items | Estimate (both slots) |
|---|---|---|---|
| Typical: 7 alts, ~150 distinct items each | ~1,000 | ~700 | ~60 KB |
| Large: 20 alts, full bags, bank and mail, ~250 distinct items each | ~5,000 | ~3,000 | ~265 KB |
| Extreme: 50 alts, ~400 distinct items each | ~20,000 | ~6,000 | ~880 KB |

The large account fits one slot with room to spare, but the extreme one would land near 1 MB. Since slots are fixed in the TOC and adding one later costs an addon update and a restart, **addon 0.4.0 ships two slots now**, split by item id parity, so each half carries about half the pairs: about 440 KB per slot even for the extreme account. B1 includes a test that generates the large and extreme accounts, prints the measured sizes, and asserts each slot stays under 512 KB (large) and under the 1 MB cap (extreme), so the estimate is checked, not assumed.

**Over the cap: refuse and report, never truncate.** If either half would exceed 1 MB, neither is written as data. Instead each slot gets a header-only file with `["tooLarge"] = true` and no `items`. The tooltip shows one quiet grey line, "Alt data too large to send", and the app's panel shows the slot as "too large to send". A truncated index would show wrong counts, which is worse than none.

### The addon side

- **Read-only hook:** `TooltipDataProcessor.AddTooltipPostCall(Enum.TooltipDataType.Item, fn)`, adding lines with `AddLine` / `AddDoubleLine` on the game's own tooltip. It never creates a tooltip of its own and never calls anything protected.
- **Guarded:** the whole callback runs in `pcall`. Any error drops our lines for that tooltip and is counted in `ForeverBuddyDB` (shape only), never shown as a Lua error.
- **Not in combat-only or unit tooltips:** item tooltips only, and nothing added while `InCombatLockdown()`, so we never touch secret values.
- **Current character:** its row is left out of the index lookup (matched on name and surname) and replaced by the live count from `C_Item.GetItemCount(id, true)` (bags and bank), so it's never stale.
- **At most 6 alt rows,** then "+N more". Names are passed through `plain()` and coloured from `RAID_CLASS_COLORS[class]`.
- **Lookup** is `slots[(id % 2) + 1].items[id]`, a table read per hover. Nothing is precomputed at load beyond checking `schema`.

### Generation

Generated after each ingest that changed items, after an AH price update, and after an addon install, and written only when the bytes differ from the last write. Before any write, both halves go through §2's checks (re-parse, cap) together.

---

## 5a. The weekly checklist (next slot, after tooltips)

Generated whenever its inputs change (after each ingest, at the weekly reset, on a lockout or bank-alt edit), and written when the content differs from the last write.

**Body:** for each character the app knows on this account and flavor, in last-played order:

```lua
["characters"] = {
	{
		["name"] = "Velyra",
		["class"] = "PALADIN",          -- class file token, for the class colour
		["raids"] = { { ["name"] = "Molten Core", ["saved"] = false, ["resetsAt"] = 1759849200 } },
		["mail"] = { ["expiring"] = 3, ["soonestAt"] = 1759871040 },
		["cooldowns"] = { { ["name"] = "Transmute: Arcanite", ["readyAt"] = 1759700000 } },
	},
}
```

| Section | Source | Notes |
|---|---|---|
| Raids | `lockouts_list` (F3), the `LOCKOUT_LIVE` filter | "saved" or "resets Tue" from `reset_at` |
| Mail | `char_mail.days_left` + `as_of` | expiring within 3 days; counts only, no senders or subjects |
| Cooldowns | **new in the checklist's addon release** | see below |

- The in-game frame shows times relative to now (`GetServerTime()`), so "resets Tue" stays right however old the file is. Past `resetsAt` rows show as clear, and past `readyAt` as ready.
- **Cooldowns are not in the db yet.** The checklist's addon release records profession cooldowns at logout (`C_TradeSkillUI.GetRecipeCooldown` for known cooldown recipes, `C_Spell.GetSpellCooldown` for transmutes), stored as `char_cooldowns (character_id, spell_id, name, ready_at, as_of)` in its own migration. Cooldowns are secret in combat under Midnight's rules, but logout is never in combat. Probe run 4 doesn't cover these calls, so that release checks them itself: each is `pcall`-wrapped, and a missing, secret or nil result records nothing. Julia's first logout with it settles it. If neither returns real data, the checklist ships with Raids and Mail and the Cooldowns section is left out (never shown empty).
- **The frame:** `/fb` toggles it, small and in the game's own dialog look (mock right side). It **never opens by itself** (INGAME.md §3, #86). At login, and only when the app delivered a stamp this character hasn't seen, the addon prints one chat line: "Forever Buddy: this week's checklist is ready. /fb to open." The addon compartment tooltip shows "This week: 4 to do". An opt-in popup can come later if players ask.
- **If `/reload` doesn't re-read a changed slot** (probe run 4), the frame says "Updates arrive at your next login" and the app says "Your characters will see it at their next login." (INGAME.md §5).

---

## 6. Quest data (research, go/no-go for the planner)

The planner the PM described (plan levels in the app, an in-game step list) needs quest data for Forever's new content. Findings (2026-10-05):

1. **QuestieDB is the best source.** Official Questie supports Forever (the "Camelot" flavor; QuestieDB 1.0.4, 2026-09-30). The source data is plain Lua (`data/Forever/forever{Quest,Npc,Object,Item}DB.lua`), with corrections layered on top (a Wowhead-derived delta, QuestieTrace player recordings, hand fixes). Quests carry 36 fields: givers, turn-ins, levels, race and class masks, objectives, prereqs and chains. Coordinates live on NPC and object spawns.
   - **Coverage of the new zones is partial.** Their own `FOREVER_WORK_LEFT_TO_DO.md` lists Skyborne and the new zones as partial, and the delta "does not supply complete objectives".
   - **License is unclear.** Neither repo has a LICENSE file. CurseForge says GPLv3, and third-party projects treat it that way.
2. **Wowhead Forever** is the most complete (about 5,400 quests) but proprietary. Don't scrape it.
3. **wago.tools DB2** (QuestV2, about 6,600 rows) has IDs only, no text. It's useful as the list of valid IDs, to measure what QuestieDB misses.
4. **Our addon can record quests** with retail APIs not tied to combat (`C_QuestLog.GetInfo`, `GetQuestObjectives`, `QUEST_ACCEPTED`, `QUEST_TURNED_IN`, `C_Map.GetPlayerMapPosition`), as QuestieTrace does. The probe already registers `QUEST_ACCEPTED` and `QUEST_TURNED_IN` and calls `C_QuestLog.GetTitleForQuestID` on turn-in; the rest is untested on 16001.

**Recommendation: no-go for a v0.4 planner, go for groundwork.**

- **Now (with Q-SPIKE, @coder):** the next addon release records accepted and completed quest IDs (no text) into the session log. That's cheap, has no licensing question, and gives the app "what have I done" per character.
- **Before any planner:** (a) Julia or the PM asks the Questie maintainers about the license, or we choose to read the user's installed QuestieDB at runtime instead of shipping its data; (b) sample QuestieDB's latest release against wago's QuestV2 IDs for the new zones. If coverage is under roughly 80% there, the planner waits.
- **Never:** Wowhead scraping.
- **Reading the user's Questie install: read it, never run it** (security). Questie's data ships as addon code, not SavedVariables, so it's usable only if the coverage sample shows it can be pulled out with a data-only parse: `sv` table constructors and string literals, or the release build's CBOR blocks (we already decode CBOR for Auctionator, F5). If it needs a Lua interpreter to evaluate, it's a no-go. We never embed a Lua VM to run third-party addon code. Reads follow the usual rules: read-only, `safe_read`, and a size cap.

---

## 7. One write path for the app and agents (with @coder)

The bridge and the agents/MCP idea both mean "something outside the game changes what the game reads". They share one contract, so there's one place to check, stage and apply changes.

```rust
pub enum Change {
    /// A data slot: the app's own generated data. Allowed while WoW runs (§3).
    Slot { slot: Slot, body: LuaValue },
    /// An edit inside a SavedVariables file (an addon's settings): gated, snapshot first.
    /// `expected` is the value the producer saw; apply refuses if it changed since.
    SvEdit { file: SvFile, path: Vec<Key>, expected: Option<LuaValue>, value: LuaValue },
    /// An addon profile, e.g. ElvUI, via its codec: gated, snapshot first.
    Profile { addon: AddonId, profile: String, expected: Option<LuaValue>, body: LuaValue },
}

/// A target is a key from the read tools, never a path. Apply resolves it
/// against the WTF roster (older folders excluded) and the installed addons,
/// then builds the RelPath in Rust.
pub enum SvFile {
    Account { account: String, addon: AddonId },
    Character { character: CharacterKey, addon: AddonId },
}
```

Apply rules (@coder's notes on #87):

- **All or nothing.** Every change in an Apply is checked before any write: target resolution, the runtime data-only check (§2, also for `SvEdit.value` and `Profile.body`), the cap, and the `expected` conflict check. Gated kinds share one gate op and one snapshot, with several edits to one file folded into one atomic write, as F6 does. While WoW runs, a set with any gated change is refused whole; a slots-only set follows §3.
- **Undo.** Gated kinds undo from the Apply's snapshot (F6's `undo`). `Slot` has no undo: the generator rewrites it from the db.
- **Queue storage.** Staged changes persist in migration 009 (@coder): id, kind, JSON body, producer (`app` or `agent:<name>`), created_at, status (`staged`, `applied`, `discarded`, `conflict`).

- **Two producers.** The app's own generators (the tooltip index, later the checklist) apply `Slot` changes directly. Agents (MCP tools) only **stage** changes into @coder's queue, and the player approves them in the F6 stage/apply bar. An agent never writes a file.
- **Ownership.** @coder owns the queue, approval and apply side. I own the slot writer (§2), the SavedVariables edit-in-place writer, the ElvUI codec, and the read-only MCP tools (characters, lockouts, prices, the checklist).
- **Per-kind rules** come from the kind, not the producer: `Slot` follows §3; `SvEdit` and `Profile` keep today's gate (WoW closed, safety snapshot, restore journal).
- **Agent text in a slot** is just a string under the same runtime check and cap. It never becomes a macro, a secure attribute or a path.

Only the `Slot` kind is built in v0.4. The other two are listed here so the queue's types don't change later.

---

## 8. Gates and order

| Gate | Needs | Blocks |
|---|---|---|
| G1 | Probe run 4: the reload route, and whether `/reload` re-reads a changed slot | §4's button; §3 (needed only if the re-read works) |
| G2 | First logout with the checklist's addon release: cooldown APIs return data (not in probe run 4) | Cooldowns in the checklist |
| G3 | Questie license answer + coverage sample | Any quest planner |

**Build order:**

1. **B1:** migration 008, `bridge::SLOTS`, `write_slots` with the runtime check, cap, link refusal and tests. Slot writes only while WoW is closed, and a test pins that (a write while running is refused), so the while-running path can't land before G1.
   B1 also covers addon 0.4.0's slot loading: the two `Tooltip` stubs in the TOC, the `schema` check, `plain()`, and receipts. Receipts add `bridge` to `ForeverBuddyDB`, so the TOC Version and `VERSION` bump together, the harness fixtures are regenerated, and V6's decoder gets a fixture test for the new key. If `_meta.schema` changes, `SCHEMAS` in `ingest/file.rs` changes in the same PR.
2. **T-TIP:** the tooltip index generator (§5, with the size test and the too-large state) and the addon's read-only tooltip hook. Sync follows G1.
3. **B3:** the Dashboard "Sent to the game" panel and its states (plus "too large to send").
4. **B4:** the while-running exception (§3), after G1 shows that `/reload` re-reads slots.

5. **Later:** the checklist slot and `/fb` frame (§5a), with cooldowns, in its own addon release.

B1 to B3 ship "written when WoW closes, picked up next login" on their own, which is security's safe default.
