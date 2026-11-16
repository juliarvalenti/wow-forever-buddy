// The item catalog for the dashboard: every item Auctionator has prices for that we can name, with
// its kind (Cloth, Herbs, ...) and its daily history.
//
// Auctionator stores item ids only. Names come from QuestieDB (if installed), the gear AH scan, and
// your characters' items.

import fs from "node:fs";
import path from "node:path";
import { decodeCbor, loadPrices, load, call } from "./fbl.mjs";

const QUESTIE_TOC = path.join(
  process.env.QUESTIEDB_DIR || "C:/Program Files (x86)/World of Warcraft/_forever_/Interface/AddOns/QuestieDB",
  "QuestieDB_Forever.toc",
);

// Auctionator counts days from 2020-01-01.
const AUCTIONATOR_DAY_ZERO = Date.UTC(2020, 0, 1);
const DAY_MS = 86400000;
const dayToDate = (day) => new Date(AUCTIONATOR_DAY_ZERO + day * DAY_MS);

// ---- Kinds -------------------------------------------------------------------------------------

// The game's item classes (and trade-goods subclasses) we group by.
const CLASS_KIND = {
  0: "Consumables", 1: "Bags", 2: "Weapons", 4: "Armor", 5: "Reagents", 6: "Ammo",
  9: "Recipes", 11: "Quivers", 12: "Quest items", 15: "Miscellaneous",
};
const TRADE_GOODS = 7;
const TRADE_KIND = {
  1: "Parts", 2: "Explosives", 3: "Devices", 5: "Cloth", 6: "Leather", 7: "Metal & stone",
  8: "Meat & fish", 9: "Herbs", 10: "Elemental", 11: "Other trade goods", 12: "Enchanting",
};

// Classic-era data files most trade goods under one subclass, so the name decides the finer kind.
// First match wins.
const KIND_BY_NAME = [
  [/^(Linen|Wool|Silk|Mageweave|Runecloth|Felcloth|Mooncloth) Cloth$|^Bolt of |^(Runecloth|Felcloth|Mooncloth|Shadow Silk|Spider's Silk|Thick Spider's Silk|Ironweb Spider Silk)$/, "Cloth"],
  [/Leather$|Hide$|Scale$|Scales$/, "Leather"],
  [/Ore$|Bar$|Stone$|^Coal$|^Elementium/, "Metal & stone"],
  [/^Elemental |^Essence of (Air|Earth|Fire|Water|Undeath)$|^(Heart|Breath|Core|Globe) of /, "Elemental"],
  [/Dust$|Essence$|Shard$|^Nexus Crystal$/, "Enchanting"],
  [/Meat$|Flesh$|Wing$|Egg$|Leg$|Tail$|Clam Meat$|Haunch|Chunk of|Fillet|Snapper|Mackerel|Squid$|Eel$|Sagefish/, "Meat & fish"],
  [/Lotus$|bloom$|thistle$|^(Sungrass|Dreamfoil|Gromsblood|Icecap|Kingsblood|Liferoot|Fadeleaf|Goldthorn|Wintersbite|Firebloom|Sorrowmoss|Blindweed|Stranglekelp|Bruiseweed|Silverleaf|Briarthorn|Mageroyal|Earthroot|Wild Steelbloom|Grave Moss|Khadgar's Whisker|Arthas' Tears|Ghost Mushroom|Mountain Silversage|Plaguebloom|Golden Sansam|Black Lotus|Swiftthistle)$/, "Herbs"],
  [/Potion$|Elixir|Flask of|Draught/, "Potions & elixirs"],
  [/^(Pattern|Recipe|Schematic|Formula|Plans|Manual|Design): /, "Recipes"],
];

export function kindOf(info) {
  for (const [pattern, kind] of KIND_BY_NAME) if (pattern.test(info?.name || "")) return kind;
  if (info?.cls === TRADE_GOODS) return TRADE_KIND[info.sub] || "Other trade goods";
  return CLASS_KIND[info?.cls] || "Other";
}

// ---- Names -------------------------------------------------------------------------------------

// QuestieDB's toc carries, per item, a CBOR record "## X-Item-<id>-S: <base64>": field 1 the name,
// 12 the class, 13 the subclass. Read once.
let questieCache = null;
function questieItems() {
  if (questieCache) return questieCache;
  questieCache = new Map();
  if (!fs.existsSync(QUESTIE_TOC)) return questieCache;
  for (const line of fs.readFileSync(QUESTIE_TOC, "utf8").split(/\r?\n/)) {
    const match = /^## X-Item-(\d+)-S: (.*)$/.exec(line);
    if (!match) continue;
    const record = decodeCbor(Buffer.from(match[2], "base64"));
    const text = (v) => (Buffer.isBuffer(v) ? v.toString("utf8") : v);
    questieCache.set(match[1], { name: text(record[1]), cls: record[12], sub: record[13] });
  }
  return questieCache;
}

// id -> { name, cls, sub }, from QuestieDB, then the gear scan and your characters for what it lacks.
function itemInfo() {
  const info = new Map(questieItems());
  const { chars, ah } = load();
  for (const item of Object.values(ah?.items || {})) {
    if (item.id && !info.has(String(item.id))) info.set(String(item.id), { name: item.name, cls: item.class, sub: item.subclass });
  }
  for (const character of Object.keys(chars)) {
    for (const row of call("items", { character }).items) {
      const id = String(row.id);
      if (row.id && !info.get(id)?.name) info.set(id, { ...(info.get(id) || {}), name: row.name });
    }
  }
  return info;
}

// ---- The catalog -------------------------------------------------------------------------------

// { items, stamp }: items = [{ id, n: name, k: kind, now: cheapest in the latest scan,
//   h: [[day "MM-DD", low, high, listed], ...] }] sorted by name; stamp = last day covered.
export function buildItems() {
  const info = itemInfo();
  const items = [];
  let lastDay = 0;

  for (const realm of Object.values(loadPrices().realms)) {
    for (const [id, entry] of Object.entries(realm)) {
      if (id === "version" || !entry?.h) continue;
      const item = info.get(id);
      if (!item?.name) continue;

      // entry: h = day -> high, l = day -> low, a = day -> quantity listed, m = cheapest now.
      const days = Object.keys(entry.h).map(Number).sort((a, b) => a - b);
      lastDay = Math.max(lastDay, days.at(-1));
      const history = days.map((day) => [
        dayToDate(day).toISOString().slice(5, 10),
        entry.l?.[day] ?? entry.h[day],
        entry.h[day],
        entry.a?.[day] ?? 0,
      ]);
      items.push({ id: Number(id), n: item.name, k: kindOf(item), now: entry.m, h: history });
    }
  }

  items.sort((a, b) => a.n.localeCompare(b.n));
  return { items, stamp: dayToDate(lastDay).toISOString().slice(0, 10) };
}
