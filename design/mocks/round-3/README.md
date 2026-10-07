# Round 3: language "D", full app

This is the complete set of mocks for engineering. The material rules are unchanged from `../round-2/README.md`:
- stone/leather = frame and system data
- parchment = records
- tooltip glass = item hover
- bronze = the one primary action
- ember = live/pending
- gold = coins only

Open any `.html` directly. `?still` freezes motion and `?tt=<item>` pins a tooltip. Screenshots (WebP, 1280x800 and 1024x700) are in `shots/`, and `errors.html` is a gallery linking every first-run and error state.

**Building it?** Start with [`IMPLEMENTING.md`](IMPLEMENTING.md). It covers the component vocabulary for plain screens, tokens and fonts, CSP, the Backups states and copy, and the startup error.

## Screens and states

| Area | File | States (query flags) |
|---|---|---|
| **First run** (O1, IMPLEMENTING §20) | `onboarding.html` | step 1: `found` (default), `?state=detecting`, `?state=notfound`, `?state=pick`. Then `?state=backup`, `addon`, `addon-running`, `extras`, `done` |
| **No addon yet** | `dashboard-noaddon.html`, `characters-noaddon.html` | **v0.1 (default):** step 1 reads "Coming in the next update", with no Install button, and "Back up now" is the bronze action. **`?v=0.2`:** the bronze "Install addon" step (locked while WoW runs). Both show only what the app knows: folder, WTF characters, backups, WoW running, and **recent sessions from the process watcher** (start, end, duration, and the character whose WTF folder changed; the running session says "Character known after you log out"; no gold or loot) |
| Dashboard | `dashboard.html` | `?recover` (restore interrupted: Roll back / Finish), `?folder=missing` |
| **Can't start** | `startup-error.html` | `AppCore::new` failed. No sidebar. `?case=newer` (default, a database from a newer version) or `?case=settings` (unreadable settings) |
| Backups | `backups.html` | `?confirm`, `?error=backup` (disk full), `?error=corrupt` (damaged snapshot) |
| Characters | `characters.html`, `character.html`, `portraits.html` | `?q=Runecloth` |
| Ledger / Adventures | `gold.html`, `session.html` | session timeline with Forever "secret values" fallbacks |
| Auction House | `ah.html` | scan freshness, price history, sales ledger, sell suggestions |
| Settings | `settings.html` | spec-accurate retention, Integrations |
| **Approvals** (P2b) | `approvals.html` | pending plan, note conflict and list change; `?state=empty`, `?state=off`. Spec in IMPLEMENTING §17 |
| Sketches | `addons.html` (v0.5), `macros.html` (v0.6) | writes locked while WoW runs |

**WeakAuras is dropped** (Julia, 4 Oct). It doesn't run on Forever because of the Midnight addon restrictions, so there's no WeakAuras page, no sidebar entry and no Wago.io integration. The Wago Addons key stays, because it's for normal addon updates.

## Round-3 decisions engineering should know
- **Type rule (Julia):**
  - No all-caps anywhere (display type included), no letter-spaced labels, and no em dashes anywhere in the copy.
  - The display face is **Marcellus** (OFL, a mixed-case flared serif close to WoW's own UI face). Use it for page titles, the wordmark, the seal number and badge glyphs. It has one weight, so never set it bold. The app should bundle `@fontsource/marcellus`; the mocks vendor it in `round-3/fonts/`.
  - Everything else is sentence case: Geist on stone, Georgia on parchment.
- **Forever facts in copy:** `C:\Program Files (x86)\World of Warcraft\_classic_beta_`, `WowB.exe`, "WoW: Forever (Beta)" 1.60.1, interface 16001. Detection stays data-driven; the folder may move at the 4 Nov launch.
- **Retention text comes from the backend:** `backup_storage().retention_summary` is shown verbatim, e.g. "Auto: 48 h, then daily for 2 weeks, weekly for 2 months · manual kept forever". There's no editable retention in v0.1. Settings shows the policy read-only, plus the 5 GB budget meter and "Prune now".
- **No-addon state:**
  - Never show broken zeros. Unknown values read "needs addon" or are left blank.
  - Names are neutral (no class colour) and use a plain silhouette portrait.
  - "Install addon" is the bronze primary but is a write, so it's locked while WoW runs.
- **Secret values (Forever hides some combat data from addons):**
  - Timeline lines degrade naturally: "Died in Stratholme" with no killer, "Gained Truestrike Shoulders" with no source (the addon can't tell loot from quest rewards or trades, so never "Looted"). A quiet ◌ marker explains what was hidden, and there's one footnote per page.
  - Item tooltips cite time and zone, never the source.
- **No SavedVariables caveat on restore.** The beta bug where the client didn't reload SavedVariables was fixed in build 70009 (see specs/feature-matrix), so the round-3 caveat line was removed.
- **The app can't know who is playing right now.** It detects that WowB.exe is running (process watcher), but character data only arrives when the addon writes at logout or /reload. So nothing claims a live character: the sidebar says "WoW is running" plus "Last played Thrandor", the Game tile shows the session length and the last-played name, and character cards say "Last played yesterday", never "Online".
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
