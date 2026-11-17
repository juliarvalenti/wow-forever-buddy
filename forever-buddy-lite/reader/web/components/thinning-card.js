// "Supply thinning" on the Opportunities page: items with far fewer listed than at the same hour
// yesterday while the price hasn't moved (thinning.rs), with the rule's score on your past scans
// first, so you can see whether it has earned trust before acting on it.

import { Component, define } from "../lib/component.js";
import { store } from "../lib/store.js";
import { html } from "../lib/html.js";
import { count, percent, ago } from "../lib/format.js";
import { coins } from "../lib/coins.js";
import { fetchThinning } from "../lib/api.js";

const ROWS = 15;

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
    const head = html`<h3>Supply thinning ${this.#data ? html`<span class="count">${this.#data.items.length}</span>` : ""}</h3>
      <p class="why">Far fewer listed than at the same hour yesterday, at a price that hasn’t moved yet: buy what’s left before sellers post higher.</p>`;
    if (this.#error) return html`<section class="card">${head}<p class="muted">Couldn’t load: ${this.#error}</p></section>`;
    if (!this.#data) return html`<section class="card">${head}<p class="muted">Loading…</p></section>`;
    const { items, record: r } = this.#data;
    const rate = (wins, n) => (n ? `${Math.round((wins / n) * 100)}%` : "–");
    const proven = r.signals >= 30 && r.wins / r.signals >= (r.baselineWins / Math.max(1, r.baseline)) * 1.5;
    const score = r.signals
      ? html`<p class="sub"><b>Track record:</b> ${count(r.signals)} past signals on ${count(r.items)} items, profitable 2–12h later
          ${rate(r.wins, r.signals)} of the time (any item at any scan: ${rate(r.baselineWins, r.baseline)}).
          ${proven ? html`<span class="up">Beating the baseline.</span>` : html`<span class="muted">Not proven yet: treat these as a watchlist.</span>`}</p>`
      : html`<p class="sub muted"><b>Track record:</b> none yet. It needs a day of scans to compare against, then a few hours to see what followed.</p>`;
    return html`
      <section class="card">
        ${head}${score}
        ${items.length
          ? html`<div class="table-scroll"><table>
              <thead><tr><th>Item</th><th>Now</th><th>Recent</th><th>Listed</th><th>Same hour yesterday</th><th>Seen</th></tr></thead>
              <tbody>
                ${items.slice(0, ROWS).map(
                  (t) => html`<tr data-id="${t.id}" class="clickable">
                    <td><button class="row-link" type="button">${t.name}</button></td>
                    <td>${coins(t.now)}</td><td>${coins(t.usual)}</td>
                    <td>${count(t.listed)} <span class="muted">(${percent(t.listed / t.yesterday - 1)})</span></td>
                    <td>${count(t.yesterday)}</td><td class="muted">${ago(t.at)}</td>
                  </tr>`,
                )}
              </tbody>
            </table></div>`
          : html`<p class="muted">Nothing thinning at the latest scan.</p>`}
      </section>`;
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
