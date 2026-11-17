// Per-item numbers and the item finder's filtering. Pure functions, no DOM.
//
// The server sends each item as
//   { id, n: name, k: kind, now: cheapest in the latest scan, h: [[day "MM-DD", low, high, listed], ...] }
// with prices in copper. Auctionator keeps one low/high per day: the cheapest listing's lowest and
// highest price seen that day.

// Costs of a resale on the AH.
export const AH_CUT = 0.95; // the AH keeps 5% of a sale
export const DEPOSIT = 100; // 1s, roughly, per listing
export const MIN_PROFIT = 200; // a flip has to clear 2s to be worth the clicks

function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.floor((sorted.length - 1) / 2)];
}

// Adds: usual, change, trend, listed, snipe.
export function enrichItem(item) {
  const days = item.h;
  const lows = days.map((d) => d[1]);
  const highs = days.map((d) => d[2]);

  // Usual price: the median of the daily highs, today left out. A day's low is often one stray
  // cheap listing; its high is where the floor sat most of the day. Same rule as the reader (reader-rs model::usual_price).
  // The server's usual (your scans' median, else Auctionator's daily midpoints); the old daily-high
  // median only for data from an older server.
  const usual = item.u ?? median(highs.length > 1 ? highs.slice(0, -1) : highs);

  const first = lows[0];
  const last = lows.at(-1);
  const change = days.length > 1 && first ? (last - first) / first : 0;
  const trend = days.length < 2 ? "New" : change > 0.1 ? "Rising" : change < -0.1 ? "Falling" : "Flat";

  // Snipe cap: 20% under the floor, and still MIN_PROFIT after the cut and deposit.
  const floor = Math.min(usual, item.now || usual);
  const snipe = Math.floor(Math.min(floor * 0.8, floor * AH_CUT - DEPOSIT - MIN_PROFIT));

  return { ...item, usual, change, trend, listed: days.at(-1)[3], snipe };
}

export const TRENDS = ["Rising", "Flat", "Falling", "New"];

export const SORTS = {
  name: { label: "Name", compare: (a, b) => a.n.localeCompare(b.n) },
  listed: { label: "Most listed", compare: (a, b) => b.listed - a.listed },
  price: { label: "Price, high first", compare: (a, b) => b.now - a.now },
  rising: { label: "Rising most", compare: (a, b) => b.change - a.change },
  falling: { label: "Falling most", compare: (a, b) => a.change - b.change },
};

// The finder's list: search text, kind and trend chips, price range (silver), minimum listed.
export function filterItems(items, prefs) {
  const query = prefs.query.trim().toLowerCase();
  const priceMin = prefs.priceMin === "" ? null : Number(prefs.priceMin) * 100;
  const priceMax = prefs.priceMax === "" ? null : Number(prefs.priceMax) * 100;
  const listedMin = Number(prefs.listedMin) || 0;

  const matches = items.filter(
    (it) =>
      (!query || it.n.toLowerCase().includes(query)) &&
      (!prefs.kinds.length || prefs.kinds.includes(it.k)) &&
      (!prefs.trends.length || prefs.trends.includes(it.trend)) &&
      (priceMin == null || it.now >= priceMin) &&
      (priceMax == null || it.now <= priceMax) &&
      it.listed >= listedMin,
  );
  const sort = SORTS[prefs.sort] || SORTS.name;
  return matches.sort(sort.compare);
}

// How many items fall in each value of a field ("k" for kind, "trend"), for the chip counts.
export function countBy(items, field) {
  const counts = new Map();
  for (const it of items) counts.set(it[field], (counts.get(it[field]) || 0) + 1);
  return counts;
}

// Kinds, the most common first.
export function kindsByCount(items) {
  return [...countBy(items, "k")].sort((a, b) => b[1] - a[1]).map(([kind]) => kind);
}
