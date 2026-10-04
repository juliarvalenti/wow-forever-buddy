# WoW Forever Buddy: design mocks

These are HTML mocks from the design review rounds (designer ↔ PM ↔ Julia). **`round-3/` is the current design.** Build from it, and read `round-3/README.md` first.

| Folder | What it is |
|---|---|
| `round-3/` | **Current.** The full app in language "D": onboarding, the no-addon v0.1 state, error states, Auction House, and sketches of Addons, Macros and WeakAuras. |
| `round-2/` | First pass in "D". Its README holds the **material rules** (stone/leather = frame and system data, parchment = records, tooltip glass = item hover, bronze = the one primary action, ember = live, gold = coins only). |
| `style-tiles/` | The divergence round: the same dashboard as A Tooltip, B Ledger, C Forge and D Mix. Julia picked **D**. |
| `round-1/` | The first look (gold-on-dark), kept for history. It was superseded because it read as a generic dark dashboard. |

## Viewing
- Open any `.html` file directly in a browser. No build step is needed.
- The mocks load fonts from `node_modules/` (Cinzel, Geist) and some art from `src/components/ui/warcraftcn/assets`, so run `npm install` once.
- Query flags:
  - `?still` freezes motion.
  - `?tt=<item>` pins an item tooltip.
  - Per-screen state flags are listed in each round's README. `round-3/errors.html` links every first-run and error state.
- Screenshots are in each round's `shots/` as WebP at 1280x800 and 1024x700. They were captured at 2× and compressed to keep the repo small.
