## 3. Taint, combat and Blizzard's frames

This chapter covers what happens when addon code touches Blizzard's UI: taint, combat lockdown, hooks, and hiding or skinning stock frames. EllesmereUI replaces whole parts of the UI, so it touches secure frames constantly and its own code breaks several of its written rules. ForeverBuddy touches no secure frame today, and the one idea to remember is to keep it that way: build our own frames, observe Blizzard from hooks, and do real work on our own tick.

### Our stance

**3.1 Build our own frames, and skin by overlaying them, rather than reskinning Blizzard windows that run secure code.**
Taint is the one review criterion EllesmereUI's maintainer "will not compromise on", and the whole Chat module is closed to outside PRs because of "its current architecture of reskinning Blizzards chat and the dangerous taint risks involved in that." Where the suite must restyle a Blizzard widget, as with chat tabs, it draws its pixels on mouse-disabled "ghosts" anchored to (never parented to) each tab, leaving clicks and state to Blizzard. Its notes record that one insecure dock-management call tainted the dock's state, "do not reintroduce ANY management call."
Source: `.github/CONTRIBUTING.md:43-48`; `EllesmereUIChat/EllesmereUIChat_Tabs.lua:7-15, 23-27`.
For ForeverBuddy: the window, its tabs and the minimap button are all our own frames. Any future module (bags, map, quest) should follow the same shape: our frame, anchored next to Blizzard's if needed, never a rewrite of theirs.

**3.2 Treat every touch on a Blizzard frame as something a reviewer must sign off.**
EllesmereUI's agent rules list "do not write to protected Blizzard frames or call protected functions from insecure code, and do not do it in combat. Hook with `hooksecurefunc` and keep the work cosmetic" as "Judgment only. No reliable mechanical check." Lint cannot catch taint, so review has to.
Source: `AGENTS.md:25-26`.
For ForeverBuddy: list each Blizzard hook in the PR description (today: `MerchantFrame_Update`, the bag open/toggle functions, the item tooltip post-call) so reviewers see the full surface.

### Combat lockdown

**3.3 Send "do this after combat" work through one keyed queue.**
EllesmereUI moved from one `PLAYER_REGEN_ENABLED` frame per call site to `CombatQueue.Defer(key, fn)`: a repeat of the same key collapses, the event is registered only while work is pending ("so idle is free"), the pending set is swapped out before draining so a callback can defer again, and each callback is error-isolated with `xpcall`. Their guardrail script fails CI on a raw REGEN_ENABLED deferral, but only on lines a PR adds or changes. About 71 older raw registrations remain, including plain deferrals such as the cast-bar repark, so the suite does not fully follow its own rule.
Source: `EllesmereUI_Ticker.lua:174-211`; `.tools/guardrails.py:2088-2106`; `EllesmereUI_SharedHelpers.lua:638`; commit `b0ede792`.
```lua
-- our sketch
local pending, order = {}, {}
local function defer(key, fn)
  if not pending[key] then order[#order + 1] = key end
  pending[key] = fn   -- latest wins, one run per key
end
-- on REGEN_ENABLED: swap pending/order for fresh tables, then pcall each fn in order
```
For ForeverBuddy: `handlers.PLAYER_REGEN_ENABLED` already acts as our single drain point (coach, then `L.waiting`). Add new after-combat work there by key rather than registering the event anywhere else.

**3.4 Check `InCombatLockdown()` at the call site, and make queued work re-read live state when it runs.**
The queue does not do the check for you. "Callers keep their own InCombatLockdown() gate: a Defer made out of combat waits for the NEXT combat end. There is no cancel ... so a queued fn must re-read live state when it runs." The canonical shape is "if in lockdown, defer myself by key and return; otherwise apply from current settings".
Source: `EllesmereUI_Ticker.lua:180-182`; `EllesmereUI_BlizzardParty.lua:31-34`.
For ForeverBuddy: `U.refresh()` already returns early in combat; the deferred redraw should read `ForeverBuddySettings` fresh, not a value captured when the deferral was made.

**3.5 Seed combat state from `InCombatLockdown()`, not only from the regen events.**
"A /reload in combat never sees PLAYER_REGEN_DISABLED." A flag kept only by events is wrong after a reload mid-fight, so the raid frames OR the event flag with `InCombatLockdown()`.
Source: `EllesmereUIRaidFrames/EUI_RaidFrames_Events.lua:267-268`.
For ForeverBuddy: the coach hide runs from `handlers.PLAYER_REGEN_DISABLED`; keep the `InCombatLockdown()` check in the coach tick so a reload in combat still hides it.

**3.6 Don't rely on a Lua handler at `PLAYER_REGEN_DISABLED` to change anything protected.**
EllesmereUI's own comments disagree about this edge. Some say "PLAYER_REGEN_DISABLED fires before the lockdown" or that `InCombatLockdown()` "can lag behind"; others say it already reports true there but "protected writes stay legal until its handler returns". A minimap fix (`ced981c0`) found the update skipped entirely because lockdown was already reported, leaving a label stuck. For protected frames the suite uses a secure state driver on the minimap, swapped only out of combat because registering evaluates immediately. That is its choice for that frame, not the only mechanism it uses.
Source: `EllesmereUI_Visibility.lua:482-486`; `EllesmereUI_PartyMode.lua:712-714`; `EllesmereUIBags/EllesmereUIBags_QuickActions.lua:109`; `EllesmereUICooldownManager/EllesmereUICooldownManager.lua:202`; `EllesmereUIMinimap/EUI_Minimap_Lifecycle.lua:160-181`; commit `ced981c0`.
For ForeverBuddy: hiding our own coach at REGEN_DISABLED is fine because the coach is not protected. Never extend this to a protected frame.

**3.7 Expect a blocked protected call to fail silently and leave the frame half-changed.**
`ADDON_ACTION_BLOCKED` does not stop the following Lua lines. A blocked `Hide()` followed by `SetAlpha(0)` left a minimap button "invisible-but-shown". The fix: "Skip only the blocked call for protected frames in combat and queue the re-apply instead."
Source: commit `844fbc98`.
For ForeverBuddy: before any Show, Hide or SetPoint on a frame we did not create, check `InCombatLockdown() and frame:IsProtected()` and defer (rule 3.3) instead of calling.

**3.8 Do one-time protected setup at file load or synchronously in `PLAYER_LOGIN`, never on a timer.**
Per EllesmereUI's comments, the login/reload loading screen is the window in which secure setup is allowed while the player is already in combat, and "C_Timer callbacks do NOT run during the loading screen, so deferring here would always miss the window on a /reload in combat." The suite's own PartyMode comment describes a second, narrow window inside the REGEN_DISABLED handler, so "the only window" in the Lite comment is too strong. Their combat-reload path works because the secure setup fires inside the `PLAYER_LOGIN` window.
Source: `EllesmereUI_Lite.lua:526-538`; `EllesmereUIActionBars/EUI_ActionBars_Disposal.lua:296-299`; `EllesmereUIActionBars/EUI_ActionBars_Setup.lua:179-242`; `EllesmereUI_PartyMode.lua:712-714`.
For ForeverBuddy: this only matters if a module ever creates a secure frame. Our current `PLAYER_LOGIN` work (button, options canvas, briefing) is plain insecure UI and is fine to defer.

**3.9 When handling a Blizzard addon's `ADDON_LOADED` after login, do the work on the next tick.**
Post-login demand-loads "can be dispatched from inside Blizzard's own SECURE executions", and running your code synchronously there "stamps every value they write with addon taint". "A timer callback is a fresh, purely addon-owned execution", so the suite pushes that work through `C_Timer.After(0)`.
Source: `EllesmereUI_Lite.lua:540-556`; commit `d5f5c23d`.
For ForeverBuddy: if `handlers.ADDON_LOADED` ever reacts to a `Blizzard_*` addon (for example to hook a frame that only exists once it loads), install the hook on the next tick.

### Hooking Blizzard code

**3.10 Post-hook Blizzard functions with `hooksecurefunc`; never assign over a Blizzard function or method.**
EllesmereUI once wrapped a Blizzard action-button method to add a combat guard. The replacement commit says the wrapper "planted an insecure function on Blizzard's ActionButton1-12 and so injected taint rather than removing it." The checker found no assignment to a Blizzard function slot left in the suite outside its bundled libraries.
Source: commits `89d46caf`, `3c6372a2`.
For ForeverBuddy: our merchant and bag hooks already use `hooksecurefunc` on global function names inside `pcall`. Keep it that way for every new module.

**3.11 Prefer hooking global functions; treat per-instance method hooks as an open question.**
Is `hooksecurefunc(frame, "Method", fn)` a field write onto the frame? One EllesmereUI comment says yes: on the Battle.net toast it "assigns toast.SetPoint = wrapper, a FIELD WRITE onto BNToastFrame's table", so that one case was moved to a script hook. The rest of the suite disagrees in practice: the chat engine hooks `AddMessage` per chat frame and calls that hook's wrapper secure, SharedHelpers hooks `SetPoint`, `SetAllPoints` and `ClearAllPoints` on Blizzard frames, and there are about 385 per-object method hooks. We have no settled answer.
Source: `EllesmereUIChat/EUI_Chat_Init.lua:920-931`; `EllesmereUIChat/EllesmereUIChat_Engine.lua:22-38, 967`; `EllesmereUI_SharedHelpers.lua:380-396`.
Safe default (our inference): hook a global function (`hooksecurefunc("Name", fn)`) when one exists. If only a method hook will do, test it with `taintLog` on (rule 3.21) before shipping.
For ForeverBuddy: every hook we have today is the global-function form.

**3.12 In a post-hook, set a dirty flag and do the real work on our own next tick.**
A post-hook "only guarantees Blizzard won't block the *call*", not that "the callback has left the secure chain that triggered Update". Walking Blizzard's frame pools or hiding things inline from a hook still tainted the guild roster and, via Edit Mode's layout pass, "everything Edit Mode touched afterwards". The fixes flag the change and apply it from a self-owned `OnUpdate` or timer. The suite still acts inline in a few hooks of its own, so this is a lesson it learned piecemeal.
Source: commits `f67c1981`, `dca9c4fc`, `d5f5c23d`; inline exceptions at `EllesmereUIActionBars/EUI_ActionBars_Disposal.lua:51-57` and `EllesmereUIUnitFrames/EUI_PlayerAuraBars_Bars.lua:428-430`.
For ForeverBuddy: the bag hooks already defer with `C_Timer.After(0)`. Hold the `MerchantFrame_Update` hook to the same rule if its body does more than set a flag.

**3.13 Don't `HookScript` OnShow or OnHide on Blizzard frames that open or refresh during a protected action.**
Guild roster columns show and hide "INSIDE the secure roster refresh that a protected guild action triggers, so the insecure hook bodies executed mid-secure-execution and tainted the action." The fix moved to a post-hook on the refresh function plus a throttled watcher. The suite still HookScripts OnShow on other Blizzard frames (the Shifter panels, disposed action bars) where no protected action runs through them.
Source: commit `004db067`; `EllesmereUIQoL/EllesmereUIQoL_Shifter.lua:586-587`.

**3.14 Don't put our function inside a Blizzard menu description; use the registry-style hooks instead.**
Skinning context menus with `menuDescription:AddMenuAcquiredCallback` left our function "inside Blizzard's menu description, and Blizzard CALLS it from inside its own menu pipeline". Deferring the registration did not help, and the field result was a secret whisper target and an error loop. The fix was "self-owned staggered passes ... handing Blizzard nothing." The suite still registers callbacks through `Menu.ModifyMenu` and `EventRegistry:RegisterCallback`, so the rule is narrower than "never give Blizzard a callback".
Source: commit `0c7f1a6e`; `EllesmereUI_MenuFallbacks.lua:408-409`; `EllesmereUIQuestTracker/EllesmereUIQuestTracker_QuestLinks.lua:66-69`.
For ForeverBuddy: our tooltip lines use `TooltipDataProcessor.AddTooltipPostCall`, the registry kind (our inference that it is safe in the same way). Keep both bodies inside `pcall` as now.

**3.15 Don't call Blizzard's update or layout functions, or write their state flags, to make a feature work.**
An aura reskin wrote `BuffFrame.isExpanded` and called `BuffFrame:Update` and friends. "Running Blizzard's aura machinery under addon taint makes its own UpdateExpirationTime compare the secret expirationTime and error on every aura update." The conclusion: "There is no taint-safe way to force-expand auras from an addon ... Make the setting purely visual."
Source: commit `bd23a411`.
For ForeverBuddy: if a planned module needs us to drive Blizzard's own refresh, cut it to a visual-only version or drop it.

**3.16 Don't open Blizzard panels by calling their toggle functions from our Lua.**
Calling `ToggleCharacter("PaperDollFrame")` from addon code tainted the character frame's show path, so "the player health bar values come back secret". EllesmereUI's fix is a secure button that clicks the micro button for you, built out of combat. Even so, it keeps a Lua `ToggleCharacter` fallback for the moment before that button exists, so its own "never" has an exception.
Source: commit `6d554f2d`; `EllesmereUIDataBars/Blocks/ItemLevel.lua:219-253`.
For ForeverBuddy: no tab or briefing line should call `ToggleCharacter`, `ShowUIPanel` or similar. Tell the player which panel to open instead.

**3.17 If we ever ship a secure button, never put a Lua function in an attribute secure code will run.**
"ExecuteAttribute runs the function with the taint of the execution that SET the attribute", so a `menu-function` written from addon Lua opens a tainted menu. The suite does store strings, numbers, booleans and frame refs in secure attributes. Two further details from the same code: set `useOnKeyDown` to false on buttons that act on "AnyUp", or the key-down CVar discards the click, and disable a gated action with the string `"none"`, not nil, because nil falls back to the wildcard.
Source: `EllesmereUI_Kick.lua:155-163, 229-245`; `EllesmereUIDataBars/Blocks/ItemLevel.lua:239-241`; `EllesmereUIRaidFrames/EUI_RaidFrames_ClickCast.lua:1447-1462`.

### Leave Blizzard's frames alone

**3.18 Keep our per-frame state in a weak-keyed side table, not as fields on Blizzard frames.**
EllesmereUI keeps addon state for frames it did not create in `setmetatable({}, { __mode = "k" })` tables. A stray key on the quest tracker "becomes a 'fake entry'" in Blizzard's own `pairs()` loops, and on Cooldown Viewer items "Blizzard's refresh chains read it MID-EXECUTION and the taint poisons everything downstream." The suite's one sanctioned exception is setting a field Blizzard exposes as an input, such as `GameTooltip.suppressAutomaticCompareItem`.
Source: `EllesmereUI_SharedHelpers.lua:13-25`; `EllesmereUIQuestTracker/EllesmereUIQuestTracker_Skin.lua:89-93`; `EllesmereUICooldownManager/EUI_CDM_Rotation.lua:372-380`; `EllesmereUIBlizzardSkin/EUI_BlizzardSkin_TooltipVisibility.lua:131-135`.
For ForeverBuddy: our tooltip code must not mark `GameTooltip` with fields of our own; track "already added" in a side table keyed by tooltip.

**3.19 Ask `IsForbidden()` first on any frame a hook hands us, including the tooltip's owner.**
On a forbidden frame, "IsForbidden is the only method safe to call", and "Testing the tooltip alone was not enough: the tooltip is GameTooltip and perfectly fine, it is the OWNER that is off limits" (for example nameplate aura buttons).
Source: commit `65a5275c`.
For ForeverBuddy: the item post-call in `ForeverBuddy.lua` (around line 1553) does not check this today. Bail early if `tooltip:IsForbidden()` or its owner's is true.

**3.20 Don't hide, reparent or move Blizzard frames; if a module ever must, follow the suite's narrow paths.**
EllesmereUI uses several tools and each has a scar. Alpha 0 plus mouse off is used where `Hide()` would run Blizzard handlers (a POI button's `OnHide` wrote into `EventRegistry` and tainted every subscriber) or hit protected setters, but the suite also uses plain `Hide()`, `HideBase` and reparenting elsewhere. Reparenting to a hidden parent is its main disposal tool, done out of combat and never while Edit Mode is open, with chat widgets banned and `MainActionBar` reparented only inside a secure snippet. On Edit Mode systems it calls `SetPointBase`, `ClearAllPointsBase` and `SetScaleBase`, because the Lua overrides record our call as a layout write; `HideBase` on the stock multibars caused thousands of `ADDON_ACTION_BLOCKED` errors. Protected frames are never made movable from Lua (`if not frame:IsProtected()`).
Source: commits `c38836ad`, `0eff8f00`, `512eb6a7`, `f96dcd1e`; `EllesmereUIChat/EllesmereUIChat_Engine.lua:26-32`; `EllesmereUIActionBars/EUI_ActionBars_Disposal.lua:21-23, 42-57`; `EllesmereUI_SharedHelpers.lua:103-181, 604-641`; `EllesmereUIQoL/EllesmereUIQoL_Shifter.lua:181-207, 589-592`.
Safe default for us: leave Blizzard frames where they are. If one must disappear, set alpha 0 and turn the mouse off, out of combat, and only after checking it is not protected.

**3.21 Leave event registrations on Blizzard frames alone.**
"A registration made by us taints every dispatch of that event." After a raid pull spammed blocked writes, the suite stopped touching the two seeding events (`ACTIONBAR_SLOT_CHANGED`, `PLAYER_ENTERING_WORLD`) on Blizzard's action-bar broadcasters. It still adds other events back to those frames while vehicle or extra buttons need painting, and unregisters events on Blizzard frames freely, so a blanket "never" is stricter than the suite.
Source: `EllesmereUIActionBars/EUI_ActionBars_Disposal.lua:96-102, 126-129, 213-224`; commit `3c6372a2`.
Safe default for us: every event we listen to goes on our own frame.

**3.22 Skip skinning protected subframes that do secret-value logic.**
Plain `SetAlpha` and `CreateTexture` writes on the LFG application viewer, which sits under the protected group finder and compares secret values, made "tainted by 'EllesmereUI'" errors in raids, so that skin is now gated on `IsProtected()`. This is specific: the suite skins most protected Blizzard frames without a gate, and notes that border and texture skinning of the secure guild frames "was never the problem".
Source: commit `7f82511e`; `EllesmereUIBlizzardSkin/EllesmereUIBlizzardSkin_GroupFinder.lua:900-903`; commit `004db067`.
Safe default for us: we don't skin Blizzard frames, so we never need to judge which protected frames are safe.

### Secret values and debugging

**3.23 Check `issecretvalue` before comparing or doing arithmetic on any value Blizzard hands us.**
In 12.x combat and instances, values such as `GetAlpha`, `GetPoint`, GUIDs and even `IsProtected()` can come back secret. The suite's rule is "Pass them straight to the widget API that accepts them", and its frame placer treats a secret answer as "yes, refuse". It is not uniform: one helper re-asserts its change on a secret instead of skipping.
Source: `AGENTS.md:25`; `EllesmereUI_SharedHelpers.lua:93, 115, 230-240`.
For ForeverBuddy: `isSecret` in `ForeverBuddy.lua` already guards item ids in the tooltip and bag paths. Use it on every new Blizzard-sourced compare, and on secret do nothing.

**3.24 Test every in-game feature with restrictions forced on, and diagnose taint with `taintLog` and bisection.**
`/euidev` flips the six `addon*RestrictionsForced` CVars, "which create the same restricted/secret-value environment anywhere", since "secrets are active in instances only". Taint bugs were found with `taintLog 2` captures, and the suite nags once per session if `taintLog` or `scriptProfile` is left on, because both cost combat performance. Reports also name the wrong culprit: thousands of blocked calls "blamed on EllesmereUIDataBars but reported under Cooldown Manager" came from the action bars, found by bisection.
Source: `EllesmereUI.lua:2374-2378, 3635-3651, 4466-4500`; commits `d5f5c23d`, `0eff8f00`, `004db067`, `f67c1981`.
For ForeverBuddy: our release checklist should include one pass with the forced-restriction CVars on (first confirm they exist on WoW Forever) and inside an instance. For any `ADDON_ACTION_BLOCKED` report, start with `/console taintLog 2`, read `Logs/taint.log`, then disable our hooks one at a time.

### Not for us

- Secure handler snippets fed by attributes (`SecureHandlerAttributeTemplate`). Note the correction: the suite triggers its snippet from Lua only inside the `PLAYER_LOGIN` window on a combat reload; addon Lua cannot trigger one mid-combat. Something secure (a state driver, `WrapScript`, a secure click) has to.
- State drivers, attribute drivers, `SecureGroupHeaderTemplate` headers, hover-cast `WrapScript` bodies and two-stage retirement of secure frames. All of these exist because the suite owns secure frames; we don't.
- `MenuFallbacks`, which greys out protected entries in a menu it can only reopen insecurely. A unit-frame replacement problem.
- Creating each child addon's driver frames at its own file scope. This is about CPU billing in the profiler across 25 addons, not taint.
- Caching the global addon table on `ns` to avoid "benign self-taint" in hot combat events. Harmless, and our test harness relies on reading globals through `read()`.
- Guardrail exceptions with named approvers and expiry dates. A good process for a large contributor base; our reviewers already see every hook (rule 3.2).
