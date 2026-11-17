// One item: headline numbers, its price ladder, price and quantity over time (scan by scan, from
// the live server's archive, over Auctionator's daily ranges), and the daily values.

import { Component, define } from "../lib/component.js";
import { store } from "../lib/store.js";
import { fetchHistory } from "../lib/api.js";
import { html } from "../lib/html.js";
import { percent, count } from "../lib/format.js";
import { coins } from "../lib/coins.js";
import "./ladder-chart.js";
import "./time-chart.js";

// The time ranges the charts offer, in seconds (null: everything there is).
const RANGES = [
  ["6h", 6 * 3600],
  ["24h", 24 * 3600],
  ["3d", 3 * 86400],
  ["7d", 7 * 86400],
  ["All", null],
];

// An item's daily rows as time spans. Auctionator's days are UTC; the row only has "MM-DD", so the
// year comes from the latest day (a December row seen in January is last year's).
function daySpans(item, stamp) {
  const year = Number((stamp || "").slice(0, 4)) || new Date().getUTCFullYear();
  const lastMonth = Number((stamp || "").slice(5, 7)) || 12;
  return item.h.map(([day, low, high, listed]) => {
    const [month, date] = day.split("-").map(Number);
    const start = Date.UTC(month > lastMonth ? year - 1 : year, month - 1, date) / 1000;
    return { start, end: start + 86400, low, high, listed };
  });
}

class ItemView extends Component {
  // Scans per item, fetched once per server update.
  #history = new Map(); // id -> { stamp, points }

  watch(s) {
    return [s.items, s.prefs.selectedId, s.prefs.chartRange, s.opps, s.lastUpdate];
  }

  setup() {
    this.addEventListener("click", (event) => {
      const button = event.target.closest("[data-range]");
      if (button) store.setPrefs({ chartRange: button.dataset.range });
    });
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
        <div class="chart-head">
          <h3>Over time</h3>
          <div class="range" role="group" aria-label="Time range">
            ${RANGES.map(([name]) => html`<button class="chip" type="button" data-range="${name}" aria-pressed="${prefs.chartRange === name}">${name}</button>`)}
          </div>
        </div>
        <p class="sub" data-ref="scan-note">Loading scans…</p>
        <h4 class="chart-title">Cheapest listing</h4>
        <p class="sub">Steps: the cheapest at each scan. Band: each day’s lowest to highest cheapest, from Auctionator.</p>
        <time-chart kind="price"></time-chart>
        <h4 class="chart-title">Quantity listed</h4>
        <time-chart kind="listed"></time-chart>
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
    this.#drawCharts(item);
  }

  async #drawCharts(item) {
    const { lastUpdate, stamp, prefs } = store.state;
    let cached = this.#history.get(item.id);
    if (!cached || cached.stamp !== lastUpdate) {
      try {
        const data = await fetchHistory(item.id);
        cached = { stamp: lastUpdate, points: data.points, scans: data.scans };
      } catch {
        cached = { stamp: lastUpdate, points: [], scans: 0 };
      }
      this.#history.set(item.id, cached);
      if (store.state.prefs.selectedId !== item.id) return; // moved on meanwhile
    }
    const days = daySpans(item, stamp);
    const end = Date.now() / 1000;
    const range = (RANGES.find(([name]) => name === prefs.chartRange) || RANGES[1])[1];
    const first = Math.min(cached.points[0]?.[0] ?? Infinity, days[0]?.start ?? Infinity);
    const start = range ? end - range : Number.isFinite(first) ? first : end - 86400;
    const note = this.querySelector('[data-ref="scan-note"]');
    if (note) {
      const inRange = cached.points.filter((p) => p[0] >= start).length;
      note.textContent = cached.points.length
        ? `${inRange} of ${cached.points.length} saved scans in this range.`
        : "No saved scans for this item yet: they start once fbl-rs live is running when you scan.";
    }
    for (const chart of this.querySelectorAll("time-chart")) chart.show({ points: cached.points, days, start, end });
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
