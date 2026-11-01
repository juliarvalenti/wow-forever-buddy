# Origin

`wowapi/` is vendored from WOWAPI, a WoW: Forever client emulator in plain Lua, for the in-game window pictures (`tools/addon-shots`, G1).

- **Repo:** https://github.com/Reece01-Dock/WOWAPI
- **Commit:** `aea2b451a9075a5a5591ac3d56cf6e344c0557d0` (2026-09-25, "Merge pull request #1")
- **Vendored:** 2026-10-14. Only the `wowapi/` module's code, as at that commit. The upstream specs, example addons and `wowtest` runner are left out.
- **Not committed:** `wowapi/data/` (about 8 MB of Blizzard API docs, global strings, and atlas and icon lists), so that Blizzard data isn't republished in this public repo. `fetch-data.sh` fills it from the same commit, in CI and locally. It's gitignored.
- **Licence:** the upstream repo states none.

It's a test tool. It's never bundled into the app or the addon zip: the app bundles `src-tauri/resources/`, and the release zips only the addon folder.

## Local patches

Each patch is marked `-- WFB patch (G1)` in the code. Their diffs against the commit above are in this folder's git history.

- `layout.lua`: a ScrollFrame's scroll child is placed at the frame's top left, moved up by the vertical scroll, as in the client. Without this, the window's panes draw empty.
- `framexml.lua`: `PanelTemplates_TabResize`, `_SetNumTabs` and `_SetTab`. A `PanelTabButtonTemplate` tab is sized to its label plus the template's ends, as in the client. Without this, the tabs are 0 wide and pile up. `_SetTab` also selects and deselects (`_UpdateTabs`, `_SelectTab`, `_DeselectTab`) as `SharedUIPanelTemplates.lua` does: the selected tab shows its active art, a white label set lower, and is disabled.
- `templates.lua`: `PanelTabButtonTemplate` carries its art as `SharedUIPanelTemplates.xml` defines it (the `uiframe-tab-*` and `uiframe-activetab-*` atlases, 32 tall). Without this, the tabs draw as bare labels.
- `templates.lua`: `PortraitFrameTemplate`'s portrait is placed over the top-left corner (62×62 at -5, 7), as in the client. Without this, the window has no portrait.
- `render.lua`: a frame inside a ScrollFrame is clipped to it. Without this, a pane taller than its window (Settings) draws past the window's edge.
- `assets.lua`: the local art folder (`WOWAPI_ART`) indexes `.ttf` files as well as images, so the fonts `casc-extract` pulls from the game serve `Store:font` offline. Without this, pictures drawn from the extract fall back to a browser serif.

## Updating

To move to a new commit:

1. Copy its `wowapi/` code over this one (not `data/`).
2. Put the new commit and date above, and the commit in `fetch-data.sh`.
3. Re-apply the patches, delete your local `wowapi/data/`, and look over the pictures from the PR's run.
