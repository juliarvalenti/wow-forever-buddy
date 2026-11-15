// XP-dense zones, hubs and chains for a character, from the QuestieDB addon's Forever data (read-only):
// quest XP, levels, races, classes, where quests start and end. Needs Questie installed in game.
//   node questxp.mjs [character (default: the last played)] [minQuestLevel] [maxQuestLevel] [zone name: every hub and quest there]
//   (QUESTIEDB_DIR to point elsewhere)
import fs from "node:fs";
import { decodeCbor, call } from "./fbl.mjs";
const ROOT = process.env.QUESTIEDB_DIR || "C:/Program Files (x86)/World of Warcraft/_forever_/Interface/AddOns/QuestieDB";
const meta = new Map();
for (const l of fs.readFileSync(`${ROOT}/QuestieDB_Forever.toc`, "utf8").split(/\r?\n/)) { const m = /^## (X-[^:]+): (.*)$/.exec(l); if (m) meta.set(m[1], m[2]); }
const stored = (k) => { const v = meta.get(k); if (v == null) return; const c = /^~(\d+)~$/.exec(v); return c ? Array.from({ length: +c[1] }, (_, i) => meta.get(`${k}-${i + 1}`)).join("") : v; };
const deep = (v) => Buffer.isBuffer(v) ? v.toString("utf8") : Array.isArray(v) ? v.map(deep) : v && typeof v === "object" ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, deep(x)])) : v;
const F = (p, id, x) => { const s = stored(`X-${p}${id}-${x}`); return s == null ? undefined : deep(decodeCbor(Buffer.from(s, "base64"))); };
const vals = (t) => (Array.isArray(t) ? t : Object.values(t || {}));

const XP = {};
for (const m of fs.readFileSync(`${ROOT}/support/Forever/QuestXP/xpDB-classic.lua`, "utf8").matchAll(/\[(\d+)\] = \{(\d+), (\d+)\}/g)) XP[m[1]] = { level: +m[2], xp: +m[3] };

const [whoArg, lo = "33", hi = "42", only] = process.argv.slice(2);
const who = whoArg || call("characters")[0].character; // default: the most recently played
const me = call("raw", { character: who });
const level = me.identity.level;
const done = new Set(vals(me.quests?.completed).map(Number));
const inLog = new Set(vals(me.quests?.log).map((q) => q.id));
// Classic: full XP up to quest level + 5, then 80/60/40/20/10%.
const earn = (qlvl, xp) => { const d = level - qlvl; return Math.round(xp * (d <= 5 ? 1 : [0.8, 0.6, 0.4, 0.2][d - 6] ?? 0.1)); };

const ZONES = { 33: "Stranglethorn", 45: "Arathi Highlands", 36: "Alterac Mountains", 267: "Hillsbrad", 11: "Wetlands", 405: "Desolace", 15: "Dustwallow Marsh",
  400: "Thousand Needles", 3: "Badlands", 8: "Swamp of Sorrows", 47: "Hinterlands", 440: "Tanaris", 357: "Feralas", 331: "Ashenvale", 406: "Stonetalon",
  17: "Barrens", 1519: "Stormwind", 1537: "Ironforge", 10: "Duskwood", 44: "Redridge", 38: "Loch Modan", 4: "Blasted Lands", 16: "Azshara", 361: "Felwood",
  51: "Searing Gorge", 46: "Burning Steppes", 1657: "Darnassus", 148: "Darkshore", 130: "Silverpine", 85: "Tirisfal", 28: "Western Plaguelands",
  139: "Eastern Plaguelands", 41: "Deadwind Pass", 490: "Un'Goro", 12: "Elwynn", 40: "Westfall", 1: "Dun Morogh", 1638: "Thunder Bluff", 1637: "Orgrimmar", 1497: "Undercity" };
const DUNGEON = { 796: "Scarlet Monastery", 491: "Razorfen Kraul", 722: "Razorfen Downs", 1337: "Uldaman", 721: "Gnomeregan", 1176: "Zul'Farrak", 2100: "Maraudon",
  209: "Shadowfang Keep", 718: "Wailing Caverns", 1581: "Deadmines", 717: "Stockade", 719: "Blackfathom Deeps", 2557: "Dire Maul", 1584: "Blackrock Depths" };
const zname = (z) => ZONES[z] || DUNGEON[z] || `zone ${z}`;

// Where a quest is turned in (first finisher spawn), and where it starts.
const place = (field, q) => {
  const t = F("Quest-", q, field); const npc = vals(vals(t)[0])[0], obj = vals(vals(t)[1])[0];
  const id = npc ?? obj, p = npc != null ? "Npc-" : "Object-";
  if (id == null) return undefined;
  const sp = F(p, id, npc != null ? 7 : 4) || {}; const s = F(p, id, "S") || {};
  const [z, pts] = Object.entries(sp)[0] || [];
  const pt = vals(pts)[0] || [];
  return { name: s[1], zone: z != null ? +z : s[npc != null ? 9 : 5], x: pt[0], y: pt[1] };
};

const quests = [];
for (const k of meta.keys()) {
  const m = /^X-Quest-(\d+)-S$/.exec(k); if (!m) continue;
  const id = +m[1], sc = F("Quest-", id, "S") || {};
  const qlvl = sc[5], req = sc[4] || 0, x = XP[id];
  if (!x || !qlvl || qlvl < +lo || qlvl > +hi || req > level + 3 || done.has(id)) continue;
  const races = sc[6], classes = sc[7];
  if (races && !((races % 2048) & 1101)) continue;          // Alliance
  if (classes && !(classes & 8)) continue;                   // rogue
  if (F("Quest-", id, 18)) continue;                         // profession quests
  const end = place(3, id), start = place(2, id);
  if (!end) continue;
  const zs = sc[17];
  quests.push({ id, name: sc[1], qlvl, req, xp: earn(qlvl, x.xp), end, start, dungeon: DUNGEON[zs], log: inLog.has(id),
    next: sc[22], pre: vals(F("Quest-", id, 12)).concat(vals(F("Quest-", id, 13))) });
}

// Zones by where you turn quests in; dungeon quests apart (they need a group).
const zones = {};
for (const q of quests) {
  const key = q.dungeon ? `[dungeon] ${q.dungeon}` : zname(q.end.zone);
  const z = (zones[key] ||= { xp: 0, n: 0, hubs: {} });
  z.xp += q.xp; z.n++;
  const hub = `${q.end.name} (${Math.round(q.end.x)}, ${Math.round(q.end.y)})`;
  // Hubs: turn-ins within ~8 map units of each other count as one place.
  let h = Object.values(z.hubs).find((h) => h.zone === q.end.zone && Math.hypot(h.x - q.end.x, h.y - q.end.y) < 8);
  if (!h) h = z.hubs[hub] = { name: hub, zone: q.end.zone, x: q.end.x, y: q.end.y, xp: 0, quests: [] };
  h.xp += q.xp; h.quests.push(`${q.name} (${q.qlvl}${q.log ? ", in log" : ""}) ${q.xp}`);
}
console.log(`${who}: level ${level}, quests ${lo}-${hi} not done: ${quests.length}, ${quests.reduce((t, q) => t + q.xp, 0)} XP\n`);
for (const [z, v] of Object.entries(zones).sort((a, b) => b[1].xp - a[1].xp).filter(([z]) => !only || z.toLowerCase().includes(only.toLowerCase())).slice(0, only ? 99 : 14)) {
  console.log(`${z}: ${v.xp} XP in ${v.n} quests`);
  for (const h of Object.values(v.hubs).sort((a, b) => b.xp - a.xp).slice(0, only ? 99 : 3)) {
    console.log(`   hub ${h.name}: ${h.xp} XP, ${h.quests.length} quests`);
    if (only) for (const q of h.quests) console.log(`      ${q}`);
  }
}
if (only) process.exit(0);
// Chains: follow nextQuestInChain from quests with no prerequisite in this set.
const byId = Object.fromEntries(quests.map((q) => [q.id, q]));
const chains = [];
for (const q of quests) {
  if (q.pre.some((p) => byId[p])) continue;
  const c = [q]; let n = q.next, guard = 0;
  while (n && byId[n] && guard++ < 15) { c.push(byId[n]); n = byId[n].next; }
  if (c.length > 1) chains.push({ xp: c.reduce((t, x) => t + x.xp, 0), c });
}
console.log("\nLongest-paying chains:");
for (const ch of chains.sort((a, b) => b.xp - a.xp).slice(0, 10))
  console.log(`  ${ch.xp} XP: ${ch.c.map((x) => `${x.name} (${x.qlvl})`).join(" -> ")}  [${zname(ch.c[0].start?.zone)} -> ${zname(ch.c.at(-1).end.zone)}]${ch.c.some((x) => x.dungeon) ? " [dungeon]" : ""}`);
