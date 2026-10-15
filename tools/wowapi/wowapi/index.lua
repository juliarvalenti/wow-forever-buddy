-- The Forever compatibility index: run every addon in an AddOns folder
-- through the simulator (load, log in, play a few seconds) and classify it,
-- together with its signature status from `wowtest install`.
local toc = require("wowapi.toc")

local M = {}

M.STATUS = {
  clean = "Runs cleanly",
  errors = "Runs with errors",
  ["missing-libs"] = "Missing libraries",
  incompatible = "Incompatible",
  timeout = "Hangs",
  crash = "Crashed the check",
}

local function relMessage(msg, name)
  msg = msg:gsub("^.-AddOns/", ""):gsub("%.%./", "")
  msg = msg:gsub("\n.*", "")
  if #msg > 220 then msg = msg:sub(1, 217) .. "..." end
  return msg
end

local function isMissingLib(msg)
  return msg:find("file listed in .toc not found", 1, true) or msg:find("Cannot find a library", 1, true)
    or msg:find("cannot open ", 1, true) or msg:find("file referenced in .- not found")
end

-- Hints that point at a Forever-specific cause rather than a simulator gap.
local function foreverHints(rec, t, messages)
  local hints = {}
  local hasForever = false
  for _, i in ipairs(t.interface or {}) do if tonumber(i) == 16001 then hasForever = true end end
  if not hasForever then hints[#hints + 1] = "its .toc doesn't list interface 16001 (WoW: Forever)" end
  for _, m in ipairs(messages) do
    local ev = m:match('unknown event "([%w_]+)"')
    if ev then hints[#hints + 1] = "registers " .. ev .. ", which this client doesn't have (version check?)"; break end
  end
  return hints
end

-- Check one addon folder in-process. Returns a record table.
function M.check(dir, opts)
  opts = opts or {}
  local WoW = require("wowapi")
  local name = toc.basename(dir)
  local rec = { name = name, errors = 0, messages = {}, missingLibs = {}, hints = {} }
  local t = toc.load(dir)
  if t then
    rec.title = (t.title or name):gsub("|c%x%x%x%x%x%x%x%x", ""):gsub("|r", "")
    rec.version = t.metadata and (t.metadata.version or t.metadata.Version)
    rec.author = t.metadata and (t.metadata.author or t.metadata.Author)
    rec.notes = t.metadata and (t.metadata.notes or t.metadata.Notes)
    rec.interface = t.interface
    rec.loadOnDemand = t.loadOnDemand or nil
    rec.deps = t.deps
  end
  local started = os.clock()
  local sim = WoW.new({ quiet = true, addonPaths = { toc.dirname(dir) }, art = false })
  local ok, why = sim:LoadAddon(dir)
  if not ok and type(why) == "string" and why:find("^INCOMPATIBLE") then
    rec.status = "incompatible"
    rec.reason = why:gsub("^INCOMPATIBLE:?%s*", "")
    if rec.reason == "" then
      local md = t and t.metadata or {}
      rec.reason = "its .toc excludes WoW: Forever's game type (" .. toc.GAME_TYPE .. ")"
      if md.allowloadgametype then rec.reason = rec.reason .. "; it only allows: " .. md.allowloadgametype end
    end
    sim:ClearErrors()
    return rec
  end
  if ok then
    sim:Login()
    sim:Advance(opts.seconds or 3)
  end
  rec.seconds = os.clock() - started
  -- errors raised in other addons it loaded (dependencies) are listed apart
  local function owner(msg)
    local o = msg:match("AddOns/([^/]+)/") or msg:match("^([%w_%-!]+): ")
    if o and sim.addons[o] then return o end
    return name
  end
  local seen, libs, others = {}, {}, {}
  for _, e in ipairs(sim.errors) do
    local m = relMessage(e.message, name)
    local by = owner(e.message)
    if by ~= name then
      others[by] = (others[by] or 0) + 1
    else
      rec.errors = rec.errors + 1
      if isMissingLib(e.message) then
        local lib = e.message:match('library instance of "([^"]+)"') or e.message:match("[Ll]ibs?/([^/]+)/")
        if lib and not libs[lib] then libs[lib] = true; rec.missingLibs[#rec.missingLibs + 1] = lib end
      elseif not seen[m] and #rec.messages < 6 then
        seen[m] = true
        rec.messages[#rec.messages + 1] = m
      end
    end
  end
  table.sort(rec.missingLibs)
  local names = {}
  for n, c in pairs(others) do names[#names + 1] = string.format("%s (%d)", n, c) end
  table.sort(names)
  if #names > 0 then rec.dependencyErrors = table.concat(names, ", ") end
  local all = {}
  for _, e in ipairs(sim.errors) do all[#all + 1] = e.message end
  if t then rec.hints = foreverHints(rec, t, all) end
  if not ok then
    rec.status = "errors"
    rec.reason = "did not load: " .. tostring(why)
  elseif rec.errors == 0 then
    rec.status = "clean"
  elseif #rec.missingLibs > 0 then
    rec.status = "missing-libs"
  else
    rec.status = "errors"
  end
  sim:ClearErrors() -- recorded above; the sim is thrown away
  return rec
end

------------------------------------------------------------------ output

local function jsonString(s)
  return '"' .. s:gsub('[%c"\\]', function(c)
    local map = { ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t" }
    return map[c] or string.format("\\u%04x", c:byte())
  end) .. '"'
end

function M.json(v)
  local t = type(v)
  if t == "nil" then return "null" end
  if t == "boolean" then return tostring(v) end
  if t == "number" then return (v ~= v or v == math.huge or v == -math.huge) and "null" or string.format("%.14g", v) end
  if t == "string" then return jsonString(v) end
  if t == "table" then
    if #v > 0 or next(v) == nil then
      local out = {}
      for i = 1, #v do out[i] = M.json(v[i]) end
      return "[" .. table.concat(out, ",") .. "]"
    end
    local keys = {}
    for k in pairs(v) do keys[#keys + 1] = tostring(k) end
    table.sort(keys)
    local out = {}
    for _, k in ipairs(keys) do out[#out + 1] = jsonString(k) .. ":" .. M.json(v[k]) end
    return "{" .. table.concat(out, ",") .. "}"
  end
  return "null"
end

function M.html(index)
  local data = M.json(index):gsub("</", "<\\/")
  return (M.TEMPLATE:gsub("__DATA__", function() return data end))
end

M.TEMPLATE = [==[<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Forever Addon Index</title>
<style>
:root {
  --bg: #f6f4ef; --panel: #ffffff; --ink: #1d1b16; --muted: #6b665c; --line: #e3ded3;
  --accent: #9a6b00;
  --clean-bg: #e3f3e6; --clean-ink: #1d6b33;
  --errors-bg: #fdf0d9; --errors-ink: #8a5a00;
  --libs-bg: #e6eefb; --libs-ink: #2c4f8f;
  --bad-bg: #fbe4e2; --bad-ink: #9b2a20;
  --sig-bg: #ece9f7; --sig-ink: #4b3d8f;
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --bg: #15140f; --panel: #1f1d17; --ink: #ede8dc; --muted: #a39d90; --line: #36322a;
    --accent: #e0b24a;
    --clean-bg: #17331f; --clean-ink: #8fdca3;
    --errors-bg: #3a2c10; --errors-ink: #f0c56a;
    --libs-bg: #1a2742; --libs-ink: #9dbbf2;
    --bad-bg: #3d1a17; --bad-ink: #f3a39a;
    --sig-bg: #262142; --sig-ink: #bdb2f2;
  }
}
:root[data-theme="dark"] {
  --bg: #15140f; --panel: #1f1d17; --ink: #ede8dc; --muted: #a39d90; --line: #36322a;
  --accent: #e0b24a;
  --clean-bg: #17331f; --clean-ink: #8fdca3;
  --errors-bg: #3a2c10; --errors-ink: #f0c56a;
  --libs-bg: #1a2742; --libs-ink: #9dbbf2;
  --bad-bg: #3d1a17; --bad-ink: #f3a39a;
  --sig-bg: #262142; --sig-ink: #bdb2f2;
}
* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--ink);
  font: 15px/1.5 system-ui, -apple-system, "Segoe UI", sans-serif; }
main { max-width: 1040px; margin: 0 auto; padding: 32px 16px 64px; }
h1 { font-size: 28px; margin: 0 0 4px; letter-spacing: -0.01em; }
.sub { color: var(--muted); margin: 0 0 24px; }
.summary { display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 16px; }
.chip { border: 1px solid var(--line); background: var(--panel); color: var(--ink); border-radius: 999px;
  padding: 6px 12px; font: inherit; font-size: 13px; cursor: pointer; }
.chip[aria-pressed="true"] { border-color: var(--accent); box-shadow: inset 0 0 0 1px var(--accent); }
.chip b { font-variant-numeric: tabular-nums; }
input[type=search] { width: 100%; padding: 10px 12px; border-radius: 8px; border: 1px solid var(--line);
  background: var(--panel); color: var(--ink); font: inherit; margin-bottom: 16px; }
.list { display: grid; gap: 10px; }
.card { background: var(--panel); border: 1px solid var(--line); border-radius: 10px; padding: 14px 16px; }
.head { display: flex; flex-wrap: wrap; align-items: baseline; gap: 8px 12px; }
.name { font-weight: 650; font-size: 16px; }
.meta { color: var(--muted); font-size: 13px; }
.badges { display: flex; flex-wrap: wrap; gap: 6px; margin-left: auto; }
.badge { font-size: 12px; font-weight: 600; padding: 2px 8px; border-radius: 6px; white-space: nowrap; }
.s-clean { background: var(--clean-bg); color: var(--clean-ink); }
.s-errors { background: var(--errors-bg); color: var(--errors-ink); }
.s-missing-libs { background: var(--libs-bg); color: var(--libs-ink); }
.s-incompatible, .s-timeout, .s-crash, .sig-invalid { background: var(--bad-bg); color: var(--bad-ink); }
.sig-valid { background: var(--sig-bg); color: var(--sig-ink); }
.sig-unsigned { background: transparent; color: var(--muted); border: 1px solid var(--line); }
.detail { margin-top: 8px; font-size: 13px; color: var(--muted); }
.detail ul { margin: 4px 0 0; padding-left: 18px; }
.detail code { font: 12px/1.45 ui-monospace, SFMono-Regular, Menlo, monospace; color: var(--ink);
  overflow-wrap: anywhere; }
footer { margin-top: 32px; color: var(--muted); font-size: 13px; }
@media (max-width: 560px) { .badges { margin-left: 0; width: 100%; } }
</style>
</head>
<body>
<main>
  <h1>Forever Addon Index</h1>
  <p class="sub" id="sub"></p>
  <div class="summary" id="chips" role="group" aria-label="Filter by result"></div>
  <input type="search" id="q" placeholder="Search addons, authors, errors…" aria-label="Search">
  <div class="list" id="list"></div>
  <footer>Generated by <code>wowtest index</code>. Each addon is loaded in the WoW: Forever client simulator, logged
  in and run for a few seconds. “Missing libraries” usually means a library the packager embeds wasn't available;
  signatures show whether the files match what the author signed.</footer>
</main>
<script>
const INDEX = __DATA__;
const LABEL = { clean: "Runs cleanly", errors: "Runs with errors", "missing-libs": "Missing libraries",
  incompatible: "Incompatible", timeout: "Hangs", crash: "Crashed the check" };
const ORDER = ["clean", "errors", "missing-libs", "incompatible", "timeout", "crash"];
let filter = null;
const el = (tag, cls, text) => { const e = document.createElement(tag); if (cls) e.className = cls;
  if (text != null) e.textContent = text; return e; };
document.getElementById("sub").textContent =
  `${INDEX.addons.length} addons · interface ${INDEX.interface} · ${INDEX.generated}`;
function chips() {
  const box = document.getElementById("chips"); box.textContent = "";
  const counts = {}; INDEX.addons.forEach(a => counts[a.status] = (counts[a.status] || 0) + 1);
  const all = el("button", "chip"); all.innerHTML = `All <b>${INDEX.addons.length}</b>`;
  all.setAttribute("aria-pressed", String(filter === null)); all.onclick = () => { filter = null; render(); };
  box.append(all);
  ORDER.filter(s => counts[s]).forEach(s => {
    const b = el("button", "chip"); b.append(LABEL[s] + " "); b.append(el("b", null, counts[s]));
    b.setAttribute("aria-pressed", String(filter === s)); b.onclick = () => { filter = filter === s ? null : s; render(); };
    box.append(b);
  });
}
function card(a) {
  const c = el("article", "card");
  const h = el("div", "head");
  h.append(el("span", "name", a.title || a.name));
  const bits = [a.version && (/^\d/.test(a.version) ? "v" + a.version : a.version), a.author, a.loadOnDemand && "load on demand"].filter(Boolean);
  if (bits.length) h.append(el("span", "meta", bits.join(" · ")));
  const bs = el("div", "badges");
  bs.append(el("span", "badge s-" + a.status, LABEL[a.status] || a.status));
  const sig = (a.signature && a.signature.status) || "unsigned";
  bs.append(el("span", "badge sig-" + sig, sig === "valid" ? "Signed" : sig === "invalid" ? "Signature invalid" : "Unsigned"));
  h.append(bs); c.append(h);
  const d = el("div", "detail");
  const lines = [];
  if (a.reason) lines.push(a.reason);
  if (a.errors) lines.push(`${a.errors} Lua error${a.errors === 1 ? "" : "s"}`);
  if (a.missingLibs && a.missingLibs.length) lines.push("Missing: " + a.missingLibs.join(", "));
  (a.hints || []).forEach(x => lines.push("Forever: " + x));
  if (a.dependencyErrors) lines.push("Errors in addons it loads: " + a.dependencyErrors);
  if (a.signature && a.signature.fingerprint) lines.push("Key " + a.signature.fingerprint);
  if (lines.length) d.append(el("div", null, lines.join(" · ")));
  if (a.messages && a.messages.length) {
    const ul = el("ul"); a.messages.forEach(m => { const li = el("li"); li.append(el("code", null, m)); ul.append(li); });
    d.append(ul);
  }
  if (d.childNodes.length) c.append(d);
  return c;
}
function render() {
  chips();
  const q = document.getElementById("q").value.trim().toLowerCase();
  const list = document.getElementById("list"); list.textContent = "";
  INDEX.addons
    .filter(a => !filter || a.status === filter)
    .filter(a => !q || JSON.stringify(a).toLowerCase().includes(q))
    .forEach(a => list.append(card(a)));
  if (!list.childNodes.length) list.append(el("p", "meta", "No addons match."));
}
document.getElementById("q").addEventListener("input", render);
render();
</script>
</body>
</html>
]==]

return M
