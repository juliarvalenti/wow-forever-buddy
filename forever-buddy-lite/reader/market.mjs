// Opportunities in the latest Auctionator scan, for buying cheap, selling what you hold, and reading the market.
//   node market.mjs [--budget 5] [--min-listed 100] [--top 12]
// live.mjs runs the same analysis after every scan and serves it next to the dashboard.
// Auctionator keeps, per item and day, the cheapest listing (low, high) and the quantity up; "now" is the
// latest scan's cheapest. Usual = median of each day's HIGHER cheapest price (lower middle), today left out:
// a day's low catches one stray cheap listing (Silk at 29c), its high is what the floor mostly was.
// Costs: 5% cut and a 1s deposit.
// Sections:
//   buy       now well under usual on a busy market, and under today's own high: a stray listing, not a
//             market that moved; enough profit on one item after costs
//   dips      the whole market is down today vs usual: a bet that it comes back, not a sure snipe
//   sell      things your characters hold that sell above their usual today
//   squeeze   supply falling while the price rises (someone's buying up; sellers can ask more)
//   flood     supply rising while the price falls (don't hold these; buy later)
//   swingy    the widest daily ranges on busy markets: room to buy the low and list near the high
//   gaps      from price ladders (items searched at the AH, see Ladders.lua): buy the cheapest few levels,
//             relist 1c under the next level up; only searches from the last few hours count
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { decodeCbor, loadPrices, load, call, parseLadder } from "./fbl.mjs";

const DEPOSIT = 100, CUT = 0.95, MIN_PROFIT = 200;
const median = (a) => { const s = [...a].sort((x, y) => x - y); return s[Math.floor((s.length - 1) / 2)]; };

// Item names (QuestieDB, then the gear scan and your characters' items), read once.
let questieNames, vendorItems;
function names() {
  if (!questieNames) {
    questieNames = new Map(); vendorItems = new Set();
    const toc = path.join(process.env.QUESTIEDB_DIR || "C:/Program Files (x86)/World of Warcraft/_forever_/Interface/AddOns/QuestieDB", "QuestieDB_Forever.toc");
    if (fs.existsSync(toc)) for (const l of fs.readFileSync(toc, "utf8").split(/\r?\n/)) {
      // Field 14 lists the NPCs that sell it. Limited-supply sellers (Darkmoon Faire leather, a herbalist's
      // few Peacebloom) are one to a handful; unlimited goods (water, thread, vials, arrows, spices) dozens.
      const vm = /^## X-Item-(\d+)-14: (.*)$/.exec(l);
      if (vm) { try { const v = decodeCbor(Buffer.from(vm[2], "base64")); if (Array.isArray(v) && v.length >= 15) vendorItems.add(+vm[1]); } catch {} continue; }
      const m = /^## X-Item-(\d+)-S: (.*)$/.exec(l); if (!m) continue;
      const v = decodeCbor(Buffer.from(m[2], "base64"))[1];
      questieNames.set(+m[1], Buffer.isBuffer(v) ? v.toString("utf8") : v);
    }
  }
  return questieNames;
}

// Items you've hidden (the dashboard's Hide button): never suggested. A JSON file next to the scan archive.
export const hiddenFile = () => process.env.FBL_HIDDEN || path.join(os.homedir(), "Documents", "FBL Market", "hidden.json");
export function loadHidden() {
  try { const j = JSON.parse(fs.readFileSync(hiddenFile(), "utf8")); return new Map(Object.entries(j.items || {}).map(([k, v]) => [+k, v])); } catch { return new Map(); }
}
export function setHidden(id, name, on) {
  const h = loadHidden();
  if (on) h.set(+id, { name: name || null, at: new Date().toISOString() }); else h.delete(+id);
  fs.mkdirSync(path.dirname(hiddenFile()), { recursive: true });
  fs.writeFileSync(hiddenFile(), JSON.stringify({ items: Object.fromEntries(h) }, null, 2));
  return h;
}

// What your characters hold to sell (bags, bank, mail; not worn, not soulbound), and names they know.
function holdings() {
  const held = new Map(), known = new Map();
  const { chars, ah } = load();
  for (const it of Object.values(ah?.items || {})) if (it.id && it.name) known.set(it.id, it.name);
  for (const c of Object.keys(chars)) for (const r of call("items", { character: c }).items) {
    if (r.id && r.name) known.set(r.id, r.name);
    if (!r.id || r.where === "worn" || r.bound) continue;
    held.set(r.id, (held.get(r.id) || 0) + (r.count || 1));
  }
  return { held, known };
}

// The best "buy the bottom k levels, relist under level k+1" on one ladder, or null. One deposit per
// relisting; the profit has to be MIN_PROFIT and 15% of what you spend.
export function bestGap(levels, budget) {
  let best = null, qty = 0, cost = 0;
  for (let k = 0; k < Math.min(levels.length - 1, 6); k++) {
    if (levels[k].mine) break; // your own listing: buying it back makes no sense
    qty += levels[k].qty; cost += levels[k].price * levels[k].qty;
    if (cost > budget) break;
    const relist = levels[k + 1].price - 1;
    const profit = qty * Math.floor(relist * CUT) - cost - DEPOSIT;
    if (profit >= MIN_PROFIT && profit >= cost * 0.15 && (!best || profit > best.profit))
      best = { levels: k + 1, qty, cost, upTo: levels[k].price, relist, profit, next: levels[k + 1].qty };
  }
  return best;
}

// What a ladder says, as one verdict: gap (do it), big (a gap, over budget), yours (you're the floor),
// tight (no gap; the wall is the real price), empty. With "sell" when you hold some: where to relist.
export function ladderVerdict(levels, { budget, usual = null, held = 0 } = {}) {
  const m = money, out = {};
  if (!levels.length) return { kind: "empty", text: held ? `nobody is selling: list at your price${usual ? ` (usual ${m(usual)})` : ""}` : "nothing listed" };
  // Where to sell: 1c under the first level near the wall (the biggest of the cheapest few), so a stray
  // cheap listing (buy that instead) doesn't set your price.
  const wallPrice = levels.slice(0, 6).reduce((a, b) => (b.qty > a.qty ? b : a)).price;
  const firstOther = levels.find((l) => !l.mine && l.price >= wallPrice * 0.9);
  if (held && firstOther) {
    const relist = firstOther.price - 1;
    out.sell = { relist, text: `relist yours at ${m(relist)}${usual && relist < usual * 0.8 ? ` (under usual ${m(usual)}: maybe wait)` : ""}` };
  }
  if (levels[0].mine) return { ...out, kind: "yours", text: `you're the cheapest at ${m(levels[0].price)}` };
  const g = bestGap(levels, budget);
  if (g) return { ...out, kind: "gap", gap: g, text: `buy ${g.qty} up to ${m(g.upTo)} (${m(g.cost)}), relist ${m(g.relist)}: +${m(g.profit)}` };
  const big = bestGap(levels, Infinity);
  if (big) return { ...out, kind: "big", gap: big, text: `gap needs ${m(big.cost)}: buy ${big.qty} up to ${m(big.upTo)}, relist ${m(big.relist)} (+${m(big.profit)})` };
  const wall = levels.slice(0, 6).reduce((a, b) => (b.qty > a.qty ? b : a));
  const cheap = usual && levels[0].price <= usual * 0.8 ? `; floor ${Math.round((1 - levels[0].price / usual) * 100)}% under usual, a dip bet at best` : "";
  return { ...out, kind: "tight", text: `no gap: floor ${m(levels[0].price)}, wall of ${wall.qty} at ${m(wall.price)}${cheap}` };
}

export function analyze({ budget = 5, minListed = 100, ladderHours = 3 } = {}) {
  budget *= 10000;
  const nm = names(), { held, known } = holdings(), hidden = loadHidden();
  let skippedVendor = 0;
  const nameOf = (id) => nm.get(id) || known.get(id) || `item ${id}`;
  const rows = [];
  let lastDay = 0;
  const prices = loadPrices();
  for (const data of Object.values(prices.realms)) for (const [id, e] of Object.entries(data)) {
    if (id === "version" || !e?.h || !e.m || hidden.has(+id)) continue;
    // Sold by a vendor: unlimited at a fixed price, so the AH can't be traded (water, thread, vials).
    if (vendorItems.has(+id)) { skippedVendor++; continue; }
    const days = Object.keys(e.h).map(Number).sort((a, b) => a - b);
    lastDay = Math.max(lastDay, days.at(-1));
    const lows = days.map((d) => e.l?.[d] ?? e.h[d]), highs = days.map((d) => e.h[d]), qty = days.map((d) => e.a?.[d] ?? 0);
    const usual = median(highs.length > 1 ? highs.slice(0, -1) : highs);
    rows.push({ id: +id, name: nameOf(+id), now: e.m, usual, days: days.length, low: lows.at(-1), high: highs.at(-1),
      qty: qty.at(-1), qty0: qty[0], low0: lows[0], held: held.get(+id) || 0 });
  }
  const busy = rows.filter((r) => r.qty >= minListed && r.days >= 2);
  const moves = busy.filter((r) => r.qty0 > 0 && r.low0 > 0).map((r) => ({ ...r, dq: r.qty / r.qty0 - 1, dp: r.low / r.low0 - 1 }));
  const resell = (r) => Math.floor(r.usual * CUT) - DEPOSIT - r.now;
  const usualOf = new Map(rows.map((r) => [r.id, r.usual]));
  const gaps = [];
  for (const [key, log] of Object.entries(load().ladders || {})) {
    const last = log.at(-1); if (!last?.at || Date.now() / 1000 - last.at > ladderHours * 3600) continue;
    const g = bestGap(parseLadder(last.levels), budget); if (!g) continue;
    const id = +key.split(":")[0];
    gaps.push({ ...g, id, key, name: nameOf(id), at: last.at, total: last.total, usual: usualOf.get(id) ?? null });
  }
  gaps.sort((a, b) => b.profit - a.profit);

  // The worklist: what to search at the AH (today's picks), and what each searched ladder says.
  // A ladder older than 2 hours wants a fresh search. ladders: the latest one per item, for the item view.
  const byId = new Map(rows.map((r) => [r.id, r]));
  const allLadders = load().ladders || {};
  const ladders = {}, check = new Map();
  for (const [key, log] of Object.entries(allLadders)) {
    const last = log.at(-1), id = +key.split(":")[0];
    if (!last?.at) continue;
    if (!ladders[id] || last.at > ladders[id].at) ladders[id] = { at: last.at, total: last.total, levels: parseLadder(last.levels).slice(0, 25) };
  }
  const byBusyProfit = (a, b) => b.profit * Math.log10(b.qty) - a.profit * Math.log10(a.qty);
  const sections = {
    buy: busy.map((r) => ({ ...r, profit: Math.floor(Math.min(r.usual, r.high) * CUT) - DEPOSIT - r.now }))
      .filter((r) => r.now <= r.usual * 0.8 && r.profit >= MIN_PROFIT && r.now <= budget)
      .sort(byBusyProfit),
    dips: busy.filter((r) => r.days >= 3 && r.now <= r.usual * 0.75 && r.high <= r.usual * 0.9 && r.now <= budget && resell(r) >= MIN_PROFIT)
      .map((r) => ({ ...r, profit: resell(r) })).sort(byBusyProfit),
    sell: rows.filter((r) => r.held && r.days >= 2 && r.now >= r.usual * 1.15 && r.qty >= 20)
      .map((r) => ({ ...r, worth: Math.floor(r.now * CUT) * r.held })).sort((a, b) => b.worth - a.worth),
    squeeze: moves.filter((r) => r.dq <= -0.2 && r.dp >= 0.15).sort((a, b) => (b.dp - b.dq) - (a.dp - a.dq)),
    flood: moves.filter((r) => r.dq >= 0.3 && r.dp <= -0.15).sort((a, b) => (a.dp - a.dq) - (b.dp - b.dq)),
    swingy: busy.filter((r) => r.high >= r.low * 1.5 && r.high - r.low >= 300 && r.low <= budget && r.high <= r.usual * 2)
      .sort((a, b) => (b.high - b.low) * Math.log10(b.qty) - (a.high - a.low) * Math.log10(a.qty)),
  };
  const want = (r, why) => { const c = check.get(r.id) || { id: r.id, name: r.name, why: [] }; if (!c.why.includes(why)) c.why.push(why); check.set(r.id, c); };
  // The best of each section (they're sorted), not the first by item id.
  for (const r of sections.buy.slice(0, 12)) want(r, "cheap now");
  for (const r of sections.sell.slice(0, 6)) want(r, "you hold it");
  for (const r of sections.swingy.slice(0, 6)) want(r, "swingy");
  for (const id of Object.keys(ladders).map(Number)) if (Date.now() / 1000 - ladders[id].at < 6 * 3600 && !check.has(id) && !hidden.has(id) && !vendorItems.has(id)) want({ id, name: byId.get(id)?.name || nameOf(id) }, "searched");
  const ORDER = { gap: 0, todo: 1, stale: 2, big: 3, yours: 4, tight: 5, empty: 6 };
  const checkList = [...check.values()].map((c) => {
    const r = byId.get(c.id), L = ladders[c.id];
    const age = L ? Date.now() / 1000 - L.at : null;
    const base = { ...c, usual: r?.usual ?? null, now: r?.now ?? null, held: r?.held || 0, at: L?.at ?? null };
    if (!L) return { ...base, kind: "todo", text: "search it at the AH" };
    // A ladder far under the price scan's cheapest is a bad capture (or the scan is stale): search again.
    if (r?.now && L.levels[0] && L.levels[0].price < r.now * 0.3) return { ...base, kind: "stale", text: `ladder floor ${money(L.levels[0].price)} vs scan ${money(r.now)}: doesn't add up` };
    const v = ladderVerdict(L.levels, { budget, usual: r?.usual, held: r?.held || 0 });
    return { ...base, ...v, kind: age > 2 * 3600 && v.kind !== "empty" ? "stale" : v.kind, verdict: v.kind };
  }).sort((a, b) => ORDER[a.kind] - ORDER[b.kind] || (b.gap?.profit || 0) - (a.gap?.profit || 0));
  return {
    date: new Date(Date.UTC(2020, 0, 1) + lastDay * 864e5).toISOString().slice(0, 10),
    items: rows.length, busy: busy.length, budget, minListed, skippedVendor,
    hidden: [...hidden].map(([id, v]) => ({ id, name: v.name || nameOf(id) })), scanAt: prices.scanAt || null,
    ...sections,
    gaps, check: checkList, ladders,
  };
}

export const money = (c) => { c = Math.round(c); const G = Math.floor(c / 1e4), s = Math.floor((c % 1e4) / 100), k = c % 100; return G ? `${G}g ${s}s` : s ? `${s}s ${String(k).padStart(2, "0")}c` : `${k}c`; };
const pct = (x) => `${x > 0 ? "+" : ""}${Math.round(x * 100)}%`;

export function report(a, top = 12) {
  const out = [`Auctionator through ${a.date}: ${a.items} items, ${a.busy} busy (${a.minListed}+ listed). Budget ${money(a.budget)} a buy.`, ""];
  const show = (title, list, fmt) => { out.push(title); if (!list.length) out.push("  (nothing)"); for (const r of list.slice(0, top)) out.push("  " + fmt(r)); out.push(""); };
  const g = money;
  show("BUY (a stray cheap listing on a busy market)", a.buy, (r) => `${r.name.padEnd(32)} now ${g(r.now).padStart(8)}  usual ${g(r.usual).padStart(8)}  ${pct(r.now / r.usual - 1).padStart(5)}  profit/each ${g(r.profit).padStart(7)}  listed ${r.qty}`);
  show("DIP BETS (whole market down today; bet it comes back)", a.dips, (r) => `${r.name.padEnd(32)} now ${g(r.now).padStart(8)}  usual ${g(r.usual).padStart(8)}  ${pct(r.now / r.usual - 1).padStart(5)}  if it recovers ${g(r.profit).padStart(7)}  listed ${r.qty}`);
  show("SELL what you hold (above usual today)", a.sell, (r) => `${r.name.padEnd(32)} you have ${String(r.held).padStart(4)}  now ${g(r.now).padStart(8)}  usual ${g(r.usual).padStart(8)}  ${pct(r.now / r.usual - 1).padStart(5)}  ~${g(r.worth)} after cut`);
  show("SQUEEZE (supply down, price up: sellers can ask more)", a.squeeze, (r) => `${r.name.padEnd(32)} price ${pct(r.dp).padStart(5)}  supply ${pct(r.dq).padStart(5)}  now ${g(r.now)}${r.held ? `  (you have ${r.held})` : ""}`);
  show("FLOOD (supply up, price down: don't hold, buy later)", a.flood, (r) => `${r.name.padEnd(32)} price ${pct(r.dp).padStart(5)}  supply ${pct(r.dq).padStart(5)}  now ${g(r.now)}${r.held ? `  (you have ${r.held})` : ""}`);
  const ago = (t) => { const m = Math.round((Date.now() / 1000 - t) / 60); return m < 60 ? `${m}m ago` : `${Math.round(m / 60)}h ago`; };
  const KIND = { gap: "GAP  ", big: "BIG  ", tight: "tight", yours: "yours", stale: "stale", todo: "todo ", empty: "empty" };
  show("CHECK THESE LADDERS (search each at the AH; verdict from the latest search)", a.check, (r) => `${KIND[r.kind]} ${r.name.padEnd(34)} ${r.kind === "stale" ? "search again; was: " : ""}${r.text}${r.sell ? `  | ${r.sell.text}` : ""}  [${r.why.join(", ")}]`);
  show("SWINGY (wide range today: buy near the low, list near the high)", a.swingy, (r) => `${r.name.padEnd(32)} today ${g(r.low).padStart(8)} to ${g(r.high).padStart(8)}  now ${g(r.now).padStart(8)}  listed ${r.qty}`);
  return out.join("\n");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const argv = process.argv.slice(2);
  const flag = (k, d) => { const i = argv.indexOf(`--${k}`); return i >= 0 ? argv[i + 1] : d; };
  console.log(report(analyze({ budget: Number(flag("budget", "5")), minListed: Number(flag("min-listed", "100")) }), Number(flag("top", "12"))));
}
