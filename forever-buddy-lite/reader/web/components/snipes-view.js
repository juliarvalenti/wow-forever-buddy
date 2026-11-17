// The Snipes page: build a Point Blank Sniper list. Point Blank Sniper watches an Auctionator
// shopping list and buys any listing at or under each item's price; this picks busy items and caps
// each under its floor so even a single item resold covers the cut, the deposit and some profit.
//
// The form is built once in setup() (typing keeps focus); each change rebuilds the list on the
// server (/snipes.json) after a short pause. Settings are remembered in this browser.

import { Component, define } from "../lib/component.js";
import { store } from "../lib/store.js";
import { html } from "../lib/html.js";
import { count } from "../lib/format.js";
import { coins } from "../lib/coins.js";
import { copyText, fetchSnipes } from "../lib/api.js";

// Form fields: settings key, label, help, input attributes. Prices in silver here, copper on the wire.
const FIELDS = [
  ["cap", "Buy at most", "silver per item: nothing pricier goes on the list", { min: 1, step: 1 }],
  ["ratio", "Cap at", "% of the item's floor (the lower of usual and the cheapest now)", { min: 10, max: 100, step: 5 }],
  ["minProfit", "Profit per item", "silver, at least, on one item resold at the floor", { min: 0, step: 0.5 }],
  ["deposit", "Repost deposit", "silver, counted as a cost on each item", { min: 0, step: 0.5 }],
  ["count", "Items on the list", "the best ones, by price and how busy the market is", { min: 5, max: 200, step: 5 }],
  ["minListed", "Busy markets only", "listed per day on average, at least", { min: 0, step: 50 }],
];

export const DEFAULT_SNIPES = { cap: 25, ratio: 80, minProfit: 2, deposit: 1, count: 50, minListed: 300, name: "FBL Snipes" };

function settings() {
  return { ...DEFAULT_SNIPES, ...(store.state.prefs.snipes || {}) };
}

class SnipesView extends Component {
  #timer = null;
  #data = null;
  #error = null;

  setup() {
    const s = settings();
    this.innerHTML = String(html`
      <header class="page-head">
        <h1>Snipes</h1>
        <p class="sub">A shopping list for Point Blank Sniper: busy items, each capped under its floor.</p>
      </header>
      <section class="card">
        <h3>Settings</h3>
        <div class="snipe-form">
          ${FIELDS.map(
            ([key, label, help, attrs]) => html`
              <label class="field">
                <span class="field-label">${label}</span>
                <input type="number" data-setting="${key}" value="${s[key]}" min="${attrs.min}" ${attrs.max ? html`max="${attrs.max}"` : ""} step="${attrs.step}" />
                <span class="field-help">${help}</span>
              </label>`,
          )}
          <label class="field">
            <span class="field-label">List name</span>
            <input type="text" data-setting="name" value="${s.name}" maxlength="40" />
            <span class="field-help">a short tag is added so each copy is a new list</span>
          </label>
        </div>
        <div class="tools">
          <button class="btn" type="button" data-reset>Reset to defaults</button>
        </div>
      </section>
      <section class="card">
        <div class="chart-head">
          <h3>The list <span class="count" data-ref="count"></span></h3>
          <div class="tools">
            <button class="btn primary" type="button" data-copy disabled>Copy for Auctionator</button>
          </div>
        </div>
        <p class="sub" data-ref="note">In game: Auctionator, Shopping, Import, paste. Then pick the list in Point Blank Sniper.</p>
        <div data-ref="results"><p class="sub">Building…</p></div>
      </section>`);

    this.addEventListener("input", (event) => {
      const key = event.target.dataset.setting;
      if (!key) return;
      const value = key === "name" ? event.target.value : Number(event.target.value);
      store.setPrefs({ snipes: { ...settings(), [key]: value } });
      this.#schedule();
    });
    this.addEventListener("click", async (event) => {
      if (event.target.closest("[data-reset]")) {
        store.setPrefs({ snipes: { ...DEFAULT_SNIPES } });
        for (const input of this.querySelectorAll("[data-setting]")) input.value = DEFAULT_SNIPES[input.dataset.setting];
        this.#schedule(0);
        return;
      }
      if (event.target.closest("[data-copy]") && this.#data) {
        const copied = await copyText(this.#data.list);
        if (copied) this.ref("note").textContent = `Copied "${this.#data.name}". In game: Auctionator, Shopping, Import, paste; then pick it in Point Blank Sniper.`;
        return;
      }
      const open = event.target.closest("[data-open-item]");
      if (open) store.openItem(Number(open.dataset.openItem));
    });
    this.#schedule(0);
  }

  // A new list after the server's next update (new prices), too.
  watch(s) {
    return [s.lastUpdate];
  }

  rendered() {
    if (this.#data) this.#schedule(0);
  }

  #schedule(delay = 350) {
    clearTimeout(this.#timer);
    this.#timer = setTimeout(() => this.#load(), delay);
  }

  async #load() {
    const s = settings();
    try {
      this.#data = await fetchSnipes({
        cap: Math.round(s.cap * 100),
        ratio: s.ratio / 100,
        min_profit: Math.round(s.minProfit * 100),
        deposit: Math.round(s.deposit * 100),
        count: s.count,
        min_listed: s.minListed,
        name: s.name,
      });
      this.#error = null;
    } catch (error) {
      this.#data = null;
      this.#error = error.message;
    }
    this.#draw();
  }

  #draw() {
    const results = this.ref("results");
    if (!results) return;
    const copy = this.querySelector("[data-copy]");
    if (this.#error) {
      results.innerHTML = String(html`<p class="sub">Couldn’t build the list: ${this.#error}</p>`);
      copy.disabled = true;
      return;
    }
    const items = this.#data.items;
    this.ref("count").textContent = `${items.length} items`;
    copy.disabled = items.length === 0;
    results.innerHTML = String(
      items.length === 0
        ? html`<p class="sub">Nothing fits these settings. Raise “Buy at most”, or lower “Profit per item” or “Busy markets only”.</p>`
        : html`
            <div class="table-scroll">
              <table class="worklist">
                <thead>
                  <tr><th>Item</th><th class="num">Buy at or under</th><th class="num">Floor</th><th class="num">Usual</th><th class="num">Profit on one</th><th class="num">Listed a day</th></tr>
                </thead>
                <tbody>
                  ${items.map(
                    (it) => html`
                      <tr>
                        <td class="item-cell"><button class="row-link" type="button" data-open-item="${it.id}">${it.name}</button></td>
                        <td class="num">${coins(it.max)}</td>
                        <td class="num">${coins(it.floor)}</td>
                        <td class="num">${coins(it.usual)}</td>
                        <td class="num profit">${coins(it.single, { signed: true })}</td>
                        <td class="num">${count(it.listed)}</td>
                      </tr>`,
                  )}
                </tbody>
              </table>
            </div>`,
    );
  }
}

define("snipes-view", SnipesView);
