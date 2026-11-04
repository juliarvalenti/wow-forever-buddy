# Writing a whole UI for Forever Buddy

A UI document is JSON that describes a whole in-game UI: unit frames, bars, numbers, combo points, action buttons, and which of Blizzard's frames step aside. Nothing in it runs: Forever Buddy's addon reads it and builds frames. Never send Lua.

`render_ui` draws a document on a stand-in screen (Forever Buddy's test character, a level 34 rogue, logged in with a hostile target; it reads none of the player's data) and hands you back a picture, so you can see your work and fix it before the player looks. `propose_ui` then sends the finished document to the player: they see it drawn in Forever Buddy's Approvals and pick it with **Use this**, and it's live in the game at their next login, in place of the standard HUD, for every character on the account. They can go back to the standard HUD any time. To tweak the standard HUD's look instead, use `propose_ui_draft` (see [SKILL](../SKILL.md#changing-the-look-drafts-not-one-answer)).

## The loop

1. Read [design](design.md) for the house style, and ask what the player wants if it isn't clear.
2. Read the language with `get_ui_spec`. If the player already has a UI, `get_ui` gives you its document to revise. Otherwise start from round 4: `render_ui` with no document returns round 4 and its JSON.
3. `render_ui` it with `moments: ["all"]` (standing, combat, stealth, low_health) and the player's `class`. Look at every picture.
4. If the document has problems, nothing is drawn and you get each one with its path (`/parts/3/anchor/1`) and how to fix it. Fix them and draw again.
5. Fix what looks off: overlaps, text too small to read, things that should hide out of combat. Draw again.
6. Show the player the pictures and the idea in a sentence.
7. When they like it, `propose_ui` with a `name` they'll pick it by, `why` in a sentence, their `words`, and the `doc`. It's checked exactly as `render_ui` checks it, and a document with problems isn't queued. Tell them to open Approvals and pick **Use this**; it's live at their next login.

Without the MCP, the app's command line draws the same picture: `<WoW Forever Buddy> render doc.json --moment all --class rogue --out shot.png` (one file per moment, `shot-combat.png` and so on); `render --help` lists every option.

## A first document

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
    { "kind": "actionbar", "slots": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12], "icon": 34, "gap": 4,
      "anchor": ["BOTTOM", "screen", "BOTTOM", 0, 24] }
  ]
}
```

## The language in short

- **Sizes and offsets** are pixels on a 1080p screen.
- **Parts** are drawn in order. Kinds:
  - `plate`: a rectangle (`color`, `gradient`, `border`, `rotate` for diamonds and slashes).
  - `bar`: a fill that follows `value` (`health`, `power`, `combo`), with `back`, `ticks`, `vertical`, `reverse`.
  - `text`: a `format` with tokens `{name}` `{level}` `{hp}` `{hp%}` `{power}` `{power%}` `{combo}`; `font` (`Barlow`, `Geist`, `Marcellus`), `fontSize`, `outline`, `stroke`.
  - `pips`: marks lit up to a value; combo points one each, or 20 to 40 of them as a segmented health or power gauge, in a `row`, `column` or `arc`.
  - `icon`: an action slot's icon (`slot`) or an aura's (`aura`, a spell name).
  - `actionbar`: the player's own action buttons (`slots`), in a grid (`columns`) or an `arc`.
- **`anchor`** is `[point, to, toPoint, x, y]`; `to` is `screen`, `char` (where the character stands), or the `name` of an earlier part.
- **`unit`** is whose data a part shows (`player`, `target`, `focus`, `pet`, `party1` to `party4`). A part for a unit that isn't there isn't drawn.
- **Colours** are `"#rrggbb"`, or `class`, `power`, `reaction`, or stops by how full: `[[0.35, "#d24a3a"], [1, "#4fd99a"]]`.
- **States:** `size`, `alpha`, `hidden`, `color` and `gradient` can be an object by state, `{ "always": 0.3, "combat": 1 }`. States: `ooc`, `combat`, `target`, `notarget`, `full`, `low`, `stealth`; the last that holds wins.
- **Limits:** a key a part doesn't take is an error, not ignored. The game caps how many frames and text objects an addon can make, and the checker refuses a document that's too heavy. Stroked text is the expensive part: each one is drawn many times, so prefer `stroke` 1 or 2.

## Make it theirs, not a reskin

Don't hand back round 4 in new colours. Use the layout: put things where the player looks, stack or split them, make the number the design, hide what doesn't matter out of combat. And don't make one: make two or three that differ clearly and let the player pick.

The pictures are the test rogue's, not the player's: the same bars, numbers and buttons a document would show anyone, with the action icons as grey squares. So don't tell a mage it's their screen; say it's how the layout looks, and that the numbers and icons will be theirs in game.
