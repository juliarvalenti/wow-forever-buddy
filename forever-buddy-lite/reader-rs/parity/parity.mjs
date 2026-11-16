#!/usr/bin/env node
// Parity check: runs the Node reader and fbl-rs on the same files and compares their output.
//   node parity/parity.mjs           the test fixture, plus synthetic Auctionator prices and ladders
//   node parity/parity.mjs --real    your own files (default env: FBL_WTF etc.), read only
// Build first: cargo build (or set FBL_RS to the binary).
// Character commands are compared as parsed JSON (key order aside); market and backtest as text.
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";

const here = path.dirname(fileURLToPath(import.meta.url));
const reader = path.join(here, "..", "..", "reader");
const exe = process.env.FBL_RS || path.join(here, "..", "target", "debug", process.platform === "win32" ? "fbl-rs.exe" : "fbl-rs");
const real = process.argv.includes("--real");
const show = process.argv.includes("--show");
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "fbl-parity-"));

// ---------------------------------------------------------------- synthetic data
const now = Math.floor(Date.now() / 1000);

// Enough CBOR for Auctionator's database: maps, text, small ints (digit keys as numbers).
function cbor(v) {
  const head = (major, n) => n < 24 ? [major << 5 | n] : n < 256 ? [major << 5 | 24, n]
    : n < 65536 ? [major << 5 | 25, n >> 8, n & 255] : [major << 5 | 26, n >>> 24, (n >> 16) & 255, (n >> 8) & 255, n & 255];
  if (typeof v === "number") return head(0, v);
  if (typeof v === "string") { const b = [...Buffer.from(v)]; return [...head(3, b.length), ...b]; }
  const e = Object.entries(v);
  return [...head(5, e.length), ...e.flatMap(([k, x]) => [...(/^\d+$/.test(k) ? head(0, +k) : cbor(k)), ...cbor(x)])];
}
const luaBytes = (bytes) => [...bytes].map((b) => (b >= 32 && b < 127 && b !== 34 && b !== 92 ? String.fromCharCode(b) : `\\${String(b).padStart(3, "0")}`)).join(""); // padded: \1 then a digit would read as \1d
const days = [2509, 2510, 2511, 2512, 2513, 2514];
const series = (f) => Object.fromEntries(days.map((d, i) => [d, f(i, i === days.length - 1)]));
const item = (m, high, low, listed) => ({ m, h: series(high), l: series(low), a: series(listed) });
const prices = {
  version: 2,
  2589: item(130, (i, t) => (t ? 130 : 100), (i, t) => (t ? 130 : 100), () => 30),           // Linen: you hold it, up 30%
  1001: item(5000, () => 10000, (i, t) => (t ? 5000 : 10000), () => 500),                     // a stray cheap listing
  1002: item(7000, (i, t) => (t ? 8000 : 10000), (i, t) => (t ? 7000 : 10000), () => 300),   // the whole market dipped
  1003: item(1300, () => 1300, (i) => 1000 + i * 60, (i) => 1000 - i * 100),                  // squeeze
  1004: item(800, () => 1000, (i) => 1000 - i * 40, (i) => 200 + i * 40),                      // flood
  1005: item(600, () => 1000, (i, t) => (t ? 600 : 1000), () => 300),                         // swingy
  1006: item(5000, () => 10000, (i, t) => (t ? 5000 : 10000), () => 500),                     // hidden
  1007: item(2100, () => 3900, () => 3800, () => 200),
  1008: item(5, () => 9, () => 5, () => 2000),                                                  // quiet and cheap
};
const ladders = {
  1001: [{ at: now - 600, total: 53, levels: "5000:3;9000:50" }],
  1002: [{ at: now - 3600, total: 60, levels: "7000:10;7010:50" }],
  "1007:5": [{ at: now - 5 * 3600, total: 65, levels: "2100:5;3800:20;3900:40" }],
  2589: [{ at: now - 1200, total: 30, levels: "120:5*;125:20;130:5" }],
  1009: [{ at: now - 7200 - 60, total: 0, levels: "" }],
};
const ladderLua = Object.entries(ladders).map(([k, log]) => `\t\t["${k}"] = {\n${log.map((s) =>
  `\t\t\t{ ["at"] = ${s.at}, ["total"] = ${s.total}, ["levels"] = "${s.levels}" },`).join("\n")}\n\t\t},`).join("\n");

function syntheticEnv() {
  const fixture = fs.readFileSync(path.join(reader, "test", "fixture", "ForeverBuddyLite.lua"), "utf8").replace(/\r\n/g, "\n");
  const fbl = path.join(tmp, "ForeverBuddyLite.lua");
  fs.writeFileSync(fbl, fixture.replace('["version"] = 1,', `["version"] = 1,\n\t["ladders"] = {\n${ladderLua}\n\t},`));
  const auc = path.join(tmp, "Auctionator.lua");
  fs.writeFileSync(auc, `AUCTIONATOR_PRICE_DATABASE = {\n["__dbversion"] = 8,\n["Realm"] = "${luaBytes(cbor(prices))}",\n}\nAUCTIONATOR_SAVEDVARS = {\n["TimeOfLastBrowseScan"] = ${now - 300},\n}\n`, "latin1");
  const hidden = path.join(tmp, "hidden.json");
  fs.writeFileSync(hidden, JSON.stringify({ items: { 1006: { name: "Hidden Thing", at: "2026-11-16T00:00:00.000Z" } } }));
  return { FBL_FILE: fbl, FBL_AUCTIONATOR: auc, FBL_HIDDEN: hidden, QUESTIEDB_DIR: path.join(tmp, "no-questie") };
}

// ---------------------------------------------------------------- running both
// Real files: a copy of the hidden list, so /hide checks never touch yours.
function realEnv() {
  const mine = path.join(os.homedir(), "Documents", "FBL Market", "hidden.json");
  const copy = path.join(tmp, "hidden.json");
  if (fs.existsSync(mine)) fs.copyFileSync(mine, copy);
  return { FBL_HIDDEN: copy };
}
const extra = real ? realEnv() : syntheticEnv();
const envFor = { ...process.env, ...extra };
const runNode = (script, args) => spawnSync(process.execPath, [path.join(reader, script), ...args], { env: envFor, encoding: "utf8", maxBuffer: 1 << 28 });
const runRust = (args) => spawnSync(exe, args, { env: envFor, encoding: "utf8", maxBuffer: 1 << 28 });

let failures = 0;
const report = (name, ok, detail) => {
  console.log(`${ok ? "same" : "DIFF"}  ${name}`);
  if (!ok) { failures++; if (detail) console.log(detail); }
};

// Ages ("5 min ago") are computed at run time; drop them so a minute boundary can't cause a diff.
const noAges = (v) => (Array.isArray(v) ? v.map(noAges) : v && typeof v === "object"
  ? Object.fromEntries(Object.entries(v).filter(([k]) => k !== "age").map(([k, x]) => [k, noAges(x)])) : v);

function firstDiff(a, b, at = "") {
  if (isDeepStrictEqual(a, b)) return null;
  if (a && b && typeof a === "object" && typeof b === "object") {
    for (const k of new Set([...Object.keys(a), ...Object.keys(b)])) {
      const d = firstDiff(a[k], b[k], `${at}.${k}`);
      if (d) return d;
    }
  }
  return `${at || "(top)"}: node ${JSON.stringify(a)?.slice(0, 300)} | rust ${JSON.stringify(b)?.slice(0, 300)}`;
}

function compareTool(label, args) {
  const n = runNode("fbl.mjs", args), r = runRust(args);
  if (n.status !== 0 || r.status !== 0) {
    const same = n.status !== 0 && r.status !== 0 && n.stderr.trim() === r.stderr.trim();
    return report(label, same, `  node (${n.status}): ${n.stderr.trim().slice(0, 300)}\n  rust (${r.status}): ${r.stderr.trim().slice(0, 300)}`);
  }
  const a = noAges(JSON.parse(n.stdout)), b = noAges(JSON.parse(r.stdout));
  report(label, isDeepStrictEqual(a, b), "  " + firstDiff(a, b));
}

// market.mjs and backtest.mjs are `fbl-rs market` and `fbl-rs backtest`.
function compareText(label, script, args) {
  const n = runNode(script, args), r = runRust([script.replace(".mjs", ""), ...args]);
  const same = n.stdout === r.stdout && n.status === r.status;
  if (process.argv.includes("--show")) console.log(r.stdout);
  let detail = "";
  if (!same) {
    const nl = n.stdout.split("\n"), rl = r.stdout.split("\n");
    const i = nl.findIndex((l, k) => l !== rl[k]);
    detail = `  first difference at line ${i + 1}:\n  node: ${nl[i]}\n  rust: ${rl[i]}\n  (status ${n.status}/${r.status}${n.stderr ? `, node stderr: ${n.stderr.trim().slice(0, 200)}` : ""}${r.stderr ? `, rust stderr: ${r.stderr.trim().slice(0, 200)}` : ""})`;
  }
  report(label, same, detail);
}

if (!fs.existsSync(exe)) { console.error(`No fbl-rs at ${exe}: run cargo build first (or set FBL_RS).`); process.exit(2); }

const chars = JSON.parse(runNode("fbl.mjs", ["characters"]).stdout || "[]").map((c) => c.character);
const toolRuns = [["characters"], ["find", "linen"], ["find", "17031"], ["trainers"], ["prices", "linen"], ["flips"], ["flips", "--min_profit", "0"],
  ["completed", chars[0] || "", "455", "999"]];
for (const c of chars) for (const t of ["summary", "stats", "gear", "items", "quests", "completed", "skills", "talents", "spells", "reputation",
  "lockouts", "buffs", "mail", "value", "upgrades", "recipes", "raw"]) toolRuns.push([t, c]);
for (const c of chars.slice(0, 2)) toolRuns.push(["upgrades", c, "--ahead", "10"], ["upgrades", c, "--weights", "sp=2,agi=0"], ["upgrades", c, "--source", "items"]);
toolRuns.push(["summary", "nobody"], ["find"]);
for (const args of toolRuns) compareTool(args.join(" ") || "(none)", args);

compareText("market", "market.mjs", []);
compareText("market --budget 1 --min-listed 20 --top 3", "market.mjs", ["--budget", "1", "--min-listed", "20", "--top", "3"]);
compareText("backtest", "backtest.mjs", []);
compareText("backtest --min-listed 10 --min 1 --max 100000", "backtest.mjs", ["--min-listed", "10", "--min", "1", "--max", "100000"]);

// ---------------------------------------------------------------- the other scripts
// Same text, with the snipe list's random tag blanked out.
function compareScript(label, script, args, rustCommand) {
  const n = runNode(script, args), r = runRust([rustCommand, ...args]);
  const tagless = (s) => s.replace(/FBL Snipes [0-9a-f]{6}/g, "FBL Snipes ######");
  // Both refusing (no skills, no database) counts as the same; the messages differ.
  const bothFail = n.status !== 0 && r.status !== 0;
  const same = bothFail || (n.status === r.status && tagless(n.stdout) === tagless(r.stdout) && n.stderr.trim() === r.stderr.trim());
  if (process.argv.includes("--show")) console.log(r.stdout, r.stderr);
  const nl = (n.stdout + n.stderr).split("\n"), rl = (r.stdout + r.stderr).split("\n");
  const i = nl.findIndex((l, k) => tagless(l) !== tagless(rl[k] ?? ""));
  report(label, same, `  line ${i + 1}:\n  node: ${nl[i]}\n  rust: ${rl[i]}`);
}
// skillup.mjs reads its recipes from reader/data; the crate now carries them.
const nodeData = path.join(reader, "data");
const hadData = fs.existsSync(path.join(nodeData, "engineering.json"));
if (!hadData) { fs.mkdirSync(nodeData, { recursive: true }); fs.copyFileSync(path.join(here, "..", "data", "engineering.json"), path.join(nodeData, "engineering.json")); }
compareScript("snipes", "snipes.mjs", [], "snipes");
compareScript("snipes --cap 1 --min-listed 50 --count 5", "snipes.mjs", ["--cap", "1", "--min-listed", "50", "--count", "5"], "snipes");
for (const c of chars) {
  compareScript(`skillup ${c}`, "skillup.mjs", [c], "skillup");
  compareScript(`skillup ${c} --to 150 --craft 0`, "skillup.mjs", [c, "engineering", "--to", "150", "--craft", "0"], "skillup");
  compareScript(`bis ${c}`, "bis.mjs", [c], "bis");
  if (real) {
    compareScript(`questxp ${c}`, "questxp.mjs", [c], "questxp");
    compareScript(`questxp ${c} 20 30`, "questxp.mjs", [c, "20", "30"], "questxp");
  }
}
if (real && process.env.PARITY_CHAIN) compareScript("questxp --chain", "questxp.mjs", [chars[0], "--chain", process.env.PARITY_CHAIN], "questxp");
if (!hadData) fs.rmSync(nodeData, { recursive: true, force: true });

// ---------------------------------------------------------------- the live server
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function fetchUntil(url, tries = 100) {
  for (let k = 0; k < tries; k++) {
    try { const r = await fetch(url); if (r.ok) return r; } catch { /* not up yet */ }
    await sleep(200);
  }
  throw new Error(`no answer from ${url}`);
}
const archive = path.join(tmp, "scans");
const nodeLive = spawn(process.execPath, [path.join(reader, "live.mjs"), "--port", "4791", "--no-open", "--archive", path.join(archive, "node")], { env: envFor, stdio: "ignore" });
const rustLive = spawn(exe, ["live", "--port", "4792", "--no-open", "--archive", path.join(archive, "rust")], { env: envFor, stdio: "ignore" });
try {
  const [a, b] = await Promise.all([fetchUntil("http://127.0.0.1:4791/data.json"), fetchUntil("http://127.0.0.1:4792/data.json")]);
  const [n, r] = [await a.json(), await b.json()];
  // Items are sorted by name with the platform's collation in Node; compare them by id.
  const byId = (items) => [...items].sort((x, y) => x.id - y.id);
  report("live /data.json items", isDeepStrictEqual(byId(n.items), byId(r.items)), "  " + firstDiff(byId(n.items), byId(r.items)));
  report("live /data.json opps", isDeepStrictEqual(n.opps, r.opps), "  " + firstDiff(n.opps, r.opps));
  report("live /data.json stamp", n.stamp === r.stamp, `  ${n.stamp} vs ${r.stamp}`);
  const page = await fetch("http://127.0.0.1:4792/");
  report("live serves the page", page.ok && (page.headers.get("content-type") || "").startsWith("text/html"));
  const js = await fetch("http://127.0.0.1:4792/lib/api.js");
  report("live serves scripts as JavaScript", js.ok && (js.headers.get("content-type") || "").startsWith("text/javascript"));
  for (const bad of ["/../live.mjs", "/%2e%2e/live.mjs", "/..%5Clive.mjs", "/nope.js"]) {
    const res = await fetch("http://127.0.0.1:4792" + bad);
    report(`live refuses ${bad}`, res.status === 404, `  status ${res.status}`);
  }
  // /hide rebuilds and pushes an update.
  const events = await fetch("http://127.0.0.1:4792/events");
  const reader = events.body.getReader();
  const first = r.opps.buy[0] || r.opps.check[0];
  if (first) {
    const hid = await fetch("http://127.0.0.1:4792/hide", { method: "POST", body: JSON.stringify({ id: first.id, name: first.name, on: true }) });
    let got = "";
    for (let k = 0; k < 20 && !got.includes("event: update"); k++) got += new TextDecoder().decode((await reader.read()).value);
    const after = await (await fetch("http://127.0.0.1:4792/data.json")).json();
    report("live /hide hides and pushes an update", hid.ok && got.includes("event: update") && after.opps.hidden.some((h) => h.id === first.id));
    await fetch("http://127.0.0.1:4792/hide", { method: "POST", body: JSON.stringify({ id: first.id, on: false }) });
  }
  reader.cancel();
  const bad = await fetch("http://127.0.0.1:4792/hide", { method: "POST", body: "{\"id\":\"x\"}" });
  report("live /hide rejects a bad id", bad.status === 400);
  const archived = fs.readdirSync(path.join(archive, "rust"));
  report("live archives the scan", archived.length >= 1 && archived.every((f) => f.endsWith(".json.gz")));
} finally {
  nodeLive.kill();
  rustLive.kill();
}

fs.rmSync(tmp, { recursive: true, force: true });
console.log(failures ? `\n${failures} difference(s)` : "\nall the same");
process.exit(failures ? 1 : 0);
