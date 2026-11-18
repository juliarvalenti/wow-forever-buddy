// The Snipes page: build a Point Blank Sniper list, picked one of two ways:
//   usual    by the daily usual price (Auctionator's days)
//   dips     items that keep dipping in your own scans and getting bought back (dips.rs)
// The rest of the header: Point Blank Sniper watches an Auctionator
// shopping list and buys any listing at or under each item's price; this picks busy items and caps
// each under its floor so even a single item resold covers the cut, the deposit and some profit.
//
// The form is built once in setup() (typing keeps focus); each change rebuilds the list on the
// server (/snipes.json) after a short pause. Settings are remembered in this browser.

import { Component, define } from "../lib/component.js";
import { store } from "../lib/store.js";
import { html } from "../lib/html.js";
import { count, ago } from "../lib/format.js";
import { coins } from "../lib/coins.js";
import { copyText, fetchSnipes } from "../lib/api.js";
import { toast } from "../lib/toast.js";
import { icon, pageHead, panel, table, loadingState, errorState } from "../lib/ui.js";

// Form fields: settings key, label, help, input attributes. Prices in silver here, copper on the wire.
const FIELDS = [
  ["cap", "Buy at most", "silver per item: nothing pricier goes on the list", { min: 1, step: 1 }],
  ["ratio", "Cap at", "% of the item's floor (the lower of usual and the cheapest now)", { min: 10, max: 100, step: 5 }],
  ["minProfit", "Profit per item", "silver, at least, on one item resold at the floor", { min: 0, step: 0.5 }],
  ["deposit", "Repost deposit", "silver, counted as a cost on each item", { min: 0, step: 0.5 }],
  ["count", "Items on the list", "the best ones, by price and how busy the market is", { min: 5, max: 200, step: 5 }],
  ["minListed", "Busy markets only", "listed per day on average, at least", { min: 0, step: 50 }],
];

// Only for the "seen dipping" pick.
const DIP_FIELDS = [
  ["days", "Look back", "days of your scans", { min: 0.5, max: 30, step: 0.5 }],
  ["depth", "A dip is at least", "% under the recent median price", { min: 5, max: 90, step: 5 }],
  ["minBought", "Bought back at least", "times: dips that were gone by the next change", { min: 1, max: 50, step: 1 }],
];

export const DEFAULT_SNIPES = {
  mode: "usual", cap: 25, ratio: 80, minProfit: 2, deposit: 1, count: 50, minListed: 300, name: "FBL Snipes",
  days: 3, depth: 25, minBought: 2,
};

const field = (s) => ([key, label, help, attrs]) => html`
  <label class="field">
    <span class="field-label">${label}</span>
    <input type="number" data-setting="${key}" value="${s[key]}" min="${attrs.min}" ${attrs.max ? html`max="${attrs.max}"` : ""} step="${attrs.step}" />
    <span class="field-help">${help}</span>
  </label>`;

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
      ${pageHead({
        title: "Snipes",
        lede: "A shopping list for Point Blank Sniper: busy items, each capped under its floor.",
        actions: html`<button class="btn primary" type="button" data-copy disabled>${icon("copy")}Copy for Auctionator</button>`,
      })}
      ${panel({
        title: "Settings",
        actions: html`<button class="btn quiet small" type="button" data-reset>${icon("refresh", "sm")}Reset to defaults</button>`,
        body: html`
          <fieldset class="fieldset">
            <legend>Pick items by</legend>
            <label class="radio"><input type="radio" name="snipe-mode" data-setting="mode" value="usual" ${s.mode === "usual" ? "checked" : ""} />
              Usual price <span class="hint">Auctionator’s daily prices</span></label>
            <label class="radio"><input type="radio" name="snipe-mode" data-setting="mode" value="dips" ${s.mode === "dips" ? "checked" : ""} />
              Seen dipping <span class="hint">your scans: cheap listings that keep turning up and getting bought</span></label>
          </fieldset>
          <div class="form-grid" data-ref="dip-form" ${s.mode === "dips" ? "" : "hidden"}>${DIP_FIELDS.map(field(s))}</div>
          <div class="form-grid">
            ${FIELDS.map(field(s))}
            <label class="field">
              <span class="field-label">List name</span>
              <input type="text" data-setting="name" value="${s.name}" maxlength="40" />
              <span class="field-help">a short tag is added so each copy is a new list</span>
            </label>
          </div>`,
      })}
      ${panel({
        title: "The list",
        count: html`<span data-ref="count"></span>`,
        body: html`<p class="note" data-ref="note">In game: Auctionator, Shopping, Import, paste. Then pick the list in Point Blank Sniper.</p>`,
        flush: html`<div data-ref="results">${loadingState("Building…", { compact: true })}</div>`,
      })}`);

    this.addEventListener("input", (event) => {
      const key = event.target.dataset.setting;
      if (!key) return;
      const value = key === "name" || key === "mode" ? event.target.value : Number(event.target.value);
      store.setPrefs({ snipes: { ...settings(), [key]: value } });
      if (key === "mode") this.ref("dip-form").hidden = value !== "dips";
      this.#schedule();
    });
    this.addEventListener("click", async (event) => {
      if (event.target.closest("[data-reset]")) {
        store.setPrefs({ snipes: { ...DEFAULT_SNIPES } });
        for (const input of this.querySelectorAll("[data-setting]")) {
          if (input.type === "radio") input.checked = input.value === DEFAULT_SNIPES.mode;
          else input.value = DEFAULT_SNIPES[input.dataset.setting];
        }
        this.ref("dip-form").hidden = true;
        this.#schedule(0);
        return;
      }
      if (event.target.closest("[data-copy]") && this.#data) {
        const copied = await copyText(this.#data.list);
        if (copied) {
          const note = `Copied "${this.#data.name}". In game: Auctionator, Shopping, Import, paste; then pick it in Point Blank Sniper.`;
          this.ref("note").textContent = note;
          toast(note);
        }
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
        ...(s.mode === "dips" ? { mode: "dips", days: s.days, depth: s.depth, min_bought: s.minBought } : {}),
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
      results.innerHTML = String(errorState({ title: "Couldn’t build the list", text: this.#error, compact: true }));
      copy.disabled = true;
      return;
    }
    const items = this.#data.items;
    const dips = settings().mode === "dips";
    this.ref("count").textContent = `${items.length} items`;
    copy.disabled = items.length === 0;
    const num = (label) => ({ label, cls: "num" });
    results.innerHTML = String(
      table({
        columns: [
          { label: "Item" },
          num("Buy at or under"),
          ...(dips ? [num("Recent median"), num("Dips"), num("Bought back"), num("Deepest"), num("Last dip")] : [num("Floor"), num("Usual")]),
          num("Profit on one"),
          num(dips ? "Listed now" : "Listed a day"),
        ],
        rows: items,
        sticky: true,
        empty: dips
          ? "Nothing has dipped and been bought back that often yet. Look back further, lower “Bought back at least”, or scan more often: the archive only sees what your scans catch."
          : "Nothing fits these settings. Raise “Buy at most”, or lower “Profit per item” or “Busy markets only”.",
        row: (it) => html`
          <tr>
            <td class="item"><button class="row-link" type="button" data-open-item="${it.id}">${it.name}</button></td>
            <td class="num strong">${coins(it.max)}</td>
            ${dips
              ? html`<td class="num">${coins(it.floor)}</td><td class="num">${it.dips}</td><td class="num">${it.bought}</td>
                  <td class="num down">-${Math.round(it.deepest * 100)}%</td><td class="num muted">${ago(it.lastDip)}</td>`
              : html`<td class="num">${coins(it.floor)}</td><td class="num">${coins(it.usual)}</td>`}
            <td class="num up profit">${coins(it.single, { signed: true })}</td>
            <td class="num">${count(it.listed)}</td>
          </tr>`,
      }),
    );
  }
}

define("snipes-view", SnipesView);
