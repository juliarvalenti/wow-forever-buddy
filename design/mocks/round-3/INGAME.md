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
- **When a plan arrives:** one chat line at login: "Forever Buddy: tonight's plan is ready, 5 stops in Eastern Plaguelands. /fb plan to show it."

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
- **Session coach:**
  - A small movable strip headed "This session", with the session length in the meta.
  - Rows: Gold (+312g · 184g/hr), Experience (87,000/hr), "Level 60 in ≈ 41 min" (only while levelling), and Loot (47 items · ≈ 96g, the worth only with prices).
  - It counts from login, using only out-of-combat events (`PLAYER_MONEY`, `PLAYER_XP_UPDATE`, our bag diff).
  - It's off by default, toggled with `/fb coach` or the compartment menu.
  - An option hides it in combat, and its position is saved.
- **Session card at logout:**
  - It appears during the logout countdown, above the game's own dialog, and never blocks it.
  - Contents: a gold "Ding! Level 60" if the character levelled, then played time, gold, best find (in quality colour), and quests.
  - Footer: "Saved to your journal in Forever Buddy".
  - It's the same data V9 already turns into the Adventure entry. The card is only the in-game face.

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
  - **Rows:** "Thorium Bar ×20 to Kaelor" (the name in class colour) and a grey line "you have 34 in bags · his Blacksmithing list".
  - **Fill recipient:** one `UIPanelButtonTemplate` button that only sets the Send tab's To field (`SendMailNameEditBox:SetText`). Its tooltip reads: "Types "Kaelor" in the To field. Attach the Thorium Bars yourself, then press Send."
  - **Goods in the bank:** if they're in this character's bank rather than bags, the button is disabled, with the grey line "340 in your bank · visit the bank first".
  - **Footer:** "Nothing is attached or sent for you."
  - **On the receiving alt,** the list line reads "Coinpurse can send 20" instead of "need 20".
- **Freshness:** the same rules as §8. An alt's holdings older than 7 days are grey with "(as of 21 Sep)".
- **Commands:** `/fb list` toggles the list panel anywhere (undocked, top right). `/fb errands` shows errands away from a mailbox, as read-only text.

## Sources
- forever-addon-kit (ReloadUI protected, secure snippets fixed in 70009, Edit Mode present): https://github.com/Thunderz96/forever-addon-kit
- Forever runs Mainline UI architecture, modern HUD with a Classic look option, Classic UI reskin addons: https://wowforevergame.wiki/classic-plus/wow-forever-ui-guide/ , https://wowforevergame.wiki/classic-plus/wow-forever-addons-guide/
- Addon compartment via TOC fields, combat queue on PLAYER_REGEN_ENABLED, Interface 16001 and _Camelot.toc: https://wowforeverguides.com/addons/code-templates , https://wowforeverguides.com/addons/developers
- Midnight secret values (what addons can and can't do in combat): https://news.blizzard.com/en-us/article/24246290/combat-philosophy-and-addon-disarmament-in-midnight
- ReloadUI needs a hardware event: https://wowwiki-archive.fandom.com/wiki/API_ReloadUI
