# The Forever Buddy UI language (UI1)

Status: version 1 draws today (`src-tauri/resources/addon/ForeverBuddy/UI.lua`, the addon's own, checked by `tools/addon-shots/uicheck.lua`, through `render_ui`). Issue #259. Written by @designer for Julia, coders and agents.

One document describes a whole UI. An agent writes it, `render_ui` shows it on the player's screen in each moment, the agent fixes what's off, and the player picks it. The HUD we ship is one example document (`ui-examples/round4.json`), not the only shape a UI can take.

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

A thin health line under the character that's faint until combat or low health, the percentage beside it only then, the target's name above, and one small row of buttons. The seven examples in `tools/addon-shots/ui-examples/` look nothing alike: round 4, a longer version of this one, "Columns", "Halo" (health as a ring of 32 segments around the character, buttons fanned under it), "Terminal" (a green console in a corner), "Shards" (slanted crystal segments, mirrored for the target) and "Corners" (huge numbers in two corners, the middle left clear).

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

**Values the game hides in combat.** WoW: Forever can hand an addon a hidden value in combat. A part reading one draws its bar full and its text as `?` rather than break. Which values Forever hides, for which units, is for @coder2 to confirm in game; the in-game interpreter will hand bars the value directly so they keep filling.

## 8. Checking

`render_ui` checks a document before drawing it, and draws nothing if it fails. Each problem has a path into the document (a JSON pointer, counting from 0), a code, and a sentence on how to fix it:

```
/parts/0/anchor/1  unknown-anchor: anchors to hpBar, but no part before this one has that name. It can be "screen", "char", or a part named before this one: grid, me.
/parts/0/colour    unknown-key: a bar doesn't take colour. It takes: alpha, anchor, back, ...
/parts/1/format    unknown-token: {mana} isn't a token. Tokens: {combo}, {hp}, {hp%}, {level}, {name}, {power}, {power%}.
/parts/1/fontSize  out-of-range: fontSize is 200; it takes 6 to 72.
/parts/2/slots/1   bad-slot: an action slot is a whole number from 1 to 180.
```

## 9. Seeing it

`render_ui` draws the document on the player's screen (their class, our fixture character, the logged-in scene) in a moment: standing around, in combat, in stealth, or at low health, with a target. The agent gets back a picture, fixes what's off and draws it again before the player sees anything. `luajit tools/addon-shots/ui.lua <doc.json> <dir>` draws all four from the command line.

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
- **Secure action buttons** in game: the picture's `actionbar` becomes real buttons, with stance paging.
