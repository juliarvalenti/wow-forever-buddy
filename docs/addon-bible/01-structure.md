## 1. Structure and loading

This chapter covers how an addon is laid out on disk and how the client loads it: the TOC file, SavedVariables, the load events, namespaces and Lua 5.1's size limits. The one idea to remember: load order and load timing are part of the design, so decide them in the TOC and the first lines of each file, not with runtime checks scattered through the code.

### The TOC file

**1.1 List every client build you support in `## Interface:` as one comma list.**
EllesmereUI ships one package for retail and Forever. Every TOC except its Forever-only one lists all six retail 12.x builds plus `16001`; the Forever-only `_Camelot.toc` lists `16001` alone. One line per build means no per-client TOC copies and no "out of date" flag on any listed client (our inference).
Source: `EllesmereUI.toc:1`, `EllesmereUIActionBars/EllesmereUIActionBars.toc:1`, `EllesmereUIForeverEssentials/EllesmereUIForeverEssentials_Camelot.toc:1`.
For ForeverBuddy: `ForeverBuddy.toc` says `## Interface: 16001`, which is right for a Forever-only addon. Add each new Forever build to the same line when it ships.

**1.2 Switch a whole addon off for the wrong client with `## AllowLoadGameType:`.**
EllesmereUI marks retail-only children (Friends, Mythic Timer, Group Finder, Loadout Manager) `## AllowLoadGameType: standard` and its Forever-only child `## AllowLoadGameType: camelot`. The client then refuses to load the addon, so no runtime check is needed. Its core keeps a list of these children and says to "Extend the set whenever a TOC gets that line", because a client-disabled addon is missing from every roster.
Source: `EllesmereUIForeverEssentials/EllesmereUIForeverEssentials_Camelot.toc:1-2`, `EllesmereUIFriends/EllesmereUIFriends.toc:2`, `EllesmereUI.lua:794-797`.
For ForeverBuddy: `## AllowLoadGameType: camelot` in `ForeverBuddy.toc` would state our Forever-only scope to the client. Test it on the live client before shipping (our inference that it is harmless there).

**1.3 Exclude single files per client with a TOC tag; when you can't, return on line 2.**
EllesmereUI tags retail-only libraries `[AllowLoadGameType standard]` on their TOC line, so Forever never parses them. Files that must be listed for both clients instead start with `if EllesmereUI.IS_FOREVER then return end` (or the inverse), right after the client gate, so no hooks get installed on the wrong client.
Source: `EllesmereUI.toc:20-21`, `EllesmereUI_SignupHelpers.lua:2`, `EllesmereUIQuestTracker/EllesmereUIQuestTracker_Forever.lua:2`.

**1.4 Use `<Addon>_Camelot.toc` when Forever needs different TOC metadata, and remember old TOCs linger.**
Forever Essentials ships only a `_Camelot.toc`, "which no other client reads". Simple Loot Council ships both, and the Camelot one differs only by `## DefaultState: disabled`. EllesmereUI found that a copy extracted over an older build "keeps its stale plain TOC". That a suffixed TOC wins over a plain one on Forever is our inference from the pair; the repo never says so.
Source: `EllesmereUI_Startup.lua:511-514`, `EllesmereUISimpleLootCouncil/EllesmereUISimpleLootCouncil_Camelot.toc:9`.
For ForeverBuddy: when the companion app installs or updates the addon, it should remove TOC files that the new version no longer ships.

**1.5 Don't list `Bindings.xml` in the TOC, and never rename a shipped binding action.**
WoW loads `Bindings.xml` from the addon root by itself. EllesmereUI notes that listing it too "registers every <Binding> twice"; another of its TOCs reports the symptom as "Unrecognized XML: Binding". Keybinds are stored against the action string, so a rename unbinds every player's key.
Source: `EllesmereUIQuickdraw/EllesmereUIQuickdraw.toc:12-18`, `EllesmereUIActionBars/EllesmereUIActionBars.toc:44-47`.
For ForeverBuddy: applies when we add a "toggle window" keybind.

**1.6 Order the TOC by dependency and comment every position that matters.**
Files run top to bottom in one pass, and later files read what earlier ones defined. EllesmereUI's parent TOC goes gate, libraries, framework, uninstall recorder, locale, number format, migration, startup, then features, with a `#` comment on each early file explaining why it sits there ("MUST load first").
Source: `EllesmereUI.toc:11-43`, `EllesmereUI_Lite.lua:451-453`.
For ForeverBuddy: the `Data/*.lua` slot files must stay above `ForeverBuddy.lua`, which reads them at `ADDON_LOADED`. Say so in a `#` comment in `ForeverBuddy.toc`.

**1.7 If you split into several addons, make each child hard-depend on the parent and group it under the parent.**
All 25 EllesmereUI children declare `## Dependencies: EllesmereUI` and `## Group: EllesmereUI`; none uses `## OptionalDeps`. A hard dependency makes the parent and its SavedVariables load first, rather than relying on folder-name order. `## Group` nests the children under the parent in the AddOn List. Most also set a coloured `## Category:`, though the loot council uses `## Category: Loot`.
Source: `EllesmereUIActionBars/EllesmereUIActionBars.toc:3-4,8`, `EllesmereUISimpleLootCouncil/EllesmereUISimpleLootCouncil.toc:3`, `EllesmereUI_Lite.lua:400-401`.
For ForeverBuddy: if P4 modules (merchant, bags, quest, map) become their own folders, each gets `## Dependencies: ForeverBuddy` and `## Group: ForeverBuddy`.

**1.8 Put a big settings window in a LoadOnDemand addon with no SavedVariables, and have it run its own init.**
EllesmereUI's whole settings UI is a `## LoadOnDemand: 1` addon, so "nothing of it is even PARSED" until the panel first opens. It declares no SavedVariables, so no WTF file is ever executed when it loads late. Because it loads after login, its `PLAYER_LOGIN` never fires, so 25 of its files end with `if IsLoggedIn() then` run the init now.
Source: `EllesmereUIOptions/EllesmereUIOptions.toc:9`, `EllesmereUI.lua:3118-3120`, `EllesmereUI_Lite.lua:401-404`, `EllesmereUIOptions/EUI_ForeverEssentials_Options.lua:132-133`.
```lua
local function init() --[[ build the page ]] end
if IsLoggedIn() then init()
else waitFor("PLAYER_LOGIN", init) end
```
For ForeverBuddy: the 5-tab window is the obvious candidate when `ForeverBuddy.lua` must shrink. Load it with `C_AddOns.LoadAddOn` from `/fb` and the minimap button.

**1.9 Hold back an unfinished addon with `## LoadOnDemand: 1` that nothing loads, and leave it out of the package.**
EllesmereUI's comment: "load on demand, and nothing loads it, so it never loads and its saved data stays untouched. Drop this to ship." Its `.pkgmeta` also ignores those folders. These held-back addons do declare SavedVariables, which is safe only because they never load; the core's "keep it SV-less" comment is stale on this point.
Source: `EllesmereUILoadoutManager/EllesmereUILoadoutManager.toc:10-13`, `EllesmereUI.lua:809-824`, `.pkgmeta`.

### SavedVariables

**1.10 Keep one top-level SavedVariables table per kind of data.**
EllesmereUI keeps all profile settings in the parent's `EllesmereUIDB`. Bulk data that must never travel with profiles gets its own saved table: Bags keeps `EllesmereUIBagsInventoryDB` "so profile exports and sync never carry inventories". It empties each child's old saved global with `wipe`, and its comment says the serializer "holds the original table reference from load time".
That comment is contradicted twice. Their own Reset ALL replaces the global table outright and expects it to save (see 6.1). And `ForeverBuddy.lua` assigns `ForeverBuddyDB = db` at logout (line 4947), and every app ingest since v0.2 has read that new table back. The client saves whatever each declared global holds at logout. Changing a table in place is still the tidier habit, but it isn't required.
Source: `EllesmereUI_Lite.lua:273-280`, `EllesmereUIBags/EllesmereUIBags.toc:9`, `EllesmereUIBags/EllesmereUIBags_Offline.lua:7-8`.
For ForeverBuddy: our split (`ForeverBuddyDB` per character, `ForeverBuddySettings` per account) fits this, and the logout write is correct as it is.

**1.11 Never declare the same SavedVariables name in two TOCs, even briefly.**
WoW runs a child's entire WTF file when it loads, "including variables its TOC no longer declares". A child that once declared the central DB kept a frozen copy; re-enabling it overwrote the real DB, and the next logout saved the stale data. EllesmereUI now captures the owner's table at its own `ADDON_LOADED` and restores it if a later load swaps it (that this was a real incident is our inference).
Source: `EllesmereUI_Lite.lua:387-393`.
For ForeverBuddy: if modules become separate addons, none of them may ever list `ForeverBuddyDB` or `ForeverBuddySettings`.

**1.12 If you strip defaults at logout, strip a copy.**
At `PLAYER_LOGOUT` EllesmereUI runs its pre-logout callbacks, then writes a defaults-stripped copy of the active profile, which "keeps the live db.profile references untouched".
Source: `EllesmereUI_Lite.lua:353-374`.

### Client gate

**1.13 Detect the client once, in the first file, by TOC number, and fail open.**
`EllesmereUI_ClientGate.lua` reads `select(4, GetBuildInfo())`, flags Forever for `16000` to `19999`, flags a too-old retail client as blocked, and returns quietly if the number isn't readable: "the failsafe must never break a healthy client". The core forbids gating on `WOW_PROJECT_ID`, because the TOC number is "the one signal every Forever build reports the same way". Running on the wrong client "would be destructive": migrations would rewrite settings the old addon can't read.
Source: `EllesmereUI_ClientGate.lua:4-8,18-36`, `EllesmereUI_Lite.lua:14-18`.
For ForeverBuddy: we don't need a gate today (Forever only). If one is added, it must run before `ForeverBuddy.lua` touches `ForeverBuddyDB`.

**1.14 For a version failsafe, make line 1 of every file a one-line return, and check it in CI.**
Every EllesmereUI file starts `if EUI_CLIENT_BLOCKED then return end`, and a CI script rejects changed files whose line 1 differs. With the flag set, no SavedVariables global is read or written, and WoW writes the untouched table back: "a lossless round trip".
Source: `EllesmereUI_ClientGate.lua:10-16`, `EllesmereUI_Lite.lua:1`, `.tools/guardrails.py:548-560`.

**1.15 In a dependent addon, feature-test the parent's API on load and go quiet if it's missing.**
Most EllesmereUI children check that the parent exposes what they need and set the blocked flag if not, so "a partially updated install (old parent, new child) goes dormant ... instead of erroring". The check differs per child, and in a few children it sits in the folder-named main file even though another file loads first.
Source: `EllesmereUIForeverEssentials/EllesmereUIForeverEssentials.lua:8`, `EllesmereUIQuickdraw/EllesmereUIQuickdraw.lua:48`.
For ForeverBuddy: a split-out module should check for the core table it needs before registering anything.

### Load lifecycle

**1.16 Read SavedVariables at your own `ADDON_LOADED`, touch game data at `PLAYER_LOGIN`, and wrap each module's start in `xpcall`.**
EllesmereUI's framework runs `OnInitialize` at `ADDON_LOADED` ("SavedVariables are available") and `OnEnable` at `PLAYER_LOGIN` ("game data is available"). Each call goes through `xpcall` with `geterrorhandler()`, so one module's error can't stop the rest and the traceback survives.
Source: `EllesmereUI_Lite.lua:46-48,380-381,519-523`.
For ForeverBuddy: when P4 modules arrive, start each one in its own `xpcall` so a broken merchant module can't block the login briefing.

**1.17 Do secure setup directly inside `PLAYER_LOGIN`; defer work from a post-login `ADDON_LOADED` by one tick.**
EllesmereUI runs `OnEnable` synchronously at login because that is the loading-screen window, the only time secure setup is allowed while in combat on a `/reload`, and "C_Timer callbacks do NOT run during the loading screen". A late `ADDON_LOADED` can fire inside Blizzard's own secure code, so it waits for `C_Timer.After(0, ...)`.
Source: `EllesmereUI_Lite.lua:528-557`.
For ForeverBuddy: matters once bars or unit frames exist; today we do nothing protected.

**1.18 Give each module one event frame with a handler table, created in that file's main chunk.**
EllesmereUI replaced Ace3 with a small framework: one hidden frame per module, a `_handlers[event]` table, and plain functions stored without a wrapper. It creates these frames at file scope because the client bills a handler's CPU "to the addon whose execution context called CreateFrame".
Source: `EllesmereUI_Lite.lua:4-5,58-66,121-138`, `EllesmereUI_Ticker.lua:9-13`, `.github/CONTRIBUTING.md:28-31`.
For ForeverBuddy: `ForeverBuddy.lua` already has a `handlers` table and one frame. Give each future module the same shape rather than one shared dispatcher, and no Ace3.

**1.19 A feature that is off must cost nothing, and new options default off.**
EllesmereUI's contributor rules: a setting nobody turns on means "no event registrations, no hooks doing work, no OnUpdate, no tickers", and new settings default off. Work is event-driven, never polled.
Source: `.github/CONTRIBUTING.md:26-38`.
For ForeverBuddy: register a module's events (say `MERCHANT_SHOW`) only when its setting is on, and build its frames on first use.

### Namespaces and file size

**1.20 Share state between your files through the `...` namespace; keep one global for what others must reach.**
EllesmereUI keeps its module registry and page cache in the TOC vararg table, which is "unreachable from any other addon", and puts its public API on one global, `EllesmereUI`. A LoadOnDemand addon gets its own vararg table, so each child publishes its `ns` into a registry on the global, and a missing entry means "module off".
Source: `EllesmereUI.lua:6-9,3124-3129`, `EllesmereUIOptions/EUI_GroupFinder_Options.lua:4-6`.
```lua
local ADDON_NAME, ns = ...
ForeverBuddy = ForeverBuddy or {}          -- the one public table
ForeverBuddy.modules = ForeverBuddy.modules or {}
ForeverBuddy.modules[ADDON_NAME] = ns      -- how a split-out addon finds it
```
For ForeverBuddy: `ForeverBuddy.lua` takes only `ADDON_NAME` from `...` today. Take `ns` too before the first split.

**1.21 Treat 200 locals and 60 upvalues as hard limits, and compile every file with `luac5.1 -p` in CI.**
One overflow stops the whole file from loading. EllesmereUI's CI compiles changed files and suggests "split the file / move state into a table". It also bans Lua 5.2 syntax such as `goto`.
Source: `AGENTS.md:19`, `.tools/guardrails.py:525-545`, `.github/CONTRIBUTING.md:59`.
For ForeverBuddy: `ForeverBuddy.lua` is at the 200-local limit now. The headless harness loads it, but a cheap `luac5.1 -p` step over every addon file would name the failing file directly.

**1.22 At the limit, put new names on a table and split files by concern, with runtime and settings code apart.**
EllesmereUI has about 200 comments on these limits. Its fixes: store new values as fields (`ns.X`) instead of new locals, call helpers through a table that is already an upvalue, and split big files. Each module has small per-concern files (Events, Layout, Lifecycle and so on, under two naming styles), and its settings page lives apart in the LoadOnDemand options addon.
Source: `EUI_UnlockMode_Tools.lua:822-824`, `EllesmereUIQuickdraw/EUI_Quickdraw_Slots.lua:142-145`, `EllesmereUIMinimap/EllesmereUIMinimap.toc:13-23`, `EllesmereUIGroupFinder/EllesmereUIGroupFinder.toc:12-13`.
For ForeverBuddy: split `ForeverBuddy.lua` along its existing seams (session recording, tooltip, plan, briefing, window), each a file sharing `ns`.

**1.23 When one file becomes several, have later files stop if an earlier one failed.**
When EllesmereUI split Spec Overrides into a series, the first file published its locals on an internals table whose constructor errors on an unknown name. Each later file returns if the table is missing or `broken`, sets `broken = true` at the top and `false` on its last line, so "a file that fails leaves it set, and the files behind it return at their first lines".
Source: `EllesmereUI_SpecOverrides.lua:841-842`, `EUI_SpecOverrides_Harvest.lua:11-15`, `EllesmereUI_SharedHelpers.lua:966-968`.
```lua
local I = ns.core
if not I or I.broken then return end
I.broken = true
-- ... this file's work ...
I.broken = false
```

**1.24 Keep Lua source ASCII; write any other glyph as a byte escape.**
EllesmereUI's CI enforces it: "Multi-byte punctuation corrupts in the packaging pipeline." Only locale files are UTF-8, pinned by `.gitattributes`, whose own comment warns its path pattern "matches NOTHING if the folder is renamed or moved".
Source: `AGENTS.md:23`, `.github/CONTRIBUTING.md:59-61`, `.gitattributes:1-12`.
For ForeverBuddy: `ForeverBuddy.lua` has about 98 lines with non-ASCII characters (arrows and section signs in comments, for example). Replace them, or at least keep them out of strings the game displays.

**1.25 If you localize, use English text as the key, translate only at `SetText`, and keep catalogs out of the login path.**
EllesmereUI's rule is "translate the pixels, never the data": `L(s)` returns English unchanged and skips secret values, so English identity strings still match for search and dispatch. Format strings use positional `%1$s` so translators can reorder. Catalogs live in a LoadOnDemand addon that English clients never load, each file tagged `[AllowLoadTextLocale ...]`, and CI keeps a list of literal keys current.
Source: `EllesmereUI_Locale.lua:9-23,60-79,199-212`, `EllesmereUILocales/EllesmereUILocales.toc:14-22`, `.tools/extract-locale-keys.sh:24-32`.
For ForeverBuddy: no translations planned. If they come, pass text to `L()` as literals so it can be extracted.

### Not for us

- The 25-child suite apparatus: uninstall recorder, `HELD_BACK_ADDONS` roster, cross-child CPU attribution through a shared ticker.
- Retail-only gating (M+ keystone and specialization libraries).
- Calls to globals Forever lacks, such as bare `GetItemInfo`: chapter 2 covers that list and its CI check.
