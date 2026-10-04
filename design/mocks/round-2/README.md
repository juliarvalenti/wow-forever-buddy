# Round 2: design language "D"

Julia picked tile D (`../style-tiles/d-mix.html`). Every screen in this folder uses it.
Open any `.html` directly. Query flags:
- `?still` freezes motion.
- `?tt=<item>` pins an item tooltip, for screenshots.
- `?confirm` and `?q=Runecloth` show the dialog and search states.

PNGs at 1280x800 and 1024x700 are in `shots/`.

## Material rules (for engineering)

Each material has exactly one job. If you're unsure which to use, ask "is this a record of something that happened?"

| Material | Job | Where | CSS |
|---|---|---|---|
| **Stone** | The frame, plus live/system data | Stat tiles, game folder, roster, backups, settings, tables of current state | `.panel`, `.tile` |
| **Leather** | Structure: sidebar and panel headers (stitched straps) | Sidebar, `.ph` on stone panels | `.side`, `.ph` |
| **Parchment** | Records: things that happened | Last adventure, session recap, Ledger / Journal pages, character sheets | `.parchment` (+ `.ruled` for ledger ruling, `.tilt` for one hero page per view) |
| **Tooltip glass** | Every item hover, with in-game tooltip content | Anything with `data-tt` | `.tt` |
| **Bronze** | The ONE primary action per view (never two) | "Back up now", etc. | `.primary` |
| **Ember** | Live or pending state | "WoW is running", waiting-for-WoW, the active-row edge | `.live`, `.ember` |
| **Red ribbon / wax** | "You are here" and celebration | Active nav item, level-up seal | `.nav-item.active`, `.seal` |
| **Gold** | Coins only. Nothing else is gold. | Money amounts | `.coins` |

### Other rules
- **Textures are procedural** (SVG noise in `_d.css`: `--grit`, `--mottle`, `--hidegrain`, `--fiber`, `--stain`). Don't add decorative images.
- **Colour that carries meaning:**
  - Class colours on names, re-inked darker on parchment.
  - Item-quality colours on items, also re-inked on parchment.
  - Green = OK, ember = live/pending, red = error/destructive.
- **Type:**
  - Cinzel for page titles, panel headings and the bronze button.
  - Geist for UI and data, with tabular numbers.
  - Georgia italic only for journal annotations on parchment.
- **Density:** 28px buttons, 24px segmented controls, 40px table rows. Nothing chunky.
- **Write safety is visible:** every write action is locked (muted, with a lock icon) while `Wow.exe` runs, with one "Close WoW first" explanation per view.
- **Responsive:** below 1100px the sidebar becomes a 60px icon rail, with the ribbon still marking the active item. Stat strips stay as a single row of four.
- **Window:** custom 32px titlebar. It must support Windows snap layouts and dragging (engineering to confirm).
- **Item icons:** cached real icons drop into `.ico` (24px in lists, `.ico.lg` 36px on sheets) with a quality-coloured border. The letter glyphs are stand-ins.

## Components still living in page CSS (promote when building)
- Locked write action (`.lockact` in `backups.html`): muted, lock icon, not-allowed cursor.
- Stone/ember checkbox (`.cb` in `backups.html`).
- Small inline wax dot for level-ups (`.wax` in `gold.html` and `session.html`).
- Paper chart hover readout (`.ctip` / `.stip` in `gold.html` and `session.html`).
- `.field` value ellipsis for long paths (`settings.html`).

## Files
- `_d.css`: the design system.
- `_shell.js`: titlebar, sidebar, icons, class crests, the portrait slot (`data-portrait`) and the tooltip engine (`data-tt`, `window.EXTRA_ITEMS`).

| Screen | File | States |
|---|---|---|
| Dashboard | `dashboard.html` | `?tt=truestrike` |
| Backups | `backups.html` | `?confirm` = restore dialog while WoW runs |
| Characters | `characters.html` | `?q=Runecloth` = cross-alt search |
| Character sheet | `character.html` | parchment sheet |
| Portrait slot | `portraits.html` | armory / screenshot / crest, on stone and on parchment |
| Settings | `settings.html` | Integrations (keys optional, Windows Credential Manager) |
| Ledger (Gold & History) | `gold.html` | chart + journal |
| Session recap | `session.html` | one journal entry |
