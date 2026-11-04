# The Forever Buddy UI language (UI1)

Status: version 1 draws today, in `render_ui` and in game (`src-tauri/resources/addon/ForeverBuddy/UI.lua`, checked by the addon's `UICheck.lua` in both). Issue #259. Written by @designer for Julia, coders and agents.

One document describes a whole UI. An agent writes it, `render_ui` shows it on a stand-in screen in each moment, the agent fixes what's off, and the player picks it. The HUD we ship is one example document (`ui-examples/round4.json`), not the only shape a UI can take.

The format grew from the TH1 prototype that drew round 4 from a spec. Nothing in a document runs: it's JSON, the app turns it into a Lua table, and the interpreter builds frames from it. An agent never sends Lua.

---

## 1. A first look

```json
{
  "ui": 1,
  "name": "Almost nothing",
  "hide": ["PlayerFrame", "TargetFrame", "MainActionBar", "MultiBarBottomLeft"],
  "parts": [
    { "name": "hp", "kind": "bar", "value": "health",
      "color": [[0.35, "#e0603a"], [1, "#e8ecf2"]], "back": "#000000", "backAlpha": 0.5,
      "anchor": ["TOP", "char", "CENTER", 0, -70], "size": [220, 4],
      "alpha": { "always": 0.25, "combat": 1, "low": 1 } },
    { "kind": "text", "format": "{hp%}", "font": "Geist", "fontSize": 13, "justify": "LEFT",
      "anchor": ["LEFT", "hp", "RIGHT", 8, -1], "size": [60, 16],
      "alpha": { "always": 0, "combat": 1, "low": 1 } },
    { "kind": "text", "unit": "target", "format": "{name}  {hp%}", "font": "Geist", "fontSize": 13,
      "color": "reaction", "anchor": ["BOTTOM", "char", "CENTER", 0, 150], "size": [300, 16] },
    { "kind": "actionbar", "slots": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12], "icon": 34, "gap": 4,
      "keys": false, "anchor": ["BOTTOM", "screen", "BOTTOM", 0, 24],
      "alpha": { "always": 0.4, "combat": 1 } }
  ]
}
```

A thin health line under the character that's faint until combat or low health, the percentage beside it only then, the target's name above, and one small row of buttons. The examples in `tools/addon-shots/ui-examples/` look nothing alike: round 4, a longer version of this one, "Columns", "Halo" (health as a ring of 32 segments around the character, buttons fanned under it), "Terminal" (a green console in a corner), "Shards" (slanted crystal segments, mirrored for the target), "Corners" (huge numbers in two corners, the middle left clear) and "Rogue, aura bars" (your buffs and your debuffs on the target as timer bars either side of you).

## 2. The document

| Key | What it is |
|---|---|
| `ui` | The language version: `1`. |
| `name` | What the player picks it by, up to 40 characters. |
| `parts` | The parts, drawn in order (1 to 300). |
| `hide` | Blizzard frames this UI stands in for. They go to alpha 0, out of combat, and nothing else is done to them. |
| `lowAt` | Health below this fraction is the `low` state (default 0.35). |
| `charY` | Where the character stands, in pixels from the screen's middle (default -14). |

**Units.** Sizes and offsets are pixels on a 1080p screen, as in our designs. The interpreter scales them to the UI.

## 3. Parts

Every part has a `kind`. A part with a `name` can be anchored to by the parts after it.

| Kind | Draws | Takes |
|---|---|---|
| `plate` | A rectangle | `color`, `gradient` (`["HORIZONTAL" or "VERTICAL", "#from", "#to"]`), `border`, `borderWidth`, `rotate` (degrees, about its middle: 45 on a square is a diamond, a thin turned plate is a slash) |
| `bar` | A fill that follows a value | `value` (required), `color`, `gradient` (`[lighten start, darken end]`, e.g. `[0.38, -0.18]`), `back`, `backAlpha`, `border`, `vertical`, `reverse`, `ticks`, `tickColor` |
| `text` | A line of text | `format` (section 6), `font`, `fontSize`, `outline`, `stroke`, `justify`, `color` |
| `pips` | Marks lit up to a value | `value` (default `combo`: one pip per point; `health` or `power` light their share, so 30 pips make a segmented gauge), `count` (up to 40), `w`, `h`, `gap`, `layout` (`row`, `column` or `arc`, with `radius`, `from`, `to` in degrees, 0 up), `rotate` (each mark), `orient` (on an arc, each faces out), `color` (stops colour each pip by its place), `emptyColor`, `emptyAlpha`, `border` |
| `icon` | An image | `slot` (an action slot's icon) or `aura` (a spell name), `zoom`, `border` |
| `actionbar` | The player's action buttons | `slots`, `icon` (size), `gap`, `columns`, `layout` (`grid`, or `arc` with `radius`, `from`, `to`), `zoom`, `border`, `back`, `keys`, `bind`, `bindFrom`, `keySize`, `keyColor`, `font`, `outline`, `stroke` |
| `auras` | A timer bar per aura on the `unit`: its icon, name, stacks, a fill that drains and the time left | `filter` (`mine`, the default: helpful or harmful, cast by you; or `helpful`, `harmful`, `all`), `show` (only these spell names or IDs, in that order of priority), `max` (1 to 20), `maxDuration` (seconds: leaves out longer auras and endless ones, like a poison's hour; one whose duration is hidden stays), `sort` (`time` left or `name`), `grow` (`down` or `up` from the anchor), `w` and `h` (each bar), `gap`, `icon` (true or false), `color` (or `dispel`: a debuff in its dispel type's colour), `back`, `backAlpha`, `border`, `font`, `fontSize`, `outline`, `stroke`, `nameColor` |

Every part also takes `name`, `unit`, `anchor`, `size`, `level`, `alpha` and `hidden`.

A key a kind doesn't take is an error, so a typo (`"colour"`) is caught, not ignored.

## 4. Placing things

```json
"anchor": ["TOPLEFT", "hp", "BOTTOMLEFT", 0, -4]
```

`[point, to, toPoint, x, y]`:

- `point`: which corner or edge of this part is pinned: `TOPLEFT`, `TOP`, `TOPRIGHT`, `LEFT`, `CENTER`, `RIGHT`, `BOTTOMLEFT`, `BOTTOM`, `BOTTOMRIGHT`.
- `to`: `screen`, `char` (where the character stands), or the `name` of an earlier part.
- `toPoint`: the point on `to`; defaults to `point`.
- `x`, `y`: pixels, up and right positive.

`size` is `[width, height]`. `level` (0 to 20) lifts a part above others.

## 5. Units and values

`unit` is whose data the part shows: `player` (the default), `target`, `targettarget`, `focus`, `pet`, `party1` to `party4`. A part for a unit that isn't there (no target) isn't drawn.

`value` is `health`, `power` or `combo`.

**Colours** are `"#rrggbb"`, or bound to the unit: `class`, `power` (the power type's colour), `reaction` (red, yellow, green), or stops by how full: `[[0.35, "#d24a3a"], [1, "#4fd99a"]]`.

## 6. Text

`format` mixes words and tokens:

| Token | Shows |
|---|---|
| `{name}`, `{level}` | The unit's name and level. |
| `{hp}`, `{power}`, `{combo}` | The value, short: 690, 1.2 K, 3.4 M. |
| `{hp%}`, `{power%}` | Percent of the max: 85%. |

`"{hp} | {hp%}"` shows `1.2 K | 85%`.

Fonts: `Barlow` (bold, condensed; ours), `Geist`, `Marcellus`, and the game's `Friz`, `Arial Narrow`, `Morpheus`, `Skurri`. `outline` is `thick`, `thin` or `none`. `stroke` (0 to 3) draws a heavier black stroke, as round 4 does. `fontSize` is 6 to 120, so a number can be the whole design. (The game's own fonts only draw in pictures with game art on; ours always do.)

## 7. States

`size`, `alpha`, `hidden`, `color` and `gradient` can differ by state. Give an object with `always` and any states; the last state in this order that holds wins:

`always`, `ooc` (out of combat), `combat`, `target`, `notarget`, `full`, `low`, `stealth`

```json
"alpha": { "always": 0.25, "combat": 1, "low": 1 }
```

**Values the game hides in combat.** WoW: Forever can hand an addon a hidden ("secret") value in combat, and code can't branch on one. A part shows one anyway: a bar hands it to a status bar, which fills from it without Lua reading it, and a number in text goes through the game's own `AbbreviateNumbers` (`{hp}`, `{power}`, `{combo}`) or `UnitHealthPercent` and `UnitPowerPercent` (`{hp%}`, `{power%}`) into `SetFormattedText`. A hidden number reads `1.2K` where a readable one reads `1.2 K`. What needs a comparison keeps the last it could work out: a colour from stops, the `full` and `low` states, and how many pips are lit. The interpreter tests `issecretvalue` (and `issecrettable` for a table) before it compares anything, and shows `?` only where the game has no way to show the value. When conditions come (see Next), any rule that compares a value, like health below 35%, has to say what to assume when it's hidden, and the checker refuses one that doesn't.

**Aura bars and hidden timing.** An aura's time left can be hidden too. A bar whose timing is hidden shows full with no time, never a guess. If the aura list itself is refused in combat, the bars keep the last ones they had.

**Combat.** The UI's frames are made once, out of combat: a login or reload in combat makes them when it ends (`PLAYER_REGEN_ENABLED`). After that, in combat, the interpreter only repaints: it never moves or resizes a frame (a `size` given per state waits for combat to end), and a state that hides a part does it by alpha.

## In game

The player's chosen document lives in the slot `Data/UI.lua` (`ForeverBuddyData_UI = { schema = 1, stamp, doc }`), written by the app only when the player picks Use this. At login the addon checks it again with the same `UICheck.lua` (the folder can be edited by anything on the machine); a missing or failing document draws nothing and the standard HUD stays. The app never writes a slot while WoW is running, so a new choice (or going back) reaches the game at the next login after WoW has been closed; a `/reload` loads the file as it was.

While a document draws:

- Our fixed HUD stays off. Blizzard's player and target frames, and the ones in `hide`, get alpha 0 and nothing else.
- Parts repaint from the game's events: health, power and combo points, auras, the target, combat and stealth. Aura bars tick five times a second from one shared update.
- `actionbar` parts are our real action buttons (the Action bars module's secure buttons, slots 1 to 12 and 61 to 64, paging with stealth). The document sets where each slot's button goes and its size; they're laid out only out of combat. A slot no part names has no button showing, and its key still presses it.

## 8. Checking

`render_ui` checks a document before drawing it, and draws nothing if it fails. Each problem has a path into the document (a JSON pointer, counting from 0), a code, and a sentence on how to fix it:

```
/parts/0/anchor/1  unknown-anchor: anchors to hpBar, but no part before this one has that name. It can be "screen", "char", or a part named before this one: grid, me.
/parts/0/colour    unknown-key: a bar doesn't take colour. It takes: alpha, anchor, back, ...
/parts/1/format    unknown-token: {mana} isn't a token. Tokens: {combo}, {hp}, {hp%}, {level}, {name}, {power}, {power%}.
/parts/1/fontSize  out-of-range: fontSize is 200; it takes 6 to 120.
/parts/2/slots/1   bad-slot: an action slot is a whole number from 1 to 180.
```

**What a document costs.** The client caps each addon's objects (from its own log: Frame 10,000, Texture 40,000, FontString 5,000, Font 300, Animation 5,000, AnimationGroup 2,000, Timer 500). The checker counts what a document will make and refuses one over a UI's share, a fifth of each: 2,000 frames, 8,000 textures, 1,000 font strings. Stroke is the expensive part: a stroked text is drawn once plus 8, 20 or 36 black copies (stroke 1, 2 or 3), so round 4 makes 228 font strings. The interpreter makes no timers and runs one shared update for the whole UI, not one per part; a repaint reuses the textures and text it made before.

## 9. Seeing it

`render_ui` draws the document on a stand-in screen (a level 34 test character, a rogue unless another class is asked for, logged in with a hostile target; it reads none of the player's data) in a moment:

| Moment | What it is |
|---|---|
| `standing` | Out of combat, with a target. |
| `combat` | In combat. |
| `stealth` | In stealth (the rogue's stealth bar). |
| `low_health` | Out of combat, at a quarter of your health. |
| `no_target` | Out of combat with nothing targeted: a UI's quiet look. |
| `hidden` | In combat, with every value the game hides in combat hidden from the UI (your own health included, as Forever's API docs have it). What the design really does in a fight. |

There's a test character for each class: `warrior`, `paladin`, `hunter`, `rogue`, `priest`, `shaman`, `mage`, `warlock` and `druid` (in cat form, with combo points). An unknown class or moment is refused with the names there are. The agent gets back a picture, fixes what's off and draws it again before the player sees anything. `luajit tools/addon-shots/ui.lua <doc.json> <dir> [--class mage]` draws all six from the command line.

## 10. Rules that stay

- **Declarative only.** No code, no expressions, no strings the addon runs.
- **Propose, never act.** A document reaches the game only when the player picks it.
- **No automation.** Action buttons show the player's own slots and cast only on the player's own press.
- **The game's limits apply.** Protected frames change only out of combat; designs have to work when values are hidden.

---

## Next: what version 1 doesn't do yet

From the first draft of this doc, to add as the language grows, each without breaking version 1 documents:

- **Groups and layouts:** a `group` part that holds children, with `row`, `column` and `grid` layouts, so a cluster moves as one.
- **Lists:** one item per aura, party member or boss (`of: "player.buffs"`), with an item template.
- **More data:** casts, absorbs, any aura's time left and stacks, spell cooldowns and charges, `dead`, `inRange`.
- **Conditions:** `when` with comparisons (`player.health < 35%`) and named states (`group`, `mounted`, `resting`), each saying what to assume when the game hides the value.
- **Named styles** merged per part, and theme images.
- **Per character and spec:** `for`, most specific wins.
- **Blizzard's frames placed** through an Edit Mode layout, so one document describes the whole screen.
- **Hidden aura timing shown anyway:** aura bars timed by the aura's duration object (`StatusBar:SetTimerDuration`, `C_DurationUtil.CreateDurationTextBinding`), so no Lua reads the time left; and a colour from stops worked out by the game from a hidden value (a colour curve).
- **Action buttons on any slot**, not only the sixteen the Action bars module makes, styled by the document (keybind font, border).
- **Text roles:** named styles for a frame's `header`, `value` and `label` text, instead of one font per part.
- **RGBA colours:** `"#rrggbbaa"`, alongside the separate alpha keys.
- **Backdrop and border as their own properties**, with `strata` and `scale`.
- **Hover** as a state, with children fading in on it (a bar that shows its numbers when you point at it).
- **More states,** like in an instance or in a group, each with three outcomes: show, collapse (take no room), or hide.
- **Lists with sort and group,** for auras by time left or party members by role.
- **Share strings:** a document as a short string to paste to a friend, with a header (language version, addon, kind) on a compressed payload. The client has `C_EncodingUtil.SerializeCBOR` and `CompressString` built in.
