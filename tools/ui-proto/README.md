# The UI prototype (UI1)

The designer's round 4 interpreter, `FBUnitProto/UF.lua` (from #238's `design/ingame/rogue`), draws a UI from a spec: unit frames (plates, bars, text, combo pips), an action bar grid, icons and a minimap shell. UI1 starts the UI language from this format and extends it. It isn't the addon's code: the app bundles it only so `render_ui` can draw a document in the preview's sandbox.

- `FBUnitProto/`: the interpreter as an addon. Its `Spec.lua` isn't here: it's the document being drawn, written by the app.
- `examples/round4.json`: round 4 as a document, the starting point an agent gets back when it renders without one.

## A document

JSON, the spec's table as data. These are the only kinds of value: strings, numbers, booleans, lists and objects. There's no code and no `null`, and the whole document is at most 64 KB. Object keys are names, or whole numbers such as action slots (`"cooldowns": { "3": [0.6, "6"] }`), which become number keys for the interpreter. The app checks a document and writes it out as a Lua table literal (`src-tauri/src/preview/ui.rs`). A key or value it refuses is named by its path (`units[0].parts[1]: ...`).

## Drawing one

- **Agent connection:** `render_ui` with `doc` and `moment` (`standing`, `combat`, `stealth` or `low_health`). It returns a PNG (1280x720) as image content. Leaving out `doc` draws round 4 and also returns its document.
- **Command line,** the same render:

  ```sh
  <app> render tools/ui-proto/examples/round4.json --moment combat --out shot.png
  ```

  It reads the preview data the app downloaded (Your UI tab), or `--data tools/wowapi/wowapi/data` in the repo after `tools/wowapi/fetch-data.sh`. `--scale 0.5` gives a smaller picture.

The scene is the preview's logged-in screen: a level 34 rogue with a hostile target, drawn without the game's art (icons are plain squares).
