# Rogue HUD, round 4 (TH2 and TH3 handoff)

The design Julia signed off as the base: [[uploads/th1-clean4-vs-ref.png]] and
the tuned class colours [[uploads/th1-class-tuned.png]] in the room. This folder
is the prototype that drew them. It is design reference, not shipped code.

## What's here

- `specs/round4.lua`: every value of the look, in screen pixels at 1080p. Start here.
- `FBUnitProto/UF.lua`: a small interpreter that draws frames from a spec (bars,
  pips, text, plates, an action bar redraw, a minimap shell). It shows how each
  value is meant to render; it is not the module's architecture.
- `edit-mode-layout.lua`: Julia's positions (unit frames either side of the
  character, the bar right under), in `tools/addon-shots/layouts/` form.
- `render-uf.sh <spec>`: renders the rogue scene with the spec, to `out/`.
- Media: Barlow Semi Condensed Bold (OFL, licence beside it) and a soft glow
  used for the proc border.

## What the prototype fakes (TH2 and TH3 must make these real)

- It draws once at PLAYER_LOGIN (and the target on PLAYER_TARGET_CHANGED).
  Nothing updates after that: health, energy and combo points need UNIT_HEALTH,
  UNIT_POWER_FREQUENT and UNIT_POWER_UPDATE (combo via GetComboPoints("player", "target")).
- Cooldown numbers, the proc border (Eviscerate) and the stack count (Expose
  Armor) are painted values in the spec, not game state.
- The Slice and Dice timer ("14") is fixed text.
- No secret-value handling beyond a check in `value()`; no combat lockdown handling.
- The action bar is a picture of the real slots; Blizzard's buttons are at
  alpha 0 underneath. TH3 replaces that with our own secure buttons.
- The stroke is black copies of each text 2px out, because the emulator draws
  THICKOUTLINE thin. In game, try THICKOUTLINE first.
- `charY` and the frame positions assume a 1080p screen at the client's default UI scale.

## Values worth copying exactly

Health #4fd99a (gradient +38% to -18% brightness, left to right), hostile
#d24a3a, energy #f0c94a, combo #fff569, empty segment #2a2a2e, plate #141416,
borders #000000 at 1 screen pixel. Text: Barlow Semi Condensed Bold with a 2px
black stroke; health value 22px (player) and 20px (target), names 16px, keybinds
12px, cooldowns 22px, the buff timer 32px. Icons 46px, 1px apart, texcoords
trimmed 12%.
