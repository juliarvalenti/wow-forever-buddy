// An Auctionator shopping list for Point Blank Sniper: busy items, each capped under its usual price.
//   node snipes.mjs [--cap 0.25 (gold per item)] [--ratio 0.8] [--deposit 0.01] [--min-profit 0.02] [--count 50]
//                   [--min-listed 300] [--name "FBL Snipes"] [--out file | --dir folder]
// --dir writes <folder>/<date>/<list name with its tag>.txt, so every build is kept and named apart.
// Point Blank Sniper buys whatever listing is under the cap, often a single item, and reposting it costs a deposit.
// So the cap also leaves min-profit on ONE item after the 5% cut and the deposit; items too cheap for that drop out.
// Usual = the median of the daily lowest listing in Auctionator's scans (the lower middle on an even count, so two
// days can't make yesterday's price the usual one). The cap is ratio x the lower of usual and the cheapest listing
// now: a market that has fallen isn't a snipe, only a listing under today's floor is. Names come from your characters' items,
// the gear AH scan, the recipe files, and QuestieDB if it's installed (Auctionator stores ids only).
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { decodeCbor, loadPrices, load, call } from "./fbl.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const argv = process.argv.slice(2);
const flag = (k, d) => { const i = argv.indexOf(`--${k}`); return i >= 0 ? argv[i + 1] : d; };
const cap = Number(flag("cap", "0.25")) * 10000, ratio = Number(flag("ratio", "0.8"));
const count = Number(flag("count", "50")), minListed = Number(flag("min-listed", "300"));
const deposit = Number(flag("deposit", "0.01")) * 10000, minProfit = Number(flag("min-profit", "0.02")) * 10000;
// A short random tag on every list name, so each import is a new list and never merges into an old one.
const listName = `${flag("name", "FBL Snipes")} ${crypto.randomBytes(3).toString("hex")}`;
const d = new Date(), two = (n) => String(n).padStart(2, "0"); // local time, as the folder name reads
const stamp = `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())}`;
const outFile = flag("out") ?? (flag("dir") && path.join(flag("dir"), stamp, `${listName.replace(/[^\w-]+/g, "-")}.txt`));

const names = new Map();
const { chars, ah } = load();
for (const it of Object.values(ah?.items || {})) if (it.id && it.name) names.set(String(it.id), it.name);
for (const f of fs.readdirSync(path.join(here, "data")).filter((f) => f.endsWith(".json")))
  for (const [n, id] of Object.entries(JSON.parse(fs.readFileSync(path.join(here, "data", f), "utf8")).items || {})) names.set(String(id), n);
for (const c of Object.keys(chars)) for (const r of call("items", { character: c }).items) if (r.id && r.name) names.set(String(r.id), r.name);
const toc = path.join(process.env.QUESTIEDB_DIR || "C:/Program Files (x86)/World of Warcraft/_forever_/Interface/AddOns/QuestieDB", "QuestieDB_Forever.toc");
const questie = new Map();
if (fs.existsSync(toc)) for (const l of fs.readFileSync(toc, "utf8").split(/\r?\n/)) { const m = /^## X-Item-(\d+)-S: (.*)$/.exec(l); if (m) questie.set(m[1], m[2]); }
const nameOf = (id) => {
  if (names.has(id)) return names.get(id);
  const s = questie.get(id); if (!s) return;
  const v = decodeCbor(Buffer.from(s, "base64"))[1];
  return Buffer.isBuffer(v) ? v.toString("utf8") : v;
};

const rows = [];
for (const data of Object.values(loadPrices().realms)) for (const [id, e] of Object.entries(data)) {
  if (id === "version" || !e?.h) continue;
  const days = Object.keys(e.h);
  const lows = days.map((d) => e.l?.[d] ?? e.h[d]).sort((a, b) => a - b);
  const usual = lows[Math.floor((lows.length - 1) / 2)];
  const listed = days.reduce((t, d) => t + (e.a?.[d] || 0), 0) / days.length;
  const floor = Math.min(usual, e.m || usual);
  const max = Math.floor(Math.min(floor * ratio, floor * 0.95 - deposit - minProfit));
  if (listed < minListed || max <= 0 || max > cap) continue;
  const name = nameOf(id);
  if (!name || name.includes('"')) continue;
  rows.push({ name, usual, now: e.m, listed: Math.round(listed), max, days: days.length,
    single: Math.floor(floor * 0.95 - deposit - max) });
}
// Worth most where the price and the trade are both big.
rows.sort((a, b) => b.usual * Math.sqrt(b.listed) - a.usual * Math.sqrt(a.listed));
const top = rows.slice(0, count);
const gold = (c) => (c / 10000).toFixed(4).replace(/0+$/, "").replace(/\.$/, "");
const line = `${listName}^` + top.map((r) => `"${r.name}";;;;;;;;;${gold(r.max)};;#;;`).join("^");
if (outFile) {
  fs.mkdirSync(path.dirname(outFile), { recursive: true });
  fs.writeFileSync(outFile, line + "\n");
}
const s = (c) => `${(c / 100).toFixed(2)}s`;
for (const r of top) console.error(`${r.name.padEnd(30)} max ${s(r.max).padStart(8)}  floor ${s(Math.min(r.usual, r.now || r.usual)).padStart(8)}  one-item profit ${s(r.single).padStart(7)}  listed ~${r.listed}`);
console.log(outFile || line);
