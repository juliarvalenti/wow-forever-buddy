// One item's price ladder: a bar per price level (how many are listed at it), the levels a good gap
// would buy in color, and a line where you'd relist.
//
// Not tied to the store: the item view calls chart.show(item, ladder, verdictRow).

import { html } from "../lib/html.js";
import { ago, count } from "../lib/format.js";
import { coins } from "../lib/coins.js";
import { verdictPill } from "./bits.js";

const MAX_LEVELS = 20;

class LadderChart extends HTMLElement {
  show(item, ladder, verdict) {
    if (!ladder) {
      this.innerHTML = String(html`
        <section class="card">
          <h3>Price ladder</h3>
          <p class="sub">Not searched yet: open this item at the AH, then Save prices &amp; reload.</p>
        </section>`);
      return;
    }

    const gap = verdict?.kind === "gap" ? verdict.gap : null;
    const levels = ladder.levels.slice(0, MAX_LEVELS);
    const biggest = Math.max(...levels.map((level) => level.qty));
    let runningTotal = 0;

    const rows = levels.map((level, i) => {
      runningTotal += level.qty;
      const toBuy = gap && i < gap.levels;
      return html`
        ${gap && i === gap.levels && html`<div class="relist-line">relist at ${coins(gap.relist)}</div>`}
        <div class="level-price">${coins(level.price)}</div>
        <div><div class="level-bar ${toBuy ? "to-buy" : ""}" style="width:${Math.max(1, (level.qty / biggest) * 100)}%"></div></div>
        <div class="level-qty">${count(level.qty)}${level.mine && html` <span class="yours">yours</span>`}</div>
        <div class="level-total">${count(runningTotal)}</div>`;
    });

    this.innerHTML = String(html`
      <section class="card">
        <h3>Price ladder ${verdict && verdictPill(verdict.kind)}</h3>
        <p class="sub">
          Searched ${ago(ladder.at)}, ${count(ladder.total)} listed. Each row is one price: how many are up at it, and the total so far.
          ${verdict && html`<br />${verdict.text}`}
        </p>
        <div class="ladder">${rows}</div>
      </section>`);
  }
}

customElements.define("ladder-chart", LadderChart);
