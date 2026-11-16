// Opportunities in the latest Auctionator scan, for buying cheap, selling what you hold, and reading the market.
//   node market.mjs [--budget 5] [--min-listed 100] [--top 12]
// Auctionator keeps, per item and day, the cheapest listing (low, high) and the quantity up; "now" is the
// latest scan's cheapest. Usual = median of each day's HIGHER cheapest price (lower middle), today left out:
// a day's low catches one stray cheap listing (Silk at 29c), its high is what the floor mostly was.
// Costs: 5% cut and a 1s deposit.
// Sections:
//   BUY       now well under usual on a busy market, enough profit on one item after costs
//   SELL      things your characters hold that sell above their usual today
//   SQUEEZE   supply falling while the price rises (someone's buying up; sellers can ask more)
//   FLOOD     supply rising while the price falls (don't hold these; buy later)
//   SWINGY    the widest daily ranges on busy markets: room to buy the low and list near the high
import fs from "node:fs";
import path from "node:path";
import { decodeCbor, loadPrices, load, call } from "./fbl.mjs";

const argv = process.argv.slice(2);
const flag = (k, d) => { const i = argv.indexOf(`--${k}`); return i >= 0 ? argv[i + 1] : d; };
const budget = Number(flag("budget", "5")) * 10000, minListed = Number(flag("min-listed", "100")), top = Number(flag("top", "12"));
const DEPOSIT = 100, CUT = 0.95, MIN_PROFIT = 200;

// Names: QuestieDB, then the gear scan and your characters' items.
const names = new Map();
const toc = path.join(process.env.QUESTIEDB_DIR || "C:/Program Files (x86)/World of Warcraft/_forever_/Interface/AddOns/QuestieDB", "QuestieDB_Forever.toc");
if (fs.existsSync(toc)) for (const l of fs.readFileSync(toc, "utf8").split(/\r?\n/)) {
  const m = /^## X-Item-(\d+)-S: (.*)$/.exec(l); if (!m) continue;
  const v = decodeCbor(Buffer.from(m[2], "base64"))[1];
  names.set(+m[1], Buffer.isBuffer(v) ? v.toString("utf8") : v);
}
const { chars, ah } = load();
for (const it of Object.values(ah?.items || {})) if (it.id && !names.has(it.id)) names.set(it.id, it.name);
const held = new Map(); // id -> count across characters (bags, bank, mail)
for (const c of Object.keys(chars)) for (const r of call("items", { character: c }).items) {
  if (!r.id || r.where === "worn" || r.bound) continue;
  if (r.name && !names.has(r.id)) names.set(r.id, r.name);
  held.set(r.id, (held.get(r.id) || 0) + (r.count || 1));
}

const g = (c) => { c = Math.round(c); const G = Math.floor(c / 1e4), s = Math.floor((c % 1e4) / 100), k = c % 100; return G ? `${G}g ${s}s` : s ? `${s}s ${String(k).padStart(2, "0")}c` : `${k}c`; };
const pct = (x) => `${x > 0 ? "+" : ""}${Math.round(x * 100)}%`;
const median = (a) => { const s = [...a].sort((x, y) => x - y); return s[Math.floor((s.length - 1) / 2)]; };

const rows = [];
let lastDay = 0;
for (const data of Object.values(loadPrices().realms)) for (const [id, e] of Object.entries(data)) {
  if (id === "version" || !e?.h || !e.m) continue;
  const days = Object.keys(e.h).map(Number).sort((a, b) => a - b);
  lastDay = Math.max(lastDay, days.at(-1));
  const lows = days.map((d) => e.l?.[d] ?? e.h[d]), highs = days.map((d) => e.h[d]), qty = days.map((d) => e.a?.[d] ?? 0);
  const usual = median(highs.length > 1 ? highs.slice(0, -1) : highs); // today left out of "usual"
  const mean = lows.reduce((a, b) => a + b, 0) / lows.length;
  const sd = Math.sqrt(lows.reduce((a, b) => a + (b - mean) ** 2, 0) / lows.length) || 1;
  rows.push({ id: +id, name: names.get(+id) || `item ${id}`, now: e.m, usual, sd, days: days.length,
    low: lows.at(-1), high: highs.at(-1), qty: qty.at(-1), qty0: qty[0], low0: lows[0] });
}
const busy = rows.filter((r) => r.qty >= minListed && r.days >= 2);
const date = new Date(Date.UTC(2020, 0, 1) + lastDay * 864e5).toISOString().slice(0, 10);
console.log(`Auctionator through ${date}: ${rows.length} items, ${busy.length} busy (${minListed}+ listed, 2+ days). Budget ${g(budget)} a buy.\n`);

const show = (title, list, fmt) => { console.log(`${title}`); if (!list.length) console.log("  (nothing)"); for (const r of list.slice(0, top)) console.log("  " + fmt(r)); console.log(""); };

// BUY: cheapest now well under usual, a busy market, and a single item clears costs and the minimum profit.
const buys = busy.map((r) => ({ ...r, profit: Math.floor(Math.min(r.usual, r.high) * CUT) - DEPOSIT - r.now, z: (r.usual - r.now) / r.sd }))
  .filter((r) => r.now <= r.usual * 0.8 && r.profit >= MIN_PROFIT && r.now <= budget)
  .sort((a, b) => b.profit * Math.log10(b.qty) - a.profit * Math.log10(a.qty));
show("BUY (cheap now vs usual; resell near usual)", buys, (r) =>
  `${r.name.padEnd(32)} now ${g(r.now).padStart(8)}  usual ${g(r.usual).padStart(8)}  ${pct(r.now / r.usual - 1).padStart(5)}  profit/each ${g(r.profit).padStart(7)}  listed ${r.qty}`);

// SELL: what you hold, priced above usual today.
const sells = rows.filter((r) => held.has(r.id) && r.days >= 2 && r.now >= r.usual * 1.15 && r.qty >= 20)
  .map((r) => ({ ...r, n: held.get(r.id), worth: Math.floor(r.now * CUT) * held.get(r.id) }))
  .sort((a, b) => b.worth - a.worth);
show("SELL what you hold (above usual today)", sells, (r) =>
  `${r.name.padEnd(32)} you have ${String(r.n).padStart(4)}  now ${g(r.now).padStart(8)}  usual ${g(r.usual).padStart(8)}  ${pct(r.now / r.usual - 1).padStart(5)}  ~${g(r.worth)} after cut`);

// SQUEEZE and FLOOD: supply and price moving opposite ways since the first scan.
const moves = busy.filter((r) => r.qty0 > 0 && r.low0 > 0).map((r) => ({ ...r, dq: r.qty / r.qty0 - 1, dp: r.low / r.low0 - 1 }));
show("SQUEEZE (supply down, price up: sellers can ask more)", moves.filter((r) => r.dq <= -0.2 && r.dp >= 0.15).sort((a, b) => (b.dp - b.dq) - (a.dp - a.dq)), (r) =>
  `${r.name.padEnd(32)} price ${pct(r.dp).padStart(5)}  supply ${pct(r.dq).padStart(5)}  now ${g(r.now)}${held.has(r.id) ? `  (you have ${held.get(r.id)})` : ""}`);
show("FLOOD (supply up, price down: don't hold, buy later)", moves.filter((r) => r.dq >= 0.3 && r.dp <= -0.15).sort((a, b) => (a.dp - a.dq) - (b.dp - b.dq)), (r) =>
  `${r.name.padEnd(32)} price ${pct(r.dp).padStart(5)}  supply ${pct(r.dq).padStart(5)}  now ${g(r.now)}${held.has(r.id) ? `  (you have ${held.get(r.id)})` : ""}`);

// SWINGY: the day's cheapest moved a lot, on markets busy enough to trade.
show("SWINGY (widest range today: buy near the low, list near the high)", busy.filter((r) => r.high >= r.low * 1.5 && r.high - r.low >= 300 && r.low <= budget && r.high <= r.usual * 2)
  .sort((a, b) => (b.high - b.low) * Math.log10(b.qty) - (a.high - a.low) * Math.log10(a.qty)), (r) =>
  `${r.name.padEnd(32)} today ${g(r.low).padStart(8)} to ${g(r.high).padStart(8)}  now ${g(r.now).padStart(8)}  listed ${r.qty}`);
