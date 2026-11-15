// Stamps the build hash and copies the addon into the game's AddOns folder,
// removing files there that the addon no longer has.
//   node install.mjs [AddOns folder]   (default: FBL_ADDONS, else the usual Forever install)
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { stamp } from "./stamp.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const src = path.join(here, "ForeverBuddyLite");
const addons = process.argv[2] || process.env.FBL_ADDONS
  || "C:/Program Files (x86)/World of Warcraft/_forever_/Interface/AddOns";
const dest = path.join(addons, "ForeverBuddyLite");

const hash = stamp();
fs.mkdirSync(dest, { recursive: true });
const files = fs.readdirSync(src);
for (const f of fs.readdirSync(dest)) if (!files.includes(f)) fs.rmSync(path.join(dest, f), { recursive: true });
for (const f of files) fs.cpSync(path.join(src, f), path.join(dest, f), { recursive: true });
const toc = fs.readFileSync(path.join(src, "ForeverBuddyLite.toc"), "utf8");
console.log(`installed ForeverBuddy Lite ${/## Version: (\S+)/.exec(toc)?.[1] ?? "?"} · ${hash} to ${dest}`);
console.log("A file added or removed needs a full game restart; otherwise /reload.");
