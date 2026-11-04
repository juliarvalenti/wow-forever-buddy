# The Forever Buddy UI language (UI1, phase 1)

Status: draft for Julia to read before anything is built. Issue #259. Written by @designer.

One document describes a whole UI. An agent writes it, the app checks it, the agent sees it rendered in each state, the player picks it, and the addon draws it in game. The HUD we ship today becomes one example document, not the only shape a UI can take.

Nothing in a document runs. It's data: the addon reads it and builds frames from it. An agent never sends Lua.

---

## 1. A first look

A clean rogue UI: one long health bar under the character, energy as a thin line beneath it, combo points as five squares, the target's name and health to the right only while there is a target, and twelve action buttons in two rows.

```json
{
  "ui": 1,
  "name": "Clean rogue",
  "for": { "class": "ROGUE" },
  "styles": {
    "panel": { "fill": "#101114", "border": { "size": 1, "color": "#000000" } },
    "num":   { "font": { "face": "Barlow", "size": 20, "outline": "thick" }, "color": "#ffffff" }
  },
  "elements": [
    {
      "id": "me", "type": "group",
      "anchor": { "point": "BOTTOM", "to": "screen", "at": "BOTTOM", "y": 260 },
      "size": [360, 36],
      "children": [
        { "id": "hp", "type": "bar", "style": "panel", "value": "player.health",
          "size": [360, 28], "anchor": { "point": "TOP", "to": "parent", "at": "TOP" },
          "fill": { "gradient": ["#62e3a8", "#2f9e6c"] } },
        { "id": "hpText", "type": "text", "style": "num", "text": "{player.health:short} | {player.health:pct}",
          "anchor": { "point": "RIGHT", "to": "#hp", "at": "RIGHT", "x": -8 } },
        { "id": "energy", "type": "bar", "value": "player.power", "size": [360, 4],
          "anchor": { "point": "TOP", "to": "#hp", "at": "BOTTOM", "y": -3 }, "fill": "power" },
        { "id": "combo", "type": "pips", "value": "player.combo", "count": 5, "size": [16, 6], "gap": 3,
          "anchor": { "point": "BOTTOM", "to": "#hp", "at": "TOP", "y": 6 },
          "on": "#fff569", "off": "#2a2a2e",
          "when": [ { "if": "!target", "set": { "hidden": true } } ] }
      ]
    },
    {
      "id": "them", "type": "text", "style": "num",
      "text": "{target.name}  {target.health:pct}",
      "anchor": { "point": "LEFT", "to": "#me", "at": "RIGHT", "x": 16 },
      "color": "reaction",
      "when": [ { "if": "!target", "set": { "hidden": true } } ]
    },
    {
      "id": "bars", "type": "buttons", "actions": "1-12",
      "layout": { "grid": { "columns": 6 } }, "size": [44, 44], "gap": 2,
      "anchor": { "point": "TOP", "to": "#me", "at": "BOTTOM", "y": -10 }
    }
  ]
}
```

Everything else in this doc explains what those words mean and what else exists.

## 2. The document

| Key | What it is |
|---|---|
| `ui` | The language version. `1` for now. |
| `name` | What the player picks it by, a few words. |
| `for` | Who it's for: `{ "character": "Thrandor-Vargur" }`, `{ "class": "ROGUE", "spec": 1 }`, `{ "class": "ROGUE" }`, or left out for everyone. The most specific match wins, so a rogue can have one UI and a warrior another. |
| `styles` | Named bundles of style properties (section 7), used by elements' `style`. |
| `elements` | The elements, in drawing order. Each may have `children`. |
| `blizzard` | Optional: where Blizzard's own frames go, and which of them fade (section 10). |

**Units of measure.** Sizes and offsets are pixels on a 1080p screen, as in our designs. The addon scales them to the player's resolution and UI scale, and snaps them to whole screen pixels so 1px borders stay crisp.

## 3. Elements

Every element has a `type` and an `id`. Ids are short names, unique in the document, used to anchor to (`"to": "#hp"`) and in error messages.

| Type | Draws | Notes |
|---|---|---|
| `group` | Nothing itself | Holds children, positions them together, and can lay them out (`layout`). |
| `box` | A rectangle | Fill, gradient, border. Panels, backgrounds, dividers, accents. |
| `bar` | A fill that follows a value | `value`, optional `max`, `orientation` (`horizontal`, `vertical`), `reverse`. |
| `text` | A line of text | `text` is a format (section 6). |
| `icon` | An image | `icon` is a binding (an aura's or spell's icon, class icon, raid mark) or a theme image. Optional `cooldown` swipe and `count`. |
| `pips` | A row of marks lit up to a value | `count`, `on`, `off`, `gap`, `layout`. Combo points, holy power, soul shards. |
| `list` | One child per item in a set | `of` (e.g. `player.buffs`), `max`, `filter`, `layout`, and an `item` template that reads the item as `it` (`{it.name}`, `it.icon`, `it.remaining`). Auras, party members, boss frames. |
| `buttons` | Action buttons | `actions` (`"1-12"`, `"61-64"`, or a list), `layout`, `size`, `gap`. One button per action slot (section 9). |

Common properties on any element:

- `anchor`, `size`, `layer` (section 4)
- `style` (a name, or a list of names merged in order) and any style property inline (section 7)
- `when` (section 8)
- `children` (any element can hold others; they're drawn above it and move with it)

## 4. Placing things

```json
"anchor": { "point": "TOPLEFT", "to": "#hp", "at": "BOTTOMLEFT", "x": 0, "y": -4 }
```

- `point`: which corner or edge of this element is pinned. One of `TOPLEFT`, `TOP`, `TOPRIGHT`, `LEFT`, `CENTER`, `RIGHT`, `BOTTOMLEFT`, `BOTTOM`, `BOTTOMRIGHT`.
- `to`: what it's pinned to: `screen`, `parent` (the default inside a group), `#id` for another element, or `character` (the middle of the screen, where your character stands).
- `at`: the point on `to` it's pinned to. Defaults to `point`.
- `x`, `y`: offset in pixels. Up and right are positive.

An element may have two anchors (`"anchor": [ {...}, {...} ]`) to stretch between them, as WoW frames do; then it needs no `size` on that axis.

`size` is `[width, height]`. Text sizes itself to its content unless given a width, and then it wraps or clips (`"overflow": "clip"`).

`layer` is `0` to `9`, low to high. Children sit above their parent; `layer` lifts an element above its siblings.

**Layouts.** A `group`, `list` or `buttons` can arrange its children instead of each child anchoring itself:

```json
"layout": { "row": { "gap": 2, "align": "center" } }
"layout": { "column": { "gap": 4 } }
"layout": { "grid": { "columns": 8, "gap": 1, "grow": "up" } }
"layout": { "arc": { "radius": 120, "from": 200, "to": 340 } }
```

## 5. Game data (bindings)

A binding names a value the game knows: `unit.field`. Units are those the game lets addons read: `player`, `target`, `targettarget`, `focus`, `pet`, `party1` to `party4`, `boss1` to `boss5`.

| Field | Is |
|---|---|
| `health`, `healthMax`, `absorb` | Numbers. `health` on a `bar` fills by `health / healthMax`. |
| `power`, `powerMax`, `powerType` | The unit's main power (mana, rage, energy, focus). |
| `combo` | Combo points (and the class's other points: holy power, chi, soul shards). |
| `name`, `level`, `class`, `spec`, `race` | Text. |
| `exists`, `dead`, `inRange`, `inCombat`, `isPlayer`, `hostile` | True or false: for `when`. |
| `cast` | The unit's cast or channel: `cast.name`, `cast.icon`, `cast.progress`, `cast.remaining`, `cast.interruptible`. |
| `buffs`, `debuffs` | Sets for a `list`. Each item has `name`, `icon`, `count`, `remaining`, `duration`, `mine`, `dispel`. |
| `aura("Slice and Dice")` | One aura by name or spell id, with the same fields, and `exists`. |

Not about a unit:

| Binding | Is |
|---|---|
| `spell("Kick")` | `icon`, `cooldown.remaining`, `cooldown.progress`, `usable`, `charges`. |
| `game.time`, `game.zone`, `game.fps` | The clock, where you are, frames per second. |
| `player.stealth`, `player.mounted`, `player.resting`, `player.group` (`solo`, `party`, `raid`), `player.form` | For `when`. |

Colours can be bound too: `"fill": "class"`, `"reaction"` (red, yellow, green), `"power"` (the power type's colour), or `"health"` (from a list of stops, below).

## 6. Text formats

A `text` element's `text` mixes words and bindings in braces, with an optional format after a colon:

```
"{player.health:short} | {player.health:pct}"     ->  1.2 K | 85%
"{target.name}"                                    ->  Syndicate Highwayman
"{aura(Slice and Dice).remaining:secs}"           ->  14
"{player.power} / {player.powerMax}"              ->  64 / 100
```

| Format | Shows |
|---|---|
| (none) | The value as it is. |
| `short` | 690, 1.2 K, 3.4 M. |
| `pct` | Percent of the matching max, rounded: 85%. |
| `int` | A whole number. |
| `secs` | Seconds, rounded up: 14. Over a minute: 1:05. |
| `time` | 2m, 1h, as on cooldowns. |
| `upper`, `lower` | Case. |
| `max(12)` | Text cut to 12 characters with a dot leader. |

Text that names a value with nothing behind it (no target, aura gone) shows empty, and so does a value the game is hiding (section 8).

## 7. Style

Any element can carry these, directly or through `style`:

| Property | Takes |
|---|---|
| `fill` | A colour (`"#4fd99a"`, or `class`, `reaction`, `power`, `health`), a gradient `{ "gradient": ["#62e3a8", "#2f9e6c"], "direction": "horizontal" }` (two or three stops), or a theme image name. |
| `stops` | For `bar` and `pips`: colours by how full, `{ "0": "#b3202a", "0.35": "#e0a43a", "1": "#3a8f2a" }`. Used by `"fill": "health"`. |
| `back` | The empty part of a bar: a colour or gradient. |
| `border` | `{ "size": 1, "color": "#000000" }`. |
| `color` | Text colour, or a bound colour. |
| `font` | `{ "face": "Barlow", "size": 20, "outline": "thick", "shadow": false }`. Faces: `Barlow` (bold, condensed; ours), `Geist`, `Marcellus`, and the game's `Friz`, `Arial Narrow`, `Morpheus`, `Skurri`. Outline: `thick`, `thin`, `none`. |
| `align` | Text: `left`, `center`, `right`. |
| `alpha` | 0 to 1. |
| `hidden` | True hides it (with its children). |
| `padding` | Space inside a group or list, in pixels. |

Styles merge: `"style": ["panel", "danger"]` takes `panel`, then `danger` over it, then the element's own properties over both.

## 8. States

`when` changes properties when something is true. It's a list; each entry has an `if` and a `set` of properties. Later entries win.

```json
"when": [
  { "if": "!combat",            "set": { "alpha": 0.4 } },
  { "if": "player.health < 35%", "set": { "fill": "#d24a3a" }, "hidden": "off" },
  { "if": "player.stealth",      "set": { "alpha": 0.6 } },
  { "if": "!target",             "set": { "hidden": true } }
]
```

An `if` is one of:

- A named state: `combat`, `stealth`, `target`, `focus`, `group`, `raid`, `mounted`, `resting`, `dead`.
- A true-or-false binding: `target.hostile`, `aura(Slice and Dice).exists`, `spell(Kick).usable`.
- A comparison: `player.health < 35%`, `player.power >= 60`, `target.cast.interruptible`.
- Any of these with `!` in front for "not".

**Values the game hides in combat.** WoW: Forever runs the retail client, which can hand an addon a hidden ("secret") value in combat. A hidden value can still fill a `bar`; it can't be compared, added up or printed as a number. So:

- A `bar` always works: the addon hands the value straight to the game's own status bar.
- A comparison on a hidden value needs `hidden` to say what to assume: `"on"` (treat it as true) or `"off"`. Leaving it out is an error, so no design breaks quietly in a fight.
- Text that needs arithmetic on a hidden value (`:short`, `:pct`) shows `"?"`, or a `fallback` you give: `{ "text": "{player.health:pct}", "fallback": "" }`.

Which values Forever actually hides, and for which units, is for @coder2 to confirm in game before phase 2 ships. The language is written so that the answer only changes what the renderer does, not what documents say.

**What can't change in combat.** Action buttons are protected: their place, size and whether they show can only change out of combat. A `when` on a `buttons` element (or anything inside it) may change `alpha` and colours at any time, but anything that moves, resizes or hides it is applied when combat ends. The checker warns when a design relies on that.

## 9. Action buttons

```json
{ "id": "main", "type": "buttons", "actions": "1-12", "paging": "stance",
  "layout": { "grid": { "columns": 12 } }, "size": [40, 40], "gap": 1,
  "anchor": { "point": "BOTTOM", "to": "screen", "y": 40 },
  "keybind": { "font": { "face": "Barlow", "size": 12 }, "point": "TOPRIGHT" },
  "cooldown": { "font": { "face": "Barlow", "size": 22 } } }
```

- `actions`: the action slots, as the game numbers them: 1 to 12 is the main bar, 61 to 72 bar 2, and so on. Any number of `buttons` elements, any grid or shape (`row`, `column`, `grid`, `arc`).
- `paging`: `stance` makes the main bar swap as Blizzard's does (a rogue's stealth bar, a druid's forms). Off by default for other slots.
- Each button shows what's in its slot, with its cooldown, charges, range and the proc glow. `keybind`, `cooldown`, `count` and `glow` style those parts.
- **The click and the key are the player's.** Our buttons use the game's secure action button, the same as Blizzard's and Bartender's. A button casts exactly what's in its slot when the player presses it. A document can't put anything in a slot, write a macro or bind a key.
- Blizzard's bars that ours replace fade out (alpha only), with click shields over them, as today.

## 10. Blizzard's frames

A document can include where Blizzard's own frames go, so one draft describes the whole screen:

```json
"blizzard": {
  "place": [
    { "frame": "minimap", "anchor": { "point": "TOPRIGHT", "to": "screen", "x": -20, "y": -20 } },
    { "frame": "chat",    "anchor": { "point": "BOTTOMLEFT", "to": "screen", "x": 20, "y": 20 } },
    { "frame": "questTracker", "anchor": { "point": "RIGHT", "to": "screen", "x": -20 } }
  ],
  "fade": ["playerFrame", "targetFrame", "mainBar", "bar2"]
}
```

- `place` becomes an Edit Mode layout named "Forever Buddy", the game's own way of moving its frames. The addon never moves Blizzard's frames itself.
- `fade` sets Blizzard frames we stand in for to alpha 0, out of combat. Alpha is the only change we make to them.

## 11. Checking a document

The app checks every document before it's drawn or offered to the player, and the MCP's validate tool returns the same report. Errors stop it; warnings go along with it to the player.

Each problem has a path into the document, a code, and a sentence that says how to fix it:

```
error  /elements/0/children/1/anchor/to   unknown-id
       anchors to #hpBar, but there's no element with that id. Ids here: me, hp, hpText, energy, combo.
error  /elements/2/when/1                  hidden-needed
       "player.health < 35%" compares a value the game can hide in combat. Add "hidden": "on" or "off".
error  /elements/3/layout/grid/columns     out-of-range
       columns is 40; it takes 1 to 24.
warn   /elements/1                         off-screen
       #them ends 140 px past the right edge at 1920x1080.
warn   /elements/0/children/0              overlap
       #hp and #combo cover the same 360x6 px.
```

Checked: the shape of every value, ids and anchors (no missing targets, no loops), bindings and formats against the known lists, colours, fonts and theme images, `hidden` on every comparison that needs one, protected changes in combat, and the limits below.

Warned: off screen at 1080p and 1440p, overlapping elements, text below 4.5:1 contrast on its background, health or the target hidden in combat, and a document that leaves the player without action buttons.

**Limits,** so no document can slow the game: 300 elements, nesting 8 deep, 40 items in a `list`, 120 buttons, 3 stops in a gradient, 64 KB in all.

## 12. Seeing it

The MCP renders a document to an image with the same Lua the game runs (the Your UI preview's), in each state an agent asks for: standing around, in combat, in stealth, at low health, with and without a target, in a group. The agent looks, fixes, and renders again before the player sees anything. The Your UI tab shows the drafts the same way, per character and state.

## 13. What stays the same

- **Declarative only.** No code, no expressions beyond the comparisons in section 8, no strings the addon runs.
- **Propose, never act.** A document reaches the game only when the player picks it with Use this.
- **No automation.** Nothing in the language plays the game: buttons only show the player's own slots and cast on the player's own press.
- **The game's limits apply.** Protected frames change only out of combat; designs have to work when values are hidden.

---

## For Julia

Three things I'd like your call on before phase 2:

1. **JSON,** as above, or a lighter hand-written form (YAML-like, no braces or quotes)? JSON is what agents write most reliably and what the app checks most easily; the lighter form is nicer to read. I'd start with JSON and add the other later if you want it.
2. **Per character or per spec by default?** I've made the most specific match win (character, then spec, then class, then everyone). Is that how you'd expect it to behave?
3. **Blizzard's frames (section 10) in the first build, or after?** Our own frames and buttons are the bulk of it; placing the minimap and chat through Edit Mode can follow.
