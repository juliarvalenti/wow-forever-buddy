// The market watcher. Leave it running: after every price scan or save that reaches disk (Save
// prices & reload, /fbl r, any /reload) it archives the scan, prints what's worth doing, and pushes
// an update to the dashboard.
//
//   node live.mjs [--port 4747] [--budget 5] [--min-listed 100] [--no-open] [--archive <folder>]
//
// Dashboard: http://localhost:4747 (this machine only). The page lives in web/; this serves it plus:
//   GET  /data.json   { items, opps, stamp }: the catalog and market.mjs's analysis
//   GET  /events      server-sent events: "update" after each rebuild
//   POST /hide        { id, name, on }: hide an item from suggestions, or bring it back
//
// Archived scans keep each item's cheapest price and quantity per scan, which Auctionator itself
// folds into one low/high per day.

import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import zlib from "node:zlib";
import crypto from "node:crypto";
import { execFile } from "node:child_process";
import { fileURLToPath } from "node:url";
import { loadPrices, findFiles } from "./fbl.mjs";
import { analyze, report, money, setHidden } from "./market.mjs";
import { buildItems } from "./catalog.mjs";

// ---- Options -----------------------------------------------------------------------------------

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : fallback;
};
const PORT = Number(option("port", "4747"));
const ANALYSIS = { budget: Number(option("budget", "5")), minListed: Number(option("min-listed", "100")) };
const ARCHIVE_DIR = option("archive", path.join(os.homedir(), "Documents", "FBL Market", "scans"));
const WEB_DIR = path.join(path.dirname(fileURLToPath(import.meta.url)), "web");
const SETTLE_MS = 3000; // the game writes several files on /reload; wait for them all

// ---- State -------------------------------------------------------------------------------------

let current = null; // { json } once built
const listeners = new Set(); // open /events responses
let lastArchiveHash = null;
let previousBuys = new Set();
let previousGaps = new Set();

const clock = () => new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });

// ---- Archive -----------------------------------------------------------------------------------

// Every item's cheapest price and quantity in this scan, gzipped, one file per scan. Skipped when
// nothing changed since the last one.
function archiveScan() {
  const snapshot = {};
  for (const realm of Object.values(loadPrices().realms)) {
    for (const [id, entry] of Object.entries(realm)) {
      if (id === "version" || !entry?.m) continue;
      const lastDay = Math.max(...Object.keys(entry.h || {}).map(Number));
      snapshot[id] = [entry.m, entry.a?.[lastDay] ?? 0];
    }
  }
  const hash = crypto.createHash("sha1").update(JSON.stringify(snapshot)).digest("hex");
  if (hash === lastArchiveHash) return null;
  lastArchiveHash = hash;

  fs.mkdirSync(ARCHIVE_DIR, { recursive: true });
  const at = new Date();
  const file = path.join(ARCHIVE_DIR, `${at.toISOString().slice(0, 19).replace(/[:T]/g, "-")}.json.gz`);
  fs.writeFileSync(file, zlib.gzipSync(JSON.stringify({ at: at.toISOString(), items: snapshot })));
  return file;
}

// ---- Rebuild -----------------------------------------------------------------------------------

function rebuild(reason) {
  const started = Date.now();
  const { items, stamp } = buildItems();
  const opps = analyze(ANALYSIS);
  current = { json: JSON.stringify({ items, opps, stamp }) };
  const archived = archiveScan();

  console.log(
    `\n===== ${clock()} · ${reason} · ${items.length} items in ${Date.now() - started} ms` +
      (archived ? ` · archived ${path.basename(archived)}` : ""),
  );
  printNews(opps);
  console.log(report(opps, 8));

  for (const response of listeners) response.write(`event: update\ndata: ${Date.now()}\n\n`);
}

// What's new since the last rebuild, above the full report.
function printNews(opps) {
  const gapKey = (r) => `${r.key}@${r.at}`;
  const newGaps = opps.gaps.filter((r) => !previousGaps.has(gapKey(r)));
  const newBuys = opps.buy.filter((r) => !previousBuys.has(r.id));
  previousGaps = new Set(opps.gaps.map(gapKey));
  previousBuys = new Set(opps.buy.map((r) => r.id));

  if (newGaps.length) {
    const lines = newGaps.slice(0, 6).map((r) => `${r.name}: buy ${r.qty} up to ${money(r.upTo)}, relist ${money(r.relist)} (+${money(r.profit)})`);
    console.log(`NEW GAPS: ${lines.join("; ")}`);
  }
  if (newBuys.length) {
    const lines = newBuys.slice(0, 6).map((r) => `${r.name} at ${money(r.now)} (usual ${money(r.usual)}, +${money(r.profit)} each)`);
    console.log(`NEW BUYS: ${lines.join("; ")}`);
  }
}

// ---- Watch the game's files --------------------------------------------------------------------

const ourFiles = findFiles(process.env);
const watched = [...new Set([...ourFiles, ...ourFiles.map((f) => path.join(path.dirname(f), "Auctionator.lua"))])].filter((f) =>
  fs.existsSync(f),
);

let settleTimer = null;
for (const file of watched) {
  fs.watchFile(file, { interval: 2000 }, (now, before) => {
    if (now.mtimeMs === before.mtimeMs) return;
    clearTimeout(settleTimer);
    settleTimer = setTimeout(() => {
      try {
        rebuild(`${path.basename(file)} changed`);
      } catch (error) {
        console.error("rebuild failed:", error.message);
      }
    }, SETTLE_MS);
  });
}

// ---- HTTP --------------------------------------------------------------------------------------

const CONTENT_TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
};

// A file from web/, or null. Never anything outside it.
function webFile(urlPath) {
  const relative = urlPath === "/" ? "index.html" : decodeURIComponent(urlPath).replace(/^\/+/, "");
  const file = path.resolve(WEB_DIR, relative);
  if (!file.startsWith(WEB_DIR + path.sep) || !CONTENT_TYPES[path.extname(file)]) return null;
  return fs.existsSync(file) && fs.statSync(file).isFile() ? file : null;
}

function readBody(request, limit = 10000) {
  return new Promise((resolve, reject) => {
    let body = "";
    request.on("data", (chunk) => {
      body += chunk;
      if (body.length > limit) request.destroy(new Error("too large"));
    });
    request.on("end", () => resolve(body));
    request.on("error", reject);
  });
}

async function handleHide(request, response) {
  try {
    const { id, name, on } = JSON.parse(await readBody(request));
    if (!Number.isInteger(id)) throw new Error("bad id");
    setHidden(id, typeof name === "string" ? name.slice(0, 100) : null, Boolean(on));
    rebuild(`${on ? "hid" : "unhid"} ${name || id}`);
    response.writeHead(200, { "Content-Type": "application/json" }).end("{}");
  } catch (error) {
    response.writeHead(400).end(error.message);
  }
}

function handleEvents(request, response) {
  response.writeHead(200, { "Content-Type": "text/event-stream", "Cache-Control": "no-cache", Connection: "keep-alive" });
  response.write(": hello\n\n");
  listeners.add(response);
  request.on("close", () => listeners.delete(response));
}

const server = http.createServer((request, response) => {
  const { pathname } = new URL(request.url, "http://localhost");
  if (!current) return response.writeHead(503).end("starting");
  if (pathname === "/events") return handleEvents(request, response);
  if (pathname === "/hide" && request.method === "POST") return handleHide(request, response);
  if (pathname === "/data.json") {
    return response.writeHead(200, { "Content-Type": "application/json", "Cache-Control": "no-store" }).end(current.json);
  }
  const file = webFile(pathname);
  if (!file) return response.writeHead(404).end("not found");
  response.writeHead(200, { "Content-Type": CONTENT_TYPES[path.extname(file)], "Cache-Control": "no-cache" });
  fs.createReadStream(file).pipe(response);
});

function openInBrowser(url) {
  const opener = process.platform === "win32" ? "explorer.exe" : process.platform === "darwin" ? "open" : "xdg-open";
  execFile(opener, [url]);
}

server.listen(PORT, "127.0.0.1", () => {
  rebuild("started");
  console.log(`\nDashboard: http://localhost:${PORT}  ·  watching ${watched.map((f) => path.basename(f)).join(", ")}  ·  scans archived to ${ARCHIVE_DIR}`);
  console.log("Leave this running. After each /fbl market save (or any /reload), it updates. Ctrl+C to stop.");
  if (!args.includes("--no-open")) openInBrowser(`http://localhost:${PORT}`);
});
