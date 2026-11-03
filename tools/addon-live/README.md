# addon-live (LIVE1)

Shotkit for the in-game window: ForeverBuddy running in WOWAPI (`tools/wowapi`, the WoW: Forever client emulator) as one long-lived session. You drive it from the shell, a command at a time, and look at it in PNGs. It's for checking UI changes without the game: an agent clicks around, takes a picture, changes the addon, logs in again, and compares.

```sh
tools/wowapi/fetch-data.sh                        # once
luajit tools/addon-live/live.lua start            # boots the session in the background
luajit tools/addon-live/live.lua slash /fb plan
luajit tools/addon-live/live.lua buttons          # what's clickable: name, text, centre
luajit tools/addon-live/live.lua click Lists      # by name, path or the text on it
luajit tools/addon-live/live.lua set window.coach on
luajit tools/addon-live/live.lua screenshot lists # .addon-live/shots/lists.png
luajit tools/addon-live/live.lua upload lists     # to the mycelium room; prints the link
luajit tools/addon-live/live.lua stop
```

`help` lists every command: `login [filled|empty]`, `slash`, `click`, `set`, `buttons`, `bags`, `advance`, `reload`, `combat on|off`, `state [path]`, `chat`, `errors`, `lua`, `screenshot <name> [full|bags]`, `upload`. Each answer ends with the chat lines and Lua errors the command caused.

- **`scene <class> [level] [stealth] [layout=<name|file>]`** logs in to the full logged-in screen instead: Blizzard's own UI from the CASC extract (it needs `WOWAPI_ART`, as `tools/addon-shots/hud.lua` does), a fixture character from `tools/addon-shots/fixtures.lua` with its target, and ForeverBuddy. Then **`health`**, **`power`**, **`combo`** and **`target`** change it as the game does, firing `UNIT_HEALTH`, `UNIT_POWER_UPDATE`/`UNIT_POWER_FREQUENT` and `PLAYER_TARGET_CHANGED`, and `combat on|off` and `advance` work as usual. A script plays a fight over time:

  ```
  scene rogue layout=centered
  combat on
  power player -40      # Sinister Strike
  combo +1
  health target -15%
  advance 2
  power player +20      # an energy tick
  health player 60%
  target none           # it died
  combat off
  ```

  The scene has auras (Slice and Dice on the rogue, Rupture and Expose Armor on the target) and two watched quests. **`aura <unit> <spell> [seconds] [stacks] [harmful]`** and **`unaura`** add and remove auras (`UNIT_AURA`). **`cast <spell> [seconds]`** starts a cast and leaves the cast bar up until `advance` passes its end. **`quest`** lists the quests, and **`quest <id or "title"> <objective> <n | +n>`** moves one along. Spell names with spaces go in quotes: `aura target "Kidney Shot" 6 harmful`.

  A full-screen shot with the game's art is tens of MB of SVG. QuickLook can't take that, so without `rsvg-convert` or a headless Chrome it stays an SVG.
- **`bags`** puts up a backpack and the bank with junk, gear of each quality, a shirt and stacks, for the Bags module (P4d). WOWAPI draws no bag frames, so these are stand-ins with what the addon reads from Blizzard's: item buttons that know their bag and slot, and a `BankPanel`. `screenshot <name> bags` crops to them. Run `bags` again after a `set` to redraw them without the window on top.

- **The character** is Thrandor Vargur, level 12 in Elwynn, as in `tools/addon-shots`. `filled` (the default) has the slots the app writes (`src-tauri/tests/fixtures/bridge/`). `empty` has the slots the addon ships. The quest log is empty, so the same commands always give the same pictures.
- **`login` reads the addon from disk again**, so after an edit to `ForeverBuddy.lua`, `login` shows the change. `reload` is the game's /reload: SavedVariables kept, same files.
- **PNGs** are 2x, cropped to the window and its tabs (`full` for the whole 1920x1080 screen). On a Mac they come from QuickLook (`qlmanage`) and `sips`, which are built in. Elsewhere they come from `rsvg-convert` or a headless Chrome. None of these opens a window. The SVG sits next to each PNG.
- **Art:** placeholders by default. `start --art` uses WOWAPI's art store. With `WOWAPI_ART` pointing at the CASC2 extract and `WOWAPI_OFFLINE=1` (as in `tools/addon-shots`), that's Forever's own art and font, with nothing downloaded.
- **Pictures for Julia** go to the room with `upload`, never to Downloads.
- **`script <file>`** runs a file of commands in one process, without a server. `tour.txt` is one, and the **Addon pictures** workflow runs it on every addon PR. It fails on an unknown command or a Lua error.

The session's files are in `.addon-live/` at the repo root (gitignored; `--dir` for another place). That folder holds the command pipe, `server.log`, the addon copy and `shots/`. Nothing here touches a real WoW install.
