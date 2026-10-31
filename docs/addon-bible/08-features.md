## 8. Designing features

How EllesmereUI's smaller features are designed, from the Forever-only ones (Forever Essentials) and the QoL set. These are product lessons more than API ones. The idea to remember: a feature that changes or destroys anything is opt-in, fails closed on data it can't read, and can be undone or switched off from where the player sees it. Paths are relative to the clone. FE = `EllesmereUIForeverEssentials/`, QoL = `EllesmereUIQoL/`.

### Features that act

**8.1 Make anything that destroys or sells items opt-in, and fail closed on data you can't read.**
The junk window can delete items, so it's off by default ("opt-in: this window can delete items"). Its item rules refuse to list anything they can't classify:
- They take the higher of the slot's and the item's quality, and an item that reports neither never lists.
- Quest items, battle pets, and soulbound items with no vendor value (Hearthstone, say) never list. Neither do items in an equipment set.
- Reagent and specialty bags are skipped.
- An item whose data the client says will never load goes into a `failed` set.

Source: `FE/EllesmereUIForeverEssentials_JunkItems.lua:25-36, 126-209, 158-160, 668-673`.
For ForeverBuddy: P4's Merchant module (sell greys, sell marked items) starts from this rule list, plus security's conditions on #201: at most 12 sales per visit, no unknown-bind or quest items, and never bulk-approved.

**8.2 Ask harder the more an action costs.**
White and green stacks need a second Ctrl+click to delete. Rare and better open a dialog where you type DELETE. An option (`skipWhiteConfirm`, off by default) drops the second click for white items only.
Source: `FE/EllesmereUIForeverEssentials_JunkItems.lua:36, 380-381, 418-422, 478-485`.

**8.3 Put an off switch on any popup the player didn't ask for, and get out of the way in combat.**
The group-finder teleport prompt has a "Disable Feature" link on the popup itself, and it hides when combat starts.
Source: `QoL/EllesmereUIQoL_TeleportPrompt.lua:238-246, 488`.

### Reading the game

**8.4 Never pass a value from the game into a secure attribute: map it through your own table.**
The teleport button's spell comes from the addon's own name-to-spell table (`EllesmereUI.SEASON_PORTALS`), never from a group-finder field. Search results are secret while you browse ("activityIDs is a secret table -> indexing throws"). So the dungeon is resolved after joining, every field is checked with `issecretvalue`, and the whole lookup sits in a pcall, "so a secret value can only make us skip the prompt". (This feature is retail-only: the file returns early on Forever.)
Source: `QoL/EllesmereUIQoL_TeleportPrompt.lua:2, 12-19, 293-334`; `QoL/EllesmereUIQoL_Keys.lua:31-46, 69-78`.

**8.5 When a value can't be computed because part of it is secret, return nil and draw a placeholder.**
Forever has no retail ratings, so the stats block shows the character pane's stats. "A sum or a scale is arithmetic, which a secret refuses": those getters return nil, and the row draws "?".
Source: `QoL/EllesmereUIQoL_ForeverStats.lua:5-6, 16-25`; `QoL/EllesmereUIQoL.lua:2459-2461`.

**8.6 Parse chat with the client's own format strings, never English text.**
The loot feed builds its patterns from the GlobalStrings constants, so loot, money, reputation and skill-up lines match in every client language. Currencies don't come from chat at all: they come from `CURRENCY_DISPLAY_UPDATE`. Prefer an event to chat whenever one exists.
Source: `FE/EllesmereUIForeverEssentials_LootFeed.lua:9-10, 66-95, 927-966, 981, 1036`.

**8.7 Hook the outermost function: templates may hold their own reference to the inner one.**
The quest reward marker can't hook `QuestInfo_ShowRewards`, because "its templates hold their own reference" to it. So it hooks `QuestInfo_Display` instead and draws on a texture of its own ("Nothing of Blizzard's is written"). While a reward's item data is still loading, it re-reads on each `GET_ITEM_INFO_RECEIVED`.
Source: `FE/EllesmereUIForeverEssentials_QuestRewards.lua:9-13, 39-64, 88, 96`.

**8.8 Estimate what the client won't tell you, correct the estimate from what you observe, and document the gap.**
There's no flight duration API. The flight timer divides the path length (from the game's TaxiPath data) by a speed, which starts from a constant fitted to Classic times. After each normal landing it moves 25% toward the measured speed, but only if that measurement is plausible (0.75 to 1.33 times the estimate). The file header states the known gap: a reload mid-flight loses the bar until the next takeoff, because saving it "would go stale across a logout or crash".
Source: `FE/EllesmereUIForeverEssentials_FlightTimer.lua:5-15, 23, 547-548, 580-588`.

**8.9 Don't keep a second copy of state the client already stores.**
The macro manager saves only its on switch: "the game's macro text is the macro".
Source: `FE/EllesmereUIForeverEssentials_MacroManager.lua:8-9, 13`.
For ForeverBuddy: the app's macro editing (M2) writes `macros-cache.txt` itself. That's the same principle from outside the game: the client's file is the only copy.

### Settings that change with context

**8.10 Resolve context-dependent settings by a written priority order, and never switch them in combat.**
Exactly one "conditional group" of settings is active at a time. Its priority ladder is written in the file header: an explicit keybind first ("explicit user action outranks ambient state"), then dark mode, then instance type, then party, then solo. "Condition flips NEVER happen in combat: flips are recomputed fresh at PLAYER_REGEN_ENABLED (re-resolve, never replay -- the zone can change twice during a fight)." Keybind presses during combat are ignored, not queued.
Source: `EllesmereUI_Conditions.lua:10-28, 236-239, 324, 370-375`.
For ForeverBuddy: if P4 profiles ever switch by situation (a raid profile, say), copy this ladder and the "re-resolve after combat" rule.

**8.11 Never let a disabled option trap the player.**
A condition that can't currently hold can't be newly checked, but "UNchecking is always allowed so an existing group can never be trapped".
Source: `EllesmereUI_Conditions.lua:55-59`; `EUI_SpecOverrides_Popups.lua:1436`.
For ForeverBuddy: P4's `requires` rule (an option hidden while its parent is off) must still let a player turn off something already on.

**8.12 Store per-context values as deltas over one profile, and show them where they're edited.**
Spec overrides keep one profile. Only the settings you changed while "Editing as" a spec group are stored per spec, and you edit them with the real options widgets ("full fidelity by construction"). Changes are captured by diffing on a ticker and attributed to the options row under the mouse. Each overridden row gets a 1px gold border. Path keys are joined with control characters (`\31`, `\30`), "so keys containing "." can never corrupt a path", and a sentinel marks "key not present".
Source: `EllesmereUI_SpecOverrides.lua:3-40, 132-134`; `EUI_SpecOverrides_Session.lua:99, 137`.
For ForeverBuddy: P4's preset-plus-changes model is the same idea. Show which options differ from the preset in the app and in game.

### Changing what you don't own, and putting it back

**8.13 Record the value before you change a game setting, and on revert restore only values that are still yours.**
EllesmereUI's uninstall puts back CVars, CVar bits and chat window font sizes, and "a setting goes back only while it still holds EllesmereUI's value: one the player changed since is theirs and stays". Settings it only borrows (a muted sound, chat bubbles in an instance) go through hold and release, so a restored value is never mistaken for its own. The rule has gaps, and it's worth knowing them:
- There are several setters, not one: CVars, CVar bits, Edit Mode settings, chat window sizes and keybinds.
- Some changes bypass the recorder on purpose: the Optimize FPS graphics settings keep their own backup.
- A keybind is restored even when the player has since cleared the key.
- A module turned off reverts at login only if a before-value was recorded, which only "fresh" accounts have.

Source: `EllesmereUI_Uninstall.lua:12-13, 33-41, 199, 272, 320, 347, 363-367, 388, 515, 630, 652, 666, 676-689`; `EllesmereUIOptions/EUI__General_Options.lua:1786-1792`.
For ForeverBuddy: this is P4's "Go back" rule. Restore a setting only if it still holds the value we applied; a value the player changed in game since is theirs.

**8.14 Before a module takes over a job, look for the addons already doing it, and say how to resolve it.**
Once a session, the conflict check compares loaded addons against a table of `{ addon, label, targets, message, moduleCheck }`. A conflict fires only when the clashing EllesmereUI module is loaded, plus an optional sub-feature check: Clique conflicts only when Raid Frames is loaded and HoverCast is on. A few entries say exactly what to switch off on which side. Most fall back to a generic "Please disable one of them". The table maps domains to the addons a module replaces:
- **Bags:** AdiBags, ArkInventory, Baganator, Bagnon, BetterBags, Sorted
- **Nameplates:** Plater, Platynator, Kui, TidyPlates, ThreatPlates, Aloft
- **Chat:** Prat, Chatter, Chattynator, Glass
- **Raid frames:** Grid2, DandersFrames, Harrek's frames, Clique
- **Action bars:** Bartender4, Dominos
- **Minimap:** SexyMap, MinimapButtonButton
- **M+ timers:** MythicPlusTimer, MPlusTimer, WarpDeplete
- **Character sheet:** DejaCharacterStats, ChonkyCharacterSheet, BetterCharacterPanel

Source: `EllesmereUI.lua:3330-3516` (table `:3355-3413`, gating `:3449-3464`, default message `:3489-3493`).
For ForeverBuddy: each P4 module needs its own conflict list, with a specific message for each entry. Julia may run Baganator, which a Bags module would clash with.

### First run and hints

**8.15 Onboarding asks one question per screen, shows a mock of each choice, and Escape changes nothing.**
First install shows a one-time module picker at login, before the conflict check. On the next login, a style picker shows each look as a card with a mock: four cards on Forever, three elsewhere. The current look is marked IN USE. Picking it, or pressing Escape, writes no style. Picking another card asks for the reload first.
Source: `EllesmereUI_FirstInstall.lua:13-14`; `EllesmereUI_StyleChoicePopup.lua:9-11, 36-39, 71-77, 92, 105-108`; `EllesmereUI.lua:3520`.

**8.16 Addons can't open a browser, so a link is a selected, read-only text box.**
Video guides show "a pre-selected read-only URL box (the only keystroke a user needs is Ctrl+C)" and one Okay button. A `FireOnce(id)` helper shows a guide once per account. Other one-time hints use their own flags.
Source: `EllesmereUI_VideoGuides.lua:5-7, 14-18, 55-61, 100-107`.

**8.17 Preview a visual setting in the options themselves.**
The flight timer's options page flies a sample route on a loop. The loot feed previews its rows the same way. Two modules do this, not all of them, but for visual options it's the cheapest way to see a change before playing with it.
Source: `FE/EllesmereUIForeverEssentials_FlightTimer.lua:9-10, 18-21`; `FE/EllesmereUIForeverEssentials_LootFeed.lua:13-16`.
For ForeverBuddy: the in-game half of P4's "does it look right" layer.

### Keeping data current

**8.18 Mark data that goes stale every season, and keep explicit exceptions beside numeric rules.**
Auto-logging decides which instances count as current content with a numeric threshold (map ID 959). It then adds explicit lists, marked `SEASON UPDATE:`, because "Instance map IDs are NOT chronological" and the threshold alone "would wrongly exclude" some current raids.
Source: `QoL/EllesmereUIQoL_AutoLogging.lua:10, 12-31`.

**Not for us:** the threat meter's look table (`ns.Look`, one window's chrome, `FE/EllesmereUIForeverEssentials_Window.lua:13-20`) is a neat pattern, but too much for one window. Chapter 5 covers a simpler single source of style.
