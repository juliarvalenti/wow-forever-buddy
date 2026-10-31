## Appendix: gaps in ForeverBuddy's addon today

What the chapters found in `src-tauri/resources/addon/ForeverBuddy/` at main `a1a1944` (addon 0.9.6). Each line ref was re-checked by hand. Fixes are small unless marked. None of these is a known live bug: they're where the addon is weaker than the rules above.

### Fix soon (small)

- **The tooltip post-call doesn't check `IsForbidden`.** `TooltipDataProcessor.AddTooltipPostCall(Enum.TooltipDataType.Item, …)` (`ForeverBuddy.lua:1553`) adds lines without checking `tooltip:IsForbidden()` or its owner's. Bail early if either is forbidden (3.19).
- **The record-mode `arg1` tests `v == nil` before `isSecret(v)`** (`ForeverBuddy.lua:4230-4236`). Swap the two tests (2.2).
- **Bag hooks allocate on every bag open.** `C.hook` (`:2795-2808`) makes a new closure and a new `C_Timer.After(0, …)` each time a bag opens. Use one file-level callback behind a pending flag (4.12).
- **The keyring isn't counted.** `carried()` scans bags 0 to 5 (`:298`, `:812`), so keys on the keyring (bag -2) never count as carried (2.19).

### Before P4's modules

- **Events are registered for every handler at load** (`:4977-4981`). Each module should register its events only while its setting is on (1.19, 4.1).
- **The file is at the 200-locals limit.** Split it along its seams (session recording, tooltip, plan, briefing, window), each file sharing the `...` namespace (1.20, 1.22). Do the split as a pure move in its own PR (7.6). Today the file takes only `ADDON_NAME` from `...`; take `ns` too.
- **Settings registry rules** (chapter 6):
  - settings live in `ForeverBuddySettings`, never `ForeverBuddyDB`;
  - the applied-slot stamp is saved only after a successful apply;
  - unknown keys and out-of-range values in `Data/Settings.lua` are ignored;
  - each setting is flagged as live or needing a reload.
- **Conflict lists** per module: Bags against Baganator, Bagnon and AdiBags, and so on (8.14).

### Process

- **No PR template for addon changes.** A first checklist (7.14):
  - off costs nothing;
  - within handler budgets;
  - `_meta.counts` intact;
  - every API through `read()`;
  - TOC `Version` and `VERSION` bumped together.
- **A mocked retail-only global passes the harness.** `forever.lua` requires a row for every API, but can't tell a wrong one. Add a denylist of globals Forever lacks (R3a, parked, has this as the `forever_globals` scenario) (7.4).
- **No local dry run of the addon zip.** It's built only in `release.yml` (7.18).
- **Non-ASCII in source:** about 98 lines. They're harmless with our 7z packaging, so this is low priority. Write new glyphs as byte escapes (1.24, 7.5).

### Leads to confirm in game

- **A Sync button instead of "Type /reload to sync".** EllesmereUI reloads from plain button clicks on Forever, calling `ReloadUI()` first in the click (5.22). One click in game confirms bridge §4's route (A).
- **`## AllowLoadGameType: camelot` in our TOC** would state our Forever-only scope. Test on the live client first (1.2).
