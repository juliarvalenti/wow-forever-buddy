## 6. Saved data, settings and profiles

This chapter covers how EllesmereUI lays out its SavedVariables, fills in defaults, runs migrations, moves profiles in and out as strings, and keeps private records apart from shareable settings. The one idea to remember: decide where each piece of data lives (settings or records, account or character, shareable or private) when you create it, because every later bug in this area comes from data sitting in the wrong store.

### Where data lives

**6.1 Keep one top-level saved table per scope, and retire old saved globals deliberately.**
EllesmereUI keeps nearly all settings in one account table, `EllesmereUIDB`. Child addons still declare their old `...DB` globals, and NewDB wipes each one in place so it saves as an empty table. Two exceptions hold real data in their own SVs: Simple Loot Council (`EllesmereUISimpleLootCouncilDB`) and the Bags inventory (`EllesmereUIBagsInventoryDB`). The code comment says wiping in place is needed because the serializer "holds the original table reference from load time". Their own Reset ALL contradicts that: it assigns `_G[name] = {}` and `_G["EllesmereUIDB"] = {}` and relies on the new tables being saved, so WoW evidently saves the global by name at logout. Wiping in place is harmless, but it is not the only way that works.
Source: `EllesmereUI_Lite.lua:273-280`, `EllesmereUIOptions/EUI__General_Options.lua:1939`, `:1985`, `EllesmereUISimpleLootCouncil/EllesmereUISimpleLootCouncil.toc:16-17`.
For ForeverBuddy: keep exactly two saved globals, `ForeverBuddyDB` (per character, records plus `_meta`) and `ForeverBuddySettings` (per account). P4 modules should get sub-tables inside these, not new SV names.

**6.2 Separate settings from records at the storage level, not by filtering later.**
EllesmereUI uses `SavedVariablesPerCharacter` only for chat scrollback, Loadout Manager and Mythic run history. The Bags cross-character inventory has its own account SV "so profile exports and sync never carry inventories". The Mythic timer puts it plainly: "profile data is settings only."
Source: `EllesmereUIChat/EllesmereUIChat.toc:10`, `EllesmereUILoadoutManager/EllesmereUILoadoutManager.toc:13`, `EllesmereUIMythicTimer/EllesmereUIMythicTimer.toc:11`, `EllesmereUIBags/EllesmereUIBags_Offline.lua:7-8`, `EllesmereUIMythicTimer/EllesmereUIMythicTimer.lua:358`.
For ForeverBuddy: the P4 settings registry writes only to `ForeverBuddySettings`; records the app reads at logout stay in `ForeverBuddyDB`. Never let a setting land in `ForeverBuddyDB`, where the app's counts check would see it as data.

**6.3 Make the live settings table be the saved table, and type-check every level.**
NewDB roots each module's live `db.profile` directly at `EllesmereUIDB.profiles[active].addons[folder]`, creating any missing level with a `type(x) ~= "table"` check. There is no copy-on-save layer, so "there is nothing to snapshot".
Source: `EllesmereUI_Lite.lua:250-271`, `EllesmereUI_Profiles.lua:4048-4051`.

**6.4 Never move a SavedVariables name between addons, and never let a load-on-demand addon declare the main one.**
Child TOCs once declared `EllesmereUIDB` (added in `f6840d58`, removed in `c39911dd`). WoW still runs a child's whole WTF file when it loads, "including variables its TOC no longer declares", so a frozen copy could replace the real table and "the next logout persists the stale data over the real file (all profiles wiped)". The fix in v8.1.1 captures the parent's table at its own ADDON_LOADED and puts it back if it was swapped. The options addon stays SV-less on purpose. Simple Loot Council is a later load-on-demand member with its own SV name, which is safe.
Source: `EllesmereUI_Lite.lua:387-399`, `:401-404`, `:506-512`, commit `874cde35`; `EllesmereUISimpleLootCouncil/EllesmereUISimpleLootCouncil.toc:15-16`.
For ForeverBuddy: if P4 ever splits a module into its own addon folder, that addon gets its own SV name or none. `ForeverBuddyDB` and `ForeverBuddySettings` stay declared only in `ForeverBuddy.toc`.

**6.5 Touch saved tables only from your own ADDON_LOADED onward, and never cache an earlier reference.**
SavedVariables load after the addon's files run. A table built at file scope is orphaned when the real one arrives, so "session writes go to the orphan and are never serialized (settings silently stop saving)". Writing such an orphan back "destroys real saved data -- Bags' category list did exactly that."
Source: `EllesmereUI_Lite.lua:215-226`, `:229-240`.
For ForeverBuddy: `Data/Settings.lua` runs at file scope, before `ForeverBuddySettings` exists. It must only set its own global (the stamped payload), and `ForeverBuddy.lua` applies it at ADDON_LOADED or PLAYER_LOGIN.

### Defaults: save only what the user changed

**6.6 Split accessors into a read that never creates and a write that does.**
Forever Essentials and Loadout Manager both read through a function that falls back to defaults without creating a table, and only a write accessor creates one. In their words: "a feature nobody touches saves nothing", and "a character that never changes a setting keeps none".
Source: `EllesmereUIForeverEssentials/EllesmereUIForeverEssentials.lua:19-36`, `EllesmereUILoadoutManager/EllesmereUILoadoutManager.lua:108-127`.
```lua
local function Get(mod, key)
  local t = ForeverBuddySettings and ForeverBuddySettings[mod]
  local v = t and t[key]
  if v == nil then return Registry[mod][key].default end
  return v
end
```
For ForeverBuddy: this fits the P4 settings registry. It does not apply to `ForeverBuddyDB` records: `_meta` with counts and schema is always written, because the app checks it.

**6.7 Merge defaults by filling missing keys only, never overwriting a present value.**
DeepMergeDefaults recurses into tables, replaces a non-table with `{}` where the default is a table, and sets a leaf only when it is nil. New settings appear for existing users without a migration (our inference).
Source: `EllesmereUI_Lite.lua:169-184`.

**6.8 Do not trust a shape check that runs after a merge that already repaired the shape.**
EllesmereUI checks, right after the merge, whether any table-valued default is still not a table, and if so wipes the module's profile and warns once. The merge has already replaced every such value with `{}`, so this branch cannot fire (our inference, agreed by the checker). It looks like a repair path but is not one. If you want real validation, check values (ranges, enum members, array contents) before or instead of the merge, and reset only the one broken module.
Source: `EllesmereUI_Lite.lua:173-175`, `:284-303`.

**6.9 At logout, run every snapshot first, wrap each in pcall, then trim a copy.**
At PLAYER_LOGOUT, Lite runs pre-logout callbacks under pcall "so systems like Profiles can snapshot the full profile data before defaults are stripped", then deep-copies each profile, strips default values from the copy and stores the copy. The live tables stay untouched in case a callback still reads them.
Source: `EllesmereUI_Lite.lua:340-372`.
For ForeverBuddy: write `_meta` counts last, after every other logout write has finished, so the counts describe what is actually on disk (our inference).

**6.10 Keep array indices when pruning saved data, and test prune-then-load round trips.**
StripDefaults now keeps empty numeric-key entries ("Keep empty array entries; DeepMergeDefaults fills them on login."). Commit `3fdcf93a` blames the old behavior for a full profile wipe, but the code path for that wipe is not evident: the corruption check cannot fire after the merge (6.8) and would only reset one module anyway. The same commit also fixed ReconcileMainBarSpells, which dropped spells on incomplete data and is a likelier cause of the reported reset. The rule stands on its own merits: pruning must not change what an array means.
Source: `EllesmereUI_Lite.lua:192-195`, commit `3fdcf93a`.
For ForeverBuddy: add a `tools/addon-test` case that saves, prunes, reloads and compares, for any list we trim.

### Migrations and markers

**6.11 Version data with permanent string ids, stamped inside the data, and only on success.**
`RegisterMigration{id, scope, body}` runs each body once per host (account, each profile, each spec profile) under pcall and sets `_migrations[id] = true` only if the body succeeded, "so a failed body retries next session". Their rules: ids are forever, bodies stay idempotent even with the flag, and no live game APIs at the early phase.
Source: `EllesmereUI_Migration.lua:12-27`, `:63-77`.
For ForeverBuddy: `_meta.schema` is our version stamp for `ForeverBuddyDB`. If P4 needs settings migrations, use named ids in `ForeverBuddySettings._migrations`, not a single number we have to keep in step.

**6.12 On a fresh install, mark every global migration as already done.**
With nothing saved yet, the runner stamps all global ids at once. Otherwise "an unstamped catalog would run the whole chain at next load against whatever exists by then (e.g. an imported profile), treating current-format data as legacy". Profile-scope stamps are not pre-stamped; they travel with the profile data.
Source: `EllesmereUI_Migration.lua:127-145`.
For ForeverBuddy: a new character's `ForeverBuddyDB` starts at the current `_meta.schema`, never at 0.

**6.13 Store version stamps inside the blob they certify, so copies carry them.**
Per-profile migration flags live inside each profile, and an import takes the payload's own `_migrations`, so current data is not re-migrated and "a genuinely old string carries none and still gets migrated".
Source: `EllesmereUI_Profiles.lua:3436-3444`.

**6.14 Run cheap, idempotent repairs on every load instead of trusting a one-time flag.**
The ghost-buff purge runs at every login because imported profiles "bypass migration flags. Cheap scan, runs once at login."
Source: `EllesmereUI_Migration.lua:3382-3383`.

**6.15 Re-check the condition that justifies a destructive write at the moment you write.**
The Forever-only buff wipe tests two independent things, the suite's client flag and the raw interface number from `GetBuildInfo()`, at block entry and again inside every function that writes. "Every write lives in this block and re-checks the client at the moment it writes."
Source: `EllesmereUI_Migration.lua:4591-4602`, guards at `:4797`, `:4887`, `:5105`, `:5223`, `:5295`, `:5362`.

**6.16 Make one-time markers presence-only, frozen in name, kept out of defaults, and preserved across bulk restores.**
The `fvBuffWipe` marker lives inside the table it certifies. "PRESENCE = cleared; the value is never compared." A second clear needs a new key, and the name is in no defaults table, because "a default would stamp data without clearing it". A full-account import saves the recipient's markers before the wholesale copy and puts them back after, "so that sweep never runs twice".
Source: `EllesmereUI_Migration.lua:4550-4559`, `EllesmereUI_Profiles.lua:2583-2593`, `:2608-2615`.
For ForeverBuddy: the stamp of the last applied `Data/Settings.lua` belongs in `ForeverBuddySettings`, never in the registry defaults. Apply the slot only when its stamp differs from the saved one, then save the new stamp after the apply succeeds.

**6.17 When moving data to a new home, absorb it once, delete the old key, and guard the absorb.**
The gold ledger merges three legacy stores then sets them to nil. A per-profile tracking switch is "Absorbed once ... so an imported profile cannot flip the account switch."
Source: `EllesmereUI_GoldLedger.lua:92`, `:95-97`.

**6.18 Build character keys in one helper that migrates old keys, and do not bypass it.**
`EllesmereUI.CharKey` builds `Name-RealmGroup`, which on Forever is the ruleset name since Forever is realmless, and moves an entry from the old realm-name key. It came in `0005c5c8`. The gold ledger's own RecordCharacter deletes the old realm-keyed entry without moving it, so not every key goes through the migrating helper. That is the bug this rule prevents.
Source: `EllesmereUI.lua:1221`, `:1234-1247`, commit `0005c5c8`; `EllesmereUI_GoldLedger.lua:183-185`.
For ForeverBuddy: per-character SV means the addon needs no character key. The app does, when it merges characters, and it should key by ruleset on Forever.

### Profiles and live handles

**6.19 Keep a registry of live handles so a switch or reset swaps references without stale writes.**
SwitchProfile saves outgoing appearance into the old profile, sets `activeProfile`, and RepointAllDBs points every registered `db.profile` at the new table and re-merges defaults. A full import does the same before `ReloadUI()`, because the logout write-back "still pointing at the OLD tables ... would overwrite the freshly imported blobs".
Source: `EllesmereUI_Profiles.lua:801-840`, `:851-864`, `:2659-2665`.
For ForeverBuddy: if a P4 reset or settings apply calls `ReloadUI()`, make sure no PLAYER_LOGOUT handler in `ForeverBuddy.lua` writes from a local captured before the change.

**6.20 For settings that only apply at load, save at once and offer a reload.**
When a profile change touches fonts, skins or synced modules, the switch completes and then asks for a reload, since "a live re-point won't fully apply cached addon state".
Source: `EllesmereUI_Profiles.lua:3987-4022`, `:4104-4116`.
For ForeverBuddy: the P4 registry should flag each setting as live or reload-needed, and the window shows a reload button only for the second kind.

**6.21 When you cannot tell which bucket data belongs to, skip that write rather than guess.**
If spec profiles exist but no spec resolves, EllesmereUI skips its pre-logout appearance and layout snapshot. Module settings still save; only the snapshot that could land in the wrong profile is held back. Spec is resolved from a per-character cache first, since the live API "may be nil for brand-new characters".
Source: `EllesmereUI_Profiles.lua:971-973`, `:1018-1020`, `:4395-4415`.

### Strings, imports and private data

**6.22 Prefix, version and type every exported string, and fail each decode stage with its own reason.**
Strings are `!EUI_` plus encoded, compressed, serialized `{version, type, data, client}`. Decode rejects version below 3 ("no longer compatible. Please create a new export") and above 4 ("Please update your addon"), and each stage (prefix, decode, decompress, deserialize, type) returns `nil, reason` instead of throwing. Old prefixes get their own message.
Source: `EllesmereUI_Profiles.lua:2913-2936`, `:3068-3073`, commit `bdcbab28`.
For ForeverBuddy: treat `Data/Settings.lua` the same way. Check its version range and say which side must update (app or addon) in the briefing.

**6.23 Make every parser loop consume input or stop.**
The hand-written deserializer did not advance past a truncated or negative-length token, so a damaged string could freeze the client. Commit `cfc72a78` makes it mark the parse malformed and jump to the end, without `error()`, because "the async decode yields mid-parse, which a pcall would forbid".
Source: `EllesmereUI_Profiles.lua:295-301`, commit `cfc72a78`.

**6.24 Treat imported data as untrusted: overlay, route or drop each key, and clamp numbers.**
A profile import starts from the current profile and overlays only the imported modules. Click-cast is never imported. Spell layouts and window skins are taken out of the profile and applied to their account stores (spells only from the same client). UI scale applies only between 0.40 and 1.15. Exports from Forever carry `client = "forever"` so the other client skips spell data.
Source: `EllesmereUI_Profiles.lua:1904-1925`, `:3375-3410`, `:3421-3434`, `:3598-3601`.
For ForeverBuddy: the stamped settings slot comes from the app, but apply it the same way: known registry keys only, values checked against each key's type and range, unknown keys ignored.

**6.25 Keep other characters' data out of anything shareable, and strip it in both directions.**
A fixed list of private keys is stripped on both export paths and both decode paths, after importers "saw the exporter's characters and gold in their own Gold tooltip". The full-account export copies every root key and subtracts a named exclusion list, so new account keys ride along automatically; import refuses the same keys "so a hand-edited string cannot plant a gold ledger." Old private copies were dropped, not migrated, since they held the exporter's characters.
Source: `EllesmereUI_Profiles.lua:142-145`, `:2399-2401`, `:2427-2446`, `:2573-2575`, `:2596-2598`, commit `d0d97bd5`.

**6.26 An import adds or replaces only what it carries.**
A full import carries only the exporter's active profile and appends it to the recipient's order. Per-profile stores keep the recipient's entries except the carried profile's own entry, after an earlier version "copied every root key wholesale, so the recipient's other profiles lost their CDM spell layouts". A same-named profile is replaced.
Source: `EllesmereUI_Profiles.lua:2513-2514`, `:2625-2626`, `:2648`, commit `d1bdeaa1`.

### Records: other characters and other players

**6.27 Each character writes only its own record, and a failed read never overwrites a good one.**
The gold ledger is account-level ("a profile copy would ride export strings"), built only if a feature needs it, and "A character's entry is only ever written by that character while it is online". A read that is not a usable number is skipped.
Source: `EllesmereUI_GoldLedger.lua:5-15`, `:23-27`, `:173-181`.
For ForeverBuddy: `ForeverBuddyDB` already gets this for free by being per character. Keep the second half: never overwrite a good saved count with a nil or zero from an API that was not ready.

**6.28 Keep caches about other players in memory with eviction; save other-player data only when the user curated it.**
The mount tooltip cache of other players is session-only and prunes expired entries, "or the cache grows with every player hovered". Friend groups, assignments and notes are saved in `EllesmereUIDB.global` on purpose and survive Reset ALL. Chat history refuses to save secret values.
Source: commit `7f59a345`, `EllesmereUIChat/EllesmereUIChat_SessionHistory.lua:145`, `EllesmereUIOptions/EUI__General_Options.lua:1948-1961`.
For ForeverBuddy: anything keyed by another player's name or GUID from tooltip hooks stays in a session table. Where the client has `issecretvalue`, check it before saving any string.

**6.29 Cap every growing saved list at write time, including entry size.**
Mythic run history trims to a user-set size clamped 5 to 50 on each insert. Chat history trims to a max line count, and each line to 4096 characters.
Source: `EllesmereUIMythicTimer/EUI_MythicTimer_RunSummary.lua:79-80`, `:97-103`, `EllesmereUIChat/EllesmereUIChat_SessionHistory.lua:27`, `:155-156`, `:316`.
For ForeverBuddy: any log in `ForeverBuddyDB` gets a cap, and `_meta` counts must be taken after the trim.

**6.30 List every saved store in your reset path and decide, per store, clear or keep.**
Reset ALL replaces `EllesmereUIDB`, clears the Loadout Manager store and the Bags inventory, and carries over UI scale, friend data, a list of QoL toggles, and the uninstall and graphics backups. Chat scrollback and Mythic run history are not cleared. Whether that is intended is not stated.
Source: `EllesmereUIOptions/EUI__General_Options.lua:1939-1995`.
For ForeverBuddy: a P4 "reset settings" clears `ForeverBuddySettings` (keeping the applied-stamp from 6.16) and never touches `ForeverBuddyDB` records.

### Not for us
- Multiple named profiles, spec-assigned profiles and ProfileSync mirroring between profiles (`EllesmereUI_ProfileSync.lua`). Useful lesson if we ever copy between buckets: propagate deletions too (commit `3a41208e`) and exclude positions and sizes.
- The AceDB-compatible `profiles[name].addons[folder]` path and folder-from-SV-name derivation, which exist to read older suite data.
- Full-account export of a 25-addon suite, and the LibDeflate string format details.
