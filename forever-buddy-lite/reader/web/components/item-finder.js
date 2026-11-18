// The Items page's list: search, sort, a Filters panel (category and trend chips, price and quantity),
// and the matching items. The inputs are built once in setup(), so typing never loses focus; the
// chips, counts and list are redrawn when the data or the filters change.

import { Component, define } from "../lib/component.js";
import { store, DEFAULT_PREFS } from "../lib/store.js";
import { html } from "../lib/html.js";
import { percent, count } from "../lib/format.js";
import { coins } from "../lib/coins.js";
import { icon, itemIcon } from "../lib/ui.js";
import { filterItems, countBy, kindsByCount, TRENDS, SORTS } from "../lib/model.js";

const MAX_ROWS = 400; // thousands of rows make typing sluggish
const FILTERS = ["kinds", "trends", "priceMin", "priceMax", "listedMin"];

class ItemFinder extends Component {
  #shownSelection = null;

  watch(state) {
    return [state.items, state.prefs, state.stamp];
  }

  setup() {
    this.innerHTML = String(html`
      <header class="panel-head"><h2>Find an item</h2></header>
      <div class="finder-controls">
        <label class="search">${icon("search")}<input type="search" data-pref="query" placeholder="Search items" autocomplete="off" aria-label="Search items" /></label>
        <div class="finder-row">
          <button class="filters-toggle" type="button" aria-expanded="false" aria-controls="finder-filters" data-ref="filters">
            ${icon("chevron", "sm")}Filters <span class="badge" data-ref="active" hidden></span>
          </button>
          <label class="inline-field">Sort
            <select class="small" data-pref="sort">
              ${Object.entries(SORTS).map(([key, sort]) => html`<option value="${key}">${sort.label}</option>`)}
            </select>
          </label>
        </div>
        <div class="filters-body" id="finder-filters" data-ref="filters-body" hidden>
          <fieldset class="fieldset small">
            <legend>Category</legend>
            <div class="chips" data-ref="kinds"></div>
          </fieldset>
          <fieldset class="fieldset small">
            <legend>Trend over the scans</legend>
            <div class="chips" data-ref="trends"></div>
          </fieldset>
          <div class="two-up">
            <label class="field">Price from (silver)<input type="number" min="0" data-pref="priceMin" placeholder="any" /></label>
            <label class="field">to (silver)<input type="number" min="0" data-pref="priceMax" placeholder="any" /></label>
          </div>
          <div class="two-up">
            <label class="field">Listed at least<input type="number" min="0" data-pref="listedMin" /></label>
            <div class="cluster end"><button class="btn small" type="button" data-clear-filters>Clear filters</button></div>
          </div>
        </div>
      </div>
      <p class="finder-count" data-ref="count" aria-live="polite"></p>
      <ul class="item-list" aria-label="Items" data-ref="list"></ul>
    `);

    this.#fillInputs();
    this.#showFilters(Boolean(store.state.prefs.filtersOpen));

    this.addEventListener("input", (event) => {
      const key = event.target.dataset.pref;
      if (key) store.setPrefs({ [key]: event.target.value });
    });

    this.addEventListener("click", (event) => {
      if (event.target.closest('[data-ref="filters"]')) {
        const open = this.ref("filters").getAttribute("aria-expanded") !== "true";
        this.#showFilters(open);
        store.setPrefs({ filtersOpen: open });
        return;
      }
      if (event.target.closest("[data-clear-filters]")) {
        store.setPrefs(Object.fromEntries(FILTERS.map((key) => [key, DEFAULT_PREFS[key]])));
        this.#fillInputs();
        return;
      }
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

  #showFilters(open) {
    this.ref("filters").setAttribute("aria-expanded", String(open));
    this.ref("filters-body").hidden = !open;
  }

  // The inputs from the remembered filters.
  #fillInputs() {
    for (const input of this.querySelectorAll("[data-pref]")) {
      input.value = store.state.prefs[input.dataset.pref] ?? "";
    }
  }

  rendered(state) {
    // Redrawing replaces the chips and rows; keep keyboard focus on the one you were on.
    const focused = document.activeElement;
    const selector = !this.contains(focused)
      ? null
      : focused.dataset.id
        ? `[data-id="${focused.dataset.id}"]`
        : focused.dataset.facet
          ? `[data-facet="${focused.dataset.facet}"][data-value="${CSS.escape(focused.dataset.value)}"]`
          : null;
    this.#draw(state);
    if (selector) this.querySelector(selector)?.focus({ preventScroll: true });
  }

  #draw({ items, prefs }) {
    this.ref("kinds").innerHTML = String(html`${chips("kinds", kindsByCount(items), countBy(items, "k"), prefs.kinds)}`);
    this.ref("trends").innerHTML = String(html`${chips("trends", TRENDS, countBy(items, "trend"), prefs.trends)}`);

    const active = activeFilters(prefs);
    this.ref("active").textContent = active ? String(active) : "";
    this.ref("active").hidden = !active;
    this.querySelector("[data-clear-filters]").disabled = !active;

    const matches = filterItems(items, prefs);
    this.ref("count").textContent = !items.length
      ? "Loading items…"
      : `${count(matches.length)} of ${count(items.length)} items` + (matches.length > MAX_ROWS ? `, first ${MAX_ROWS} shown` : "");

    const list = this.ref("list");
    list.innerHTML = String(html`${matches.slice(0, MAX_ROWS).map((it) => itemRow(it, it.id === prefs.selectedId))}`);

    // A selection made elsewhere (the worklist, the dialog): scroll the list to it, not the page.
    if (prefs.selectedId !== this.#shownSelection) {
      this.#shownSelection = prefs.selectedId;
      const row = list.querySelector("[aria-current]");
      if (row) {
        const top = row.offsetTop;
        if (top < list.scrollTop || top + row.offsetHeight > list.scrollTop + list.clientHeight) {
          list.scrollTop = top - list.clientHeight / 3;
        }
      }
    }
  }
}

function activeFilters(prefs) {
  return (
    prefs.kinds.length +
    prefs.trends.length +
    (prefs.priceMin !== "" ? 1 : 0) +
    (prefs.priceMax !== "" ? 1 : 0) +
    (Number(prefs.listedMin) > 0 ? 1 : 0)
  );
}

function chips(facet, values, counts, chosen) {
  return values
    .filter((value) => counts.get(value))
    .map(
      (value) => html`
        <button class="chip" type="button" data-facet="${facet}" data-value="${value}" aria-pressed="${chosen.includes(value)}">
          ${value}<span class="n">${counts.get(value)}</span>
        </button>`,
    );
}

function itemRow(item, selected) {
  const direction = item.change > 0.1 ? "up" : item.change < -0.1 ? "down" : "muted";
  return html`
    <li>
      <button class="item-row" type="button" data-id="${item.id}" ${selected ? html`aria-current="true"` : ""}>
        ${itemIcon(item.n)}
        <span class="name">${item.n}</span>
        <span class="price">${coins(item.now)}</span>
        <span class="meta">${item.k} · ${count(item.listed)} listed</span>
        <span class="change ${direction}">${item.h.length > 1 ? percent(item.change) : "new"}</span>
      </button>
    </li>`;
}

define("item-finder", ItemFinder);
