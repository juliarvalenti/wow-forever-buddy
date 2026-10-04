# Implementing round 3 in the app (T11 and after)

T11 wires real data into **plain** screens. The goal of this guide is that those plain screens need no restructuring when the D styling lands later; only a handful of components get restyled. Mocks to copy from: `backups.html` (incl. `?confirm`, `?error=backup`, `?error=corrupt`), `dashboard.html?recover`, `startup-error.html`, and the sidebar status in `_shell.js`.

## 1. Build from this component vocabulary

Make these small React components now, even if each is just a styled `div` in T11. Their names follow the D materials, so the later styling pass restyles them in one place and the screens don't change.

| Component | Mock class | Material / rule |
|---|---|---|
| `Page`, `PageHeader` (title, lede, actions) | `.main`, `.head`, `h1`, `.lede`, `.actions` | Stone hall. Title uses the display face. |
| `Panel`, `PanelHeader`, `PanelBody` | `.panel`, `.ph`, `.pb` | Stone slab with a leather-strap header. For live/system data. |
| `Record` | `.parchment` (+ `.ruled`) | Parchment. Only for records (session recaps, ledger). Not needed in T11. |
| `Tile` | `.tile` (`.k`, `.v`, `.s`) | Stat tile: label, value, sub-line. |
| `PrimaryButton` | `.primary` | Bronze. **At most one per screen.** |
| `Button` (`variant: default \| ghost \| icon`) | `.btn`, `.btn.ghost`, `.btn.icon` | Everything else. 28px tall. |
| `LockedAction` | `.lockact` | Any write while WoW runs: muted, lock icon, `cursor: not-allowed`, a `title` saying why. Never a disabled `PrimaryButton`. |
| `Callout` (`tone: ember \| bad`) | `.callout` | One per screen. Ember for "WoW is running", red only for the one failed thing. |
| `Pill` (`kind: auto \| manual \| safety \| ok \| warn \| bad`) | `.pill.*` | Snapshot type and status badges. |
| `Segmented` | `.seg` | Filters and scope pickers. |
| `Checkbox` | `.cb` | Restore tree. |
| `DataTable` (selectable rows) | `table.grid`, `tr.sel` | 40px rows; the selected row gets an ember left edge. |
| `Dialog` (header, body, footer) | `.scrim`, `.dialog`, `.foot` | Confirm restore, recovery, damaged snapshot. |
| `LiveDot`, `StatusDot` | `.live`, `.okdot` | Ember pulse = live or pending. Green = OK. |

**Don't** use the scaffold's warcraftcn `Button`, `Card`, `Badge`, `Tabs`, or the `fantasy` class in new screens. Their blue and gold art and Cinzel capitals aren't part of D.

## 2. Tokens and fonts
- Port the `:root` variables at the top of `round-3/_d.css` (stone, leather, parchment and ink, ember, bronze, ok/bad/warn, class colours) into `src/index.css`, and expose them through Tailwind `@theme` so utilities can use them. Leave the procedural textures (`--grit`, `--mottle`, `--hidegrain`, `--fiber`, `--stain`) for the styling pass.
- Fonts:
  - Geist is already installed.
  - Add `@fontsource/marcellus` as the display face, for page titles, the wordmark, the seal number and badge glyphs. It has one weight, so never set it bold.
  - Georgia (system) is for parchment headings.
  - Drop the `@fontsource/cinzel` import once nothing uses `fantasy`.
- Type rule:
  - No `text-transform: uppercase`, no letter-spaced labels, no em dashes in copy.
  - Sentence case everywhere.
  - Numbers use `font-variant-numeric: tabular-nums`.

## 3. CSP (T11 scope)
The spec's CSP (`default-src 'self'; img-src 'self' asset: data:; style-src 'self' 'unsafe-inline'`) works with D, because the textures are `data:image/svg+xml` backgrounds and the fonts are bundled by Vite (`'self'`). **Keep `data:` in `img-src`**, or every texture silently disappears. Verify in a production build that Geist and Marcellus load and the stone grain shows.

## 4. Backups screen: states and copy

| Element | Spec |
|---|---|
| Header | "Backups", lede "Snapshots of your WTF and SavedVariables, taken every time the game closes." Actions: ghost "Schedule", primary "Back up now" (backing up is allowed while WoW runs). |
| Running callout | "**WoW is running, so restores are locked.** Backing up is safe now. Close the game to restore; WoW rewrites these files when it exits." with a live dot. |
| Filter | Segmented control: "All 23 · Auto 18 · Manual 4 · Safety 1". |
| Type labels | `manual` → Manual. `app_start`, `game_exit`, `scheduled` → Auto. `pre_write`, `pre_restore` → Safety. |
| Retention and budget | Show `backup_storage().retention_summary` **verbatim**, then "1.08 GB of 5 GB" with a meter. |
| Table | Columns: When (with relative time underneath), Type, Note, Contents, Size, actions. |
| Note column | Auto snapshots read "On game exit" and the like. Safety snapshots use their generated label ("Before restoring **Velyra**"). Manual snapshots use the user's label in bold. Mid-session manual snapshots add "taken mid-session". |
| Row actions | Restore is a `LockedAction` while WoW runs, plus a "more" menu. |
| Snapshot panel | Title (date and time), "Taken when WoW closed · 48.2 MB · 1,912 files", scope `Segmented` (Everything / Characters / Addons). |
| Restore tree | Checkboxes with aggregate sizes from `backup_get`: Character, then "Keybindings & macros", "Addon settings (41)", "Chat & layout". |
| Restore button | "Restore 1 character…" as a `LockedAction`, with "Unlocks when WoW closes. You can still pick what to restore." |
| Confirm dialog | "Restore Thrandor?". The one-line summary from `restore_preview`, the grouped file list, and **deletions listed separately with their own count** (mirror mode). "A safety snapshot of the current files is taken before anything changes, so you can undo this." |
| Confirm while running | Footer reads "Waiting for WoW to close…" with "Restore enables automatically when it exits." Restore enables on `game://status-changed` but never runs by itself. |
| Backup failed | One red callout, "Today's 14:20 backup didn't finish: drive D: is full.", with Retry and "Change location…". Also a Failed row at the top of the table. |
| Damaged snapshot | Dialog "This snapshot is damaged". "Nothing was changed. We stop before writing a single file." Lists the files, then "Verify all snapshots", Cancel, and "Use 1 Oct, 21:15 instead". |
| Interrupted restore | Startup dialog "Your last restore didn't finish" with Roll back (recommended, primary), Finish restore, and two ghost actions in the footer. "Leave files as they are" is `discard`: it changes nothing and keeps the safety copy, and its tooltip says so. "Decide later" shows a persistent stone banner on Dashboard and Backups; all restores stay locked (`RestorePending`) and backups keep running. |
| Unreadable journal | `dashboard.html?recover=unreadable`. Roll back and Finish aren't possible. The dialog says nothing has changed since, and that the safety copy is in Backups. Primary: "Open the safety copy" (goes to that Safety snapshot). Ghost: "Clear notice" (`discard`). Restores stay locked until it's cleared. |

## 5. App status (sidebar), everywhere
The app knows WowB.exe is running, but not who is logged in.
- "WoW is running" with a live dot.
- "Last played Thrandor" (only once addon data exists).
- "Game folder found", or an ember "Game folder not set".

Never write "Online" or name a live character.

## 6. Startup error (T11 scope)
When `AppCore::new` fails, open the window anyway and show `startup-error.html`: no sidebar, one stone panel.
- **Title:** says what happened in plain words.
- **Reassurance:** "Your backups and your game files are untouched."
- **Paths:** the data paths, with the one at fault in red:
  - `%LOCALAPPDATA%\com.juliarvalenti.wowforeverbuddy\buddy.db`
  - `%APPDATA%\com.juliarvalenti.wowforeverbuddy\settings.json`
  - `…\backups`
- **Actions:** primary "Open data folder", then "Copy error details" and Quit.
- **Error details:** collapsed under "Error details".

Cases: `?case=newer` (default: the database is from a newer version) and `?case=settings` (unreadable settings).
