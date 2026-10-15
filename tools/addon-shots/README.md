# In-game window pictures (G1)

Pictures of the Forever Buddy window as the game draws it, one per tab (Plan, Lists, Errands, Cleanup, Settings), in two states:

- **filled-\***: with the slots the app writes (`src-tauri/tests/fixtures/bridge/`, built by `contract.rs`) for Thrandor Vargur;
- **empty-\***: with the empty slots the addon ships.

They're for the designer to check against the mock. They're not compared automatically.

**How it works.** `scene.lua` runs the addon in WOWAPI, a WoW: Forever client emulator in plain Lua, with the game's real UI art. It writes one SVG per picture, cropped to the window and its tabs.

WOWAPI is vendored in `tools/wowapi`, with patches for the gaps the window hit: scroll children, tab sizes and scroll clipping. `tools/wowapi/ORIGIN.md` gives its source, commit and patches. It's a test tool, never bundled into the app or the addon zip.

**In CI.** `.github/workflows/addon-shots.yml` runs on any PR that touches the addon, the bridge slots, this folder or `tools/wowapi`. Headless Chrome turns each SVG into a PNG at 2x, and the PNGs are the run's **addon-pictures** artifact. The run's summary page lists them.

**Locally:**

```sh
tools/wowapi/fetch-data.sh        # once: WOWAPI's data/, which isn't committed
luajit tools/addon-shots/scene.lua /tmp/pictures --no-art
```

This writes SVGs only. `--no-art` draws placeholders instead of downloading the game's art. The script never starts a browser or opens a window.

**Known gap:** tabs show as labels, without their tab art.
