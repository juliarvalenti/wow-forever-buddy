// The Trades page: are you trading at a profit? From the AH mail invoices the addon records
// (Journal.lua): what each item cost you, what it sold for after the cut, the profit on what's sold,
// and what's still held, against what it would bring at today's cheapest listing.

import { Component, define } from "../lib/component.js";
import { store } from "../lib/store.js";
import { html } from "../lib/html.js";
import { count, ago } from "../lib/format.js";
import { coins } from "../lib/coins.js";
import { fetchJournal } from "../lib/api.js";

const signed = (copper) => html`<span class="${copper > 0 ? "up" : copper < 0 ? "down" : "muted"}">${coins(copper, { signed: true })}</span>`;

class TradesView extends Component {
  #data = null;
  #error = null;
  #loadedFor = undefined;

  watch(s) {
    return [s.lastUpdate];
  }

  setup() {
    this.addEventListener("click", (event) => {
      const open = event.target.closest("[data-open-item]");
      if (open) store.openItem(Number(open.dataset.openItem));
    });
  }

  render() {
    if (this.#error) return html`<p class="empty">Couldn’t load trades: ${this.#error}</p>`;
    if (!this.#data) return html`<p class="empty">Loading trades…</p>`;
    const { totals, items, recent } = this.#data;
    if (!totals.trades) {
      return html`
        <header class="page-head"><h1>Trades</h1></header>
        <section class="card">
          <h3>No trades recorded yet</h3>
          <p class="sub">
            Every auction house buy and sale arrives by mail with an invoice. Open your mailbox after buying or selling and the
            addon records them; Save &amp; Reload and they show here. It needs the updated addon (restart the game once).
          </p>
        </section>`;
    }
    return html`
      <header class="page-head">
        <h1>Trades</h1>
        <p class="sub">${count(totals.trades)} buys and sales from your AH mail. Sales are matched to your oldest buys first.</p>
      </header>
      <div class="tiles">
        ${tile("Profit on sales", signed(totals.realized), "sold units, less what they cost")}
        ${tile("Spent", coins(totals.spent), "on everything bought")}
        ${tile("Earned", coins(totals.earned), "from sales, after the 5% cut")}
        ${tile("Still held", coins(totals.heldCost), html`cost; worth ${coins(totals.heldValue)} at today’s floor (${signed(totals.unrealized)})`)}
      </div>
      <section class="card">
        <h3>By item</h3>
        <div class="table-scroll">
          <table class="worklist">
            <thead>
              <tr>
                <th>Item</th><th class="num">Bought</th><th class="num">Avg cost</th><th class="num">Sold</th><th class="num">Earned</th>
                <th class="num">Profit</th><th class="num">Held</th><th class="num">Worth now</th><th class="num">Last</th>
              </tr>
            </thead>
            <tbody>
              ${items.map(
                (l) => html`
                  <tr>
                    <td class="item-cell">
                      ${l.id ? html`<button class="row-link" type="button" data-open-item="${l.id}">${l.item}</button>` : l.item}
                      ${l.sold_unmatched > 0 && html`<span class="sell-hint muted">${count(l.sold_unmatched)} sold without a recorded buy</span>`}
                    </td>
                    <td class="num">${count(l.bought)}</td>
                    <td class="num">${l.avgCost == null ? "–" : coins(l.avgCost)}</td>
                    <td class="num">${count(l.sold)}</td>
                    <td class="num">${coins(l.earned)}</td>
                    <td class="num">${l.sold > l.sold_unmatched ? signed(l.realized) : html`<span class="muted">–</span>`}</td>
                    <td class="num">${l.held > 0 ? count(l.held) : html`<span class="muted">0</span>`}</td>
                    <td class="num">${l.held > 0 && l.held_value != null ? html`${coins(l.held_value)} ${signed(l.held_value - l.held_cost)}` : html`<span class="muted">–</span>`}</td>
                    <td class="num muted">${ago(l.last)}</td>
                  </tr>`,
              )}
            </tbody>
          </table>
        </div>
      </section>
      <section class="card">
        <h3>Latest trades</h3>
        <div class="table-scroll">
          <table class="worklist">
            <thead><tr><th>Item</th><th class="text">What</th><th class="num">Count</th><th class="num">Each</th><th class="num">Total</th><th class="num">When</th></tr></thead>
            <tbody>
              ${recent.map(
                (t) => html`
                  <tr>
                    <td class="item-cell">${t.id ? html`<button class="row-link" type="button" data-open-item="${t.id}">${t.item}</button>` : t.item}</td>
                    <td class="text ${t.kind === "buy" ? "" : "up"}">${t.kind === "buy" ? "Bought" : "Sold"}</td>
                    <td class="num">${count(t.count)}</td>
                    <td class="num">${coins(t.price / Math.max(1, t.count))}</td>
                    <td class="num">${coins(t.kind === "buy" ? t.price : t.price - t.cut)}</td>
                    <td class="num muted">${ago(t.t)}</td>
                  </tr>`,
              )}
            </tbody>
          </table>
        </div>
      </section>`;
  }

  async rendered({ lastUpdate }) {
    if (this.#loadedFor === lastUpdate && (this.#data || this.#error)) return;
    this.#loadedFor = lastUpdate;
    try {
      this.#data = await fetchJournal();
      this.#error = null;
    } catch (error) {
      this.#error = error.message;
    }
    this.refresh(true);
  }
}

function tile(label, value, note) {
  return html`<div class="tile"><div class="tile-label">${label}</div><div class="tile-value">${value}</div><div class="tile-note">${note}</div></div>`;
}

define("trades-view", TradesView);
