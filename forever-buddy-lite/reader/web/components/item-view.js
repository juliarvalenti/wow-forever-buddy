// One item: headline numbers, its price ladder, price and quantity over time (scan by scan, from
// the live server's archive, over Auctionator's daily ranges), and the daily values.

import { Component, define } from "../lib/component.js";
import { store } from "../lib/store.js";
import { fetchHistory } from "../lib/api.js";
import { html } from "../lib/html.js";
import { percent, count } from "../lib/format.js";
import { coins } from "../lib/coins.js";
import { pageHead, panel, tiles, tile, table, seg, legend, itemIcon, loadingState, emptyState } from "../lib/ui.js";
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
    if (!items.length) return loadingState("Loading prices…");
    if (!item) return emptyState({ title: "Pick an item", text: "Choose one from the list to see its ladder and prices.", icon: "scale" });

    return html`
      ${pageHead({
        title: html`<span class="item-title">${itemIcon(item.n, "lg")}<span>${item.n}</span></span>`,
        lede: `${item.k} · item ${item.id}`,
      })}
      ${tiles(itemTiles(item))}
      <ladder-chart></ladder-chart>
      ${panel({
        title: "Over time",
        actions: seg("range", RANGES.map(([name]) => [name]), prefs.chartRange, "Time range"),
        body: html`
          <p class="note" data-ref="scan-note">Loading scans…</p>
          <div class="chart-block">
            <h3 class="chart-title">Cheapest listing</h3>
            ${legend([
              { label: "Cheapest at each scan", color: "var(--chart-1)" },
              { label: "Each day’s low to high, from Auctionator", kind: "band" },
            ])}
            <time-chart kind="price"></time-chart>
          </div>
          <div class="chart-block">
            <h3 class="chart-title">Quantity listed</h3>
            ${legend([{ label: "Listed at each scan", color: "var(--chart-2)" }])}
            <time-chart kind="listed"></time-chart>
          </div>`,
      })}
      ${panel({
        title: "Values",
        meta: "each day’s cheapest listing, from Auctionator",
        flush: table({
          columns: [{ label: "Day" }, { label: "Lowest", cls: "num" }, { label: "Highest", cls: "num" }, { label: "Listed", cls: "num" }],
          rows: [...item.h].reverse(),
          sticky: true,
          compact: true,
          row: ([day, low, high, listed]) => html`<tr><td>${day}</td><td class="num">${coins(low)}</td><td class="num">${coins(high)}</td><td class="num">${count(listed)}</td></tr>`,
        }),
      })}`;
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

function itemTiles(item) {
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

define("item-view", ItemView);
