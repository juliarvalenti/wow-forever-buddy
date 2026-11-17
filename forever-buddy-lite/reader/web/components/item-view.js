// One item: headline numbers, its price ladder, price and quantity by day, and the raw values.

import { Component, define } from "../lib/component.js";
import { html } from "../lib/html.js";
import { percent, count } from "../lib/format.js";
import { coins } from "../lib/coins.js";
import "./ladder-chart.js";
import "./day-chart.js";

class ItemView extends Component {
  watch(s) {
    return [s.items, s.prefs.selectedId, s.opps];
  }

  render({ items, prefs }) {
    const item = items.find((it) => it.id === prefs.selectedId);
    if (!items.length) return html`<p class="empty">Loading prices…</p>`;
    if (!item) return html`<p class="empty">Pick an item from the list.</p>`;

    return html`
      <header class="page-head">
        <h1>${item.n}</h1>
        <p class="sub">${item.k} · item ${item.id}</p>
      </header>
      <div class="tiles">${tiles(item)}</div>
      <ladder-chart></ladder-chart>
      <section class="card">
        <h3>Lowest price by day</h3>
        <p class="sub">Line: the day’s lowest listing. Band: lowest to highest cheapest listing seen that day.</p>
        <day-chart kind="price"></day-chart>
      </section>
      <section class="card">
        <h3>Quantity listed by day</h3>
        <day-chart kind="listed"></day-chart>
      </section>
      <section class="card">
        <h3>Values</h3>
        <div class="table-scroll"><table>
          <thead><tr><th>Day</th><th>Lowest</th><th>Highest</th><th>Listed</th></tr></thead>
          <tbody>
            ${item.h.map(([day, low, high, listed]) => html`<tr><td>${day}</td><td>${coins(low)}</td><td>${coins(high)}</td><td>${count(listed)}</td></tr>`)}
          </tbody>
        </table></div>
      </section>`;
  }

  rendered({ items, prefs, opps }) {
    const item = items.find((it) => it.id === prefs.selectedId);
    if (!item) return;
    const verdict = opps?.check?.find((row) => row.id === item.id);
    this.querySelector("ladder-chart").show(item, opps?.ladders?.[item.id], verdict);
    for (const chart of this.querySelectorAll("day-chart")) chart.show(item);
  }
}

function tiles(item) {
  const direction = item.change > 0.1 ? "up" : item.change < -0.1 ? "down" : "muted";
  const change = item.h.length > 1 ? html`<span class="${direction}">${percent(item.change)}</span>` : "–";
  const snipe = item.snipe > 0;
  return [
    tile("Cheapest now", coins(item.now), "latest scan"),
    tile("Usual", coins(item.usual), "median daily price"),
    tile("Change", change, "first to last day"),
    tile("Listed", count(item.listed), "quantity, latest day"),
    tile("Snipe at or under", snipe ? coins(item.snipe) : "–", snipe ? "80% of floor, clears 1s deposit + 2s" : "too cheap to flip singly"),
  ];
}

function tile(label, value, note) {
  return html`<div class="tile"><div class="tile-label">${label}</div><div class="tile-value">${value}</div><div class="tile-note">${note}</div></div>`;
}

define("item-view", ItemView);
