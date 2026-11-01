# In-game window pictures (G1)

Pictures of the Forever Buddy window as the game draws it, one per tab (Plan, Lists, Errands, Cleanup, Settings), in two states:

- **filled-\***: with the slots the app writes (`src-tauri/tests/fixtures/bridge/`, built by `contract.rs`) for Thrandor Vargur;
- **empty-\***: with the empty slots the addon ships.

They're for the designer to check against the mock. They're not compared automatically.

**How it works.** `scene.lua` runs the addon in WOWAPI, a WoW: Forever client emulator in plain Lua, with the game's real UI art. It writes one SVG per picture, cropped to the window and its tabs.

WOWAPI is vendored in `tools/wowapi`, with patches for the gaps the window hit: scroll children, tab sizes, tab art and selection, and scroll clipping. `tools/wowapi/ORIGIN.md` gives its source, commit and patches. It's a test tool, never bundled into the app or the addon zip.

**In CI.** `.github/workflows/addon-shots.yml` runs on any PR that touches the addon, the bridge slots, this folder or `tools/wowapi`. Headless Chrome turns each SVG into a PNG at 2x, and the PNGs are the run's **addon-pictures** artifact. The run's summary page lists them.

**Locally:**

```sh
tools/wowapi/fetch-data.sh        # once: WOWAPI's data/, which isn't committed
luajit tools/addon-shots/scene.lua /tmp/pictures --no-art
```

This writes SVGs only. `--no-art` draws placeholders instead of downloading the game's art. The script never starts a browser or opens a window.

**With Forever's own art (local only).** If the game is installed, `casc-extract` (`src-tauri/src/bin/casc-extract.rs`) pulls its UI art and fonts into a cache under app data, keyed by build. Point WOWAPI at that cache and keep it offline, so every texture and the Friz Quadrata font come from Forever's files instead of the retail mirrors:

```sh
WOWAPI_ART="$HOME/Library/Application Support/com.juliarvalenti.wowforeverbuddy/cache/casc-extract/wowf/<build key>" \
WOWAPI_OFFLINE=1 luajit tools/addon-shots/scene.lua /tmp/pictures
```

CI can't do this (no game on the runners, and the files aren't ours to ship), so CI's pictures keep the downloaded art.
