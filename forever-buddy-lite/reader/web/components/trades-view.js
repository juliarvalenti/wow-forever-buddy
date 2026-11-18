// The Trades page: are you trading at a profit? From the AH mail invoices the addon records
// (Journal.lua): what each item cost you, what it sold for after the cut, the profit on what's sold,
// and what's still held, against what it would bring at today's cheapest listing.
// The figures are live (stone); the list of trades is a ledger of what happened (parchment).

import { Component, define } from "../lib/component.js";
import { store } from "../lib/store.js";
import { html } from "../lib/html.js";
import { count, ago } from "../lib/format.js";
import { coins } from "../lib/coins.js";
import { fetchJournal } from "../lib/api.js";
import { pageHead, panel, tiles, tile, table, signed as tone, emptyState, loadingState, errorState } from "../lib/ui.js";

const signed = (copper) => tone(coins(copper, { signed: true }), copper);
const itemName = (id, name) => (id ? html`<button class="row-link" type="button" data-open-item="${id}">${name}</button>` : name);

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
    const head = (lede) => pageHead({ title: "Trades", lede });
    if (this.#error) return html`${head()}${errorState({ title: "Couldn’t load trades", text: this.#error })}`;
    if (!this.#data) return html`${head()}${loadingState("Loading trades…")}`;
    const { totals, items, recent } = this.#data;
    if (!totals.trades) {
      return html`
        ${head("Your auction house buys and sales, from the invoices in your mail.")}
        ${panel({
          title: "No trades recorded yet",
          record: true,
          body: emptyState({
            icon: "mail",
            title: "The ledger is blank",
            text: "Every auction house buy and sale arrives by mail with an invoice. Open your mailbox after buying or selling and the addon records them; Save & Reload and they show here. It needs the updated addon (restart the game once).",
          }),
        })}`;
    }
    return html`
      ${head(`${count(totals.trades)} buys and sales from your AH mail. Sales are matched to your oldest buys first.`)}
      ${tiles([
        tile("Profit on sales", signed(totals.realized), "sold units, less what they cost"),
        tile("Spent", coins(totals.spent), "on everything bought"),
        tile("Earned", coins(totals.earned), "from sales, after the 5% cut"),
        tile("Still held", coins(totals.heldCost), html`cost; worth ${coins(totals.heldValue)} at today’s floor (${signed(totals.unrealized)})`),
      ])}
      ${panel({
        title: "By item",
        count: items.length,
        flush: table({
          sticky: true,
          columns: [
            { label: "Item" }, { label: "Bought", cls: "num" }, { label: "Avg cost", cls: "num" }, { label: "Sold", cls: "num" }, { label: "Earned", cls: "num" },
            { label: "Profit", cls: "num" }, { label: "Held", cls: "num" }, { label: "Worth now", cls: "num" }, { label: "Last", cls: "num" },
          ],
          rows: items,
          row: (l) => html`
            <tr>
              <td class="item">
                ${itemName(l.id, l.item)}
                ${l.sold_unmatched > 0 && html`<span class="sub">${count(l.sold_unmatched)} sold without a recorded buy</span>`}
              </td>
              <td class="num">${count(l.bought)}</td>
              <td class="num">${l.avgCost == null ? html`<span class="muted">–</span>` : coins(l.avgCost)}</td>
              <td class="num">${count(l.sold)}</td>
              <td class="num">${coins(l.earned)}</td>
              <td class="num profit">${l.sold > l.sold_unmatched ? signed(l.realized) : html`<span class="muted">–</span>`}</td>
              <td class="num">${l.held > 0 ? count(l.held) : html`<span class="muted">0</span>`}</td>
              <td class="num">${l.held > 0 && l.held_value != null ? html`${coins(l.held_value)} ${signed(l.held_value - l.held_cost)}` : html`<span class="muted">–</span>`}</td>
              <td class="num muted">${ago(l.last)}</td>
            </tr>`,
        }),
      })}
      ${panel({
        title: "Latest trades",
        record: true,
        meta: "from your AH mail",
        flush: table({
          columns: [{ label: "Item" }, { label: "What", cls: "text" }, { label: "Count", cls: "num" }, { label: "Each", cls: "num" }, { label: "Total", cls: "num" }, { label: "When", cls: "num" }],
          rows: recent,
          compact: true,
          row: (t) => html`
            <tr>
              <td class="item">${itemName(t.id, t.item)}</td>
              <td class="text ${t.kind === "buy" ? "" : "up"}">${t.kind === "buy" ? "Bought" : "Sold"}</td>
              <td class="num">${count(t.count)}</td>
              <td class="num">${coins(t.price / Math.max(1, t.count))}</td>
              <td class="num">${coins(t.kind === "buy" ? t.price : t.price - t.cut)}</td>
              <td class="num muted">${ago(t.t)}</td>
            </tr>`,
        }),
      })}`;
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

define("trades-view", TradesView);
