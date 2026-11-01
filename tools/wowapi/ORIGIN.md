# Origin

`wowapi/` is vendored from WOWAPI, a WoW: Forever client emulator in plain Lua, for the in-game window pictures (`tools/addon-shots`, G1).

- **Repo:** https://github.com/Reece01-Dock/WOWAPI
- **Commit:** `aea2b451a9075a5a5591ac3d56cf6e344c0557d0` (2026-09-25, "Merge pull request #1")
- **Vendored:** 2026-10-14. Only the `wowapi/` module's code, as at that commit. The upstream specs, example addons and `wowtest` runner are left out.
- **Not committed:** `wowapi/data/` (about 8 MB of Blizzard API docs, global strings, and atlas and icon lists), so that Blizzard data isn't republished in this public repo. `fetch-data.sh` fills it from the same commit, in CI and locally. It's gitignored.
- **Licence:** the upstream repo states none.

It's a test tool. It's never bundled into the app or the addon zip: the app bundles `src-tauri/resources/`, and the release zips only the addon folder.

## Local patches

Each patch is marked `-- WFB patch (G1)` or `-- WFB patch (HUD1)` in the code. Their diffs against the commit above are in this folder's git history.

- `layout.lua`: a ScrollFrame's scroll child is placed at the frame's top left, moved up by the vertical scroll, as in the client. Without this, the window's panes draw empty.
- `framexml.lua`: `PanelTemplates_TabResize`, `_SetNumTabs` and `_SetTab`. A `PanelTabButtonTemplate` tab is sized to its label plus the template's ends, as in the client. Without this, the tabs are 0 wide and pile up. `_SetTab` also selects and deselects (`_UpdateTabs`, `_SelectTab`, `_DeselectTab`) as `SharedUIPanelTemplates.lua` does: the selected tab shows its active art, a white label set lower, and is disabled.
- `templates.lua`: `PanelTabButtonTemplate` carries its art as `SharedUIPanelTemplates.xml` defines it (the `uiframe-tab-*` and `uiframe-activetab-*` atlases, 32 tall). Without this, the tabs draw as bare labels.
- `templates.lua`: `PortraitFrameTemplate`'s portrait is placed over the top-left corner (62×62 at -5, 7), as in the client. Without this, the window has no portrait.
- `templates.lua`: `PortraitFrameTemplate`, `ButtonFrameTemplate` and `InsetFrameTemplate` are built as `SharedUIPanelTemplates.xml` builds them: the metal nine-slice and the inset's inner nine-slice from `NineSliceLayouts.lua` (anchored as `NineSlice.lua` does, 2x atlases at half size), the rock and marble backgrounds tiled, the top streaks, a gold title and the close button at TOPRIGHT 0, 3. Without this, the window draws as a grey dialog box.
- `render.lua`: a frame inside a ScrollFrame's scroll child is clipped to the ScrollFrame. Without this, a pane taller than its window (Settings) draws past the window's edge. The ScrollFrame's other children, such as its ScrollBar outside the right edge, aren't clipped.
- `render.lua`: a vertical slider with its own thumb texture (a scroll bar) draws only that thumb, at its size, moving down the bar with the value. Without this, it draws as a horizontal settings slider.
- `templates.lua`: `UIPanelScrollFrameTemplate`'s scroll bar has `UIPanelScrollBarTemplate`'s 16 width, `UI-ScrollBar-Knob` thumb (18×24, cropped) and `UIPanelScrollBarButton`'s arrow crop, and an arrow is disabled at its end of the range, as `ScrollFrame_OnScrollRangeChanged` does. Without this, the bar is 0 wide and draws nothing.
- `assets.lua`: the local art folder (`WOWAPI_ART`) indexes `.ttf` files as well as images, so the fonts `casc-extract` pulls from the game serve `Store:font` offline. Without this, pictures drawn from the extract fall back to a browser serif.
- `secrets.lua` (marked P4d): `canaccesstable` and `canaccessallvalues`, false only when something in them is secret. Upstream leaves them to the documented-function fakes, which answer with a random bool, so an addon that checks a table before reading it skipped plain tables about half the time.

HUD1, so Blizzard's own UI addons (from the extract) load as the client loads them:

- `toc.lua`: a `[AllowLoadGameType ...]` tag names game types only: `mainline` is retail's, not Forever's family. (HUD1a briefly matched the family too, which loaded retail's `ActionButtonOverrides.lua` over Forever's; Blizzard_ActionBar lists the two as alternatives.)
- `templates.lua`: in XML, an unknown inherited node is logged and the frame built without it, as in the client (Forever's `PetFrame.xml` inherits a template only retail loads); Lua's `CreateFrame` still throws.
- `resources.lua`: on a placeholder frame, a key whose last word is a child noun (`EndCaps`, `CloseButton`, `TitleText`) is a placeholder child, not a stub method, unless it starts with a method-only prefix (`GetButton`, `SetText`). Without this, `frame.EndCaps:Hide()` fails (EllesmereUI's action bars and window skin).
- `secure.lua`: restricted snippets have the `table` (with `table.new()`), `string` and `math` libraries (copies), as in the client's restricted environment.
- `framexml.lua`: `CreateSecureDelegate(fn)` returns a function calling `fn`, and `RemoveKStrings` strips `|K...|k` strings. Without these, the error handler `Blizzard_ScriptErrors.lua` installs throws, so an addon reporting a soft error through `geterrorhandler()` (LibSharedMedia does) dies mid-file.
- `framexml.lua`: `FACTION_BAR_COLORS` and `PowerBarColor` (by power token and type) for the stand-in FrameXML; Blizzard's own files define them in the real UI.
- `assets.lua`, `render.lua`: an addon's own fonts (`Interface\AddOns\X\...ttf`/`.otf`) are found in its folder and each FontString draws in its own font, every font file embedded once (the game's Friz Quadrata keeps its `WoWFriz` family). Without this, all text draws in Friz.
- `widgets.lua`: `Font:SetFont` returns success, as the client's does. Without this, LibSharedMedia calls every addon font invalid.
- `widgets.lua`, `xml.lua`, `render.lua`: text shadows. `SetShadowOffset`/`SetShadowColor` on a Font or FontString (and `<Shadow>` in a `<Font>`) are kept; the renderer draws a set shadow as an offset copy and an explicit `0, 0` as clean text. Text whose font says nothing about shadows keeps the old light stroke. Without this, every string had a stroke, smearing dark ink on light panels.
- `render.lua`: a colour texture (`SetColorTexture`) turns with `SetRotation`, like a file texture.
- `assets.lua`: a miss for an addon's file rescans its folder (at most once a second) and isn't memoized, so art or fonts added while a session runs show up on the next render.
- `docs.lua`: `SetAttributeNoHandler` takes any value, like `SetAttribute`.
- `xml.lua`: a region (Texture, FontString) applies its `mixin`, inline or from a region template, after its XML attributes (which the client sets natively, so a mixin's `SetText` doesn't run for them). Without this, `TextStatusBarSparkTemplate` has no `Initialize` and every unit frame's health bar setup fails.
- `framexml.lua`: `RegionalUniqueNamesEnabled()` returns false (US realms). Without it, `UnitFrame_Update` stops on its first line.
- `xml.lua`: `parentKey` and `parentArray` may come from an inherited template (`EditModeSystemSelectionBaseTemplate` sets `parentKey="Selection"`). Without this, every Edit Mode system's `OnLoad` fails on `self.Selection`, so the target and focus frames never initialize.
- `xml.lua`: `$parent` under an unnamed parent stands for the nearest named ancestor, as in the client. Without it, the gamepad bar's `$parentActionButton1` became a second global `ActionButton1`, replacing the main bar's.
- `xml.lua`: a Texture or FontString with no anchors and no size, here or in a template, fills its parent (`ActionButtonTemplate`'s bare `icon`).
- `render.lua`: in one layer and sublevel, the region created first draws on top, as in the client (the action button's icon over the slot art declared after it). Our window pictures look the same; only the order of non-overlapping regions in the SVG changes.
- `widgets.lua`: `GetID()` is 0 until set, as in the client. Without it, `CalculateAction` fails on every action button.
- `api.lua`: `UnitPower`/`UnitPowerMax` take the power type: a unit can have secondary powers (`powers[type] = { cur, max }`, a rogue's combo points), and an unmodelled other type is 0.
- `framexml.lua`: `GetCurrentEnvironment()` returns the global environment, and `C_UIColor.GetColors()` serves the game's named colours from `data/globalcolor.csv` (`fetch-globalcolors.sh`, GlobalColor.db2 from wago.tools). Without them, `Color.lua` stops and `NORMAL_FONT_COLOR`, `ACTIONBAR_HOTKEY_FONT_COLOR` and the rest are nil.
- `docs.lua`: with `apiDocsDir`, the client's own docs also add their events (to the known set) and functions missing from the environment, answering each documented return with an empty default. Without this, Forever's `INPUT_DEVICE_INTERFACE_TRANSITION` is refused and `C_InputInterfaceStyle` is nil, so `InputUtil` and the action bars stop.
- `api.lua`: the client's `math` (`clamp`, `round`, `saturate`, `lerp`, `normalize`, `sign`, `wrap`) and `table` (`create`, `isempty`, `count`, `contains`, `indexof`, `keys`, `values`, `removevalue`, `removeunordered`) extensions. Without them `Clamp = math.clamp` is nil and every Edit Mode setting converts to nil (the minimap drew at 5%).
- `framexml.lua`: `Menu.GetManager()` returns one manager, with `OpenMenu` and the other methods addons hook (`hooksecurefunc`).
- `widgets.lua`: `GetFrameMetatable()`, `GetButtonMetatable()`, `GetEditBoxMetatable()`, `GetFontStringMetatable()` and `GetTextureMetatable()` return the widget type's metatable. Without this, `SecureTemplates.lua` fails on its first line and no secure action button gets `SecureActionButtonMixin`.
- `toc.lua`: every `## Dep:`, `## Dependencies:`, `## RequiredDep(s):` and `## OptionalDep(s):` line counts, each with its own game-type tag. Without this, only the last line is kept (Blizzard lists one dependency per line), and `RequiredDep`, which `Blizzard_WorldMap` uses for the load-on-demand `Blizzard_MapCanvas`, was missed: the map then opened at login with its full-screen black backdrop.
- `toc.lua`: file lines tagged `[AllowLoad Glue]` aren't loaded in game, and any other tag (`[LoadIntoEnvironment secure]`) is dropped from the file name after `[Family]`/`[Game]` are filled in. Without this, those files are reported missing.
- `sim.lua`: addon names match case-insensitively, as in the client. Without this, a dependency named `Blizzard_ActionBar` loads the folder `blizzard_actionbar` a second time.
- `templates.lua`: a FontString template may inherit a font object (`TooltipTextLeftTemplate inherits="GameTooltipText"`). Without this, it's "Couldn't find inherited node".
- `docs.lua`: with `apiDocsDir` (a local `Blizzard_APIDocumentationGenerated`), enums and values the bundled data predates are added from the client's own docs. Without this, Forever's UI indexes enums like `EditModePresetLayouts.Gamepad` that the retail data doesn't have.
- `xml.lua`: a `type="global"` KeyValue or Attribute may be a dotted path (`Enum.EditModeSystem.UnitFrame`). Without this, every Edit Mode system frame has a nil `system`, none registers, and the unit frames are never placed.
- `assets.lua`: an atlas's size is in UI units: the smallest of its 1x size and its 2x size halved (some art ships a "1x" file at 2x density; the minimap ring is 438 px in both, 215 units in Blizzard's XML). Without this, 2x-authored HUD art draws at twice its size. The window frame's nine-slice relies on this instead of halving its own pieces.

## Updating

To move to a new commit:

1. Copy its `wowapi/` code over this one (not `data/`).
2. Put the new commit and date above, and the commit in `fetch-data.sh`.
3. Re-apply the patches, delete your local `wowapi/data/`, and look over the pictures from the PR's run.
