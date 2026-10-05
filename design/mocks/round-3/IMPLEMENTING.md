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
| Table | Columns: When (with relative time underneath), Type, Note, Contents, Size, actions. While the snapshot panel is open, hide Contents (the panel shows it), so dates don't wrap at 1280px. |
| Note column | Auto snapshots read "On game exit" and the like. Safety snapshots use their generated label ("Before restoring **Velyra**"). Manual snapshots use the user's label in bold. Mid-session manual snapshots add "taken mid-session". |
| Row actions | Restore is a `LockedAction` while WoW runs, plus a "more" menu. |
| Snapshot panel | Title (date and time), "Taken when WoW closed · 48.2 MB · 1,912 files", scope `Segmented` (Everything / Characters / Addons). |
| Restore tree | Checkboxes with aggregate sizes from `backup_get`: Character, then "Keybindings & macros", "Addon settings (41)", "Chat & layout". |
| Restore button | "Restore 1 character…" as a `LockedAction`, with "Unlocks when WoW closes. You can still pick what to restore." |
| Confirm dialog | "Restore Thrandor?". The one-line summary from `restore_preview`, the grouped file list, and **deletions listed separately with their own count** (mirror mode). "A safety snapshot of the current files is taken before anything changes, so you can undo this." |
| Confirm while running | Three footer steps. (a) While WoW runs: "Waiting for WoW to close…" with a live dot, and Restore locked. (b) Once WoW closes, until a `restore_preview` fetched after that moment has loaded: "WoW closed. Checking what changed…", with Restore still locked. Don't wait for `backup-created`: it never fires if game-exit backups are off or the exit backup is skipped as identical. (c) That fresh plan has loaded: Restore enables. If a later `backup-created` arrives, re-preview again. Whenever a fresh plan differs from the one on screen (deletions or file count changed), re-render the lists and add one ember line, "The list changed after WoW closed. Please check it again." Never enable Restore on a plan the user hasn't seen, and never run without a click. |
| Deletions guard | `backup_restore` refuses if the run-time deletions aren't a subset of the confirmed list. Show it as the dialog's one red callout: "Restore stopped before changing anything. More files would be removed than you confirmed." Then re-preview and show the new list with the "list changed" line. |
| Backup failed | One red callout, "Today's 14:20 backup didn't finish: drive D: is full.", with Retry and "Change location…". Also a Failed row at the top of the table. |
| Damaged snapshot | Dialog "This snapshot is damaged". "Nothing was changed. We stop before writing a single file." Lists the files, then "Verify all snapshots", Cancel, and "Use 1 Oct, 21:15 instead". |
| Interrupted restore | Startup dialog "Your last restore didn't finish" with Roll back (recommended, primary), Finish restore, and two ghost actions in the footer. "Leave files as they are" is `discard`: it changes nothing and keeps the safety copy, and its tooltip says so. "Decide later" shows a persistent stone banner on Dashboard and Backups; all restores stay locked (`RestorePending`) and backups keep running. |
| Unreadable journal | `dashboard.html?recover=unreadable`. Roll back and Finish aren't possible. The dialog says nothing has changed since, and that the safety copy is in Backups. Primary: "Open the safety copy" (goes to that Safety snapshot). Ghost: "Clear notice" (`discard`). Restores stay locked until it's cleared. |
| Data mapping | `restore_journal_status()` returns one of three kinds. `none` shows nothing. `pending` shows the recovery dialog: the "Safety copy" row and its link use `journal.original_pre_restore` (the state before the user's restore, even after a recovery attempt). `unreadable` shows this dialog, linked to `latest_safety`. If `latest_safety` is null, the primary becomes "Open Backups" filtered to Safety, and the copy says "Check Backups for a safety copy from around then". Both ghost actions call `restore_journal_resolve('discard')`. |

## 5. App status (sidebar), everywhere
The app knows WowB.exe is running, but not who is logged in.
- "WoW is running" with a live dot.
- "Last played Thrandor" (only once addon data exists).
- "Game folder found", or an ember "Game folder not set".

Never write "Online" or name a live character.

## 5b. Recent sessions (T14, `dashboard-noaddon.html`)
Each session comes from the process watcher (start and end), and the character comes from per-character WTF folder mtimes before and after the session.

| Case | Show |
|---|---|
| Running now | Ember live dot, "Today, since 19:12", the duration counting up in minutes, and "Character known after you log out". |
| One character changed | Its name. |
| Several changed (alt swap) | "Thrandor, Coinpurse". Above 2, "Thrandor and 2 others", with the full list in the tooltip. |
| None changed | "No character settings changed", muted. Never guess from last played. |
| Under 2 minutes | Hidden from the list, but counted in the weekly total. |
| Crash or kill (no clean exit) | End time = when the process vanished, and the who line gets " · ended unexpectedly". No red. |
| Crosses midnight | Listed under the start day: "Yesterday, 23:40 – 01:12". |
| Weekly total | From Monday 00:00 local time: "This week: 7h 21m across 3 characters", or "This week: none yet". |
| Empty (fresh install) | "Sessions appear here after you play. Forever Buddy notes when WoW starts and stops." |

Durations read "1h 42m" or "22m". No gold, XP or loot until the addon exists.

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

**`?case=copy`: the safety copy before a data update failed** (V5; shown only after `VACUUM INTO` *and* the plain file-copy fallback have both failed).
- **Title:** "Forever Buddy couldn't make a safety copy before updating".
- **Body:** "This version needs to update your data, and it always copies it first. The copy didn't work because <reason in plain words>. Nothing was changed." For a full disk, also show the free space against the space needed, with a small meter. The database path is red. Keep the green "Your backups and your game files are untouched."
- **Actions, in order:**
  1. Primary "Try again"; most causes, such as an antivirus lock, pass.
  2. "Open data folder".
  3. Ghost "Update without a safety copy…".
  4. Quit.
- **Confirm (`&confirm`):**
  - Title: "Update without a safety copy?"
  - Body: "If the update fails partway, your gold history and adventures may be lost. Forever Buddy would fall back to the daily copy from <date>, or start fresh if there isn't one." Then "Your game backups and your game files aren't affected."
  - A muted line: "This only applies to this update."
  - Buttons: Cancel (the default), and "Update anyway" as plain danger text, never bronze.
  - The override is one-shot and never saved, and the command only works from this state.

## 7. v0.2 (companion addon) notes
These follow `specs/v0.2-addon` §8.
- **Item tooltips** cite date and zone, never a source: "Gained 12 Sep · Molten Core". Use "Gained", not "Looted": the addon can't tell loot from quest rewards, crafting or trades, so "Looted" would over-claim. With no zone known, it's just "Gained 12 Sep". Boss *encounters* are shared in full, so a timeline line like "Defeated Baron Rivendare" is fine, but a tooltip never names a source ("from <boss>").
- **Bank and mail freshness:** the addon only sees the bank or mailbox when you open it. On those tabs, put a muted line under the tab header: "As of your last bank visit, 2 Oct" or "As of your last mailbox visit, 2 Oct". Older than 7 days, use the ember colour with "Visit the bank in-game to refresh". If it has never been seen: "Not seen yet. Open your bank once in-game and it appears here."
- **AH-derived numbers stay hidden until v0.4.** Remove them and let the layout close up; never show zeros or dashes. That means:
  - **Characters header:** drop "Net worth", leaving "Gold" and "Items".
  - **Character sheet:** drop the "Worth carried" stat, leaving four stats.
  - **Ledger:** use three tiles (Account gold, Last 30 days, Best earner). Drop the Net worth panel, and let the gold chart take the full width with the Journal below.
  - **Session recap:** drop "≈ 45g" worth cells, keeping quantities only.
  - **Search:** hidden until satchels are indexed.

## 8. F3: lockouts and the bank alt (v0.2.1)
The addon records `lockouts[] = {name, difficulty, reset_at, raid}` at login, for saved instances only. There's no boss progress, so don't show any.
- **Character sheet (`character.html`):** add a stone **Lockouts** panel in the side column, after Professions.
  - The meta reads "as of login, 4 Oct", using the snapshot's date.
  - Show one row per saved instance, soonest reset first. The name sits left and the time to reset right: "resets in 14 h" under a day, "2 days" otherwise. The row's tooltip holds the full date: "Resets Tuesday 6 October, 09:00".
  - Add the difficulty after the name in muted text only when it isn't the instance's only size.
  - Drop rows whose `reset_at` has passed, since they're no longer saved.
  - If none are left, show "Not saved anywhere this week." in muted text. If lockouts have never been seen, show "Lockouts appear after your next login."
- **Dashboard (`dashboard.html`):** add a **Lockouts this week** panel under Characters, shown only with addon data and only when someone is saved. Otherwise leave it out and let the column close up.
  - Group by instance, soonest reset first. Each row has the instance and the time to reset, with the saved alts' names in class colour on the line below.
  - The meta reads "N characters saved". Show at most five rows.
- **Bank alt:** a switch labelled "Bank alt" in the sheet's top bar, left of the alt arrows. It's set by the user, stored in the app's database, never written to the game, and off by default.
  - When it's on, the card shows the "Bank" tag after the name (`characters.html`) and the Dashboard roster shows "bank" (`dashboard.html`). Nothing else changes: sorting and totals stay the same.

## 9. F4: Addons, read-only (`addons-readonly.html`)
This is the first cut of `addons.html` (the v0.5 sketch). **Nothing on this screen writes.** That means no sets, installs, updates, toggles, sources or sizes, no running callout, and no locked buttons. The sidebar's "Addons · soon" becomes a normal entry.
- **Header:**
  - The lede reads "51 installed in _classic_beta_\Interface\AddOns · WoW: Forever reads Interface 16001". Take the interface number from the flavour, not a constant, if it's known.
  - The one action is a stone "Open AddOns folder". This screen has no bronze primary.
- **Toolbar:**
  - A filter field that matches the name or author.
  - A `Segmented` with counts: "All 51 · Out of date 2 · Off everywhere 3".
  - Meta on the right: "Read from each character's AddOns.txt · 4 min ago".
- **Table:**
  - **Addon:** a letter tile, the TOC title, and the version underneath.
  - **Author:** hidden under 1100px.
  - **Interface:** the number. If it's lower than the game's, use warn colour and "11504 · out of date".
  - **Enabled for:** one class-coloured diamond per character, hollow when off, then "all", "none" or "3 of 5". The tooltip lists "Name: on/off".
  - Sort by title. Clicking a row selects it, with an ember left edge as in `DataTable`.
- **Side panel (the selected addon, the first one by default):**
  - The title, with the version in the meta.
  - The TOC `## Notes` as a sentence.
  - Author, Interface, "Needs" (from `## Dependencies` / `## RequiredDeps`, or "nothing else"), and Folder (start-truncated, full path in the tooltip).
  - **Enabled for:** each character in class colour with "on" or "off", and off rows dimmed.
  - The closing note: "Toggling addons comes later, with a safety snapshot first. For now, change them in-game from the AddOns button on the character screen."
  - For an out-of-date addon, add a warn note: "Built for an older interface. WoW loads it only with 'Load out of date AddOns' checked."
- **Reading TOCs:**
  - Strip WoW colour codes (`|cffRRGGBB…|r`) and texture tags (`|T…|t`) from titles and notes before showing them. Render everything as React text.
  - Skip `Blizzard_*` folders.
  - When a folder has several TOCs, use the one the game would load for this flavour, and fall back to the plain `<Folder>.toc`.
  - A folder without a TOC isn't an addon, so leave it out of the count.
- **Enabled state:**
  - Read `WTF/Account/*/<group>/<Character>/AddOns.txt`. A missing line means the TOC's `## DefaultState` (enabled if unset). Characters without an AddOns.txt use the defaults.
  - Show the same characters as Characters, older folders excluded (W1b).
- **Empty:** if no AddOns folder or no addons exist, show one stone panel reading "No addons in _classic_beta_\Interface\AddOns yet.", with Open AddOns folder.

## 10. F5: auction prices from Auctionator (lifts §7's hiding)
Prices come from Auctionator's own scan database, never a live feed. So every number they produce is an estimate with an age, and it says so. Use the round-3 mocks (`ah.html`, `gold.html`, `characters.html`, `character.html`, `session.html`) with these changes.
- **Only with data:** while there's no Auctionator price database, everything in §7 stays hidden, exactly as now. Unhiding starts at the first ingested scan, never with zeros or dashes.
- **Which price:** an item's worth is its **lowest buyout from the latest scan that saw it**. Use the median only in Price history. Keep the realm and faction that match each character; if a character's realm has no scan, its items are unpriced.
- **Wording for any AH number:**
  - Mark worth with "≈", as the mocks do: "≈ 506g".
  - Every surface with AH numbers carries one freshness line: "Prices from your AH scan 3 days ago" (`gold.html`). Past 7 days it turns ember and adds "Scan the AH in-game to refresh."
  - The AH screen's scan bar reads "Last scan: 3 days ago · 1 Oct, 21:04 · 4,812 prices". Drop the mock's "by Coinpurse", since Auctionator doesn't record who scanned.
  - Item tooltips add "Last scanned: 36g 40s each · seen 12 times" under the sell price, never a source.
- **Unpriced items:**
  - Count them, don't zero them: "673 items have no price yet" in the freshness line.
  - Totals built from partial prices still show "≈". An item with no price shows nothing in its worth cell, never "0".
- **What's never priced:**
  - Equipped gear: the recap's Worth cell reads "equipped".
  - Soulbound items, if the item info says so: skip them, as the Worth selling footer says.
  - Coins.
  - Worth carried and Net worth are gold plus priced bag, bank and mail items.
- **Where it returns:**
  - **Ledger:** the Net worth tile (a fourth tile) and the Net worth panel per `gold.html`. The chart goes back to its mock width.
  - **Characters:** "Net worth" in the header, and the Value column and "≈ at last scan" in search.
  - **Sheet:** the fifth stat, "Worth carried".
  - **Recap:** the Worth cells.
  - **Dashboard:** the Game folder panel's "Auction prices are 3 days old" row, shown only once prices exist, in warn colour past 7 days.
- **No Auctionator:** the Auction House nav entry stays live, and the screen shows one stone panel: "Prices come from the Auctionator addon's scans. Install Auctionator, scan once at the auction house, and prices appear here." Add a link to the Addons screen if Auctionator is installed but disabled.
- **The Sales ledger** in `ah.html` stays out until mail invoices are read (matrix row 43). Leave it out and let the layout close up.

## 11. F6: turning addons on and off per character (`addons-readonly.html?state=…`)
The side panel's "Enabled for" list becomes editable. Everything else in §9 stands. The states are `?state=edit`, `staged`, `applied`, `running` and `linked`; without `?state`, the page is the F4 read-only screen.
- **Switches:**
  - Each character row gets a switch on the right, in place of "on" / "off". Off rows keep their dimmed name.
  - "On for everyone" sits at the right of the "Enabled for" heading, and sets every switch on. It's staged like any other change.
- **Stage, then apply. Never write on click:**
  - Changed rows show "changed" in ember before the switch.
  - Under the list, an apply bar reads "1 change" (or "3 changes"), with ghost "Discard" and bronze **Apply**, the screen's only primary.
  - Switching to another addon with changes pending keeps them, and the count covers all addons.
- **Apply:**
  - It goes through the write gate: one safety snapshot of the affected characters' AddOns.txt first, then an atomic replace per file.
  - On success, the bar turns into a green check line: "Questie is on for Thrandor. A safety snapshot was taken first." For several changes: "4 changes applied. A safety snapshot was taken first." Add a ghost **Undo**, which restores that snapshot.
  - The line stays until the next change or until you leave the screen.
  - On failure, use one red line under the list with the reason, keep the changes staged, and change nothing on disk.
- **WoW running:**
  - Show the ember callout above the toolbar: "**WoW is running, so addon changes are locked.** WoW rewrites each character's AddOns.txt when it closes, so changes wait until then." Add the live dot.
  - The switches show their state but are locked (dimmed, `not-allowed`, title "Close WoW first"). There's no apply bar.
  - If WoW starts while changes are staged, keep them and lock Apply as a `LockedAction`.
- **Refused characters:** if a character's settings folder is a link, its row shows "linked folder" with a link icon instead of a switch. The tooltip reads "Velyra's settings folder is a link to another place, so Forever Buddy won't write there. Change it in-game." The same treatment applies to any character the write gate refuses for a reason known in advance.
- **The note** under the panel becomes "Changes are written to each character's AddOns.txt when you apply them. A safety snapshot is taken first, so you can undo." While WoW runs, hide it, since the callout says enough.
- **The table** keeps showing the saved state. Its diamonds update only after Apply succeeds.

## 12. F7: Macros, read-only (`macros-readonly.html`)
This is the first cut of `macros.html` (the v0.6 sketch). **Nothing on this screen writes.** That means no editing, icon picker, library, "New macro" or copy-to-character. The sidebar's "Macros · soon" becomes a normal entry. This screen has no bronze primary.
- **Header:** "Macros", with the lede "From each character's macros-cache.txt, as of their last logout". It has no actions.
- **Toolbar:**
  - A `Segmented` "Account 18 · Character 11", followed by the character (class dot and name in class colour) with a ghost "Change" that lists the WTF roster (older folders excluded, W1b).
  - Meta on the right: "18 account macros · 11 for Thrandor". Don't show slot limits until the flavour's limits are confirmed.
- **List (left):**
  - One row per macro, in the file's order: a letter tile, the name, and the character count on the right.
  - The count turns warn colour at 230 and above, and red above 255.
  - The selected row gets the ember left edge.
- **Viewer (right):**
  - The header shows the name, with the meta "character macro · as of logout, 3 Oct" (or "account macro"), using the file's modified date.
  - Below that: the tile, the name, "Thrandor · slot 1", and a stone **Copy** that puts the body on the clipboard. Afterwards show "✓ Copied" in green for about 2 s.
  - The body sits in the dark code block, selectable, with light tinting: `#showtooltip`/`#show` in tan, `/commands` in ember, `[conditions]` in blue, and the rest plain. Tokenize it simply and render it as React text, never as HTML.
  - Count bar: "231 / 255", with "24 left" in warn colour at 230 and above. Above 255, use red and "7 over: WoW cuts it off at 255".
  - Note: "Editing macros and copying them to other characters come later, with a safety snapshot first. For now, Copy puts the text on your clipboard to paste in-game."
- **Icons:** the file gives an icon id or name, but there's no icon media yet (row 20), so use the first letter of the macro's name. The `INV_MISC_QUESTIONMARK` default is a letter too.
- **Empty states:**
  - With no macros-cache.txt for the character: "No macros for Thrandor yet. WoW writes them when you log out."
  - With an empty account list: "No account macros yet."

## 13. F8: real item icons (`icons.html`)
Icons come from the player's own game files and replace the letter tiles in the **same frame**, so nothing moves when they arrive.
- **One frame, everywhere:**
  - A rounded box (3 px radius) with a 1.5 px border in the item's quality colour and a 1 px black inner ring. 56 px and larger use a 2 px border.
  - The art fills it, scaled up so about 7% is cropped on each side. Blizzard's icons have a dark rim baked in, and without the crop it doubles our border.
  - This replaces `.d-ico`, `.ch-ico` and the mocks' `.ico` in one component, with a `size` prop of 18, 24, 36 or 56 px.
- **Quality colours:**
  - On stone (bright): poor `#6f6a62`, common `#5d5852` (deliberately dim, since a white border on every common item is noise), uncommon `#58d23c`, rare `#4f95ff`, epic `#b86cf0`, legendary `#ff8000`.
  - On parchment: the inked palette we already use for names (common `#8c7b62`, uncommon `#23710f`, rare `#1a4f9c`, epic `#6b2a9a`, legendary `#a85400`), plus a 1-2 px shadow so the art sits on the page.
- **Stack counts:** only where a count belongs on the icon (bag grids), in white bottom right with a black outline. In lists, the count stays in its own column as now.
- **Loading and failure:** show the letter tile until the icon is ready, then swap the art in. No shimmer, spinner or fade longer than about 120 ms. A missing or undecodable icon stays a letter tile, with no error at the item.
- **Where:** every letter-tile spot: the sheet's gear and bags, Characters search results, the Ledger journal's "Of note", the recap's Gained / Spent and timeline, the AH (watchlist, Worth selling, Price history), and the Dashboard's Last adventure.
- **Settings › Game › Game data cache:**
  - One row: "Game data cache", described as "Item icons read from your own game files and kept on this PC. Never uploaded or shared. Rebuilt automatically after a game patch."
  - Controls: a stone **Rebuild** and a ghost **Clear**. Clear needs no confirm, since it rebuilds on its own.
  - Under the row, a small details block:
    - **ok:** Status "Up to date" (green), Icons "1,284 · 18.4 MB", Game build, and Read from "…\World of Warcraft\Data (read only)", with the full path in the tooltip.
    - **building:** a live dot with "Reading icons from your game · 412 of 1,284" and a meter. Rebuild and Clear are disabled. Add "(new since last time)" after the build if a patch triggered it.
    - **failed:** one warn line (amber, not red): "Couldn't read the game's art files, so items show letters instead. Nothing else is affected. Details". Details opens the log entry.
    - **empty:** "Not built yet. Icons are read the first time you open a screen with items."
- **Never redistributed:** no icons in exports, screenshots for sharing, release assets, or the shots harness (it keeps the letter tiles).
