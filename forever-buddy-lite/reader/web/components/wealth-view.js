// The Wealth tab: what you're worth now across characters, how it's moved, and who holds what.
// Gold comes from the addon's money log; item value from fbl-rs live's record at each save.
// The history is a record of what happened, so it's on parchment; the live figures are on stone.

import { Component, define } from "../lib/component.js";
import { html } from "../lib/html.js";
import { coins } from "../lib/coins.js";
import { pageHead, panel, tiles, tile, table, signed, character, loadingState } from "../lib/ui.js";
import "./wealth-chart.js";

class WealthView extends Component {
  watch(s) {
    return [s.wealth];
  }

  render({ wealth }) {
    if (!wealth) return loadingState("Loading wealth…");
    const chars = [...wealth.characters].sort((a, b) => b.gold + b.items - (a.gold + a.items));
    const gold = chars.reduce((sum, c) => sum + c.gold, 0);
    const items = chars.reduce((sum, c) => sum + c.items, 0);
    const first = wealth.gold[0];
    const sinceFirst = first ? gold - first[1] : 0;
    const historyStarts = first ? new Date(first[0] * 1000) : null;
    const thin = wealth.gold.length < 2 && wealth.history.length < 2;

    return html`
      ${pageHead({ title: "Wealth", lede: "Gold on every character, plus what their bags and bank would fetch at the latest AH prices." })}
      ${tiles([
        tile("Worth now", coins(gold + items), "gold + items"),
        tile("Gold", coins(gold), `on ${chars.length} characters`),
        tile("Items", coins(items), "bags and bank at today’s prices"),
        tile(
          "Gold change",
          sinceFirst === 0 ? html`<span class="muted">no change yet</span>` : signed(coins(sinceFirst, { signed: true }), sinceFirst),
          historyStarts ? `since ${historyStarts.toLocaleString([], { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" })}` : "no history yet",
        ),
      ])}
      ${panel({
        title: "Over time",
        record: true,
        meta: historyStarts ? `since ${historyStarts.toLocaleDateString([], { month: "short", day: "numeric" })}` : "",
        body: html`
          ${thin &&
          html`<p class="note">
            History starts now. From your next /reload the addon logs every change in gold (loot, sales, quests, repairs), and
            each save records what your items are worth. Come back after a session.
          </p>`}
          <wealth-chart></wealth-chart>`,
      })}
      ${panel({
        title: "By character",
        count: chars.length,
        flush: table({
          columns: [{ label: "Character" }, { label: "Level", cls: "num" }, { label: "Gold", cls: "num" }, { label: "Items", cls: "num" }, { label: "Total", cls: "num" }],
          rows: chars,
          row: (c) => html`
            <tr>
              <td>${character(c.name, c.class)}${c.class && html` <span class="muted small">${titleCase(c.class)}</span>`}</td>
              <td class="num">${c.level ?? "–"}</td>
              <td class="num">${coins(c.gold)}</td>
              <td class="num">${coins(c.items)}</td>
              <td class="num strong">${coins(c.gold + c.items)}</td>
            </tr>`,
        }),
      })}`;
  }

  rendered({ wealth }) {
    const chart = this.querySelector("wealth-chart");
    if (!chart || !wealth) return;
    const total = wealth.history.map(([t, g, i]) => [t, g + i]);
    chart.show(wealth.gold, total);
  }
}

// "ROGUE" (the game's class token) -> "Rogue".
function titleCase(word) {
  return word.charAt(0).toUpperCase() + word.slice(1).toLowerCase();
}

define("wealth-view", WealthView);
