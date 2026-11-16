// The left column: search, category and trend chips, price and quantity filters, and the matching
// items. The inputs are built once in setup(), so typing never loses focus; the summary, chips and
// list are redrawn when the data or the filters change.

import { Component, define } from "../lib/component.js";
import { store } from "../lib/store.js";
import { html } from "../lib/html.js";
import { percent, count } from "../lib/format.js";
import { coins } from "../lib/coins.js";
import { filterItems, countBy, kindsByCount, TRENDS, SORTS } from "../lib/model.js";

const MAX_ROWS = 400; // thousands of rows make typing sluggish

class ItemFinder extends Component {
  watch(state) {
    return [state.items, state.prefs, state.stamp];
  }

  setup() {
    this.innerHTML = String(html`
      <header class="finder-head">
        <h1>Auction house</h1>
        <p class="sub" data-ref="summary"></p>
      </header>
      <div class="filters">
        <input type="search" data-pref="query" placeholder="Find an item…" autocomplete="off" aria-label="Find an item" />
        <details open>
          <summary>Category</summary>
          <div class="facet" data-ref="kinds"></div>
        </details>
        <details open>
          <summary>Trend over the scans</summary>
          <div class="facet" data-ref="trends"></div>
        </details>
        <div class="two-up">
          <label class="field">Price from (silver)<input type="number" min="0" data-pref="priceMin" placeholder="any" /></label>
          <label class="field">to (silver)<input type="number" min="0" data-pref="priceMax" placeholder="any" /></label>
        </div>
        <div class="two-up">
          <label class="field">Listed at least<input type="number" min="0" data-pref="listedMin" /></label>
          <label class="field">
            Sort
            <select data-pref="sort">
              ${Object.entries(SORTS).map(([key, sort]) => html`<option value="${key}">${sort.label}</option>`)}
            </select>
          </label>
        </div>
        <p class="sub" data-ref="count"></p>
      </div>
      <div class="item-list" role="listbox" aria-label="Items" data-ref="list"></div>
    `);

    // Fill the inputs from the remembered filters.
    for (const input of this.querySelectorAll("[data-pref]")) {
      input.value = store.state.prefs[input.dataset.pref] ?? "";
    }

    this.addEventListener("input", (event) => {
      const key = event.target.dataset.pref;
      if (key) store.setPrefs({ [key]: event.target.value });
    });

    this.addEventListener("click", (event) => {
      const chip = event.target.closest("[data-facet]");
      if (chip) {
        const { facet, value } = chip.dataset;
        const chosen = store.state.prefs[facet];
        store.setPrefs({ [facet]: chosen.includes(value) ? chosen.filter((v) => v !== value) : [...chosen, value] });
        return;
      }
      const row = event.target.closest("[data-id]");
      if (row) store.openItem(Number(row.dataset.id));
    });
  }

  rendered({ items, prefs, stamp }) {
    this.ref("summary").textContent = `${count(items.length)} items, prices through ${stamp || "…"}`;

    this.ref("kinds").innerHTML = String(html`${chips("kinds", kindsByCount(items), countBy(items, "k"), prefs.kinds)}`);
    this.ref("trends").innerHTML = String(html`${chips("trends", TRENDS, countBy(items, "trend"), prefs.trends)}`);

    const matches = filterItems(items, prefs);
    this.ref("count").textContent =
      `${count(matches.length)} match` + (matches.length > MAX_ROWS ? `, first ${MAX_ROWS} shown` : "");
    this.ref("list").innerHTML = String(html`${matches.slice(0, MAX_ROWS).map((it) => itemRow(it, it.id === prefs.selectedId))}`);
  }
}

function chips(facet, values, counts, chosen) {
  return values
    .filter((value) => counts.get(value))
    .map(
      (value) => html`
        <button class="chip" type="button" data-facet="${facet}" data-value="${value}" aria-pressed="${chosen.includes(value)}">
          ${value}<span class="chip-count">${counts.get(value)}</span>
        </button>`,
    );
}

function itemRow(item, selected) {
  const direction = item.change > 0.1 ? "up" : item.change < -0.1 ? "down" : "muted";
  return html`
    <div class="item-row" role="option" data-id="${item.id}" aria-selected="${selected}">
      <div class="name">${item.n}</div>
      <div class="price">${coins(item.now)}</div>
      <div class="meta">${item.k}, ${count(item.listed)} listed</div>
      <div class="change ${direction}">${item.h.length > 1 ? percent(item.change) : "new"}</div>
    </div>`;
}

define("item-finder", ItemFinder);
