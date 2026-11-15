import { test } from "node:test";
import assert from "node:assert/strict";
import path from "node:path";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import fs from "node:fs";
import os from "node:os";
import { call, parseSavedVariables, parseLink, decodeCbor, TOOLS } from "../fbl.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const env = { FBL_FILE: path.join(here, "fixture", "ForeverBuddyLite.lua") };
const run = (tool, args) => call(tool, args, env);

test("parses what the client writes", () => {
  const db = parseSavedVariables('X = {\n["a"] = "q\\"uote\\\\",\n[3] = true,\n{ 1, 2, -- [2]\n},\n}\nY = 5');
  assert.equal(db.X.a, 'q"uote\\');
  assert.equal(db.X[3], true);
  assert.deepEqual(db.X[1], [1, 2]);
  assert.equal(db.Y, 5);
});

test("item links", () => {
  assert.deepEqual(parseLink("|cff0070dd|Hitem:5201::::|h[Emberstone Staff]|h|r"), { id: 5201, name: "Emberstone Staff", quality: "rare" });
});

test("characters, newest first", () => {
  const c = run("characters");
  assert.deepEqual(c.map((x) => x.character), ["Testmage-Realm", "Testrogue-Realm"]);
  assert.equal(c[0].gold, 12.35);
});

test("character by first name; unknown names list the known ones", () => {
  assert.equal(run("summary", { character: "testmage" }).level, 20);
  assert.throws(() => run("summary", { character: "nobody" }), /Known: Testmage-Realm/);
});

test("gear by slot", () => {
  const g = run("gear", { character: "Testmage" }).gear;
  assert.equal(g.main_hand.name, "Emberstone Staff");
  assert.equal(g.main_hand.durability, "80/90");
  assert.equal(g.ranged.id, 5198);
});

test("find across characters, bags and bank", () => {
  const rows = run("find", { query: "linen" });
  assert.deepEqual(rows.map((r) => [r.character, r.where, r.count]), [["Testmage-Realm", "bags", 17], ["Testrogue-Realm", "bank", 40]]);
  assert.equal(run("find", { query: "17031" })[0].name, "Rune of Teleportation");
});

test("quests and completed", () => {
  const q = run("quests", { character: "Testmage" });
  assert.equal(q.completed_count, 3);
  assert.equal(q.log[0].objectives[0], "[ ] Mosshide Gnoll slain: 6/15");
  assert.deepEqual(run("completed", { character: "Testmage", ids: "455, 999" }).quests, { 455: true, 999: false });
});

test("talent trees are split by column gaps and named by class", () => {
  const t = run("talents", { character: "Testmage" });
  assert.deepEqual(t.trees.map((x) => [x.name, x.points]), [["Arcane", 9], ["Fire", 0], ["Frost", 2]]);
  assert.deepEqual(t.trees[0].talents, ["Arcane Focus 5/5", "Improved Channeling 3/3", "Arcane Blast 1/1"]);
});

test("skills, spells, reputation, stats, mail", () => {
  assert.deepEqual(run("skills", { character: "Testmage" }).skills, ["Tailoring 112/150", "Staves 95/100", "Cloth 1/1"]);
  assert.equal(run("spells", { character: "Testmage", query: "blink" }).spells[0].id, 1953);
  assert.equal(run("reputation", { character: "Testmage" }).factions[0], "Stormwind: Friendly 2500/6000");
  assert.equal(run("stats", { character: "Testmage" }).spell_power.arcane, 24);
  assert.equal(run("mail", { character: "Testmage" }).letters[0].subject, 'A "Return" Customer');
});

// { version = 2, ["2589"] = { m = 125, h = { ["2509"] = 130 } }, ["5201"] = { m = 2500000 } } as CBOR.
const CBOR = Buffer.from([0xa3,
  0x67, ...Buffer.from("version"), 0x02,
  0x64, ...Buffer.from("2589"), 0xa2, 0x61, 0x6d, 0x18, 0x7d, 0x61, 0x68, 0xa1, 0x64, ...Buffer.from("2509"), 0x18, 0x82,
  0x64, ...Buffer.from("5201"), 0xa1, 0x61, 0x6d, 0x1a, 0x00, 0x26, 0x25, 0xa0]);

test("CBOR, as Auctionator stores prices", () => {
  assert.deepEqual(decodeCbor(CBOR), { version: 2, 2589: { m: 125, h: { 2509: 130 } }, 5201: { m: 2500000 } });
});

test("prices and bag value from an Auctionator file (bytes escaped the way the game writes them)", () => {
  // %q-style: every byte outside printable ASCII, plus " and \, as \ddd.
  const esc = [...CBOR].map((b) => (b >= 32 && b < 127 && b !== 34 && b !== 92 ? String.fromCharCode(b) : `\\${b}`)).join("");
  const file = path.join(os.tmpdir(), `fbl-auctionator-${process.pid}.lua`);
  fs.writeFileSync(file, `AUCTIONATOR_PRICE_DATABASE = {\n["__dbversion"] = 8,\n["Realm"] = "${esc}",\n}\nAUCTIONATOR_SAVEDVARS = {\n["TimeOfLastReplicateScan"] = 1794600000,\n}\n`, "latin1");
  const e = { ...env, FBL_AUCTIONATOR: file };
  const p = call("prices", { query: "linen" }, e);
  assert.equal(p.prices[0].last, "0g 1s 25c");
  assert.equal(p.prices[0].history[0].day, "2026-11-14");
  const v = call("value", { character: "Testmage" }, e);
  assert.equal(v.items[0].name, "Linen Cloth");
  assert.equal(v.items[0].total, "0g 21s 25c"); // 17 x 1s 25c
  fs.unlinkSync(file);
});

test("upgrades: skills, class and level decide what's usable; weights decide what's better", () => {
  const u = run("upgrades", { character: "Testmage" });
  assert.equal(u.role, "caster");
  assert.equal(u.tree, "Arcane");
  // Elder's Hat: 6 Sta + 7 Int = 13 over an empty head.
  assert.deepEqual(u.upgrades.head.map((r) => [r.name, r.gain, r.replaces]), [["Elder's Hat of the Eagle", 13, "(empty)"]]);
  // Fire Staff: 30 sp * 0.8 = 24, Fire damage counts for nothing in Arcane; Emberstone is 8 + 24*0.8 = 27.2: no upgrade.
  assert.equal(u.upgrades.main_hand, undefined);
  assert.deepEqual(u.not_usable, { "needs Leather": 1, class: 1, "needs Engineering 120": 1 });
  // Big Staff (req 30) appears only when looking ahead.
  assert.equal(run("upgrades", { character: "Testmage", ahead: "10" }).upgrades.main_hand[0].name, "Big Staff");
  // Weights can be overridden: Spell power alone, the Fire Staff still loses (30 vs 24... with sp=2: 60 > 48).
  assert.equal(run("upgrades", { character: "Testmage", weights: "sp=2" }).upgrades.main_hand[0].name, "Fire Staff");
  assert.deepEqual(Object.keys(run("upgrades", { character: "Testmage", budget: "0.01" }).upgrades), []);
});

test("MCP: initialize, list tools, call one", async () => {
  const child = spawn(process.execPath, [path.join(here, "..", "fbl.mjs"), "mcp"], { env: { ...process.env, ...env } });
  const lines = [];
  let buf = "";
  child.stdout.on("data", (d) => { buf += d; let n; while ((n = buf.indexOf("\n")) >= 0) { lines.push(JSON.parse(buf.slice(0, n))); buf = buf.slice(n + 1); } });
  const send = (m) => child.stdin.write(JSON.stringify(m) + "\n");
  send({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-06-18" } });
  send({ jsonrpc: "2.0", method: "notifications/initialized" });
  send({ jsonrpc: "2.0", id: 2, method: "tools/list" });
  send({ jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "gear", arguments: { character: "Testmage" } } });
  send({ jsonrpc: "2.0", id: 4, method: "tools/call", params: { name: "summary", arguments: { character: "nobody" } } });
  for (let k = 0; k < 100 && lines.length < 4; k++) await new Promise((r) => setTimeout(r, 20));
  child.kill();
  const byId = Object.fromEntries(lines.map((l) => [l.id, l]));
  assert.equal(byId[1].result.serverInfo.name, "forever-buddy-lite");
  assert.equal(byId[2].result.tools.length, Object.keys(TOOLS).length);
  assert.match(byId[3].result.content[0].text, /Emberstone Staff/);
  assert.equal(byId[4].result.isError, true);
});

// Enough CBOR to stand in for C_EncodingUtil.SerializeCBOR: maps, arrays, text, small ints.
function cbor(v) {
  const head = (major, n) => n < 24 ? [major << 5 | n] : n < 256 ? [major << 5 | 24, n]
    : n < 65536 ? [major << 5 | 25, n >> 8, n & 255] : [major << 5 | 26, n >>> 24, (n >> 16) & 255, (n >> 8) & 255, n & 255];
  if (typeof v === "number") return head(0, v);
  if (typeof v === "string") { const b = [...Buffer.from(v)]; return [...head(2, b.length), ...b]; } // the game writes byte strings
  if (Array.isArray(v)) return [...head(4, v.length), ...v.flatMap(cbor)];
  const e = Object.entries(v);
  return [...head(5, e.length), ...e.flatMap(([k, x]) => [...cbor(k), ...cbor(x)])];
}

test("AH scan stored packed (CBOR, deflated, base64) reads like the plain one", async () => {
  const { unpackAh } = await import("../fbl.mjs");
  const zlib = await import("node:zlib");
  const items = { "7357:1205": { id: 7357, name: "Elder's Hat of the Eagle", slot: "INVTYPE_HEAD", class: 4, subtype: "Cloth", buyout: 4500, listings: 2,
    tip: { 1: "+6 Stamina", 2: "+7 Intellect", 3: "Requires Level 20" } } }; // a Lua sequence that came out as a map
  const packed = zlib.deflateRawSync(Buffer.from(cbor(items))).toString("base64");
  const ah = unpackAh({ at: 5, count: 1, packed });
  assert.deepEqual(ah.items["7357:1205"].tip, ["+6 Stamina", "+7 Intellect", "Requires Level 20"]);
  assert.equal(ah.packed, undefined);
  // zlib-wrapped works too, and the old plain table passes through.
  assert.equal(unpackAh({ packed: zlib.deflateSync(Buffer.from(cbor(items))).toString("base64") }).items["7357:1205"].buyout, 4500);
  assert.equal(unpackAh({ items: { a: 1 } }).items.a, 1);

  const file = path.join(os.tmpdir(), `fbl-packed-${process.pid}.lua`);
  // The fixture may come out of git with CRLF line endings on Windows.
  fs.writeFileSync(file, fs.readFileSync(env.FBL_FILE, "utf8").replace(/\r\n/g, "\n").replace(/\["ah"\] = \{[\s\S]*?\n\t\},\n\t\["characters"\]/,
    `["ah"] = { ["at"] = 1794600000, ["complete"] = false, ["packed"] = "${packed}" },\n\t["ahError"] = "browse failed",\n\t["characters"]`));
  const u = call("upgrades", { character: "Testmage" }, { FBL_FILE: file });
  fs.unlinkSync(file);
  assert.equal(u.upgrades.head[0].name, "Elder's Hat of the Eagle");
  assert.match(u.scan_note, /stopped early/);
  assert.equal(u.scan_error, "browse failed");
});

test("flips: under its usual daily low, listed often, worth the cut; too little history says so", () => {
  // 2589 usually 200c, now 100c: a flip. 2592 now at its usual price. 2840 has one day only.
  const day = { 2509: 200, 2510: 210, 2511: 190, 2512: 100 };
  const db = { version: 2,
    2589: { m: 100, h: day, l: day, a: { 2509: 20, 2510: 20, 2511: 20, 2512: 20 } },
    2592: { m: 200, h: { 2509: 200, 2510: 200, 2511: 200, 2512: 200 }, a: { 2509: 9, 2510: 9, 2511: 9, 2512: 9 } },
    2840: { m: 10, h: { 2512: 500 }, a: { 2512: 50 } } };
  const esc = [...cbor(db)].map((b) => (b >= 32 && b < 127 && b !== 34 && b !== 92 ? String.fromCharCode(b) : `\\${String(b).padStart(3, "0")}`)).join("");
  const file = path.join(os.tmpdir(), `fbl-flips-${process.pid}.lua`);
  fs.writeFileSync(file, `AUCTIONATOR_PRICE_DATABASE = {\n["__dbversion"] = 8,\n["Realm"] = "${esc}",\n}\n`, "latin1");
  const e = { ...env, FBL_AUCTIONATOR: file };
  const f = call("flips", { min_profit: "0" }, e);
  assert.deepEqual(f.flips.map((x) => [x.id, x.name, x.share, x.profit]), [[2589, "Linen Cloth", "50%", "0g 0s 90c"]]);
  assert.equal(f.note, undefined);
  assert.match(call("flips", { min_days: "9" }, e).note, /Only 4 day/);
  fs.unlinkSync(file);
});

test("schema 2: the scan error moved under debug, and still reaches upgrades", () => {
  const file = path.join(os.tmpdir(), `fbl-schema2-${process.pid}.lua`);
  fs.writeFileSync(file, fs.readFileSync(env.FBL_FILE, "utf8")
    .replace('["version"] = 1,', '["schema"] = 2,\n\t["debug"] = { ["ah_error"] = "browse failed" },'));
  const u = call("upgrades", { character: "Testmage" }, { FBL_FILE: file });
  fs.unlinkSync(file);
  assert.equal(u.scan_error, "browse failed");
  assert.equal(u.upgrades.head[0].name, "Elder's Hat of the Eagle");
});
