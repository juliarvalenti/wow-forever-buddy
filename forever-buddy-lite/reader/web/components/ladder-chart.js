// One item's price ladder: a bar per price level (how many are listed at it), the levels a good gap
// would buy glowing ember, and a line where you'd relist.
//
// Not tied to the store: the item view calls chart.show(item, ladder, verdictRow).

import { html } from "../lib/html.js";
import { ago, count } from "../lib/format.js";
import { coins } from "../lib/coins.js";
import { panel, pill, legend, emptyState } from "../lib/ui.js";
import { verdictPill } from "./bits.js";

const MAX_LEVELS = 20;

class LadderChart extends HTMLElement {
  show(item, ladder, verdict) {
    if (!ladder) {
      this.innerHTML = String(
        panel({
          title: "Price ladder",
          body: emptyState({ text: "Not searched yet: open this item at the AH, then Save prices & reload.", compact: true }),
        }),
      );
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
        <div class="level-qty">${level.mine && html`${pill("yours", "bronze")} `}${count(level.qty)}</div>
        <div class="level-total">${count(runningTotal)}</div>`;
    });

    this.innerHTML = String(
      panel({
        title: "Price ladder",
        meta: html`${verdict && verdictPill(verdict.kind)}`,
        body: html`
          <p class="note">
            Searched ${ago(ladder.at)}, ${count(ladder.total)} listed. Each row is one price: how many are up at it, and the total so far.
            ${verdict && html`<br /><b>${verdict.text}</b>`}
          </p>
          ${gap &&
          legend([
            { label: "Levels to buy", kind: "bar", color: "var(--chart-bar-hot)" },
            { label: "Where to relist", kind: "dash", color: "var(--up)" },
          ])}
          <div class="ladder">
            <div class="head">Price</div><div class="head" aria-hidden="true"></div>
            <div class="head level-qty">Listed</div><div class="head level-total">Total</div>
            ${rows}
          </div>`,
      }),
    );
  }
}

customElements.define("ladder-chart", LadderChart);
