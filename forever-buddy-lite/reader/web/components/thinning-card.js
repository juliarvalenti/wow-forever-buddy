// "Supply thinning" on the Opportunities page: items with far fewer listed than at the same hour
// yesterday while the price hasn't moved (thinning.rs), with the rule's score on your past scans
// first, so you can see whether it has earned trust before acting on it.

import { Component, define } from "../lib/component.js";
import { store } from "../lib/store.js";
import { html } from "../lib/html.js";
import { count, percent, ago } from "../lib/format.js";
import { coins } from "../lib/coins.js";
import { fetchThinning } from "../lib/api.js";
import { panel, table, pill, loadingState, errorState } from "../lib/ui.js";

const ROWS = 15;
const WHY = "Far fewer listed than at the same hour yesterday, at a price that hasn’t moved yet: buy what’s left before sellers post higher.";

class ThinningCard extends Component {
  #data = null;
  #error = null;
  #loadedFor = undefined;

  watch(s) {
    return [s.lastUpdate];
  }

  setup() {
    this.addEventListener("click", (event) => {
      const row = event.target.closest("tr[data-id]");
      if (row) store.openItem(Number(row.dataset.id));
    });
  }

  render() {
    const base = { title: "Supply thinning", id: "thinning-title", count: this.#data?.items.length };
    if (this.#error) return panel({ ...base, body: html`<p class="note">${WHY}</p>${errorState({ title: "Couldn’t load", text: this.#error, compact: true })}` });
    if (!this.#data) return panel({ ...base, body: html`<p class="note">${WHY}</p>${loadingState("Loading…", { compact: true })}` });

    const { items, record: r } = this.#data;
    const rate = (wins, n) => (n ? `${Math.round((wins / n) * 100)}%` : "–");
    const proven = r.signals >= 30 && r.wins / r.signals >= (r.baselineWins / Math.max(1, r.baseline)) * 1.5;
    const score = r.signals
      ? html`<p class="note"><b>Track record:</b> ${count(r.signals)} past signals on ${count(r.items)} items, profitable 2–12h later
          ${rate(r.wins, r.signals)} of the time (any item at any scan: ${rate(r.baselineWins, r.baseline)}).</p>`
      : html`<p class="note"><b>Track record:</b> none yet. It needs a day of scans to compare against, then a few hours to see what followed.</p>`;

    return panel({
      ...base,
      meta: r.signals ? (proven ? pill("Beating the baseline", "ok") : pill("Not proven yet: a watchlist", "warn")) : pill("No track record yet", "quiet"),
      body: html`<p class="note">${WHY}</p>${score}`,
      flush: table({
        columns: [{ label: "Item" }, { label: "Now", cls: "num" }, { label: "Recent", cls: "num" }, { label: "Listed", cls: "num" }, { label: "Same hour yesterday", cls: "num" }, { label: "Seen", cls: "num" }],
        rows: items.slice(0, ROWS),
        empty: "Nothing thinning at the latest scan.",
        row: (t) => html`
          <tr data-id="${t.id}" class="clickable">
            <td class="item"><button class="row-link" type="button">${t.name}</button></td>
            <td class="num">${coins(t.now)}</td>
            <td class="num">${coins(t.usual)}</td>
            <td class="num">${count(t.listed)} <span class="muted">(${percent(t.listed / t.yesterday - 1)})</span></td>
            <td class="num">${count(t.yesterday)}</td>
            <td class="num muted">${ago(t.at)}</td>
          </tr>`,
      }),
    });
  }

  async rendered({ lastUpdate }) {
    if (this.#loadedFor === lastUpdate && (this.#data || this.#error)) return;
    this.#loadedFor = lastUpdate;
    try {
      this.#data = await fetchThinning();
      this.#error = null;
    } catch (error) {
      this.#error = error.message;
    }
    this.refresh(true);
  }
}

define("thinning-card", ThinningCard);
