# Round 1 mocks: shell + dashboard, backups, characters

Open any `.html` directly in a browser (they load the real warcraftcn art and fonts from `src/` and `node_modules/`).
PNGs at 1280x800 and 1024x700 are in `shots/`.

| Screen | File | States |
|---|---|---|
| Shell + Dashboard | `dashboard.html` | WoW running |
| Backups | `backups.html` | list + snapshot detail; `?confirm` = restore dialog, WoW running |
| Characters (roster) | `characters.html` | `?q=Runecloth` = cross-alt search |
| Character drill-in | `character.html` | Gear tab |

## Visual language

- **Base:** near-black warm surfaces, 1px gold hairlines, Geist for UI and data (tabular numbers), Cinzel only for titles, panel headings and the primary action.
- **Ornate art, used in exactly five places:**
  - the dark gold plaque (`tabs/tab-list`) on the active nav item
  - the bright gold plaque (`tabs/tab-list-active`) on the one primary action per view
  - iron corner brackets (`dropdown-menu-bg`) on the one featured panel per view, and on dialogs
  - the iron frame (`input-frame`) on the cross-alt search
  - faction portrait frames (`avatar-*`)
- **Everything else is compact:** 28px buttons, 26px segmented filters, 36–42px table rows, pills for status.
- **WoW-native color carries meaning, not decoration:** class colors on names, item-quality colors on items, coin dots for gold/silver/copper.
- **Write safety is visible:** restore is locked with "Close WoW first" while `Wow.exe` runs, and every restore names the files it replaces and takes a safety snapshot first.

## Open questions

1. **Item icons.** The mocks use quality-bordered letter squares as stand-ins. We could pull real icons from the local client files later, or keep a stylised stand-in.
2. **Window chrome.** The mocks draw a custom 32px titlebar (Tauri `decorations: false`). The other option is native Windows chrome.
3. **Faction frames.** Dwarf/gnome characters use the neutral wooden frame. We can choose per-race frames or one frame for all.
