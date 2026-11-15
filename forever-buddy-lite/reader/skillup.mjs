// Cheapest way up a profession, priced from Auctionator's last scan.
//   node skillup.mjs <character> [profession=engineering] [--to 225] [--from N] [--craft copper per craft, default 20]
// For each skill point it picks the recipe with the lowest expected cost: reagent cost / chance of a point.
// Reagents are priced at the cheapest of: auction house, vendor, crafting them, smelting them (if Mining allows).
// What your characters hold (bags, bank, mail) is free and used up first.
// What the game told the addon wins over the recipe file: learn levels from trainers you visited,
// recipes you know and the tools they need from your profession window. Unconfirmed steps get a "?".
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { call, load } from "./fbl.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const argv = process.argv.slice(2);
const flag = (k) => { const i = argv.indexOf(`--${k}`); return i >= 0 ? argv.splice(i, 2)[1] : undefined; };
const to = Number(flag("to") || 225);
const fromArg = flag("from");
const CRAFT_TIME = Number(flag("craft") || 20); // copper a craft's time is worth (--craft 0 for pure cash)
const [whoArg, prof = "engineering"] = argv;
const who = whoArg || call("characters")[0].character; // default: the most recently played
const data = JSON.parse(fs.readFileSync(path.join(here, "data", `${prof}.json`), "utf8"));

// Vendor prices in copper (approximate; the AH is used when cheaper).
const VENDOR = { "Weak Flux": 200, Coal: 500, "Wooden Stock": 200, "Heavy Stock": 800, Nightcrawlers: 50 };

const skills = Object.fromEntries(call("skills", { character: who }).skills.map((s) => { const m = /^(.*) (\d+)\/(\d+)$/.exec(s); return [m[1], [+m[2], +m[3]]]; }));
if (!Object.keys(skills).length) throw new Error(`No skills saved for ${who}: log in, then /fbl r`);
const start = fromArg ? Number(fromArg) : skills[data.profession]?.[0];
if (start == null) throw new Error(`${who} has no ${data.profession}`);
const mining = skills.Mining?.[0] || 0;

const ah = {}, owned = {};
for (const [name, id] of Object.entries(data.items)) {
  const p = call("prices", { query: String(id) }).prices.find((x) => x.last_copper);
  if (p) ah[name] = p.last_copper;
  try { owned[name] = call("find", { query: String(id) }).reduce((n, r) => n + (r.count || 1), 0); } catch { owned[name] = 0; }
}
const recipe = Object.fromEntries(data.recipes.map((r) => [r.name, r]));

const vals = (t) => (Array.isArray(t) ? t : Object.values(t || {}));
const { trainers } = load();
const mine = call("raw", { character: who }).professions?.[data.profession];
const known = new Map(vals(mine?.recipes).map((r) => [r.name, r]));
const taught = {};
for (const t of Object.values(trainers || {})) for (const sv of vals(t.services))
  if (sv.skill === data.profession && sv.skill_req != null) taught[sv.name] = Math.min(taught[sv.name] ?? Infinity, sv.skill_req);
for (const r of data.recipes) {
  if (taught[r.name] != null) r.skill[0] = taught[r.name];
  r.confirmed = known.has(r.name) || taught[r.name] != null;
  r.learn = mine && !known.has(r.name); // window seen and it isn't there: buy it at the trainer first
  r.anvil = vals(known.get(r.name)?.tools).some((t) => /anvil/i.test(t));
}
// The window's colours are the truth at the rank it was seen at: stretch or cut the file's bands to fit.
const seenAt = mine?.rank;
if (seenAt != null) for (const r of data.recipes) {
  const live = known.get(r.name)?.color, k = r.skill;
  if (live === "orange") k[1] = Math.max(k[1], seenAt + 1);
  if (live === "yellow") { k[1] = Math.min(k[1], seenAt); k[2] = Math.max(k[2], seenAt + 1); }
  if (live === "green") { k[2] = Math.min(k[2], seenAt); k[3] = Math.max(k[3], seenAt + 1); }
  if (live === "grey") k[3] = Math.min(k[3], seenAt);
  for (let j = 1; j < 4; j++) k[j] = Math.max(k[j], k[j - 1]);
}
const unlisted =[...known.keys()].filter((n) => !recipe[n]);

// Classic odds: orange/yellow always, green falls off to grey.
const chance = ([, y, , g], s) => (s < y ? 1 : s >= g ? 0 : Math.min(1, (g - s) / (g - y)));

// Get qty of an item: from stock first (free), then the cheapest of buying, crafting or smelting.
// Mutates stock and adds to bought / used. Returns the copper spent.
// Crafted intermediates are counted in made: they give skill-ups too.
function acquire(item, qty, skill, stock, bought, used, made = {}, seen = new Set()) {
  const free = Math.min(stock[item] || 0, qty);
  if (free > 0) { stock[item] -= free; used[item] = (used[item] || 0) + free; qty -= free; }
  if (qty <= 1e-9) return 0;
  if (seen.has(item)) return Infinity;
  seen = new Set(seen).add(item);
  const opts = [];
  const buyAt = Math.min(ah[item] ?? Infinity, VENDOR[item] ?? Infinity);
  if (buyAt < Infinity) opts.push({ how: ah[item] === buyAt ? "AH" : "vendor", run: (st, b) => { b[item] = b[item] || { n: 0, each: buyAt, how: ah[item] === buyAt ? "AH" : "vendor" }; b[item].n += qty; return qty * buyAt; } });
  const make = (reagents, per, craft) => (st, b, u, m) => {
    if (craft) m[item] = (m[item] || 0) + qty / per;
    return Object.entries(reagents).reduce((t, [k, n]) => t + acquire(k, (qty * n) / per, skill, st, b, u, m, seen), 0);
  };
  const r = recipe[item];
  if (r && r.skill[0] <= skill) opts.push({ how: "craft", run: make(r.reagents, r.makes || 1, true) });
  const sm = data.smelting[item];
  if (sm && sm.mining <= mining) opts.push({ how: "smelt", run: make(sm.reagents, sm.makes, false) });
  let best;
  for (const o of opts) {
    const c = o.run({ ...stock }, {}, {}, {});
    if (!best || c < best.c) best = { o, c };
  }
  if (!best || best.c === Infinity) return Infinity;
  return best.o.run(stock, bought, used, made);
}

const caps = data.caps.map((c) => c.cap);
const stock = { ...owned }, bought = {}, used = {};
const plan = [];
// Skill is fractional: crafting intermediates (blasting powder, gizmos...) adds expected points on the way.
const sum = (o) => Object.values(o).reduce((a, b) => a + b, 0);
for (let sk = start; sk < to; ) {
  const s = Math.floor(sk);
  let best;
  for (const r of data.recipes) {
    if (r.skill[0] > s) continue;
    const p = chance(r.skill, s);
    if (!p) continue;
    const made = {};
    const c = Object.entries(r.reagents).reduce((t, [k, n]) => t + acquire(k, n / p, s, { ...stock }, {}, {}, made), 0);
    const bonus = Object.entries(made).reduce((t, [k, n]) => t + n * chance(recipe[k].skill, s), 0);
    // Each craft also costs time: with free mats, a sure point beats a 1-in-3 green.
    const score = (c + CRAFT_TIME * (1 / p + sum(made))) / (1 + bonus);
    if (!best || score < best.score) best = { r, p, c, score, gain: 1 + bonus };
  }
  if (!best || best.c === Infinity) { plan.push({ s, to: s + 1, none: true }); sk = s + 1; continue; }
  const made = {};
  for (const [k, n] of Object.entries(best.r.reagents)) acquire(k, n / best.p, s, stock, bought, used, made);
  // What you make for points goes in your bags: later steps can use it (powder for grenades, struts, gizmos...).
  stock[best.r.name] = (stock[best.r.name] || 0) + (best.p ? 1 / best.p : 0) * (best.r.makes || 1);
  sk = Math.min(to, sk + best.gain);
  const last = plan[plan.length - 1];
  const step = { s, to: Math.floor(sk), r: best.r, crafts: 1 / best.p, total: best.c, made };
  if (last && last.r === best.r) { last.to = step.to; last.crafts += step.crafts; last.total += step.total; for (const [k, n] of Object.entries(made)) last.made[k] = (last.made[k] || 0) + n; }
  else plan.push(step);
}

const g = (c) => `${Math.floor(c / 10000)}g ${Math.floor((c % 10000) / 100)}s`;
console.log(`${data.profession} ${start} -> ${to}, Mining ${mining}`);
console.log(`  game data: ${mine ? `${known.size} known recipes (window seen)` : "profession window not seen"}, ${Object.keys(taught).length} trainer entries; ? = level from the web, not confirmed in game`);
if (unlisted.length) console.log(`  known but not in the recipe file (no colour data, not planned): ${unlisted.join(", ")}`);
// Each rank is trained when the one before it is maxed.
data.caps.forEach((c, k) => { const at = caps[k - 1]; if (at && at >= start && at < to) console.log(`  at ${at}: train ${c.rank} (${c.needs}, ${g(c.cost)})`); });
let total = 0;
for (const st of plan) {
  if (st.none) { console.log(`  ${st.s}: nothing trainable gives a point`); continue; }
  total += st.total;
  const sub = Object.entries(st.made).map(([k, n]) => `${Math.ceil(n)} ${k}`).join(", ");
  const tag = (st.r.confirmed ? "" : "?") + (st.r.anvil ? " [anvil]" : "") + (st.r.learn ? " (learn it)" : "");
  console.log(`  ${st.s}-${st.to}  ${(st.r.name + tag).padEnd(28)} ~${Math.ceil(st.crafts)} crafts  ${g(st.total)}${sub ? `  (first make ${sub})` : ""}`);
}
console.log(`  expected cost ${g(total)}
From what you hold:`);
for (const [k, n] of Object.entries(used).sort((a, b) => b[1] - a[1])) console.log(`  ${String(Math.ceil(n)).padStart(4)} ${k}  (of ${owned[k]})`);
console.log("To buy:");
let spend = 0;
for (const [k, b] of Object.entries(bought).sort((a, b) => b[1].n * b[1].each - a[1].n * a[1].each)) {
  const n = Math.ceil(b.n);
  spend += n * b.each;
  console.log(`  ${String(n).padStart(4)} ${k.padEnd(24)} ${b.how.padEnd(6)} @ ${(b.each / 100).toFixed(2)}s  = ${g(n * b.each)}`);
}
console.log(`  to spend ~${g(spend)}`);
