// Writes the addon's build hash into Core.lua: the first 7 hex characters of a
// SHA-256 over every file in ForeverBuddyLite/ (sorted by name, line endings as LF,
// the ns.build line itself left out), so the same code always gets the same hash.
//   node stamp.mjs           stamp it; prints the hash
//   node stamp.mjs --check   exit 1 if Core.lua's hash isn't the code's
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const dir = path.join(path.dirname(fileURLToPath(import.meta.url)), "ForeverBuddyLite");
const BUILD = /^ns\.build = "[^"]*"$/m;

export function buildHash() {
  const h = crypto.createHash("sha256");
  for (const f of fs.readdirSync(dir).filter((f) => /\.(lua|toc|xml)$/.test(f)).sort()) {
    const text = fs.readFileSync(path.join(dir, f), "utf8").replace(/\r\n/g, "\n").replace(BUILD, 'ns.build = ""');
    h.update(`${f}\n${text}\n`);
  }
  return h.digest("hex").slice(0, 7);
}

export function stamp() {
  const core = path.join(dir, "Core.lua");
  const hash = buildHash();
  const text = fs.readFileSync(core, "utf8");
  if (!BUILD.test(text)) throw new Error("Core.lua has no ns.build line");
  fs.writeFileSync(core, text.replace(BUILD, `ns.build = "${hash}"`));
  return hash;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv.includes("--check")) {
    const want = buildHash();
    const have = BUILD.exec(fs.readFileSync(path.join(dir, "Core.lua"), "utf8"))?.[0];
    console.log(have, "code:", want);
    process.exit(have === `ns.build = "${want}"` ? 0 : 1);
  }
  console.log(stamp());
}
