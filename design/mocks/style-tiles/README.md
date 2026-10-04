# Style tiles: one dashboard, three materials

The markup (`_markup.js`) and layout (`_base.css`) are identical in all three tiles. Only the material CSS differs, so you're comparing look and feel, not structure. Open the `.html` files to see the motion; add `?still` to freeze it. Screenshots (WebP) are in `shots/`, and `shots/compare-1280x800.webp` puts all four side by side.

All textures are procedural: SVG noise for grain, inner shadows and off-centre light. None of the tiles adds new image files.

| Tile | Material | Gold is used for | Delight |
|---|---|---|---|
| **A · Tooltip** (`a-tooltip.html`) | The in-game item tooltip: blue-black glass, cool steel borders, white text. WoW yellow `#ffd100` for headers, and the classic red panel button | Money only | A real item tooltip pinned to the loot list: "Equip:" in green, sell price, where it dropped |
| **B · Ledger** (`b-ledger.html`) | Parchment pages in sepia ink, bound in tooled, stitched leather. The recap page is ruled with a red margin. Active nav is a red ribbon bookmark, the title is debossed, and quality colours are re-inked darker for paper | Coins only | A wax seal stamped "60" |
| **C · Forge** (`c-forge.html`) | Dark mottled stone lit by a forge glow from below-left. Stitched leather straps as panel headers. Aged, riveted bronze on the primary action only, and ember orange for anything live | Coins only | Anvil sparks around the level-up |

All three share the in-world copy (Last adventure, Ledger, Adventures, Travelled, Spent) and round-1 fix #6: a single row of four stat tiles, with the sidebar collapsing to an icon rail under 1100px.

## Designer's recommendation: a mix of all three

- **Forge for the app shell**: stone, leather straps, one bronze action, ember for "live". It's quiet enough for dense tables (backups, inventory) and never shouts.
- **Ledger parchment only for journal content**: Last adventure, session recap, the Ledger (gold history) page. Paper then *means* "a record of what happened", rather than being wallpaper.
- **Tooltip for every item hover in the app.** It's literally the game's tooltip, so items feel like items.

Each material then has a job, which keeps the app cohesive rather than collaged.

**D · Mix** (`d-mix.html`) builds this, with PM's condition that the soul (parchment and tooltips) must be prominent rather than token:

- **Forge is the frame only:** the leather sidebar, stone slabs for system data, bronze on the primary action, and ember for live state.
- **From Ledger:** the red ribbon nav and the wax seal.
- **Parchment for records:** Last adventure, recaps, the Ledger page and character sheets.
- **Tooltip glass on item hover.**

The rule is that records are parchment, and live or system data is stone.
