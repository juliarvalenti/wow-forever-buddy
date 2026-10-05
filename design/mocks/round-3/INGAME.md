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
  - **Equippable items:** the compact line can instead say "Upgrade for Kaelor (+9 item level, once level 58)". This uses item level only, never stats we can't read.
  - **Add nothing** when only the current character has the item, or nobody does. Never show an empty head.
  - **Stale data:** a count older than 7 days turns grey, with "(as of 21 Sep)". A price older than 7 days adds "· scan 12 days ago".
  - The current character's bags come live from the game. Other alts come from the Bridge's item-index slot, which holds ids and counts only. Names come from the game's cache.
  - Never on unit tooltips, and nothing extra in combat.
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

## Sources
- forever-addon-kit (ReloadUI protected, secure snippets fixed in 70009, Edit Mode present): https://github.com/Thunderz96/forever-addon-kit
- Forever runs Mainline UI architecture, modern HUD with a Classic look option, Classic UI reskin addons: https://wowforevergame.wiki/classic-plus/wow-forever-ui-guide/ , https://wowforevergame.wiki/classic-plus/wow-forever-addons-guide/
- Addon compartment via TOC fields, combat queue on PLAYER_REGEN_ENABLED, Interface 16001 and _Camelot.toc: https://wowforeverguides.com/addons/code-templates , https://wowforeverguides.com/addons/developers
- Midnight secret values (what addons can and can't do in combat): https://news.blizzard.com/en-us/article/24246290/combat-philosophy-and-addon-disarmament-in-midnight
- ReloadUI needs a hardware event: https://wowwiki-archive.fandom.com/wiki/API_ReloadUI
