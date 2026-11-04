# Designing in-game UI for Forever Buddy

Read this before proposing any change to how the game looks: unit frames, action bars, the HUD, or the Forever Buddy window. It is the house style the player signed off, learned from what players run today (the top of r/wowui).

## The look: clean boxes

The base theme is **Forever Clean**. Copy its feel, not a single screenshot.

- **Flat boxes with 1px black borders**, snapped to whole screen pixels. No bevels, no noise or stone textures, no glow, no glossy highlights.
- **Gentle horizontal gradients** on bar fills: a little lighter on the left, a little deeper on the right. Visible, not loud.
- **Stroked text everywhere.** Bold condensed type (the theme uses Barlow Semi Condensed Bold) with a heavy black outline. Stroked text is what makes a WoW UI read over a busy world.
- **Numbers are the biggest text in a frame.** Health as `1.2 K | 85%`, large and right-aligned; names smaller.
- **A tight cluster under the character.** Player frame and target frame butt straight into the action bar grid, at the height of one icon row. Everything else stays small at the edges.
- **Icons are the colour.** Trimmed 12% so the baked border disappears, 1px apart on black. Keybind small in the top right, cooldown number large in the middle, stacks bottom right, a gold border on what's ready to use.

## Colour

- **Health:** green for every class (`#4fd99a`, mint to deep green). Hostile targets red (`#d24a3a`).
- **Class colour goes on the name,** not the health bar. Use Blizzard's class colours (RAID_CLASS_COLORS).
- **Power colours keep their hue but are softened.** Blizzard's raw values (`#0000ff` mana, `#ff0000` rage, `#ffff00` energy) are too harsh. Use mana `#4a7fdc`, rage `#d64541`, energy `#f0c94a`.
- **Combo points** in pale gold (`#fff569`), as a segmented strip across the top of the action grid. They belong to the target: show them only while you have one.
- One accent at a time. If everything is bright, nothing is.

## Expressiveness comes from values, not new looks

Change the theme's values for a player rather than inventing a style: dark mode (dark fill, the missing part shows the colour), slim bars, stacked frames, bigger icons, a different accent. Keep the language the same.

The values a draft can set today are the `look: true` options in `get_settings`: health, power and accent colours, the gradient on or off, frame width and height, number size, text outline (thick, thin or none), side by side or stacked, and button size. Their defaults are this page's look. A good set of drafts changes one or two of them each, in ways the player can tell apart at a glance.

## Things players disliked

- Material textures (stone, leather, parchment) on HUD elements read as 2007.
- Arcs, rings, bloom and glowing crescents read as "a cheap MMO".
- Flat shapes without stroke or gradient read as "hacky SVGs".
- Text without an outline gets lost over the world.

## How to check a design

Put it side by side with a screenshot of a UI the player likes, at the same zoom, and match proportions, text size and stroke. Fix what differs. Never judge a HUD on a plain background: put a real game screenshot behind it.

## What can and can't be styled in game

- **Our own frames** (the Forever Buddy window, the Rogue HUD module, our action buttons): anything above.
- **Blizzard's frames:** positions come from an Edit Mode layout (the player imports it, or the addon applies one named "Forever Buddy"). Their look can only be faded (alpha) and covered by our frames; never moved, scaled or hidden from Lua, and never touched in combat.
