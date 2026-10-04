# Round 3: language "D", full app

This is the complete set of mocks for engineering. The material rules are unchanged from `../round-2/README.md`:
- stone/leather = frame and system data
- parchment = records
- tooltip glass = item hover
- bronze = the one primary action
- ember = live/pending
- gold = coins only

Open any `.html` directly. `?still` freezes motion and `?tt=<item>` pins a tooltip. PNGs at 1280x800 and 1024x700 are in `shots/`, and `errors.html` is a gallery linking every first-run and error state.

## Screens and states

| Area | File | States (query flags) |
|---|---|---|
| **First run** | `onboarding.html` | `found` (default), `?state=detecting`, `?state=notfound`, `?state=pick` |
| **No addon yet** | `dashboard-noaddon.html`, `characters-noaddon.html` | **v0.1 (default):** step 1 reads "Coming in the next update", with no Install button, and "Back up now" is the bronze action. **`?v=0.2`:** the bronze "Install addon" step (locked while WoW runs). Both show only what the app knows: folder, WTF characters, backups, WoW running |
| Dashboard | `dashboard.html` | `?recover` (restore interrupted: Roll back / Finish), `?folder=missing` |
| Backups | `backups.html` | `?confirm`, `?error=backup` (disk full), `?error=corrupt` (damaged snapshot) |
| Characters | `characters.html`, `character.html`, `portraits.html` | `?q=Runecloth` |
| Ledger / Adventures | `gold.html`, `session.html` | session timeline with Forever "secret values" fallbacks |
| Auction House | `ah.html` | scan freshness, price history, sales ledger, sell suggestions |
| Settings | `settings.html` | spec-accurate retention, Integrations |
| Sketches | `addons.html` (v0.5), `macros.html`, `weakauras.html` (v0.6) | writes locked while WoW runs |

## Round-3 decisions engineering should know
- **Forever facts in copy:** `C:\Program Files (x86)\World of Warcraft\_classic_beta_`, `WowB.exe`, "WoW: Forever (Beta)" 1.60.1, interface 16001. Detection stays data-driven; the folder may move at the 4 Nov launch.
- **Retention text comes from the backend:** `backup_storage().retention_summary` is shown verbatim, e.g. "Auto: 48 h, then daily for 2 weeks, weekly for 2 months · manual kept forever". There's no editable retention in v0.1. Settings shows the policy read-only, plus the 5 GB budget meter and "Prune now".
- **No-addon state:**
  - Never show broken zeros. Unknown values read "—" or "needs addon".
  - Names are neutral (no class colour) and use a plain silhouette portrait.
  - "Install addon" is the bronze primary but is a write, so it's locked while WoW runs.
- **Secret values (Forever hides some combat data from addons):**
  - Timeline lines degrade naturally: "Died in Stratholme" with no killer, "Looted Truestrike Shoulders" with no source. A quiet ◌ marker explains what was hidden, and there's one footnote per page.
  - Item tooltips cite time and zone, never the source.
- **Forever beta SavedVariables bug:** one muted line you can dismiss, wherever addon settings get restored (the snapshot panel and the confirm dialog).
- **The v0.1 build ships without the companion addon** (it's v0.2). Use the default `dashboard-noaddon.html` state for v0.1, and the `?v=0.2` state once the addon exists.
- **Interrupted restore → "Decide later" rule:**
  - Until the user picks Roll back or Finish restore, show a persistent stone banner on Dashboard and Backups ("Your last restore didn't finish", with Roll back and Finish).
  - Keep every restore locked, using the same `.lockact` treatment as when WoW is running.
  - Backups still run, and the restore journal stays until it's resolved.
- **Auction House scanning is addon-dependent:** "press Scan at the Auction House" assumes the ForeverBuddy addon's scan button (v0.4). Until then, prices come only from an existing Auctionator database, if one is installed, and the freshness bar should name that source.
- **Errors:**
  - Red is used only for the one failed thing.
  - Recovery choices are explicit: Roll back is the recommended bronze action, Finish restore is the alternative, and "Decide later" is also offered.
  - A corrupt snapshot stops before any write is made, and the dialog says so.
- **Shared components** in `_d.css`: `.lockact` (locked write), `.cb`, `.wax`, `.empty`.
- **Shell flags:**
  - `data-wow="running|closed|nofolder"`
  - `data-addon="none"`
  - `data-portrait="crest|armory|shot"`
  - `data-tt` / `data-pin`
  - `window.EXTRA_ITEMS`
- **Still page-local, promote when building:**
  - plain silhouette portrait (no-addon pages)
  - `.primary.locked`
  - `.sketch` pill
  - paper chart hover readout (`.ctip` / `.stip`)
  - `.field` value ellipsis
  - `.empty` is a generic name. Consider renaming it to `.well-empty` to avoid collisions like `.bar.empty`.
