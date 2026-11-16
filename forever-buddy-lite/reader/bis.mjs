// Best in slot by level for an agility melee (rogue), from the WoWSims Forever item database, against
// what the character wears now: per slot the best item at each level, where it comes from, and how much
// it beats yours.
//   node bis.mjs [character (default: the last played)] [--levels 40,45,50,60] [--all]
//   WOWSIMS_DB=<path to wowsims-forever/assets/database/db.json>
// Scoring: the reader's agility weights (agi 1, sta 0.6, str 0.5, AP 0.5, 1% crit or hit 12, weapon dps 3;
// ranged dps 1, since a rogue's ranged slot is mostly stats). On-use and proc effects aren't scored, random
// suffix ("of the Monkey") items are left out, and so are raids and PvP rank rewards unless --all.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { call, parseStats, parseLink } from "./fbl.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const argv = process.argv.slice(2);
const flag = (k, d) => { const i = argv.indexOf(`--${k}`); return i >= 0 ? argv.splice(i, 2)[1] : d; };
const all = argv.includes("--all") && !!argv.splice(argv.indexOf("--all"), 1);
const levels = flag("levels", "40,45,50,60").split(",").map(Number);
const who = argv[0] || call("characters")[0].character;
const DB = process.env.WOWSIMS_DB || path.join(here, "..", "..", "..", "wowsims-forever", "assets", "database", "db.json");
if (!fs.existsSync(DB)) throw new Error(`No WoWSims item database at ${DB}; clone wowsims/forever next to this repo or set WOWSIMS_DB.`);
const db = JSON.parse(fs.readFileSync(DB, "utf8"));
const zones = Object.fromEntries((db.zones || []).map((z) => [z.id, z.name]));

// Stat indexes (wowsims proto/common.proto): 0 Str, 1 Agi, 2 Sta, 14 AP, 17 hit rating, 18 crit rating.
// About 10 hit rating and 14 crit rating are 1% in this data.
const W = { 0: 0.5, 1: 1, 2: 0.6, 14: 0.5, 17: 12 / 10, 18: 12 / 14 };
const ROGUE_WEAPONS = new Set([2, 3, 4, 9]); // dagger, fist, mace, sword
const ROGUE_RANGED = new Set([1, 2, 3, 4]); // bow, crossbow, gun, thrown
const RAIDS = new Set([2717, 2677, 3428, 3429, 1977, 2159, 3456]); // MC, BWL, AQ40, AQ20, ZG, Onyxia, Naxx

const opt = (it) => Object.values(it.scalingOptions || {})[0] || {};
function score(it) {
  const o = opt(it); let s = 0;
  for (const [k, v] of Object.entries(o.stats || {})) s += (W[k] || 0) * v;
  if (it.type === 13) s += (o.unroundedDps || 0) * 3;
  if (it.type === 14) s += (o.unroundedDps || 0) * 1;
  return s;
}
// Sources carry a faction (1 Alliance, 2 Horde) on vendors, quests and reputations; keep what Alliance can get.
const ALLIANCE = 1;
const fac = (src) => src.soldBy?.faction ?? src.quest?.faction ?? src.rep?.factionId;
const forUs = (src) => { const f = fac(src); return f == null || f === ALLIANCE; };
function usable(it) {
  if (it.randomSuffixOptions || it.pvp) return false;
  const facs = (it.sources || []).map(fac).filter((f) => f != null);
  if (facs.length && !facs.includes(ALLIANCE)) return false; // only Horde sources name a faction
  if (it.classAllowlist && !it.classAllowlist.includes(4)) return false; // 4 = rogue
  if ([1, 3, 5, 6, 7, 8, 9, 10].includes(it.type) && !(it.armorType === 1 || it.armorType === 2)) return false; // cloth or leather
  if (it.type === 13 && (!ROGUE_WEAPONS.has(it.weaponType) || it.handType === 4)) return false;
  if (it.type === 14 && !ROGUE_RANGED.has(it.rangedWeaponType)) return false;
  return it.type >= 1 && it.type <= 14;
}
// Forever's new items often say level 1 whatever their item level: then about item level - 5.
const req = (it) => (it.reqLvl > 1 ? it.reqLvl : Math.max(1, (opt(it).ilvl || 1) - 5));
function source(it) {
  const list = (it.sources || []).filter(forUs);
  const s = list.find((x) => !x.pvp) || list[0];
  if (list.some((x) => x.pvp)) return `PvP rep: ${s?.soldBy?.npcName || "battleground vendor"}`;
  if (!s) return "?";
  if (s.quest) return `quest: ${s.quest.name}`;
  if (s.crafted) return `crafted (${{ 1: "Alchemy", 2: "Blacksmithing", 4: "Engineering", 8: "Leatherworking", 10: "Tailoring" }[s.crafted.profession] || "profession"})`;
  if (s.drop?.otherName) return s.drop.otherName.toLowerCase();
  if (s.rep) return `reputation${s.soldBy ? `: ${s.soldBy.npcName}` : ""}`;
  if (s.drop) return `drop${s.drop.npcName ? `: ${s.drop.npcName}` : ""}${s.drop.zoneId ? `, ${zones[s.drop.zoneId] || "zone " + s.drop.zoneId}` : ""}`;
  if (s.soldBy) return `vendor: ${s.soldBy.npcName}`;
  return Object.keys(s)[0];
}
const raid = (it) => (it.sources || []).some((s) => s.drop && RAIDS.has(s.drop.zoneId));

const SLOTS = [["Head", 1, [1]], ["Neck", 2, [2]], ["Shoulder", 3, [3]], ["Back", 4, [15]], ["Chest", 5, [5]], ["Wrist", 6, [9]],
  ["Hands", 7, [10]], ["Waist", 8, [6]], ["Legs", 9, [7]], ["Feet", 10, [8]], ["Ring", 11, [11, 12]], ["Trinket", 12, [13, 14]],
  ["Main hand", 13, [16]], ["Off hand", 13, [17]], ["Ranged", 14, [18]]];
const byId = new Map(db.items.map((it) => [it.id, it]));
const pool = db.items.filter((it) => usable(it) && (all || !raid(it)));

// What's worn, scored from its own tooltip lines (random suffixes included), with the same weights.
const gear = call("raw", { character: who, section: "gear" }).gear || {};
function tipScore(g, slot) {
  const s = parseStats(g.tip || []);
  return (s.agi || 0) + 0.6 * (s.sta || 0) + 0.5 * (s.str || 0) + 0.5 * (s.ap || 0) + 12 * (s.crit || 0) + 12 * (s.hit || 0)
    + (s.dps || 0) * (slot === 18 ? 1 : 3);
}
const worn = (slot) => {
  const g = gear[slot];
  if (!g) return { s: 0, name: "(empty)" };
  return { s: tipScore(g, slot), name: parseLink(g.link).name || byId.get(Number(g.id))?.name || `item ${g.id}` };
};

console.log(`${who}: best in slot by level (agility weights; raids${all ? "" : " and PvP"} ${all ? "included" : "left out"})\n`);
for (const L of levels) {
  let cur = 0, bis = 0;
  const lines = [];
  for (const [name, type, slots] of SLOTS) {
    const cands = pool.filter((it) => it.type === type && req(it) <= L
      && (name !== "Off hand" || it.handType !== 1) && (name !== "Main hand" || it.handType !== 3)).sort((a, b) => score(b) - score(a));
    for (let k = 0; k < slots.length; k++) {
      const w = worn(slots[k]);
      // Two ring and trinket slots: the next best that isn't unique-equipped twice.
      const best = cands.filter((it) => k === 0 || !(it.unique && it.id === cands[0]?.id))[k] || cands[k];
      if (!best) continue;
      const b = score(best);
      cur += w.s; bis += Math.max(b, w.s);
      const gain = w.s > 0 ? Math.round(((b - w.s) / w.s) * 100) : null;
      lines.push(`  ${(name + (slots.length > 1 ? ` ${k + 1}` : "")).padEnd(11)} ${String(w.name).padEnd(30)} ${w.s.toFixed(0).padStart(4)}  ->  ${best.name.padEnd(32)} ${b.toFixed(0).padStart(4)}  ${b <= w.s ? "   keep" : gain === null ? "    new" : `+${gain}%`.padStart(7)}  req ${req(best)}  ${source(best)}`);
    }
  }
  console.log(`Level ${L}: total ${cur.toFixed(0)} now -> ${bis.toFixed(0)} best (+${Math.round(((bis - cur) / Math.max(1, cur)) * 100)}%)`);
  console.log(lines.join("\n") + "\n");
}
