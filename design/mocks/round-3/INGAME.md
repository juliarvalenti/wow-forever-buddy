# In-game UI guide (the ForeverBuddy addon)

The app has its own look (D: stone, parchment, ember). **The addon doesn't.** Inside WoW, our frames should look as if Blizzard made them, in whatever skin the player runs. Mocks: `ingame.html` (`?skin=modern|classic`, `?state=…`). Research notes and sources are at the bottom.

## 1. What we're designing for
- **The UI underneath is retail.** Forever runs Mainline's UI architecture (the Midnight-era API, Interface 16001). Its default HUD is a modern, customisable one, and many players add a "Classic UI" addon (Classic UI (Forever), Forevermore, ClassicUiForever) that reskins Blizzard's frames.
- **So we build only from Blizzard templates and fonts.** Those reskins restyle the templates, and our frame follows them for free. A frame with art of our own would look wrong in one skin or the other.
- **Combat is a black box.** Secret values hide combat state, and protected actions can't run in combat lockdown. Our frames show out-of-combat facts (lockouts, mail, cooldown readiness as of the last sync), so they never need combat data.
- **`ReloadUI()` is protected on Forever** (forever-addon-kit). An addon button calling it won't work. Typing `/reload` does. A secure button running the macro text `/reload` might; probe run 4 decides. Every design has a typed fallback.
- **SavedVariables are read back since build 70009**, so frame position and the "seen" state can persist.

## 2. Building blocks (and nothing else)
| Need | Use | Not |
|---|---|---|
| Window | `ButtonFrameTemplate` (portrait off) or `DefaultPanelFlatTemplate`, with a close button and title | Custom backdrops, our textures, parchment |
| Title | the template's title, which uses `GameFontNormal` (gold in both skins) | Our fonts, Marcellus |
| Section heads | `GameFontNormalSmall` (gold) | Bold or all caps |
| Body text | `GameFontHighlightSmall` (white), with `GameFontDisableSmall` for done or muted items | Coloured text other than class colours |
| Character names | class colour from `C_ClassColor.GetClassColor(classFile)` | Our hex palette |
| Checkboxes | `UICheckButtonTemplate`, display only (see §4) | Custom tick art |
| Buttons | `UIPanelButtonTemplate` (red, gold text), at most one in a frame | More than one button row |
| Tooltips | `GameTooltip` | A custom tooltip |
| Icons | real texture ids are fine in-game (`C_Item.GetItemIconByID`), unlike in the app | Letter tiles |

Sizes: a small window, about 300 px wide, 6 to 12 rows. It never covers the centre of the screen. By default it's anchored top right, below the minimap, and it can be moved (drag the title) with its position saved.

## 3. Entry points
- **`/fb`** toggles the window, and **`/fb sync`** syncs (or prints how, see §5). Use no other commands.
- **The addon compartment** (the minimap's addon button) gets one entry, "Forever Buddy", through the TOC fields `AddonCompartmentFunc` / `…OnEnter` / `…OnLeave`. That needs no LibDataBroker and no minimap button of our own.
- **One chat line at login**, and only when the app delivered something new since the last login: "Forever Buddy: this week's checklist is ready. /fb to open." Use the gold `|cffffd100` for "Forever Buddy:" and nothing more. Never repeat it in a session.
- **Escape closes it** (add it to `UISpecialFrames`). It never opens by itself.

## 4. Behaviour
- **The app owns the data, so the game shows it.** Checkboxes reflect the app's facts (saved this week, letters collected). They aren't clickable to-dos, because a click can't get back to the app until the next sync, and a box that unticks itself would lie. If players want manual ticks later, those become our own SV state, clearly separate.
- **Freshness:** the footer shows when the app wrote the data: "From Forever Buddy · 21:04". Past 24 hours, it adds "· may be out of date".
- **Combat:**
  - The frame stays visible if open, and nothing in it changes in combat.
  - The Sync button is disabled while `InCombatLockdown()`, with the tooltip "Not in combat".
  - Any frame changes that touch protected state are queued for `PLAYER_REGEN_ENABLED`.
- **Text safety:** everything shown comes from our Data slots, which are data-only Lua. Escape `|` as `||` before setting text, so no string can inject colour codes, textures or hyperlinks. Show strings with `SetText`, never as code. Cap rows (12) and line length (truncate with `…`).

## 5. Sync
The route follows probe run 4, in the order of the Bridge spec (`docs/specs/bridge-v0.4.md` §4). The look is the same for both button routes.
- **(A) A plain button calling `ReloadUI()`,** if the probe shows it reloads. This is preferred, since there's no macro text at all.
- **(B) A secure button:** `SecureActionButtonTemplate` + `UIPanelButtonTemplate`, with `type = "macro"` and a constant `macrotext = "/reload"`.
- **On both routes:** the label is "Sync", with the tooltip "Reloads your UI so Forever Buddy and the game swap the latest notes. Not in combat." It's disabled in combat.
- **(C) Typed:** there's no button. The footer says "Type /reload to sync". `/fb sync` prints the same line to chat, and so does route B, since a secure button can't be clicked from Lua.
- **If `/reload` doesn't re-read a changed slot** (probe run 4 checks this), Sync can't deliver mid-session. The footer then reads "Updates arrive at your next login", and the app's how-to line says "Your characters will see it at their next login."
- **Either way,** the app's copy follows the same choice (bridge.html: "press Sync…" or "type /reload…"). A new data slot needs an addon update **and a WoW restart once**. The app says so, and the frame says nothing (it can't know).

## 6. Copy rules (same voice as the app)
- Sentence case, no em dashes, no all caps.
- Short facts, with times as the game shows them ("resets Tue", "in 2 days", "ready").
- Class-coloured names, plain everything else. Never name a loot source (IMPLEMENTING §7).
- Refer to characters by name, never "he" or "she". We don't know a character's gender, and a character's sex isn't the player's.
- Empty: "Nothing to do this week." Never seen: "Open Forever Buddy on your PC to fill this in."

## 7. The quest plan: tracker and waypoints (D2, `ingame-d2.html`)
The content waits on Q-SPIKE (what quest data we can trust). The shape doesn't.
- **Tracker:**
  - A compact list in the objective tracker's own style: gold stop titles, white detail lines, grey done stops.
  - It sits in its own small movable frame, headed "Tonight's plan" with the zone in the meta. It's never injected into Blizzard's tracker, and is registered with Edit Mode if the API allows.
  - Stops are numbered in plan order, and the current one gets a gold left edge.
  - Footer: "From Forever Buddy · 21:04 · 2 of 5 done".
  - Shown with `/fb plan` or from the compartment menu. Like the checklist, it never opens by itself.
- **Waypoints:**
  - Each stop has a small "→". A click sets the game's own waypoint (`C_Map.SetUserWaypoint`, one at a time), and the game draws the arrow and distance.
  - If TomTom is installed, we hand the waypoint to TomTom instead.
  - We never super-track for the player: that call is protected on Forever. The tooltip says "Click the map pin to track it."
  - Stops outside the current zone keep their place, with "→" off and the tooltip "Go to Eastern Plaguelands first."
- **World map:**
  - Numbered gold pins for the plan, through a map data provider, as Questie does. The current stop glows, and done stops turn grey.
  - Pin tooltip: "3 · Stratholme: Ysida Harmon", then "Tonight's plan · Forever Buddy", then "Click to set a waypoint".
- **With Questie:** Questie keeps its own givers and objectives. We add only a small number beside its icon, never a second icon.
- **When a plan arrives:** one chat line at login: "Forever Buddy: tonight's plan is ready, 5 stops in Eastern Plaguelands. /fb plan to show it." It's part of the B1 briefing (§9), not a separate line.
- **Live tick-off (P1).** A step's `kind` decides what completes it:
  - `accept`: `QUEST_ACCEPTED` for its quest id.
  - `turn_in`: `QUEST_TURNED_IN` for its quest id.
  - `objective`: the quest is complete in the log (`C_QuestLog.IsComplete` on `QUEST_LOG_UPDATE`).
  - With no kind and a quest id, it ticks on hand-in.
  - A step with **no quest id** never ticks by itself. Clicking its number marks it done or undone. This is the one manual tick allowed, because plan progress is ours: it's saved per character and goes back to the app at logout.
  - **On a tick,** the step goes grey, the next unfinished step gets the gold edge, the footer count updates, and the step's "→" waypoint is cleared if it was the active one. There's no sound and no chat line.
  - **Ticks are never undone by the game.** If a quest is abandoned, its accept step stays done. The current step is always the first unfinished one.
  - **When all steps are done,** the footer reads "All 5 done · from Forever Buddy" in green, and one chat line follows: "Forever Buddy: tonight's plan is done." It stays visible until the next plan replaces it or `/fb plan` closes it.
  - **Waypoints use our own recorded positions**, which the app fills from Q1b's accepts and hand-ins. A step with no recorded position has its "→" off, with the tooltip "No position recorded for this quest yet."

## 8. v0.4 proposals (mocked in `ingame.html`, not yet specced)
- **Alt-aware tooltip (D2, `ingame-d2.html`):**
  - Appended to the game's own item tooltip with `TooltipDataProcessor.AddTooltipPostCall(Enum.TooltipDataType.Item, …)`, read-only and `pcall`-guarded. Use no tooltip of our own.
  - **Compact, the default:** two lines after a gap.
    - First: "Your alts: Coinpurse 340 bank · Velyra 60 bank", with the line in gold and names in class colour. Show at most three names, then "+3 more". Each alt's place is its largest bucket (bank, bags, mail or worn). The current character is never in this line.
    - Second: "~1g 12s each at your last scan · plan needs 20". Each part only when known. Use "~", not "≈": the game's fonts likely lack that glyph.
    - Then a small blue-grey "Shift for details".
  - **Shift:** a gold "Forever Buddy" head, then the current character first ("Thrandor · 40 · on you", the live count), then one row per alt ("Coinpurse · 340 · bank, 2 Oct"), "All characters 440", "Last scan ~1g 12s each · 3 days ago", and "Blacksmithing plan needs 20 more". Show at most eight rows, then "+2 more".
  - **Add nothing** when only the current character has the item, nobody does, and there's no upgrade hint (v2 below). Never show an empty head.
  - The current character's bags come live from the game. Other alts come from the Bridge's item-index slot, which holds ids and counts only. Names come from the game's cache.
  - Never on unit tooltips, and nothing extra in combat.
- **Tooltips v2 (TIP2):** three additions to the shipped tooltip (#96). The exact strings below are the harness targets. `G` is our grey, `|cff808080`, closed with `|r`. Names keep the existing class-colour codes, shown here as plain names.
  - **(a) Stale greying.** "Stale" means older than 7 days, measured with `GetServerTime()`.
    - **Which date counts:** each place has its own date. Bags and worn use `alts[i].seen`, bank uses `alts[i].bank`, and mail uses `alts[i].mail`. All of these are already in the index.
    - **Compact line:** an alt whose main place is stale is shown whole in grey, name included, followed by the date: `Your alts: Coinpurse 340 bank · G[Evil 3 mail (as of 21 Sep)]`. The date is `"%d %b"` with no leading zero ("5 Oct", not "05 Oct").
    - **Compact price:** a scan older than 7 days gets a grey suffix: `~1g 12s each at your last scan G[· 12 days ago]`.
    - **Shift rows:** for a stale alt, the whole right side is grey: `Evil | G[3 mail · 15 days ago]`. For a stale scan, the right side of the Last scan row is grey: `Last scan | G[~1g 12s each · 12 days ago]`.
    - Nothing else changes. Fresh data reads exactly as now.
  - **(b) Upgrade hint.** It says which of your *other* characters the hovered item would be an upgrade for, by item level only.
    - **Data (PM-approved):** each `alts` entry gains `["level"] = 27` and `["worn"] = { 21, 0, 18, … }`: that character's equipped base item level in all 19 inventory slots, in slot order, with 0 for an empty slot. These are numbers only. v1 reads only the slots below, and the rest are there so weapons can follow without changing the index.
    - **What the game gives us:** the item's `equipLoc`, `ilvl`, required level, classID and subclassID, from `C_Item.GetItemInfo` (already cached while its tooltip shows).
    - **Slots compared:** head 1, neck 2, shoulder 3, chest and robe 5, waist 6, legs 7, feet 8, wrist 9, hands 10, back 15. Finger compares with the lower of 11 and 12, and trinket with the lower of 13 and 14. An empty slot counts as 0.
    - **Not in v2:** weapons, shields, off-hands, ranged, relics, shirts and tabards. These need proficiency rules we'd get wrong.
    - **Armour type:** the alt's class must be able to wear it.
      - Cloth: everyone.
      - Leather: everyone except mage, priest and warlock.
      - Mail: warrior and paladin always. Hunter and shaman only if `level >= 40` or the item requires 40 or more.
      - Plate: warrior and paladin only if `level >= 40` or the item requires 40 or more.
      - Jewellery and back items: any class.
    - **Bound items:** skip the hint if the item is bound or binds on pickup, by matching the tooltip's own lines against the game's `ITEM_SOULBOUND` and `ITEM_BIND_ON_PICKUP`. A soulbound item can't reach another character.
    - **Gain:** the item's base ilvl minus the compared slot's ilvl. Only show a gain of **+5 or more**, to avoid noise on sidegrades. Show the top two, best first, with ties going to the higher-level character.
    - **Compact, own line, after the price line:**
      - One character: `Upgrade for Kaelor (+9 item level)`.
      - Under the required level: `Upgrade for Kaelor (+9 item level, once level 58)`.
      - Two characters: `Upgrade for Kaelor (+9 item level) · Sela (+6)`.
      - "Upgrade for" is gold, names are in class colour, and the rest is white. ", once level 58" is grey.
    - **When the line shows:** it can appear on its own, when no alt holds the item. That's the useful case at vendors, the AH and loot. Then the compact view is a gap, the upgrade line, and "Shift for details" (only if there's a Shift view to see).
    - **Shift:** a row `Upgrade for | Kaelor +9 · Sela +6`. Grey "(level 58)" goes after a name that's under the required level.
    - **No hint when:** the character you're on is the best fit (the game's own comparison covers that), the item is bound, its slot is out of scope, the class can't wear it, or no gain reaches +5.
  - **(c) Shopping-list need.** Built only once shopping lists exist (goals and lists ticket). Until then, never show it.
    - **Compact:** appended to the price line: `~1g 12s each at your last scan · your list needs 20`. With no price, the line is just `Your list needs 20`. The count is white and "your list needs" is gold.
    - **Shift:** a row `Shopping list | 20 more (Blacksmithing 300)`, naming the list.
    - **Data:** a future `lists` slot, `item id → { need, listName }`. It's not part of TIP2's index.
- **Session coach and session card:** now specced as S2 in §11.

## 9. B1: login briefing (`ingame-errands.html`)
One chat line at login, once per login, only when there is something to say. It replaces the §3 checklist line, so there's never more than one Forever Buddy line at login.
- **Format:** `|cffffd100Forever Buddy:|r` followed by facts joined with " · ", in this order, each only when true:
  1. "3 quests ready to hand in" (the game's own quest log, live).
  2. "repair due (24%)" (the lowest durability under 30%, live).
  3. "Sela has 2 letters waiting" (from the app, for another alt, mail expiring within 3 days first).
  4. "1 errand at the mailbox" (B2, when this character holds goods for another's list).
  5. "tonight's plan is ready" (P1, when a new plan has arrived).
- At most **4 facts**. Past that, end with "and more: /fb brief".
- **Notes:** a note for this character, written in the app (by Julia, or proposed by Claude and approved), goes on a second line: `|cffffd100Note:|r "Hand in the Onyxia attunement before raid on Thursday."` One note per login, the newest first, with the text escaped (`||`).
- **Nothing to say:** no line at all.
- **`/fb brief`** repeats the last briefing at any time. A "Login briefing" toggle in the compartment menu turns it off, saved per account.
- Never in combat (login isn't), never about other players, and never a popup.

## 10. B2: shopping list and alt errands (`ingame-errands.html`)
Display only. The addon never buys, attaches or sends. **One Bridge slot ("Lists")** carries the lists, their needs and the errands. Holdings come from the existing tooltip index, and prices from the last scan.
- **The list panel at vendors and the AH:**
  - A small `DefaultPanelFlatTemplate` panel docks to the right of `MerchantFrame` or `AuctionHouseFrame` while it's open. It's headed "Your list" with the list name in the meta. With several lists, show the ones with items at this place, then the rest under a "+N lists" line.
  - **Rows:** the item name, then "need 6" on the right, then a grey line saying where your characters stand: "Sela has 4 in bank, still 2 short", "Coinpurse 340 bank · ~1g 12s at last scan", or "your alts have 0".
  - **"· here":** in gold after an item this vendor sells, or that shows in the current AH results.
  - **Done items** are grey with "done" once your characters together hold enough.
  - **Footer:** "From Forever Buddy · 21:04 · /fb list".
- **Highlighting:**
  - Merchant item buttons on a list get a 1.5px gold glow and a small "list" tag.
  - AH result rows that match get a gold left edge.
  - At the AH, a row priced under your last scan reads "first two rows are under it" in the panel's grey line. We never say "buy".
  - Highlighting reads the frames' own item ids. It never clicks and never hooks a buy button.
- **Alt errands at the mailbox:**
  - When the current character holds goods that another character's list needs, an "Errands" panel docks beside `MailFrame`. It's headed "Errands" with "from Coinpurse" in the meta.
  - **Rows:** "Thorium Bar ×20 to Kaelor" (the name in class colour) and a grey line "you have 34 in bags · Kaelor's Blacksmithing list". Use the character's name, never a pronoun: we don't know a character's gender.
  - **Fill recipient:** one `UIPanelButtonTemplate` button that only sets the Send tab's To field (`SendMailNameEditBox:SetText`). Its tooltip reads: "Types "Kaelor" in the To field. Attach the Thorium Bars yourself, then press Send."
  - **Goods in the bank:** if they're in this character's bank rather than bags, the button is disabled, with the grey line "340 in your bank · visit the bank first".
  - **Footer:** "Nothing is attached or sent for you."
  - **On the receiving alt,** the list line reads "Coinpurse can send 20" instead of "need 20".
- **Freshness:** the same rules as §8. An alt's holdings older than 7 days are grey with "(as of 21 Sep)".
- **Commands:** `/fb list` toggles the list panel anywhere (undocked, top right). `/fb errands` shows errands away from a mailbox, as read-only text.

## 11. S2: session coach and session card (`ingame.html`, the coach and logout tiles)
Addon only, from live events; no Bridge slot. Both read what the addon already records for the Adventure entry, so the numbers match the app's Adventures. Display only.
- **A session** starts at login (`PLAYER_ENTERING_WORLD` with `isInitialLogin`) and survives `/reload`: keep the start time and running totals in the per-character saved table, and continue them when `isReloadingUi` is true.
- **Session coach (a strip while you play):**
  - **Off by default.** `/fb coach` toggles it, and the compartment menu has a "Session coach" checkbox. Saved per account.
  - A 210px `DefaultPanelFlatTemplate` strip at top right, headed "This session" (`GameFontNormal`), with the session length in the meta ("1h 42m", "12 min").
  - **Rows** (label gold, value white, right-aligned), each shown only when it has something true to say:
    - **Gold:** "+312g · 184g/hr". A loss reads "-45g" (white, not red). The rate appears after 10 minutes; before that, only the total.
    - **Experience:** "87,000/hr", only while levelling and after 10 minutes.
    - **Level 60 in:** "~41 min", only while levelling, after 10 minutes, and under 10 hours (otherwise hide the row; never "~38 h").
    - **Loot:** "47 items · ~96g". The worth uses the last scan from the tooltip index, counting only priced items; with no prices it's just "47 items".
  - **Updates:** at most every 5 seconds, from `PLAYER_MONEY`, `PLAYER_XP_UPDATE` and our bag diff. **Nothing updates in combat**, so the strip freezes and catches up on `PLAYER_REGEN_ENABLED`. A secret or missing value hides its row rather than showing 0.
  - **Moving:** shift-drag to move, with the position saved per account. Right-click shows "Hide" and "Hide in combat" (off by default).
- **Session card (at logout):**
  - Shown on `PLAYER_CAMPING` (the logout or quit countdown), top centre above the game's countdown dialog, never covering its Cancel. It hides on `LOGOUT_CANCEL` and when the countdown ends. A × closes it. It's never shown on `/reload` or a disconnect.
  - **Title:** the character's name and the part of the day by the local clock at logout: "Thrandor's morning" (5 to 12), "afternoon" (12 to 17), "evening" (17 to 22), "night" (otherwise). Always the name, never a pronoun.
  - **Rows**, each only when true, in this order:
    - "Ding! Level 60" in gold if the character levelled; "Ding! Levels 58 to 60" for more than one level.
    - **Played:** the session length ("3h 12m").
    - **Gold:** "+312g" in green, or "-45g" in white.
    - **Best find:** the highest-quality item gained this session (uncommon or better, ties broken by last-scan worth), as its name in quality colour. With none, no row.
    - **Quests:** the number turned in. With 0, no row.
  - **Too short to say anything** (under 5 minutes and no rows besides Played): no card at all.
  - **Footer:** "In Adventures after you close WoW" (one line at 270px). It's honest: the app picks the session up from saved variables after the game exits.
  - **On by default,** because it only appears while you're already leaving. `/fb card` toggles it, and there's a "Session card at logout" checkbox in the compartment menu.
- **Never:** comparisons with other players, damage or kill meters, advice ("you should"), a sound, or anything that delays or blocks logging out.

## 12. C1: crafting across alts (tooltip lines)
Two additions to the shipped item tooltip (§8), using the same rules: `TooltipDataProcessor` post-call, `pcall`, nothing in combat, other characters only, names in class colour, max 3 names in compact view then "+N more", and `G` = grey `|cff808080`. The strings below are the harness targets. Materials across alts is C2, in (c).
- **Data (what the tooltip needs; the C1 data plan decides how):** per character, the known recipes as `result item id → { profession, skill }`, plus the profession's skill and max. Also, for the learn line, `recipe item id → { profession, required skill }`. Ids and numbers only, in the tooltip index. A character's recipes are as of their last trade-skill window scan, so the stale rule from §8 (a) applies, with the date of that scan.
- **(a) Can make** (hovering an item another character can craft):
  - **Compact, own line, after the price line:** `Sela can make this` · two: `Sela and Kaelor can make this` · more: `Sela, Kaelor and Velyra can make this` · past three: `Sela, Kaelor, Velyra +2 can make this`. "can make this" is gold.
  - **Shift:** one row per character, `Sela | Tailoring 285`. Stale (that profession's last scan older than 7 days): the right side grey with ` · as of 21 Sep`. *Later, once cooldowns are recorded:* add ` · ready` or ` · ready Tue` (the game's day).
  - Not shown when only the current character can make it (the game's own profession UI covers that).
- **(b) Recipe items: C1b, not in C1.** No reliable API maps a pattern item to its recipe, so C1 ships without these lines, and a logout probe checks whether a real mapping exists. If it does, build these strings as written:
  - **Knows:** `Sela knows this` (several names as in (a)). Gold verb, class-coloured names.
  - **Could learn:** `Kaelor could learn this (Tailoring 280 of 300)`, for a character who has the profession, doesn't know the recipe and whose skill is at or above the required skill. The bracket is grey.
  - **Not yet:** `Kaelor could learn this at Tailoring 290 (now 280)`, only when within 25 points. Further away, say nothing.
  - Both kinds can show, knows first: `Sela knows this` then `Kaelor could learn this (Tailoring 280 of 300)`, at most two lines.
  - **Shift:** rows `Sela | knows · Tailoring 300`, `Kaelor | could learn · 280 of 300`.
  - **If the recipe item can't be mapped to a recipe reliably,** drop the learn lines entirely. Never guess from the name.
- **(c) Materials across your characters (C2).** Only on an item that (a) shows, so some other character can make it.
  - **Data:** per known recipe, its reagents as `{ item id, count }` for one craft (from `GetRecipeSchematic`), carried with C1's recipe data. Holdings come from the existing index, plus the current character's bags live.
  - **What counts:** for each reagent, everything your characters hold together (bags, bank, mail, any character, including the one you're on). A reagent is "covered" when that total reaches the count for one craft. Crafted items made by different characters may have different recipes; use the first maker's recipe (the order of (a)).
  - **Compact:** appended to the can-make line after " · ": `Sela can make this · materials 4 of 6`. When all are covered: `Sela can make this · all materials on hand`. "materials" is gold, and the numbers are white. "all materials on hand" is a light green (`|cff40ff40`), not the uncommon-quality green, so it doesn't read as an item colour.
  - **Shift:** after the maker rows, a gold sub-head "Materials for one", then one row per reagent: `Mooncloth | 2 of 2 · Sela bags` (the character with the most, and their place, as in §8). Short (missing or partly held): the whole row grey, `Rune Thread | 0 of 1`, `Runecloth | 3 of 5 · Coinpurse bank`. The current character's place reads "on you". At most 8 reagent rows.
  - **Stale holdings:** the §8 (a) rule applies to the place: a holding older than 7 days is grey with its date in Shift. Compact counts it anyway.
  - **Never:** a price for the missing materials, "buy", a shopping-list nudge, or a count of how many crafts you could make (that invites a craft queue). Lists can come later as their own ticket.
- **The current character:** never named as a maker or learner (its materials still count in (c)). The game's own tooltip already says "Already known" or shows the requirement in red.
- **Nothing to say:** no line, no head. These lines can appear alone (no alt holds the item), like the upgrade hint.
- **Never:** "used in…" on reagents, other players or guild crafters, prices beyond the existing scan line, or anything that opens the profession window or crafts.

## 13. L2: lockouts at the entrance (one chat line)
When you enter a dungeon or raid, one quiet chat line names your *other* characters already saved there this week. It comes from F3's lockouts (`name`, `difficulty`, `reset_at`), which have no boss progress, so never show any.
- **When:** on `PLAYER_ENTERING_WORLD` while `IsInInstance()` is true and the instance type is `party` or `raid`. Once per instance per session, so a ghost run back in, a /reload or zoning back after a wipe stays silent. If you're in combat, wait for `PLAYER_REGEN_ENABLED`.
- **Matching:** the instance name and difficulty from `GetInstanceInfo()` against each alt's lockouts, by name and difficulty. Drop any lockout whose `reset_at` has passed (`GetServerTime()`), since it's already gone.
- **The line** (gold prefix, names in class colour, the reset as the game shows days):
  - One: `|cffffd100Forever Buddy:|r Velyra is saved to Molten Core (resets Tue).`
  - Two: `… Velyra and Kaelor are saved to Molten Core (resets Tue).`
  - Three: `… Velyra, Kaelor and Sela are saved to Molten Core (resets Tue).`
  - Past three: `… Velyra, Kaelor, Sela +2 are saved to Molten Core (resets Tue).`
  - Under a day to reset, use the time instead: `(resets in 5 h)`.
  - With a difficulty that isn't the normal one, name it as the game does: `Molten Core (Heroic)`.
- **Stale:** a lockout is as of that alt's last login. That's fine for a weekly reset, so no date and no grey. Expired lockouts are already dropped.
- **Nothing to say** (no other alt saved, or no lockout data): no line.
- **The current character:** never named. The game's own Raid Info covers the character you're on.
- **Off switch:** a "Lockouts at the entrance" checkbox in the compartment menu, on by default and saved per account.
- **Never:** a popup or sound, other players or your group, anything that suggests leaving or resetting, or advice ("you should").

## 14. B3: bag cleanup (marks in your bags)
You mark items in the app ("sell" or "send to Sela"); the addon shows the marks where you act on them. **Selling and sending stay manual:** no sell button, no auto-sell at a vendor, no attaching. The app side is IMPLEMENTING §18. Data: one Bridge slot ("Cleanup"), per character, `item id → { "sell" }` or `{ "send", altIndex }`. Ids and our own names only. **B3b** adds a `reason`, one of a few fixed codes (`grey`, `outgrown`, `upgrade` with its +N), never free text, and the addon turns it into the words below.
- **In your bags** (Blizzard's own bag frames, combined or separate):
  - A marked item's button gets a small corner tag in the top-left: a coin (`Interface\MoneyFrame\UI-GoldIcon`, 12 px) for sell, or a letter (`Interface\Minimap\Tracking\Mailbox`, 12 px) for send. It's kept in a side table, like B2's merchant mark, and nothing is written onto Blizzard's buttons beyond our own child texture.
  - Bag addons (Baganator, Bagnon and so on) don't get tags in v1. The tooltip line below still works there.
- **Tooltip line** (the §8 post-call, own line, after the price line):
  - Sell: `Marked to sell in Forever Buddy`, gold, then (B3b) the reason in grey when there is one (` · grey`, ` · outgrown`), then ` · 2s 40c each at a vendor` when the game gives a sell price.
  - Send: `Marked to send to Sela`, with "Marked to send to" gold and the name in class colour, then (B3b) ` (+9 item level)` in grey for an upgrade.
  - Send, but the item is soulbound (the tooltip's `ITEM_SOULBOUND` line): `Marked to send to Sela, but it's soulbound`, with the last part grey. Never hide the mark; the app learns at the next sync and drops it.
- **At a vendor:** one grey line under the B2 list panel, or alone in a small docked panel when there's no list: `5 marked to sell · ~1g 20s at the vendor`. No button. The tags in the bags show which. With nothing marked, nothing shows.
- **At the mailbox:** marked sends join the B2 Errands panel as ordinary rows, `Truestrike Shoulders to Kaelor`, with the grey line `in your bags · marked in Forever Buddy` and the same Fill recipient button (it only types the name).
- **When an item is gone** (sold, sent, used), its tag disappears right away. The app clears the mark at the next sync, after it sees the item has left this character.
- **Commands:** `/fb cleanup` prints `Forever Buddy: 5 marked to sell, 2 to send.` or `Forever Buddy: nothing marked on Thrandor.`
- **Never:** a sell or send button, auto-selling greys, deleting items, or marks on another player's items.

## 15. TIP3 (a): weapons in the upgrade hint
Extends §8 (b). Same copy, gain threshold (+5), top two, bound-item skip and "once level N" rule. The data is already there: `worn` covers slots 16 (main hand), 17 (off hand) and 18 (ranged).
- **Who can use it** (by class, what the class can train; we don't know what each alt has trained, so this is "can use", never "is skilled in"):
  - Warrior: every weapon type except wands.
  - Paladin: one- and two-handed axes, maces and swords, and polearms.
  - Hunter: daggers, fist weapons, one- and two-handed axes and swords, polearms, staves, bows, crossbows, guns and thrown.
  - Rogue: daggers, fist weapons, one-handed maces and swords, bows, crossbows, guns and thrown.
  - Shaman: daggers, fist weapons, one- and two-handed axes and maces, and staves.
  - Druid: daggers, fist weapons, one- and two-handed maces, and staves.
  - Priest: daggers, one-handed maces, staves and wands.
  - Mage and warlock: daggers, one-handed swords, staves and wands.
  - Shields: warrior, paladin and shaman. "Held in off hand" items: every class.
  - Dual wield (a one-hand weapon counts for the off hand): warrior, rogue and hunter.
  - These are Classic 1.x rules. Keep them in one table in the addon, so a Forever change is a one-line fix.
- **What it's compared with:**
  - **Two-handed:** the alt's main hand (their two-hander, or a one-hander with nothing in the off hand). When an off hand is worn too, the *average* of main hand and off hand. Which hand holds what comes from each alt's `hands` item ids (main, off, ranged) and `GetItemInfo`'s equip location.
  - **One-hand or main hand:** the alt's main hand. If they wear a two-hander, skip it (we can't judge a one-hander against a two-hander). For dual-wield classes, a "One-Hand" item also compares with the off hand, and the better gain wins.
  - **Shield, off hand, held in off hand:** the alt's off hand. Skip it if they wear a two-hander.
  - **Ranged, wand, thrown:** slot 18, for classes that can use that type.
  - **Still out:** relics (librams, totems, idols), shirts and tabards.
- **Copy:** unchanged in compact view, `Upgrade for Kaelor (+9 item level)`. In Shift, add what it was compared with, in grey: `Upgrade for | Kaelor +9 (over main and off hand)`, `(over the off hand)`. One-slot comparisons need no suffix.
- **No hint** when the class can't use the type, the item is bound, or the comparison is skipped as above. Never guess from the item's name.

## Sources
- forever-addon-kit (ReloadUI protected, secure snippets fixed in 70009, Edit Mode present): https://github.com/Thunderz96/forever-addon-kit
- Forever runs Mainline UI architecture, modern HUD with a Classic look option, Classic UI reskin addons: https://wowforevergame.wiki/classic-plus/wow-forever-ui-guide/ , https://wowforevergame.wiki/classic-plus/wow-forever-addons-guide/
- Addon compartment via TOC fields, combat queue on PLAYER_REGEN_ENABLED, Interface 16001 and _Camelot.toc: https://wowforeverguides.com/addons/code-templates , https://wowforeverguides.com/addons/developers
- Midnight secret values (what addons can and can't do in combat): https://news.blizzard.com/en-us/article/24246290/combat-philosophy-and-addon-disarmament-in-midnight
- ReloadUI needs a hardware event: https://wowwiki-archive.fandom.com/wiki/API_ReloadUI
