# Feature matrix: where each mocked feature's data comes from

Status: **research complete, pending in-game probe** · Author: @coder2 · 2026-10-05

Every feature in the round-3 mocks, with its data source on the WoW: Forever client, the API or file, a verdict, and a fallback where the answer isn't a clean yes. Rows and targets come from `specs/feature-matrix` v1; the original "data needed" and "freshness" columns are in that memory.

**Verdicts.** *yes*: the API exists on Forever 1.60.1 and nothing known blocks it. *partial*: it works with a real limit (only while a frame is open, secret in combat, needs an external service). *no*: not available; use the fallback. Every *yes* is still unconfirmed until the probe addon (bottom) runs once on Julia's client. "Exists in the API dump" only proves the function is there, not that it returns real data.

## Sources

- **API dump:** [`forever-addon-kit/data/forever_api.json`](https://github.com/Thunderz96/forever-addon-kit), captured on build 1.60.1.69893 (6,045 global functions, 269 `C_*` namespaces). Each API named below was checked against it.
- **Measured findings:** the same repo's README and `docs/BETA_WATCHLIST.md` (first week of the beta, rechecked 2026-09-24).
- **Community reports:** Blizzard forums, addon authors' repos and changelogs (linked inline). These are weaker evidence; the probe confirms them.

## Platform facts that shape the whole matrix

1. **Forever runs the Retail client API, not Classic.** `WOW_PROJECT_ID` is mainline, the interface is `16001`, and the old Classic globals are gone (`GetItemInfo`, `GetContainerNumSlots`, `GetMerchantItemInfo`, `QueryAuctionItems`, `CombatLogGetCurrentEventInfo`…). The companion addon must be written against `C_Item`, `C_Container`, `C_Bank`, `C_AuctionHouse`, `C_TooltipInfo`, `C_AddOns`.
2. **Midnight's combat restrictions apply.** There are secret values (`C_Secrets`, `issecretvalue`), the combat log is restricted, and health, auras and unit identity can be secret in combat. Out of combat, inventory, money, XP, zone, mail, bank and AH data are not secret-gated. Anything that needs a combat detail (killer name, damage taken) is *partial* at best.
3. **SavedVariables are written on logout and `/reload`.** That's the only way addon data reaches the app. The beta bug where the client **never read them back** was fixed in build 1.60.1.70009 (2026-09-24, [confirmed by players](https://eu.forums.blizzard.com/en/wow/t/solvedforever-beta-160169913-savedvariables-fail-to-load-on-client-startupreload-%E2%80%94-all-addon-settings-reset-on-restart/629888); built-in chat settings still reset). The app keeps history in its own DB regardless. The companion addon writes "what's true now", and the app accumulates over time.
4. **Nothing reaches disk mid-session.** CVars registered by addons never reach disk, and SavedVariables are only written at logout and `/reload`. So "live" data (row 3) can't come from the addon while the game is running.
5. **No realms: rulesets.** Forever has rulesets instead of realms, and the client reports the ruleset where a realm name normally goes. The AH is one market per ruleset and faction ([source](https://ahledger.com/wow-forever/auction-house)). The WTF `Account/<ACCOUNT>/<Realm>/` folder is expected to be the ruleset name; to confirm.
6. **Registering an unknown event throws** and aborts the file, so every `RegisterEvent` in the companion addon must be `pcall`-wrapped (the probe does this).

## Matrix

| # | Area | Feature | Target | Source | API / file | Verdict | Notes / fallback |
|---|------|---------|--------|--------|------------|---------|------------------|
| 1 | Setup | Find install + flavors | v0.1 | local game files | registry, common paths, `.build.info`, `_classic_beta_/WowB.exe` | **yes** | Built in T5 (#8). |
| 2 | Setup | WoW running indicator | v0.1 | local process list | `sysinfo` poll, exe under root / `WowB.exe` | **yes** | Built in T6 (#11). |
| 3 | Setup | Logged-in character name, live | v0.2 | addon API (written at logout) | `UnitName`, `GetRealmName` | **partial** | The addon knows the name instantly but can't get it to disk until logout or `/reload` (fact 4). **Fallback:** show "WoW is running · last played Thrandor" from the newest per-character SV / WTF char folder mtime. Flagged below. |
| 4 | Backups | Snapshot / restore WTF + SV | v0.1 | WTF files | file system (T4–T9) | **yes** | |
| 5 | Characters | Roster, no-addon state | v0.1 | WTF files | `WTF/Account/<A>/<Realm>/<Char>/` folders; last played = newest file mtime in it | **yes** | The realm folder is likely the ruleset name (fact 5); the probe and Julia's `dir` will confirm. |
| 6 | Characters | Class, race, level, guild | v0.2 | addon API | `UnitClass`, `UnitRace`, `UnitSex`, `UnitLevel`, `UnitFactionGroup`, `GetGuildInfo` | **yes** | |
| 7 | Characters | XP %, rested | v0.2 | addon API | `UnitXP`, `UnitXPMax`, `GetXPExhaustion`, `GetRestState` | **yes** | Rested keeps accruing after logout; the app can extrapolate from logout time + rested state if wanted. |
| 8 | Characters | Item level | v0.2 | addon API | `GetAverageItemLevel`, `C_Item.GetCurrentItemLevel` | **yes** | |
| 9 | Characters | Gold per char | v0.2 | addon API | `GetMoney` | **yes** | |
| 10 | Characters | Time played | v0.2 | addon API | `RequestTimePlayed` → `TIME_PLAYED_MSG(total, level)` | **yes** | It prints to chat; call it once at login and suppress the chat line. |
| 11 | Characters | Zone, last seen | v0.2 | addon API | `GetRealZoneText`, `GetSubZoneText`, `C_Map.GetBestMapForUnit`, `GetServerTime` | **yes** | |
| 12 | Characters | Equipped gear | v0.2 | addon API | `GetInventoryItemID/Link` (slots 1–19), `C_Item.GetCurrentItemLevel(ItemLocation)` | **yes** | Store the link: it carries enchants and suffixes. |
| 13 | Characters | Bags + free slots | v0.2 | addon API | `C_Container.GetContainerNumSlots/NumFreeSlots/GetContainerItemInfo/GetBagName` | **yes** | Bags 0–4 + reagent bag 5. |
| 14 | Characters | Bank contents | v0.2 | addon API, **while the bank is open** | `BANKFRAME_OPENED`, `C_Bank.FetchPurchasedBankTabIDs`, `C_Container.*` | **partial** | Only readable at the banker. Forever has bank tabs but no Warbank ([BetterBags note](https://github.com/Cidan/BetterBags)). **Fallback:** show "as of last bank visit <date>". |
| 15 | Characters | Mailbox contents | v0.2 | addon API, **while the mailbox is open** | `MAIL_INBOX_UPDATE`, `GetInboxNumItems`, `GetInboxHeaderInfo` (sender, money, days left, item count) | **partial** | Same limit: "as of last mailbox visit". Expiry is derivable (days left + visit time), so the app can warn about expiring mail. |
| 16 | Characters | Professions + skill | v0.2 | addon API | `GetProfessions`, `GetProfessionInfo` (rank, max rank) | **yes** | Specialisation details may need `C_TradeSkillUI` while the profession window is open; the probe records both. |
| 17 | Characters | Raid/dungeon lockouts | v0.3 | addon API | `RequestRaidInfo` → `UPDATE_INSTANCE_INFO`, `GetSavedInstanceInfo` | **yes** | |
| 18 | Characters | Cross-alt item search | v0.3 | app (index of 12–15) | SQLite over ingested data | **yes** | Same freshness caveats as rows 14–15. |
| 19 | Items | Name, quality, ilvl by id | v0.2 | addon API (+ cache) | `C_Item.GetItemInfo`, `GetItemInfoInstant`, `GetItemQualityByID`, `GetDetailedItemLevelInfo` | **yes** | Item info is async on a cold cache (`C_Item.RequestLoadItemDataByID`). The addon records static info for every item it sees, and the app caches it forever. |
| 20 | Items | Item icon image | v0.2 | addon API (id) + external art | `C_Item.GetItemIconByID` returns a texture **file ID**, not an image | **partial** | An addon can't export pixels. **Fallback chain:** (a) Battle.net Game Data API item media, keyed by item id, once Forever has a namespace (needs the optional key); (b) a bundled icon set keyed by file ID from the community listfile; (c) the mocks' letter tiles. Recommend (c) in v0.2 and (a) as an upgrade. |
| 21 | Items | Full tooltip | v0.2 | addon API | `C_TooltipInfo.GetItemByID` / `GetHyperlink` (line text + colours) | **partial** | Works, but only for items the addon has seen, and the text is client-locale. Stored per item id. **Fallback:** name + quality + ilvl only for unseen items. |
| 22 | Items | "Looted from X" | v0.3 | addon API | `LOOT_READY`, `GetLootSourceInfo(slot)` returns the source **GUID** | **partial** | We get the creature GUID (→ NPC id), not the name, and `UnitName("target")` may be secret (`C_Secrets.ShouldUnitIdentityBeSecret`). **Fallback:** NPC id → name from a bundled table (e.g. built from [forever-quest-markers](https://github.com/TylerAkins/forever-quest-markers) / AllTheThings data), else "Looted" with no source. This is the designer's fallback line from round 3. |
| 23 | Portraits | Class crest + race badge | v0.2 | bundled art + row 6 | – | **yes** | |
| 24 | Portraits | Armory render | v0.4 | Battle.net Profile API | `/profile/wow/character/{realm}/{name}/character-media` | **no (for now)** | Blizzard hasn't published a Forever profile namespace ([as of late Sept](https://github.com/Indicaza/holdfast/issues/27)), and realm slugs are rulesets. **Fallback:** crest (23) or screenshot (25). Revisit after the 2026-11-04 launch. |
| 25 | Portraits | Portrait from screenshot | v0.3 | local game files | `_classic_beta_/Screenshots/*.jpg` | **yes** | App-only crop UI. |
| 26 | Ledger | Gold over time | v0.3 | app history of row 9 | one point per logout/`/reload` (+ `PLAYER_MONEY` points from row 31) | **yes** | Resolution is per session, which matches the chart. |
| 27 | Ledger | Net worth | v0.4 | app (12–15 × 39) | – | **partial** | Only as good as the price coverage (39) and the bank/mail freshness (14, 15). Show the freshness note the mock already has. |
| 28 | Ledger | Most valuable holdings | v0.4 | app (12–15 × 39) | – | **partial** | Same as 27. |
| 29 | Sessions | Session start/end, duration | v0.3 | **app process watcher** (+ addon login/logout times) | T6 process events; `PLAYER_LOGIN`/`PLAYER_LOGOUT` + `GetServerTime` | **yes** | The process watcher gives game sessions already in **v0.1**, without the addon. Per-character sessions need the addon. Flagged below. |
| 30 | Sessions | Gold/XP/loot/quests delta | v0.3 | app diff of per-logout snapshots | rows 6–13 at each logout | **yes** | Quest and loot counts come from the event log (35, 36). |
| 31 | Sessions | Gold through the evening | v0.3 | addon event log | `PLAYER_MONEY` + `GetMoney` + time | **yes** | Log each change in-session; written at logout. Cap the log size. |
| 32 | Sessions | Zones entered | v0.3 | addon event log | `ZONE_CHANGED_NEW_AREA` + `GetRealZoneText` | **yes** | |
| 33 | Sessions | Level up "Ding" | v0.3 | addon event log | `PLAYER_LEVEL_UP(level)` | **yes** | |
| 34 | Sessions | Deaths + killer + repair cost | v0.3 | addon event log | `PLAYER_DEAD` (time, zone); repairs via `MERCHANT_SHOW` + `GetRepairAllCost` and the money delta | **partial** | The death and repair spend are fine. **Killer is not available:** the combat log is restricted (`CombatLogGetCurrentEventInfo` is gone), and death recap and unit identity can be secret. **Fallback:** the round-3 line "Died · 20:38 · Hillsbrad". |
| 35 | Sessions | Loot received | v0.3 | addon event log | `CHAT_MSG_LOOT` (own loot lines: item link + qty) or a `BAG_UPDATE_DELAYED` diff | **yes** | Parse only "You receive…" lines (locale strings via the `LOOT_ITEM_SELF*` globals), or diff bags, which is locale-free and also catches crafted and quest items. Recommend the bag diff. |
| 36 | Sessions | Quests turned in + rewards | v0.3 | addon event log | `QUEST_TURNED_IN(questID, xp, money)`, `C_QuestLog.GetTitleForQuestID` | **yes** | |
| 37 | Sessions | Consumed / vendor sold | v0.3 | addon event log | bag diff (35) while `MERCHANT_SHOW` is open = sold; outside = used | **partial** | Inferred, not an API: "used" can't tell consumed from destroyed. Fine for a recap table. |
| 38 | Sessions | User note on a session | v0.3 | app | SQLite | **yes** | App-only. |
| 39 | AH | Price history per item | v0.4 | **Auctionator SV** and/or our own scans | `AUCTIONATOR_PRICE_DATABASE` (Forever edition), or `C_AuctionHouse.SendSearchQuery` per item while the AH is open | **partial** | Auctionator has a Forever edition ([v339, 2026-09-21](https://woweternity.com/addons/auctionator)), and its price DB is the cheapest source; the app keeps the history. Full scans (`ReplicateItems`) are **throttled** (15 min account-wide; reported to return an empty market after a long wait), so don't build on full scans. Blizzard publishes **no** Forever AH API. **Fallback:** our addon prices the user's own items with per-item searches when the AH is open. |
| 40 | AH | Lowest buyout + median | v0.4 | as 39 | `C_AuctionHouse.GetCommoditySearchResultInfo` / `GetItemSearchResultInfo` | **partial** | Same as 39. Median needs repeated observations, which the app accumulates. |
| 41 | AH | Last scan freshness + who scanned | v0.4 | our addon / Auctionator SV | scan time + character in the SV | **yes** | |
| 42 | AH | Watchlist with sparkline | v0.4 | app history of 39 | – | **partial** | Gaps between visits; the sparkline should show them. |
| 43 | AH | Sales ledger | v0.4 | addon API at the mailbox | `GetInboxInvoiceInfo` (sold/bought, buyer, bid, buyout, deposit, cut); `C_AuctionHouse.GetOwnedAuctions` | **partial** | Invoices are only readable from mail the player opens (before it's looted). **Fallback:** record invoices on every `MAIL_INBOX_UPDATE`, dedupe by sender+subject+time. |
| 44 | AH | Worth selling across alts | v0.4 | app (13–15 × 39) | – | **partial** | Same as 27. |
| 45 | Addons | Installed addons, versions, enabled | v0.5 | local files (+ addon API) | `Interface/AddOns/*/*.toc`; `WTF/.../AddOns.txt` per char | **yes** | No addon needed. Note that addons may ship a `_Camelot.toc` (or similar) Forever-specific TOC alongside the main one: read that first. |
| 46 | Addons | Out of date vs interface 16001 | v0.5 | local files | TOC `## Interface:` (may list several) | **partial** | The number alone misleads: many Retail addons check `>= 100000` and take Classic code paths on 16001. **Fallback:** "Interface lists 16001" = OK; otherwise "Not marked for Forever" (amber, not red). |
| 47 | Addons | Updates from CurseForge / Wago / GitHub | v0.5 | web APIs (optional keys) | CurseForge, Wago Addons, GitHub Releases | **partial** | No addon site has a Forever game flavour yet, and "Forever" in titles is author-chosen. **Fallback:** match by project id + check the files' TOC for 16001; GitHub Releases works for anything hosted there. |
| 48 | Macros | List/edit macros | v0.6 | WTF files | `WTF/Account/<A>/macros-cache.txt`, `.../<Char>/macros-cache.txt` | **yes** | Edits need the guarded write path (T6), with WoW closed. |
| 49 | WeakAuras | ~~Installed auras + import library~~ | **dropped** | – | – | **no** | **Dropped from the roadmap** (Julia, 2026-10-04). WeakAuras does not support Forever: the team stopped at Midnight's restrictions, which Forever carries ([Icy Veins](https://www.icy-veins.com/wow/news/weakauras-to-end-support-in-midnight/), [PCGamesN](https://www.pcgamesn.com/world-of-warcraft/midnight-weakauras-update)). |
| 50 | WeakAuras | ~~Wago.io update check~~ | **dropped** | – | – | **no** | Dropped with 49, along with the Wago.io key. |

## Roadmap flags

1. **Drop WeakAuras (rows 49–50) and the Wago.io key.** *Decided: dropped.* WeakAuras won't run on Forever. Remove the WeakAuras sketch and the Wago.io row from Settings → Integrations. The Wago **Addons** key (row 47) stays.
2. **The restore caveat in round 3 (#5) is out of date.** The "client doesn't load SavedVariables" bug is fixed in build 70009. Drop the caveat, or keep it only for clients older than 70009 (we know the version from `.build.info`).
3. **The live "WoW is running · Thrandor" (row 3) can't be live.** Design for "WoW is running" + "last played Thrandor", updated at each logout or `/reload`. The v0.1 mock already does this without the addon.
4. **Sessions arrive earlier than planned (row 29).** Game sessions (start, end, duration) come from the T6 process watcher, so v0.1 can show "Last session: 2h 14m, ended 23:40" with no addon. Suggest pulling a minimal session list into v0.1/v0.2.
5. **No combat details in recaps (row 34).** Killer names and damage won't exist. The timeline shows the death, place and time; repair costs come from money deltas.
6. **AH is "own scans + Auctionator", not full scans (rows 39–44).** No public API and throttled full scans mean price coverage grows with play. The AH screen's freshness banner (41) is essential, not decoration. Reading Auctionator's SV is cheap and should come first in v0.4.
7. **Item icons need an external source (row 20).** Plan letter tiles for v0.2 and the Battle.net item-media upgrade later; that depends on Blizzard publishing a Forever namespace (also row 24).
8. **The companion addon must target the Retail API with `pcall`-wrapped event registration**, and save one top-level table per the sv contract (`ForeverBuddyDB`). Prefer `SavedVariablesPerCharacter` for per-character snapshots, so playing two characters in one game session can't overwrite one with the other.

## Probe addon

`tools/probe-addon/ForeverBuddyProbe/` checks every *yes* and *partial* row on the live client. It records, per call: missing, error, empty, secret, or the real values. Data lands in `ForeverBuddyProbeDB`, one table keyed by character. It also records whether the client read the file back on the next launch (fact 3) and which events the client refused to register (fact 6).

**For Julia, once (about 15 minutes of play):**

1. Copy the `ForeverBuddyProbe` folder into `<WoW root>\_classic_beta_\Interface\AddOns\`.
2. Start WoW, make sure "ForeverBuddy Probe" is enabled at character select, and log in.
3. After about 10 seconds, type `/fbprobe`. It prints a one-line summary.
4. Play normally for a bit: kill and loot something, turn in a quest, visit a vendor (repair if you can), open your **bank**, open your **mailbox**, and open the **auction house**. At the AH, type `/fbprobe search`, wait a few seconds, then `/fbprobe scan` and leave the AH open for a minute. If nothing happens, that's a result too. **Heads-up:** the full-scan throttle is account-wide, so Auctionator's own full scan won't work for about 15 minutes afterwards. Skip `/fbprobe scan` if you need Auctionator right then.
5. Log out normally (not "Exit Now"). Then launch once more, log in, and log out again. The second launch tells us whether the client reads the file back.
6. Send the file `<WoW root>\_classic_beta_\WTF\Account\<ACCOUNT>\SavedVariables\ForeverBuddyProbe.lua`.

**What it does to your game:** nothing. It never uses items, touches mail, spends money, bids, posts or chats. It does send these read-only requests to the server:

- at login: `RequestTimePlayed` (you'll see the usual "Total time played" lines in chat) and `RequestRaidInfo`;
- at a mailbox: `CheckInbox`;
- at the auction house: `QueryOwnedAuctions`;
- only when you type them: `/fbprobe search` (one item search for Linen Cloth) and `/fbprobe scan` (one full scan).

**What's in the file:** your own characters' data (names, gear, gold, bags, professions). Anything that could hold *other* players' names or text is stored as shape only, i.e. numbers and booleans plus `<string:LENGTH>` in place of text: mail headers and invoices, loot, death, auction listings and every event sample. So no mail senders, subjects, auction owners or chat text reach the file. It's safe to delete afterwards.
