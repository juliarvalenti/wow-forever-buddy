// Serves a built frontend with the app's real Content-Security-Policy header
// (from src-tauri/tauri.conf.json), so a browser check catches anything the
// CSP would block in the Tauri window. Unknown paths fall back to index.html.
//
// Usage: node scripts/serve-csp.mjs <dir> [port]
import fs from "node:fs";
import http from "node:http";
import path from "node:path";

const root = path.resolve(process.argv[2] ?? "dist-mock");
const port = Number(process.argv[3] ?? 4173);
const conf = new URL("../src-tauri/tauri.conf.json", import.meta.url);
const csp = JSON.parse(fs.readFileSync(conf)).app.security.csp;
const types = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".css": "text/css",
  ".woff2": "font/woff2",
  ".woff": "font/woff",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".webp": "image/webp",
};

http
  .createServer((req, res) => {
    let p = path.join(root, decodeURIComponent(req.url.split("?")[0]));
    if (!p.startsWith(root) || !fs.existsSync(p) || fs.statSync(p).isDirectory()) {
      p = path.join(root, "index.html");
    }
    res.writeHead(200, {
      "Content-Type": types[path.extname(p)] ?? "application/octet-stream",
      "Content-Security-Policy": csp,
    });
    fs.createReadStream(p).pipe(res);
  })
  .listen(port, () => console.log(`serving ${root} on :${port} under the app CSP`));
