## 2. The Forever API and secret values

WoW Forever runs the 12.1 engine, so combat, unit, aura, cooldown and chat APIs can hand addon code "secret" values: opaque values Lua may pass along but not inspect. This chapter covers how to handle secrets, how to show them without reading them, and the concrete ways Forever's API differs from retail. The one idea to remember: test `issecretvalue` first, then either drop the value or hand it whole to an engine API that accepts it.

### What a secret allows

**2.1 Treat every value from a combat, unit, aura, cooldown or chat API as possibly secret, and pass it whole to an API that accepts it.**
EllesmereUI's house rule is: "Do not compare, index, or do arithmetic on secret values ... Pass them straight to the widget API that accepts them." They add that this is "Judgment only. No reliable mechanical check." The reason, from commit `c9e30a25`: "addon execution is never untainted, and only untainted code may inspect a secret".
Source: `AGENTS.md:25`, `EllesmereUI_Colors.lua:480-481`, commit `c9e30a25`.

**2.2 Call `issecretvalue(v)` before any nil test, truthiness test or `x or default` on a possibly secret value.**
The evidence conflicts. Commits `9f2d1b6a` and `68316f6b` treat comparing a secret to nil as an error, and the ThreatData module says "comparing a secret, even to nil, is an error". Other comments say the opposite: "A nil test is safe on a secret" (QoL), "truthiness works, but comparisons/format error" (ResourceBars), "A truthiness check (not text) is secret-safe" (Nameplates). Live code also does it: their shared `IsSecret` does `v ~= nil` before `issecretvalue`, and has shipped since v9.6.5. So putting the secret test first is not proven necessary, but it is the order that is safe under every reading. In `9f2d1b6a` the throw also aborted a `pairs()` loop in a shared handler, so every other icon stopped updating.
Source: commits `9f2d1b6a`, `68316f6b`; `EllesmereUIForeverEssentials/EllesmereUIForeverEssentials_ThreatData.lua:15-18`; against: `EllesmereUIQoL/EllesmereUIQoL.lua:2457-2458`, `EllesmereUIResourceBars/EUI_ResourceBars_Update.lua:1228-1229`, `EllesmereUINameplates/EllesmereUINameplatesFriendly.lua:105-108`, `EllesmereUI_SharedHelpers.lua:916-929`.
```lua
if issecretvalue and issecretvalue(v) then return keep end -- first, always
if type(v) ~= "number" then return keep end                  -- existence via type()
```
For ForeverBuddy: the record-mode `arg1` helper (`ForeverBuddy.lua:4230-4236`) tests `v == nil` before `isSecret(v)`; swap the two lines.

**2.3 Never compare, format, concatenate or `or`-default a secret in Lua.**
Again the evidence conflicts. Shipping code formats and concatenates possible secrets and relies on getting a secret string back that `SetText` or `SetAtlas` accepts: `format("%d", pctRaw) .. "%"` on `UnitHealthPercent`, and `SetAtlas("classicon-" .. ct)` with a known-secret `ct`. Against that, one comment says "string.format handles secrets natively" while another in the same file says "format error", a chat comment says "concat on secrets errors", and commit `6bdfc134` saw `format` reject a secret character name (though that call was `format(..., charName or "?")`, so the `or` truth test may have been the real fault). The conservative rule is to do none of these and hand the raw value to the widget. If two possibly secret strings must be compared, EllesmereUI pcalls a preallocated function and treats failure as "not equal".
Source: `EllesmereUIResourceBars/EUI_ResourceBars_Update.lua:88-93, 184-198, 324-325, 489, 1229`; `EllesmereUIUnitFrames/EUI_UnitFrames_Portrait.lua:45-46, 55-61`; `EllesmereUIChat/EllesmereUIChat_Engine.lua:788`; commit `6bdfc134`; `EllesmereUIBlizzardSkin/EllesmereUIBlizzardSkin_ChatBubbles.lua:191-197`.

**2.4 When a read comes back secret, keep the last plain answer instead of falling back to a default.**
EllesmereUI's Forever-only modules pass every API read through a `Plain(v)` gate, and on a secret they return the previous verdict: "A restricted answer keeps the last verdict." A default would flip the UI each time the value goes secret.
Source: `EllesmereUIResourceBars/EUI_ResourceBars_ForeverDruidMana.lua:66-77`.
For ForeverBuddy: `read()` turns a secret into nil, so any caller that writes `read(X) or 0` turns "unknown" into "zero"; `carried()` already gets this right by returning nil when `secretHits` rose during the scan.

**2.5 Never branch on a widget getter; keep your own plain shadow state.**
Once any secret has been written into a frame's render state, alpha reads on that frame's regions, not only the one written, can come back secret, and comparing them throws. In `673f4921` a `SetAlphaFromBoolean` on one cast bar texture made a sibling's `GetAlpha()` secret, and the throw aborted the whole styling pass. Frames you did not create are worse: `GetFrameStrata`, `GetPoint`, `GetLeft`, `IsMouseClickEnabled` and similar have all returned secrets. Read getters into locals, test them, and skip the frame on a secret: leaving the getter inside an `and` chain "throws on the spot instead of skipping".
Source: commits `673f4921`, `c9e30a25`, `cd8a7778`, `80be7d49`, `d58ef762`; `EllesmereUIRaidFrames/EUI_RaidFrames_PetFrames.lua:149-153`; `EUI_UnlockMode_Movers.lua:116-119`.
For ForeverBuddy: our frames only ever hold plain data, so `IsShown()` and `GetPoint()` on them are fine today; this matters the day a module skins or sweeps Blizzard frames, or feeds a secret into one of ours.

**2.6 Store per-frame data in a weak-keyed table you own, never as fields on Blizzard frames.**
Their comment: "NEVER write custom keys onto Blizzard's secure widget tables (taints them -> 'secret value' errors)." They also skip any object that `issecretvalue` or `issecrettable` flags.
Source: `EllesmereUI_PixelPerfect.lua:300-308`.

**2.7 Gate any walk over an API-returned table with `canaccesstable` or `canaccessallvalues`, and cache nothing from a partial read.**
They check tooltip data, LFG info and upgrade info this way before walking it: "A read that meets a secret value caches nothing." Feature-test the functions; UpgradeTooltip calls `canaccessallvalues` without doing so.
Source: `EllesmereUIQoL/EllesmereUIQoL_UpgradeTooltip.lua:12, 19-30`; `EllesmereUICooldownManager/EUI_CDM_HookIconWatch.lua:730`.

**2.8 Never put a secret into addon comms, and (our inference) never into SavedVariables.**
Their loot council refuses to send any payload where `hasanysecretvalues` is true, before serialising it. Nothing in EllesmereUI guards SavedVariables this way; extending the rule to saved data is our inference.
Source: `EllesmereUISimpleLootCouncil/Core/Comms.lua:331-336`.

### What ForeverBuddy's `read()` already covers

**2.9 Route every API call through `read()`, and guard by hand the paths it cannot see.**
`read()` resolves a dotted name, pcalls it, and replaces each secret return with nil while counting `secretHits`; `isSecret()` tests `issecretvalue` first, then `issecrettable` on tables. So a missing API, an erroring API and a secret top-level return are all handled, and the 2.2 ordering is built in. It does not cover:
- secret fields inside a plain table it returns (container item info, tooltip `data.lines`): test each field, as `carried()` does with `arg(info.itemID)`, or gate with `canaccesstable` (2.7);
- event arguments, which reach handlers raw: use `arg()` on any that can be secret;
- widget getters and `GetText()` on frames (2.5);
- display: `read()` discards the secret, so it can never feed a secret-accepting setter. A module that shows combat values must call the API directly and pass the value on (2.10 to 2.14).
Source: `ForeverBuddy.lua:48-99` (`isSecret` at 52, `read` at 84), `arg` at 253-259, `carried` at 291-321 (our code).

### Showing secrets without reading them

**2.10 Turn a secret boolean into visibility with `SetAlphaFromBoolean`, and a secret count with `SetAlpha`.**
Range, interruptible and overshield flags go to `region:SetAlphaFromBoolean(flag, alphaTrue, alphaFalse)`, with a plain fallback when the method is missing. For "hide at zero" on a secret charge count they set alpha to the count itself: "SetAlpha clamps to [0,1] engine-side ... with no comparison". The clamping is their stated claim, not something a test checks. After either write, clear any cached alpha of your own so other code does not overwrite it.
Source: `EllesmereUIRaidFrames/EUI_RaidFrames_Visibility.lua:81-91`; `EllesmereUI_AbsorbGlow.lua:87-107`; `EllesmereUICooldownManager/EUI_CDM_HookIconWatch.lua:452-458`.

**2.11 Pick between two colours on a secret boolean with `C_CurveUtil.EvaluateColorValueFromBoolean`, and type-check `UnitIsUnit`.**
The signature is `EvaluateColorValueFromBoolean(bool, valueIfTrue, valueIfFalse)`, called per colour component, so the choice happens in C. Gate `UnitIsUnit` with `C_Secrets.CanCompareUnitTokens`; when the comparison is refused it returns nil, not an error. Measured in a 12.1 dungeon, only "is this unit me?" was answered.
Source: `EllesmereUI_Colors.lua:483-489, 534-560`.

**2.12 Get health colour and percent from `UnitHealthPercent` with a cached curve, never from `UnitHealth / UnitHealthMax`.**
`UnitHealthPercent(unit, true, curve)` "handles secret values internally and returns a clean ColorMixin"; `CurveConstants.ScaleTo100` gives 0 to 100. Build curves once and rebuild only when stops change. For a threshold effect, use a hard step at `t` and `t + 0.0001` that varies only RGB (black under ADD blend means "off"): "curve alpha interpolation is deliberately never relied on."
Source: `EllesmereUIRaidFrames/EUI_RaidFrames_Colors.lua:41-61`; `EllesmereUINameplates/EUI_Nameplates_Overlays.lua:157-164`.

**2.13 Drive spell cooldowns and countdown text from engine objects, never from numbers.**
`C_Spell.GetSpellCooldownDuration` returns an opaque duration object that goes into `Cooldown:SetCooldownFromDurationObject` or a status bar's `SetTimerDuration`. `Cooldown:SetCooldown` refuses secrets from addon code, and `CooldownFrame_Set` opens with `start > 0 and duration > 0`, which throws. Countdown text uses `C_StringUtil.CreateNumericRuleFormatter` (pcall `SetBreakpoints`, keep a seconds-formatter fallback) or `C_DurationUtil.CreateDurationTextBinding`. Do not trust `SetFormattedText` with a secret: the one field report shows a secret remaining time rendered as "0.0", "which reads as 'ready'", and no source confirms it renders any secret correctly. Items have no duration object; `C_Item.GetItemCooldown` stays numeric.
Source: `EllesmereUIQuickdraw/EUI_Quickdraw_Actions.lua:863-873`; `EllesmereUIQoL/EllesmereUIQoL_MovementAlert.lua:624-628, 1448-1451`; commit `52a33e57`; `EllesmereUI_AuraKit.lua:95-99, 139, 1181-1182`; `EllesmereUIResourceBars/EUI_ResourceBars_SwingTimer.lua:399`.

**2.14 Check an API's `SecretArguments` annotation before passing it a secret, and degrade to a flat look when it refuses.**
`AllowedWhenTainted` APIs (SetAlpha, SetStatusBarColor, SetMinMaxValues, SetColorTexture) accept secrets from addons; `AllowedWhenUntainted` ones (Texture:SetGradient, Cooldown:SetCooldown) do not. The annotation is not the whole story: `C_Spell.GetSpellTexture` is marked as accepting secrets, yet commit `6bdfc134` saw it reject a secret spell ID. Check in game.
Source: commits `96c86803`, `6bdfc134`; `EllesmereUIUnitFrames/EllesmereUIUnitFrames.lua:2120-2122`.

**2.15 Format numbers with `AbbreviateNumbers` and one cached config, nothing below 10,000 on Forever, and hide a secret zero with `C_StringUtil.TruncateWhenZero`.**
EllesmereUI builds its `CreateAbbreviateConfig` at most once per session and drops every tier below 10,000 on Forever ("9999 stays '9999', 10123 reads '10.1K'"). `TruncateWhenZero` blanks a zero inside the engine, and its result stays secret, so only set it. ResourceBars feature-tests and pcalls it; RaidFrames calls it bare as `TruncateWhenZero(amt or 0)`, which truth-tests a possible secret. Follow ResourceBars, and track "hidden" with a plain boolean.
Source: `EllesmereUI_NumberFormat.lua:18-28, 109-113, 146`; `EllesmereUIResourceBars/EllesmereUIResourceBars.lua:62-69`; `EllesmereUIRaidFrames/EUI_RaidFrames_Colors.lua:1009-1020`; commit `7f9f2f3f`.

### Restriction probes

**2.16 Ask `C_Secrets.Should*BeSecret` before choosing a path, pcall it, treat anything but a plain `false` as secret, and cache only the "restricted" answer.**
EllesmereUI is inconsistent here: only RaidBuffCheck pcalls the predicate and fails closed; other sites call it bare or treat a pcall error as "not secret". Our rule is the stricter version. Their aura probe caches only the restricted verdict, because "a stale 'false' sends callers into hard-erroring scans" while "A stale 'true' merely suppresses a display for one frame".
Source: `EllesmereUIQoL/EllesmereUIQoL_RaidBuffCheck.lua:137-145`; `EllesmereUI_AuraKit.lua:2343-2362`.

### Forever versus retail

**2.17 Detect Forever by interface number 16000 to 19999, and ship `16001` in the TOC.**
Forever reports a 1.60+ TOC (16001, game type "camelot"); Classic Era is 115xx and retail 12xxxx, "so the ranges never meet". EllesmereUI gates on this "never on WOW_PROJECT_ID", fails open if the number can't be read, and ships Forever-only addons as `<Name>_Camelot.toc` with `## AllowLoadGameType: camelot`. Forever reads `_Camelot.toc`, then `_Mainline`, then the plain TOC.
Source: `EllesmereUI_ClientGate.lua:18-33`; `EllesmereUI_Lite.lua:13-17`; `EllesmereUIForeverEssentials/EllesmereUIForeverEssentials_Camelot.toc:1-2`; `.tools/guardrails.py:2136`.
For ForeverBuddy: `ForeverBuddy.toc` already says `## Interface: 16001`; no change unless we also target retail.

**2.18 Call only the `C_` forms of item, spec, coin and PvP functions; the bare globals are nil on Forever.**
Retail gets the bare names from deprecation addons that Forever never loads. EllesmereUI's CI fails any new unguarded use, because "Agents kept adding them ... and each needed a later fix". The list includes `GetItemInfo`, `GetItemInfoInstant`, `GetItemCount`, `GetItemCooldown`, `GetItemQualityColor`, `GetItemIcon` (use `C_Item.GetItemIconByID`), `GetCoinTextureString` (use `C_CurrencyInfo`), `GetSpecialization*` (use `C_SpecializationInfo`) and `PlayVocalErrorSoundID`. `GetSpecializationInfoByID` has no `C_` form and must be nil-guarded.
Source: `.tools/guardrails.py:563-601`; commit `2c447fac`.
For ForeverBuddy: we already call `C_Item.*` and `C_Container.*` through `read()`; a `tools/addon-test` check that fails on the bare names would keep it that way.

**2.19 Keep Forever's known differences in one constants table, and verify each in game.**
These are the concrete facts the source supports:
- Global cooldown spell is 29515, not 61304 ("Forever reports nothing on 61304"). Its start and duration go secret in combat, so use the duration object (2.13). Source: `EllesmereUI_Lite.lua:19-21`; commits `1f1c6e0a`, `e550b9fb`.
- Combo points: call `GetComboPoints("player", "target")`. `UnitPower` still reports the previous target's count when `PLAYER_TARGET_CHANGED` fires. Source: commit `ec2a52aa`; `EllesmereUINameplates/EUI_Nameplates_ClassPower.lua:489-492`.
- Threat: the 5th return of `UnitDetailedThreatSituation` is in display units, not Classic's x100 scale (commit `825ca37f` removed a `/ 100`). Forever returns it readable, since `ShouldUnitThreatValuesBeSecret` is false there. Source: `EllesmereUIForeverEssentials/EllesmereUIForeverEssentials_ThreatData.lua:8-9, 102`.
- Realm: `GetRealmName()` still returns a name, but it does not group Forever characters. EllesmereUI keys by ruleset from `C_GameRules.IsGameRuleActive(Enum.GameRule.HardcoreRuleset / PvPRuleset / RPRuleset)`, but stores the localized label (`GUILD_PLAYSTYLE_HARDCORE`, `GDAPI_REALMTYPE_*`), so the key changes with client locale (our inference). Key on a locale-independent token such as the enum name instead. Source: `EllesmereUI.lua:1220-1246`; commit `0005c5c8`.
- Interrupt spell IDs: several Classic IDs are different spells on retail, and ranks are grouped. Source: commit `80c7ec5e`.
- Spell ranks exist: use `C_SpellBook.FindSpellBookSlotForSpell` and `C_SpellBook.IsSpellBookItemLowRank` to find the newest rank. Source: `EllesmereUIForeverEssentials/EllesmereUIForeverEssentials_Uprank.lua:18-30`.
- Keyring is bag -2, read as `Enum.BagIndex.Keyring`. Source: `EllesmereUIBags/EUI_Bags_Start.lua:539-542`.
- Character bank tabs are bag slots, up to `CharacterBankTab_9`; "a bought tab holds nothing until a bag is placed in it". Enumerate `Enum.BagIndex` keys that exist and skip missing ones. `PurchaseBankTab` is protected. Source: `EllesmereUIBags/EllesmereUIBags_Bank.lua:115-121, 378-379, 1158-1163`.
- Swing timer: Forever ships a native `Blizzard_SwingTimer` with `C_SwingTimer.*`, `PLAYER_SWING` and `Enum.PlayerSwingType`. Source: `EllesmereUIResourceBars/EUI_ResourceBars_SwingTimer.lua:5-11, 456, 466`.
- Deprecated globals are nil (2.18).
For ForeverBuddy: `identity()` stores `realm` as data, which is fine, but nothing should key on it; and `carried()` scans bags 0 to 5 only, so keys on the keyring (-2) never count.

**2.20 Never assume specs, Mastery, Versatility, the Great Vault, delves, keystones, the combat log or newer art on Forever.**
"WoW Forever has no specs"; with none, a spec-based range check "returned the 5 yd melee cutoff for every class". The combat log is gone for addons on all of 12.0, not just Forever. Newer icon file IDs and atlases are missing: EllesmereUI keeps a map of stand-in icons ("Extend the map whenever a tester reports a green square") and probes atlases with `C_Texture.GetAtlasInfo`.
Source: `EllesmereUI.lua:858-884, 2079`; commit `579f8bd3`; `EllesmereUIDataBars/Blocks/MicroMenu.lua:549`; `EllesmereUIQuickdraw/EUI_Quickdraw_Slots.lua:391`; `EllesmereUIDamageMeters/EUI_DamageMeters_Window.lua:843`.

### Items, bags, tooltips and load events

**2.21 Register `GET_ITEM_INFO_RECEIVED` only while items are pending, record failures, and retry `ContinueOnItemLoad` at most once.**
The event "fires on every item-cache resolve", so they register it only while needed. They treat `arg2 == false` as a failure; without the retry, an item "drops out of the better-items scan permanently".
Source: `EllesmereUIForeverEssentials/EllesmereUIForeverEssentials_JunkItems.lua:668-675, 731-739`; `EllesmereUIForeverEssentials/EllesmereUIForeverEssentials_LootFeed.lua:821-825`; `EllesmereUIBlizzardSkin/EllesmereUIBlizzardSkin_WindowPacks.lua:8102-8104`.

**2.22 Mark bags dirty on `BAG_UPDATE`, rescan on `BAG_UPDATE_DELAYED`, and coalesce repeats to one next-frame scan.**
`BAG_UPDATE` "can land before the slot counts change"; `BAG_UPDATE_DELAYED` fires once the batch settles, but still several times per bag open.
Source: `EllesmereUIForeverEssentials/EllesmereUIForeverEssentials_JunkItems.lua:661-663`; `EllesmereUIBags/EUI_Bags_Start.lua:739-740`; `EllesmereUIQoL/EllesmereUIQoL.lua:745`.
For ForeverBuddy: `handlers.BAG_UPDATE_DELAYED` (`ForeverBuddy.lua:4684`) already does the work there; add a next-frame coalesce if a bags module rescans heavily.

**2.23 Hook tooltips per data type, never AllTypes, memoise per link, and skip secret lines.**
"Register per-type callbacks instead of AllTypes to avoid firing on every unit/currency tooltip in the game (major CPU savings)." Shown tooltips are rebuilt every 0.2 s, so memoise per link. There is no removal API, so guard the callback with an enabled flag. EllesmereUI's own line guards are uneven (GatherSkill truth-tests the title before `issecretvalue`); follow 2.2.
Source: `EllesmereUI.lua:4405, 4603-4610`; `EllesmereUIForeverEssentials/EllesmereUIForeverEssentials_SimplifiedStats.lua:35-36`; `EllesmereUIQoL/EllesmereUIQoL_UpgradeTooltip.lua:61, 102-103`.
For ForeverBuddy: our `AddTooltipPostCall(Enum.TooltipDataType.Item, ...)` (`ForeverBuddy.lua:1553`) and `isBound` (type, then `isSecret`, then compare) already match.

**2.24 Read SavedVariables at your own `ADDON_LOADED`, set up synchronously in `PLAYER_LOGIN`, and defer later `ADDON_LOADED` work one tick.**
`PLAYER_LOGIN` runs inside the loading screen, "the only window in which those secure operations are permitted while the player is in combat", and timers do not run then. Later `ADDON_LOADED` events can arrive inside Blizzard's secure execution, so defer them. On `PLAYER_ENTERING_WORLD`, branch on `isInitialLogin, isReloadingUi`, and re-check zone data once, because `GetInstanceInfo()` "can still report the PREVIOUS zone on the first frame after a loading screen".
Source: `EllesmereUI_Lite.lua:380-381, 460, 528-549`; `EllesmereUIChat/EllesmereUIChat_SessionHistory.lua:600-601`; `EllesmereUIMythicTimer/EUI_MythicTimer_Where.lua:160-164`.

**2.25 For a Blizzard module you depend on, check `C_AddOns.IsAddOnLoaded` at load as well as listening for its `ADDON_LOADED`.**
Modules loaded before your addon never fire `ADDON_LOADED` for you: "Blizzard_ObjectiveTracker ... is loaded before our addon, so ADDON_LOADED for it never fires."
Source: `EllesmereUIQuestTracker/EllesmereUIQuestTracker.lua:159-165`.
For ForeverBuddy: relevant when the quest and map modules hook Blizzard's tracker or world map.

### Not for us
- Simple Loot Council comms serialisation details beyond the "no secrets" check (2.8).
- Multi-interface TOC lines for retail builds, and `[AllowLoadGameType standard]` per-file gating: we ship for Forever only.
- Cross-faction BNet and group-finder secret cases, which only matter for social modules.
