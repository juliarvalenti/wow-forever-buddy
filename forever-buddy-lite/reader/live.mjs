// Leave it running: it watches Auctionator's file, and after every scan that reaches disk (/fbl market,
// or any /reload) it archives the scan, prints what's worth doing, and updates the dashboard in your browser.
//   node live.mjs [--port 4747] [--budget 5] [--min-listed 100] [--no-open] [--archive <folder>]
// The dashboard is at http://localhost:4747 (this machine only). Archived scans keep each item's cheapest
// price and quantity per scan, which Auctionator itself folds into one low/high per day.
import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import zlib from "node:zlib";
import crypto from "node:crypto";
import { loadPrices, findFiles } from "./fbl.mjs";
import { analyze, report, money } from "./market.mjs";
import { buildItems, page, openInBrowser } from "./dashboard.mjs";

const argv = process.argv.slice(2);
const flag = (k, d) => { const i = argv.indexOf(`--${k}`); return i >= 0 ? argv[i + 1] : d; };
const port = Number(flag("port", "4747"));
const opts = { budget: Number(flag("budget", "5")), minListed: Number(flag("min-listed", "100")) };
const archiveDir = flag("archive", path.join(os.homedir(), "Documents", "FBL Market", "scans"));

let state = null; // { items, stamp, opps, html, json }
const clients = new Set();
const time = () => new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });

// Every item's cheapest now and quantity, per scan, gzipped; skipped when nothing changed.
let lastHash = null;
function archive() {
  const snap = {};
  for (const data of Object.values(loadPrices().realms)) for (const [id, e] of Object.entries(data)) {
    if (id === "version" || !e?.m) continue;
    const last = Math.max(...Object.keys(e.h || {}).map(Number));
    snap[id] = [e.m, e.a?.[last] ?? 0];
  }
  const body = JSON.stringify(snap);
  const hash = crypto.createHash("sha1").update(body).digest("hex");
  if (hash === lastHash) return null;
  lastHash = hash;
  fs.mkdirSync(archiveDir, { recursive: true });
  const at = new Date();
  const file = path.join(archiveDir, `${at.toISOString().slice(0, 19).replace(/[:T]/g, "-")}.json.gz`);
  fs.writeFileSync(file, zlib.gzipSync(JSON.stringify({ at: at.toISOString(), items: snap })));
  return file;
}

let previousBuys = new Set();
function rebuild(reason) {
  const t0 = Date.now();
  const { items, stamp } = buildItems();
  const opps = analyze(opts);
  state = { items, stamp, opps, html: page(items, stamp, { opps, live: true }), json: JSON.stringify({ items, opps, stamp }) };
  const saved = archive();
  // The terminal: what's new since the last scan first, then the full report.
  const fresh = opps.buy.filter((r) => !previousBuys.has(r.id));
  previousBuys = new Set(opps.buy.map((r) => r.id));
  console.log(`\n===== ${time()} · ${reason} · ${items.length} items in ${Date.now() - t0} ms${saved ? ` · archived ${path.basename(saved)}` : ""}`);
  if (fresh.length) console.log(`NEW BUYS: ${fresh.slice(0, 6).map((r) => `${r.name} at ${money(r.now)} (usual ${money(r.usual)}, +${money(r.profit)} each)`).join("; ")}`);
  console.log(report(opps, 8));
  for (const res of clients) res.write(`event: update\ndata: ${Date.now()}\n\n`);
}

// Watch Auctionator's file and ours; the game writes both on /reload, so wait for them to settle.
const files = [...new Set([...findFiles(process.env), ...findFiles(process.env).map((f) => path.join(path.dirname(f), "Auctionator.lua"))])]
  .filter((f) => fs.existsSync(f));
let timer = null;
for (const f of files) fs.watchFile(f, { interval: 2000 }, (cur, prev) => {
  if (cur.mtimeMs === prev.mtimeMs) return;
  clearTimeout(timer);
  timer = setTimeout(() => { try { rebuild(`${path.basename(f)} changed`); } catch (e) { console.error("rebuild failed:", e.message); } }, 3000);
});

http.createServer((req, res) => {
  if (!state) { res.writeHead(503).end("starting"); return; }
  if (req.url === "/events") {
    res.writeHead(200, { "Content-Type": "text/event-stream", "Cache-Control": "no-cache", Connection: "keep-alive" });
    res.write(": hello\n\n");
    clients.add(res);
    req.on("close", () => clients.delete(res));
    return;
  }
  if (req.url === "/data.json") { res.writeHead(200, { "Content-Type": "application/json" }).end(state.json); return; }
  if (req.url === "/" || req.url.startsWith("/?")) { res.writeHead(200, { "Content-Type": "text/html; charset=utf-8" }).end(state.html); return; }
  res.writeHead(404).end();
}).listen(port, "127.0.0.1", () => {
  rebuild("started");
  console.log(`\nDashboard: http://localhost:${port}  ·  watching ${files.map((f) => path.basename(f)).join(", ")}  ·  scans archived to ${archiveDir}`);
  console.log("Leave this running. After each /fbl market save (or any /reload), it updates. Ctrl+C to stop.");
  if (!argv.includes("--no-open")) openInBrowser(`http://localhost:${port}`);
});
