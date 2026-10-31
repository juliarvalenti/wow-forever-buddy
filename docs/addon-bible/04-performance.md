## 4. Performance

This chapter covers how EllesmereUI keeps CPU time and garbage low: what a feature may cost when off, how it reacts to events when on, how it batches bursts, and how it measures. The one idea to remember: a feature that is off costs nothing, and a feature that is on does work only when something actually changed.

### Off means free

**4.1 Let a disabled feature own at most an inert frame: no registered events, no working hooks, no OnUpdate, no tickers.**
EllesmereUI makes this acceptance criterion 1 for contributions: "users who never turn it on must pay nothing". An event frame may be created at load, but empty; everything else is built on first enable. Their history shows how easily this slips: always-on aura watchers "fired for every Forever player even with the block off", and a cooldown hook was installed "with no check on the setting it exists for". When reviewing, list every RegisterEvent and hooksecurefunc and check each one sits behind its feature's on-state.
Source: `.github/CONTRIBUTING.md:17,26-33`; commits `0c34f239`, `9366dca3`, `ce1bf2bc`.
For ForeverBuddy: `ForeverBuddy.lua` registers every key of `handlers` at load (around line 4977); as modules arrive (merchant, bags, quest, map), register each module's events only in its enable path.

**4.2 Drive each feature's event registration from one idempotent `SetEvents(on)` function.**
EllesmereUI's missing-buffs indicator registers or unregisters all its trackers through one toggle, under a section titled "Events (registered only while the indicator is on somewhere)". Their mythic-timer commit says registration "is reconciled against the desired state", so turning an option on mid-combat or after a reload still takes effect. One function means "on" and "off" cannot drift apart.
Source: `EllesmereUIRaidFrames/EUI_RaidFrames_ForeverMissingBuffs.lua:634,719-727`; commit `41e3c839`.
```lua
local function SetEvents(on)
  if on then f:RegisterEvent("MERCHANT_SHOW") else f:UnregisterAllEvents() end
end
-- call SetEvents(settings.merchant) on login, on settings change, on profile change
```
For ForeverBuddy: call each module's `SetEvents` from every path that changes `ForeverBuddySettings`, including changes Claude writes through the companion app.

**4.3 Make hidden UI do no event work, and catch up on the show edge.**
EllesmereUI's action bars found that "the module still paid near-full event cost for every hidden bar". Hidden bars now strip their per-button event lists and drop out of the central walks, and bars that are never shown are not built at all, which took one profile's load from 164.9 ms to 124.3 ms.
Source: commits `0e0cf364`, `8589ed8d`, `bdd68c9c`.
For ForeverBuddy: the 5 tab bodies should not refresh while the window is closed; mark them stale and rebuild in OnShow.

**4.4 Build rarely used things on first use, not at login.**
EllesmereUI's global search index is "Deliberately NOT run at login": only users who open search pay for it, and it replaced a hidden build that kept "10-25MB of permanent page frames".
Source: `EllesmereUI_GlobalSearch.lua:776-781`.
For ForeverBuddy: create the window, its tabs and any lookup indexes over `Data/*.lua` on first open; keep the login briefing to what it prints.

### Event-driven, not polling

**4.5 Drive state from events, and look for an existing event before writing any poll.**
Their contribution rules require enabled features to be "event-driven, never OnUpdate polling; no wall-clock timers as logic gates". In practice they replaced a 0.5 s range poll with `UNIT_IN_RANGE_UPDATE`, and covered the event's known gap ("Phasing doesn't fire UNIT_IN_RANGE_UPDATE") with an explicit re-check. Note that the buff-reminder module skips that path on Forever and uses its own group watcher, so do not assume a retail event behaves the same on our client.
Source: `.github/CONTRIBUTING.md:39-41`; commit `0038b599`; `EllesmereUIAuraBuffReminders/EUI_AuraBuffReminders_Lifecycle.lua:421,424,488`; `EllesmereUIRaidFrames/EUI_RaidFrames_Events.lua:444`.
For ForeverBuddy: our pcall'd RegisterEvent already records events this client lacks in `missingEvents`; check that list before choosing an event over a poll.

**4.6 Filter per-unit events in the engine with RegisterUnitEvent, except for large churning sets.**
EllesmereUI registers UNIT_AURA per frame for exactly two unit tokens each, so 45 group tokens ride 23 frames. Nameplates do the opposite: 13 spellcast events registered "ONCE globally instead of 13 RegisterUnitEvent calls per plate", then an O(1) lookup from unit to plate. The reasoning (fewer Lua calls for units you don't show, no registration churn as plates come and go) is our inference.
Source: `EllesmereUIRaidFrames/EUI_RaidFrames_ForeverMissingBuffs.lua:687-699`; `EllesmereUINameplates/EUI_Nameplates_Events.lua:28-30,42-60`.

**4.7 In UNIT_AURA, read the update info and return early unless something you track changed.**
EllesmereUI's handlers check `isFullUpdate`, `addedAuras` and `removedAuraInstanceIDs` first: "only an added or removed Missing Buffs aura sends the member back for a look-up; a full update always does." A missing info table counts as a full update. Nameplate colouring has a similar probe, though it bails out on a secret payload where the raid code marks dirty.
Source: `EllesmereUIRaidFrames/EUI_RaidFrames_ForeverMissingBuffs.lua:646-666`; `EllesmereUINameplates/EUI_Nameplates_Colors.lua:156-190`.

### Coalescing bursts

**4.8 Let bursty events only mark work dirty, and do the work once in a single next-frame flush.**
EllesmereUI's missing-buffs code adds the unit to a dirty set and arms one `C_Timer.After(0, Flush)` behind a flag; the flush walks the set and wipes it, and a "full" flag replaces per-unit work. Unit-frame rebuilds use the same idea with a hidden frame whose OnUpdate hides itself and runs the rebuild once, so the public refresh function only arms it.
Source: `EllesmereUIRaidFrames/EUI_RaidFrames_ForeverMissingBuffs.lua:594,617-626`; `EllesmereUIUnitFrames/EUI_UnitFrames_OptionsSetup.lua:59-61,85-88,125-130`; commit `41e3c839` ("coalesced into one update per burst").
```lua
local dirty, armed = {}, false
local function Flush() armed = false; for k in pairs(dirty) do Refresh(k) end; wipe(dirty) end
local function MarkDirty(k) dirty[k] = true; if not armed then armed = true; C_Timer.After(0, Flush) end end
```
For ForeverBuddy: bag and merchant refreshes should go through one `MarkDirty`, since BAG_UPDATE fires many times per loot or sort.

**4.9 Give every throttle a trailing edge so the last update is never dropped.**
EllesmereUI's nameplate target text passes the first call, marks later calls inside the 0.2 s window, and a flusher applies the latest mark when the window closes. The fix commit is titled "Catch up throttled target texts instead of dropping them". A throttle that only drops calls leaves the screen showing stale state.
Source: `EllesmereUINameplates/EUI_Nameplates_Plate.lua:650-656`; commit `0bb44e55`.

**4.10 Coalesce any handler that walks everything, however rare its trigger looks.**
EllesmereUI throttled every ACTIONBAR_SLOT_CHANGED except arg1 == 0, assumed "rare and always passes", which ran an unthrottled walk over ~140+ buttons. That walk is the suspected cause of a user-reported 15-20 s freeze while another addon applied a loadout; the author throttled it and wrote "this should help" without reproducing it. Other addons can fire any event in bursts.
Source: commit `4864e216`.

**4.11 For on/off event pairs, record the wanted final state and apply it once after the burst.**
In EllesmereUI one bag sort fired "hundreds of pairs" of ACTIONBAR_SHOWGRID/HIDEGRID in one frame. A 0.1 s throttle could not help "because every HIDEGRID in the storm reset the state it was gated on". Now edges settle once, and "a pair that nets back to the applied state costs nothing".
Source: commit `39e7e6dd`.

**4.12 Keep one pending timer per target with a callback built once, and make every hook install idempotent.**
EllesmereUI's action bars allocated "a fresh closure plus a fresh C_Timer" on each hover, and one mouse sweep queued 12 or more scans. The fix keeps "one pending timer per bar" and a run-once guard, because re-entry "would stack permanent duplicate handlers".
Source: commit `599e7ef3`.
For ForeverBuddy: `C.hook` (around `ForeverBuddy.lua:2795`) builds a new closure and a new `C_Timer.After(0, ...)` on every bag open; switch it to a file-level callback behind a pending flag, and guard `C.hook` so a second call installs nothing.

### Per-frame work

**4.13 Put any OnUpdate on a frame that is hidden by default, shown only while work is pending, and hidden again when done.**
A hidden frame's OnUpdate does not run. EllesmereUI's shared Ticker shows its driver when the first subscriber arrives and hides it when the last leaves, "so an idle UI runs no per-frame code at all". Arming is idempotent and every subscriber "MUST remove itself once its work has settled"; callers arm on every relevant event because "a missed re-arm freezes a bar".
Source: `EllesmereUI_Ticker.lua:5-7,38-47,104,120`.
For ForeverBuddy: the drag OnUpdate near `ForeverBuddy.lua:4153` already clears itself on release; keep that shape for anything new.

**4.14 Use a looping AnimationGroup for fixed-rate work instead of a Lua OnUpdate.**
EllesmereUI's fixed-rate ticker uses OnLoop so "the C engine sleeps between fires, so subscribing costs zero Lua at frame rate". It is reserved for work that is truly time-based (a 20 Hz drain); value bars should be event-driven with engine smoothing, and the code warns "never add a color poll".
Source: `EllesmereUI_Ticker.lua:137-141`; `EllesmereUIResourceBars/EUI_ResourceBars_Update.lua:2913-2931`.

**4.15 If an OnUpdate must exist, run one driver per addon, cap it at about 60 fps, and pass subscribers the accumulated time.**
EllesmereUI's glow driver adds up `elapsed` and dispatches only past a 0.016 s gate, so "the sum of dispatched dt equals real elapsed exactly". Its subscriber list is a dense array with an index map and swap-remove, "so churn stays O(1)", and the loop re-tests the current slot so a subscriber can remove itself mid-dispatch.
Source: `EllesmereUI_Glows.lua:78,98-114,119-136`; `EllesmereUI_Ticker.lua:73-85,107`.

**4.16 Create each module's frames in the main chunk of the addon that owns the work.**
EllesmereUI found (and probe-verified) that the engine bills a script handler's whole call tree to the addon whose code called CreateFrame for that frame; frames made in OnInitialize/OnEnable "are billed to the PARENT forever". Misbilled work hid in "the core addon row where no profile of this module ever looked".
Source: `EllesmereUI_Ticker.lua:9-16`; commit `0e0cf364`.
For ForeverBuddy: no effect while we ship one addon; if a module ever becomes its own load-on-demand addon, create its frames at that addon's file scope.

### Budgets and load time

**4.17 Split any loop that can run more than a few milliseconds into per-frame slices with a `debugprofilestop()` budget.**
EllesmereUI's search prebuild runs "as many builds as fit in a small per-frame budget (at least one), then yield a frame", with a 4 ms budget. Its aura kit allows 250 ms per slice only at login, out of combat and outside raids, because a raid login "can use the lower script watchdog" and a single-pass build was "field-hit at 1500ms". A memory scan was also moved out of combat because it could push a tooltip "past the 'script ran too long' watchdog".
Source: `EllesmereUI_GlobalSearch.lua:783,836-848`; `EllesmereUI_AuraKit.lua:2119-2129,2252` and the comment added in commit `caabb884`; commit `c5d83e5c`.
```lua
local function Step()
  local t0 = debugprofilestop()
  repeat i = i + 1; DoJob(jobs[i]) until i >= #jobs or debugprofilestop() - t0 > 4
  if i < #jobs then C_Timer.After(0, Step) end
end
```
For ForeverBuddy: anything that walks a whole `Data/*.lua` slot file or every bag slot should be sliced like this, and skipped in combat (`InCombatLockdown`).

**4.18 Never walk the global frame tree at load, and delete load-time work that measurement shows earns nothing.**
EllesmereUI's whole-UI `EnumerateFrames()` pass visited 11,305 frames and cost 248.5 ms of a 419.1 ms load (59 %); 21 of the 22 object types it touched contributed nothing. "It is removable because it never earned that cost."
Source: commit `6826fbbb`.

### Garbage and allocation

**4.19 Never call CreateFrame on a repeating path: take frames from a pool and reset them fully on release.**
EllesmereUI's popup rows come from a pool whose release clears points, scripts and mouse state; nameplates use Blizzard's `CreateFramePool`. Frames are never garbage collected, so creating them per show leaks for the session (our inference). For frames that appear in bursts, nameplates pre-create 20 after login, starting 2 s in, one every 0.1 s, so a pull does not pay "2ms+ per-plate creation cost" all at once.
Source: `EllesmereUIDataBars/EllesmereUIDataBars.lua:533-534,544-560`; `EllesmereUINameplates/EUI_Nameplates_PlatePool.lua:33,634-661`.
For ForeverBuddy: tab rows that change with data (merchant lists, bag lists) should be pooled, not rebuilt.

**4.20 In anything called per event or per frame, reuse tables and define functions once at file scope.**
A fresh `{r,g,b,a}` per border-colour call was the core addon's "largest garbage source (1.2 MB/min)"; it is now mutated in place. Closures go the same way: one helper "allocated a closure on every hover for no reason", and another was inlined because a closure built before the early-outs "is pure garbage on the common path". Use `wipe(t)` instead of `t = {}`.
Source: `EllesmereUI_PixelPerfect.lua:830-831,2075`; `EllesmereUIResourceBars/EUI_ResourceBars_Frames.lua:122-126`; `EllesmereUIDataBars/Blocks/GreatVault.lua:242`; commit `a68b0e67`.

**4.21 Cache values that change rarely and refresh them on their change event.**
EllesmereUI caches the spec ID because the spec lookups "allocate fresh strings per call (~1.9 kb garbage/call, the dominant memory churn)" and spec only changes on a swap. It also prebuilds unit token strings like `"raid"..i` once, and keeps a roster list instead of walking the roster on every health update.
Source: `EllesmereUI.lua:2127-2129`; `EllesmereUIAuraBuffReminders/EUI_AuraBuffReminders_Lifecycle.lua:428`; commit `e79d0fac`.

**4.22 Store the last value written to a widget and skip the call when it has not changed.**
EllesmereUI's bar gradients skip four C calls when inputs are unchanged, which would otherwise "repaint an identical gradient a dozen times a second", and threshold code checks a change-signature before building anything. Note that a nearby comment still says the gradient is "never skipped or cached"; the code skips, so trust the code and keep comments in step.
Source: `EllesmereUIResourceBars/EUI_ResourceBars_Frames.lua:122-125,130-133`; `EllesmereUIResourceBars/EUI_ResourceBars_Thresholds.lua:383-384`; commit `a68b0e67`.
For ForeverBuddy: the coach frame redraw (`renderCoach`) and tab text should compare before `SetText`.

**4.23 Localize hot globals at the top of the file, but group the long tail into one table to stay under Lua 5.1's limits.**
EllesmereUI aliases `pairs`, `wipe`, `math.floor` and API functions as locals, which saves a global lookup per call (standard Lua 5.1, our inference). Its CI enforces "at most 200 locals in scope at once and 60 upvalues per function. One overflow stops the whole file from loading." One panel groups names into a table, "one upvalue instead of one per name".
Source: `EllesmereUIRaidFrames/EUI_RaidFrames_BuffManager.lua:15-25`; `AGENTS.md:19`; `EllesmereUI_Panel.lua:2045`; `.github/workflows/guardrails.yml`.
For ForeverBuddy: `ForeverBuddy.lua` is already at the 200-locals limit, so new hot aliases must go into an existing table (like `S` or a module table), never as new top-level locals.

### Measuring

**4.24 Measure with C_AddOnProfiler before and after a change, and keep any debug instrumentation free when off.**
EllesmereUI's action-bar perf series began with a label profiler scaled to real addon milliseconds via C_AddOnProfiler, "Zero cost while disarmed: one boolean or nil-check per probe". The options panel gathers five metrics (session, recent, encounter, last and peak time) but shows only RecentAverageTime, refreshed every 5 s by a ticker that starts in OnShow and stops in OnHide. Their loot council also reads the counts of runs over 1, 5, 10 and 50 ms against a 1000/60 ms frame budget.
Source: commit `c66ba882`; `EllesmereUI_Panel.lua:2285-2295,2311-2312,2337,2340-2349,2355-2365`; `EllesmereUISimpleLootCouncil/Core/Perf.lua:28-36`.
For ForeverBuddy: `coachTick` already stops when the coach frame is hidden; any perf readout we add should follow the same rule, at the slowest rate that still reads well.

**4.25 When asking users to turn on `scriptProfile` or `taintLog`, also tell them to turn it off and /reload.**
EllesmereUI reminds once per session on instance zone-in about "the two debug CVars that quietly tax combat performance": taintLog is "often left on after a bug-report capture", and scriptProfile "adds per-call overhead to every addon and only changes effect on reload".
Source: `EllesmereUI.lua:4467-4480,4490`.
For ForeverBuddy: any bug-report help text in the app or addon should include the turn-off step.

### Not for us
- Nameplate dispatchers and plate pre-warming at nameplate scale (we draw no nameplates; the general pooling rule is 4.19).
- Per-addon CPU attribution across 25 sibling addons (matters only if we split into several addons, see 4.16).
- The aura kit's raid-login build budgets (we build nothing that large at login).
