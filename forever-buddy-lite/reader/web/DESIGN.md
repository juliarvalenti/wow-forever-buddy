# ForeverBuddy Lite: design system

The dashboard wears the Forever Buddy app's design language "D" (`design/mocks/round-3`, `src/styles/d.css`). The living style guide is in the app at **`#/design`** (linked from the footer): every token and component, in its states, drawn by the real CSS.

## Materials, one job each

| Material | Job | How |
|---|---|---|
| Stone | the frame and live data: panels, tiles, tables of current state | `.panel`, `.tile` |
| Leather | structure: the sidebar, the strap across a panel's head | `.panel-head`, `app-header` |
| Parchment | records, things that happened: the trade ledger, wealth over time | `panel({ record: true })` / `.record` |
| Bronze | the **one** primary action on a view | `.btn.primary` |
| Ember | live or pending: the connection, a new scan, the selected row, focus | `.dot.live`, `.panel.act`, `--accent` |
| Ribbon | "you are here" in the navigation | `.nav-link[aria-current]` |
| Gold | coins only. Nothing else is gold. | `coins()` |

Type: Geist for the interface, Marcellus for page titles and the wordmark only, Georgia on parchment. Sentence case; no capitals or letter-spacing for labels. Dark only, like the app.

## Files

- `styles/tokens.css`: every raw value. `--c-*` is the palette (use only in tokens.css); everything else is a role (`--text-muted`, `--surface-well`, `--up`, `--chart-1`). `.record` re-points the roles to ink, so a component written for stone reads right on paper.
- `styles/base.css`: reset, the app shell, page heads, layout utilities (`.stack`, `.cluster`, `.cols`, `.cols.even`).
- `styles/components.css`: every reusable piece (its header lists them).
- `styles/pages.css`: only what is particular to one page. A new page should rarely need anything here.
- `lib/ui.js`: markup builders for the pieces. `lib/icons.js`: the app's icon set. `lib/coins.js`: money. `lib/toast.js`: toasts. `components/fb-tabs.js`: tabs.

## Building a new page

1. Add its name to `VIEWS` in `lib/store.js` (or to `PAGES` if it shouldn't be in the navigation), a line to `NAV` in `components/app-header.js`, and a line to `components/main-view.js`. Back/Forward and links (`#/name`) then work by themselves.
2. Write the component. A whole page is usually this much:

```js
import { Component, define } from "../lib/component.js";
import { html } from "../lib/html.js";
import { coins } from "../lib/coins.js";
import { pageHead, panel, tiles, tile, table, loadingState } from "../lib/ui.js";

class GoldView extends Component {
  watch(s) {
    return [s.wealth]; // re-render only when this changes
  }

  render({ wealth }) {
    if (!wealth) return loadingState("Loading…");
    return html`
      ${pageHead({ title: "Gold", lede: "Everything your characters carry." })}
      ${tiles([tile("Gold", coins(wealth.total), "on every character")])}
      ${panel({
        title: "By character",
        count: wealth.characters.length,
        flush: table({
          columns: [{ label: "Character" }, { label: "Gold", cls: "num" }],
          rows: wealth.characters,
          row: (c) => html`<tr><td>${c.name}</td><td class="num">${coins(c.gold)}</td></tr>`,
        }),
      })}`;
  }
}

define("gold-view", GoldView);
```

3. Import it from `app.js`, and add the tag to the list of block elements at the top of `styles/base.css`.

## Rules of thumb

- **Escape everything.** Build markup with the `html` tagged template; anything interpolated is escaped (item and character names come from the game). `raw()` only for markup you built yourself.
- **One bronze button per view**, at most one `tone: "act"` panel per page. Everything else is stone (`.btn`) or quiet (`.btn.quiet`).
- **Figures right-aligned** (`cls: "num"`), money through `coins()`, gains and losses through `signed()` (or `.up` / `.down`).
- **Rows that open something** are `tr.clickable` with the name as a `.row-link` button, so the keyboard reaches it.
- **Empty, loading, error**: `emptyState()`, `loadingState()`, `errorState()`; add `compact: true` inside a panel.
- **Charts** use the `.chart` classes and `--chart-*` tokens; money and quantity axes start at zero.
- **Phone width**: every grid sets `grid-template-columns` with `minmax(0, 1fr)`; wide tables scroll inside `.table-scroll`. Nothing may scroll the page sideways at 375px.
- **No new colours in a component.** If you need one, add a role to `tokens.css` and show it on `#/design`.
