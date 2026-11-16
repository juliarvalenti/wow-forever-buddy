// Do the market rules make money? Replays them on Auctionator's day-by-day history: a rule fires on day d,
// you buy one at that day's low, you sell it on day d+1. Compared against a control (buy every busy item at
// its low), because "buy the low, sell the next day" flatters any rule.
//   node backtest.mjs [--min-listed 100] [--min 500] [--max 50000]   (prices in copper)
// Sale price two ways: "undercut" = 1c under the next day's cheapest (you sell fast; conservative),
// "patient" = the next day's high cheapest (you wait for the floor to clear; optimistic).
// Costs: 5% cut, 1s deposit. Toy-sized until there are a couple of weeks of days.
import { loadPrices } from "./fbl.mjs";

const argv = process.argv.slice(2);
const flag = (k, d) => { const i = argv.indexOf(`--${k}`); return i >= 0 ? Number(argv[i + 1]) : d; };
const MIN_LISTED = flag("min-listed", 100), MIN = flag("min", 500), MAX = flag("max", 50000);
const CUT = 0.95, DEPOSIT = 100;

const RULES = {
  control: () => true,
  stray: (t) => t.low <= t.high * 0.8,                         // a cheap listing under the day's own floor
  dip: (t, y) => y && t.high <= y.high * 0.75,                 // the whole floor fell vs the day before
  "stray+flat": (t, y) => y && t.low <= t.high * 0.8 && t.high >= y.high * 0.9, // cheap listing, market not falling
};

const trades = Object.fromEntries(Object.keys(RULES).map((k) => [k, []]));
for (const data of Object.values(loadPrices().realms)) for (const [id, e] of Object.entries(data)) {
  if (id === "version" || !e?.h) continue;
  const days = Object.keys(e.h).map(Number).sort((a, b) => a - b);
  const at = (d) => (e.h[d] == null ? null : { low: e.l?.[d] ?? e.h[d], high: e.h[d], qty: e.a?.[d] ?? 0 });
  for (let i = 0; i < days.length - 1; i++) {
    const t = at(days[i]), y = i > 0 ? at(days[i - 1]) : null, n = at(days[i + 1]);
    if (!n || days[i + 1] !== days[i] + 1 || t.qty < MIN_LISTED || t.low < MIN || t.low > MAX) continue;
    for (const [k, rule] of Object.entries(RULES)) if (rule(t, y)) trades[k].push({
      id, buy: t.low,
      undercut: Math.floor((n.low - 1) * CUT) - DEPOSIT - t.low,
      patient: Math.floor(n.high * CUT) - DEPOSIT - t.low,
    });
  }
}

const median = (a) => { const s = [...a].sort((x, y) => x - y); return s[Math.floor((s.length - 1) / 2)] ?? 0; };
const pct = (x) => `${x > 0 ? "+" : ""}${(x * 100).toFixed(1)}%`;
console.log(`Busy (${MIN_LISTED}+ listed), buy price ${MIN / 100}s to ${MAX / 1e4}g; one unit per signal.\n`);
console.log("rule          trades | undercut: win%  median  return on gold | patient: win%  median  return on gold");
for (const [k, list] of Object.entries(trades)) {
  if (!list.length) { console.log(k.padEnd(13), "     0"); continue; }
  const spent = list.reduce((s, t) => s + t.buy, 0);
  const col = (f) => {
    const p = list.map((t) => t[f]);
    return `${(100 * p.filter((x) => x > 0).length / p.length).toFixed(0).padStart(4)}%  ${pct(median(list.map((t) => t[f] / t.buy))).padStart(6)}  ${pct(p.reduce((a, b) => a + b, 0) / spent).padStart(8)}`;
  };
  console.log(`${k.padEnd(13)} ${String(list.length).padStart(6)} |     ${col("undercut")}      |    ${col("patient")}`);
}
