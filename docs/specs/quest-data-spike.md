# Q-SPIKE: quest data for the planner

Status: findings, 2026-10-05 (@coder). Input to the Bridge spec (bridge-v0.4.md §6) and the planner go/no-go. This is research, not legal advice.

## Questions

1. Can the app read a user's local QuestieDB install **without running it** (security's condition: a data-only parse, no Lua VM)?
2. How much of Forever's quest content does QuestieDB cover, especially the new zones?
3. What does its license mean for reading a local install?
4. What can our own addon record instead?

## 1. Reading QuestieDB without running it

QuestieDB (github.com/Questie/QuestieDB, flavor `Forever`, TOC alias `QuestieDB_Camelot.toc`) installs in one of two modes, chosen by which TOC exists:

| Mode | What's on disk | Data-only readable? |
|---|---|---|
| **Source** (`QuestieDB.toc`, a git clone) | `data/Forever/forever{Quest,Npc,Object,Item}DB.lua`: a Lua header, then `QuestieDB.questData = [[return { [id] = {…}, … }]]` | **Yes, tested.** The `[[…]]` long string holds one table constructor of numbers, strings, `nil` and nested tables. Located by text and parsed with our own `sv::parse_value`, the quest table (4,244 rows) reads in about 5 ms and the NPC table (with spawn coordinates) reads too. No Lua runs. |
| **Baked** (`QuestieDB_<Flavor>.toc`, generated; what an addon manager installs) | Entity data in the TOC's `## …` metadata as CBOR rows and tables, with compressed ID headers, read in game via `GetAddOnMetadata` and `C_EncodingUtil.DeserializeCBOR` | **Likely yes, not tested.** It's data, not code: TOC metadata lines, then decompress and CBOR-decode. We already depend on `ciborium` (F5). But QuestieDB calls the numeric encoding private, so it can change between releases. |

**Caveat for Source mode:** QuestieDB's corrections (`src/corrections/Forever/*.lua`, including the generated delta below) are **Lua code** (`[questKeys.requiredLevel] = 22` inside functions), applied at load. A data-only read of Source mode gets the raw tables **without** corrections. Baked mode has them folded in, which is the better target, at the cost of tracking a private format.

## 2. Coverage

Compared against the client's own quest list: wago.tools `QuestV2` for build **1.60.1.70205** (`wow_classic_beta`), 6,609 quest IDs (IDs only, no names or flags).

| Set | Quest IDs | Client IDs covered |
|---|---|---|
| QuestieDB raw `foreverQuestDB` | 4,244, **all below 10,000** (original-game quests) | 3,535 of 3,756 client IDs below 10k (94%) |
| + generated delta-base (`foreverBaseQuest.lua`) | +734, all 55,000 and up (new content) | 721 of the client's **2,853** IDs of 55k and up (**25%**) |
| Both | 4,978 | 4,256 of 6,609 (64%) |

- **New content is the gap.** QuestieDB's own `docs/forever.md`: the delta "adds snapshot-derived entities and gameplay deltas, but does not supply complete objectives, restrictions or other gameplay behavior", and Questie's `FOREVER_WORK_LEFT_TO_DO.md` lists Skyborne and new-zone quest content as partial.
- **The 25% understates it somewhat:** some of the 2,853 high IDs are likely hidden tracking quests, and QuestV2 carries no flags to separate them. A rough bound from Wowhead's public count (about 5,400 Forever quests) puts real new player-facing quests at about 1,900, so Questie has roughly **40%** of them, with partial objectives.
- The delta's rows cite wowhead.com URLs per quest. Its provenance is QuestieDB's to document (`docs/forever-delta-base.md`); it's another reason not to redistribute it.

**Against the spec's bar** (planner waits if new-zone coverage is under ~80%): **not met**, by a wide margin.

## 3. License

- Neither `Questie/Questie` nor `Questie/QuestieDB` has a LICENSE file (the GitHub API reports none). CurseForge lists Questie as GPLv3.
- The GPL governs copying and **distributing** the work. Reading the files of a copy the user installed, on the user's machine, without shipping any of it, is generally not distribution. That's the reasoning behind @project-mgmt's option B. A local read doesn't make our app a derivative work in the usual reading, but this isn't legal advice, and the missing LICENSE file is itself an ambiguity.
- **Recommendation:** before shipping any feature that depends on QuestieDB, ask the maintainers (GitHub issue or Discord) whether they're fine with a companion app reading a local install. It's cheap and removes the ambiguity. Never bundle their files.

## 4. What our addon can record (no license question)

Retail APIs not tied to combat, all on the Forever client (feature matrix fact 1, retail API); **to verify in probe run 5**:

- `QUEST_ACCEPTED(questID)` and `QUEST_TURNED_IN(questID, xp, money)`: per-session log entries (the session log already exists, V3).
- `C_QuestLog.GetAllCompletedQuestIDs()`: the character's **entire** completed list in one call, so "what have I done" is complete from the first login with the addon, not just from then on. Cheap to snapshot at logout (IDs only, a few KB).
- `C_QuestLog.GetInfo` / `GetQuestObjectives` / `C_QuestLog.GetTitleForQuestID`: names and objective progress for quests **in the log**, to name IDs we've seen without any third-party data.
- `C_Map.GetBestMapForUnit` + `C_Map.GetPlayerMapPosition` at accept and turn-in: where the player was. Crowd data is out of scope; this is only the player's own history.

That gives per-character quest history and "done / not done" for any quest ID, with names for the quests the player has had. It does not give a planner (where quests start, what's next) for content they haven't touched.

## Recommendation

- **No-go for a planner in v0.4**, confirmed by measurement: new-content coverage is about 25–40% with partial objectives.
- **Go for groundwork in addon 0.4.0:** record `QUEST_ACCEPTED`/`QUEST_TURNED_IN` in the session log and snapshot `GetAllCompletedQuestIDs()` at logout (IDs only). Add these to probe run 5.
- **Option B stays viable for later** if coverage improves: data-only read of the user's **Baked** QuestieDB (CBOR + decompress via `ciborium`), behind a version check that refuses unknown formats (the same pattern as F5's Auctionator gate), plus a size cap and `safe_read`. Never a Lua VM. Ask the maintainers first.
- **Re-measure** when QuestieDB ships new Forever data: the method is a QuestV2 CSV from wago.tools for the current build, set-compared with QuestieDB's quest IDs, and takes minutes.

## Sources

- QuestieDB: github.com/Questie/QuestieDB (`README.md`, `docs/forever.md`, `PROVENANCE.md`, `data/Forever/*`, `src/corrections/Forever/generated/foreverBaseQuest.lua`), read 2026-10-05.
- Questie: github.com/Questie/Questie (`FOREVER_WORK_LEFT_TO_DO.md`).
- wago.tools DB2 `QuestV2`, `QuestLineXQuest`, `AreaTable`, build 1.60.1.70205.
