-- Compare the real client (SimCheck's SavedVariables from the game) with the
-- simulator running the same SimCheck addon, and optionally import the real
-- facts (wowapi/data/forever_client.lua) so every future sim uses them.
local M = {}

M.ADDON_DIR = "addons/SimCheck"
M.IMPORT_PATH = "wowapi/data/forever_client.lua"

-- Read SimCheck.lua (a SavedVariables file) without running anything else.
function M.loadSaved(path)
  local chunk, err = loadfile(path)
  if not chunk then return nil, err end
  local env = {}
  setfenv(chunk, env)
  local ok, e = pcall(chunk)
  if not ok then return nil, e end
  local db = env.SimCheckDB
  if type(db) ~= "table" or type(db.result) ~= "table" then
    return nil, path .. " has no SimCheck results yet (run /simcheck in game, then /reload or log out)"
  end
  return db.result
end

-- Run SimCheck inside a fresh simulator; returns its result and the sim.
function M.runInSim(addonDir)
  local WoW = require("wowapi")
  local sim = WoW.new({ quiet = true })
  local ok, why = sim:LoadAddon(addonDir or M.ADDON_DIR)
  if not ok then error("could not load SimCheck in the simulator: " .. tostring(why), 0) end
  sim:Login()
  sim:Advance(16)
  for _ = 1, 3000 do
    local db = sim:Get("SimCheckDB")
    if db and db.result then return db.result, sim end
    sim:Advance(0.05)
  end
  error("SimCheck did not finish in the simulator", 0)
end

------------------------------------------------------------------ diff helpers

local function sortedKeys(t)
  local out = {}
  for k in pairs(t or {}) do out[#out + 1] = k end
  table.sort(out, function(a, b) return tostring(a) < tostring(b) end)
  return out
end

local function show(v)
  if type(v) == "table" then
    local parts = {}
    for _, x in ipairs(v) do parts[#parts + 1] = tostring(x) end
    return "{" .. table.concat(parts, ", ") .. "}"
  end
  if type(v) == "string" then return string.format("%q", v) end
  return tostring(v)
end

local function same(a, b, tol)
  if type(a) == "number" and type(b) == "number" then return math.abs(a - b) <= (tol or 0) end
  if type(a) == "table" and type(b) == "table" then
    for k, v in pairs(a) do if not same(v, b[k], tol) then return false end end
    for k in pairs(b) do if a[k] == nil then return false end end
    return true
  end
  return a == b
end

local TYPE_NAMES = { f = "functions", t = "tables", s = "strings", n = "numbers", b = "booleans", w = "frames", u = "userdata" }

------------------------------------------------------------------ compare
-- Returns a list of sections { title, rows = {string}, count }.
function M.compare(real, simr, sim)
  local sections = {}
  local function section(title, rows, total, note)
    sections[#sections + 1] = { title = title, rows = rows, count = total or #rows, note = note }
  end

  -- client identity
  do
    local rows = {}
    local rc, sc = real.client or {}, simr.client or {}
    for _, k in ipairs({ "version", "build", "date", "interface", "buildType", "locale", "game", "family", "projectId",
      "expansion", "serverExpansion", "expansionCurrent", "maxLevel" }) do
      if not same(rc[k], sc[k]) then rows[#rows + 1] = string.format("%s: real %s, simulator %s", k, show(rc[k]), show(sc[k])) end
    end
    if not same(rc.gameTypes, sc.gameTypes) then
      rows[#rows + 1] = string.format("game type ([AllowLoadGameType]): real %s, simulator %s", show(rc.gameTypes), show(sc.gameTypes))
    end
    for k, v in pairs(rc.projects or {}) do
      if (sc.projects or {})[k] ~= v then rows[#rows + 1] = string.format("%s: real %s, simulator %s", k, show(v), show((sc.projects or {})[k])) end
    end
    section("Client identity", rows)
  end

  -- behaviours
  do
    local rows = {}
    local rb, sb = real.behaviours or {}, simr.behaviours or {}
    for _, k in ipairs(sortedKeys(rb)) do
      if not same(rb[k], sb[k]) then rows[#rows + 1] = string.format("%s: real %s, simulator %s", k, show(rb[k]), show(sb[k])) end
    end
    for _, k in ipairs(sortedKeys(real.behaviourErrors)) do
      if not (simr.behaviourErrors or {})[k] then
        rows[#rows + 1] = string.format("%s: errors in the real client (%s), works in the simulator", k, real.behaviourErrors[k])
      end
    end
    for _, k in ipairs(sortedKeys(simr.behaviourErrors)) do
      if not (real.behaviourErrors or {})[k] then
        rows[#rows + 1] = string.format("%s: errors in the simulator (%s), works in the real client", k, simr.behaviourErrors[k])
      end
    end
    section("Behaviour", rows)
  end

  -- layout
  do
    local rows = {}
    for _, k in ipairs(sortedKeys(real.layout)) do
      local a, b = real.layout[k], (simr.layout or {})[k]
      if not same(a, b, 0.6) then
        rows[#rows + 1] = string.format("%s: real rect %s, simulator %s", k, show(a), show(b))
      end
    end
    section("Layout (rects relative to a test frame: left, bottom, width, height)", rows)
  end

  -- text metrics
  do
    local rows, ratios = {}, {}
    local rf, sf = (real.text or {}).fonts or {}, (simr.text or {}).fonts or {}
    for _, font in ipairs(sortedKeys(rf)) do
      local a, b = rf[font], sf[font]
      if b then
        for i, w in ipairs(a.widths or {}) do
          local sw = b.widths and b.widths[i]
          if sw and sw > 0 and w > 0 then ratios[#ratios + 1] = w / sw end
        end
        if not same(a.height, b.height, 0.6) then
          rows[#rows + 1] = string.format("%s line height: real %s, simulator %s", font, show(a.height), show(b.height))
        end
        if a.wrapLines ~= b.wrapLines then
          rows[#rows + 1] = string.format("%s wrapped lines at 120px: real %s, simulator %s", font, show(a.wrapLines), show(b.wrapLines))
        end
      else
        rows[#rows + 1] = font .. ": font object missing in the simulator"
      end
    end
    local note
    if #ratios > 0 then
      local sum = 0
      for _, x in ipairs(ratios) do sum = sum + x end
      local avg = sum / #ratios
      note = string.format("text width: real / simulator = %.3f on average over %d samples", avg, #ratios)
      if math.abs(avg - 1) > 0.05 then
        rows[#rows + 1] = string.format("text widths are off by %.0f%% on average (simulator factor 0.52 -> %.3f)",
          (avg - 1) * 100, 0.52 * avg)
      end
    end
    section("Text metrics", rows, nil, note)
  end

  -- events
  do
    local rows = {}
    local simInvalid = {}
    for _, e in ipairs((simr.events or {}).invalid or {}) do simInvalid[e] = true end
    local realInvalid = {}
    for _, e in ipairs((real.events or {}).invalid or {}) do
      realInvalid[e] = true
      if not simInvalid[e] then rows[#rows + 1] = e .. ": doesn't exist in the real client, the simulator accepts it" end
    end
    for _, e in ipairs((simr.events or {}).invalid or {}) do
      if not realInvalid[e] then rows[#rows + 1] = e .. ": exists in the real client, the simulator rejects it" end
    end
    section("Events", rows)
  end

  -- login events
  do
    local rows = {}
    local rl, sl = real.login or {}, simr.login or {}
    local simSeen = {}
    for _, e in ipairs(sl.order or {}) do simSeen[e] = true end
    local missing = {}
    for _, e in ipairs(rl.order or {}) do if not simSeen[e] then missing[#missing + 1] = e end end
    if #missing > 0 then
      rows[#rows + 1] = "fired at login in the real client, never by the simulator: " .. table.concat(missing, ", ")
    end
    local function core(seq)
      local out = {}
      local keep = { ["ADDON_LOADED:self"] = true, VARIABLES_LOADED = true, PLAYER_LOGIN = true, PLAYER_ENTERING_WORLD = true,
        SPELLS_CHANGED = true, LOADING_SCREEN_DISABLED = true, UPDATE_BINDINGS = true, PLAYER_ALIVE = true }
      for _, e in ipairs(seq or {}) do if keep[e] then out[#out + 1] = e end end
      return table.concat(out, " > ")
    end
    local a, b = core(rl.sequence), core(sl.sequence)
    if a ~= b then rows[#rows + 1] = "login order: real " .. a .. "; simulator " .. b end
    section("Login events", rows)
  end

  -- globals, looked up in the simulator (which creates many lazily)
  do
    local env = sim.env
    local missingBy, typeDiff = {}, {}
    local total = 0
    for _, name in ipairs(sortedKeys(real.globals)) do
      local code = real.globals[name]
      local v = env[name]
      local kind = code:sub(1, 1)
      if v == nil then
        missingBy[kind] = missingBy[kind] or {}
        table.insert(missingBy[kind], name)
        total = total + 1
      else
        local t = type(v)
        local simKind = (t == "function" and "f") or (t == "string" and "s") or (t == "number" and "n")
          or (t == "boolean" and "b") or (t == "table" and (sim.widgetState[v] and "w" or "t")) or "u"
        if simKind ~= kind and not (kind == "w" and simKind == "t") then
          typeDiff[#typeDiff + 1] = string.format("%s: real %s, simulator %s", name, TYPE_NAMES[kind] or kind, TYPE_NAMES[simKind] or simKind)
        end
      end
    end
    local rows = {}
    for _, kind in ipairs({ "f", "t", "w", "n", "b", "s", "u" }) do
      local list = missingBy[kind]
      if list then
        rows[#rows + 1] = string.format("%d %s missing: %s", #list, TYPE_NAMES[kind] or kind,
          table.concat(list, ", ", 1, math.min(#list, 40)) .. (#list > 40 and ", ..." or ""))
      end
    end
    for _, r in ipairs(typeDiff) do rows[#rows + 1] = r end
    -- namespace and mixin members
    local nsMissing = {}
    for _, nsName in ipairs(sortedKeys(real.namespaces)) do
      local st = env[nsName]
      if type(st) == "table" then
        for _, m in ipairs(sortedKeys(real.namespaces[nsName])) do
          if st[m] == nil then nsMissing[#nsMissing + 1] = nsName .. "." .. m end
        end
      end
    end
    if #nsMissing > 0 then
      rows[#rows + 1] = string.format("%d namespace/mixin members missing: %s", #nsMissing,
        table.concat(nsMissing, ", ", 1, math.min(#nsMissing, 40)) .. (#nsMissing > 40 and ", ..." or ""))
    end
    -- what the simulator defines that the real client doesn't have
    local extra = {}
    for k, v in pairs(env) do
      if type(k) == "string" and real.globals[k] == nil and type(v) ~= "table" and k ~= "_G" and not k:match("^SimCheck")
        and not k:match("^SLASH_") then
        extra[#extra + 1] = k
      end
    end
    table.sort(extra)
    if #extra > 0 then
      rows[#rows + 1] = string.format("%d defined by the simulator but not in the real client: %s", #extra,
        table.concat(extra, ", ", 1, math.min(#extra, 40)) .. (#extra > 40 and ", ..." or ""))
    end
    section("Globals and API", rows, total + #typeDiff + #nsMissing + #extra)
  end

  -- enums and constants
  do
    local rows = {}
    local senv = sim.env
    local simEnum = senv.Enum or {}
    local missingEnums, diffs = {}, 0
    for _, name in ipairs(sortedKeys(real.enums)) do
      local s = simEnum[name]
      if s == nil then missingEnums[#missingEnums + 1] = name
      else
        for k, v in pairs(real.enums[name]) do
          if s[k] ~= v then
            diffs = diffs + 1
            if diffs <= 30 then rows[#rows + 1] = string.format("Enum.%s.%s: real %s, simulator %s", name, k, show(v), show(s[k])) end
          end
        end
      end
    end
    if #missingEnums > 0 then
      table.insert(rows, 1, string.format("%d enums missing: %s", #missingEnums,
        table.concat(missingEnums, ", ", 1, math.min(#missingEnums, 40)) .. (#missingEnums > 40 and ", ..." or "")))
    end
    if diffs > 30 then rows[#rows + 1] = string.format("... %d enum values differ in total", diffs) end
    local cdiff = 0
    for _, k in ipairs(sortedKeys(real.constants)) do
      local v = senv[k]
      if v ~= nil and v ~= real.constants[k] then
        cdiff = cdiff + 1
        if cdiff <= 30 then rows[#rows + 1] = string.format("%s: real %s, simulator %s", k, show(real.constants[k]), show(v)) end
      end
    end
    local ctMissing = {}
    for _, k in ipairs(sortedKeys(real.constTables)) do
      if senv[k] == nil then ctMissing[#ctMissing + 1] = k end
    end
    if #ctMissing > 0 then
      rows[#rows + 1] = string.format("%d constant tables missing: %s", #ctMissing,
        table.concat(ctMissing, ", ", 1, math.min(#ctMissing, 40)) .. (#ctMissing > 40 and ", ..." or ""))
    end
    section("Enums and constants", rows, #missingEnums + diffs + cdiff + #ctMissing)
  end

  -- cvars
  do
    local rows, missing, diff = {}, {}, 0
    local defaults = sim.cvarDefaults or {}
    for _, name in ipairs(sortedKeys(real.cvars)) do
      local rv = real.cvars[name]
      if rv == false then missing[#missing + 1] = name
      elseif defaults[name] ~= nil and tostring(defaults[name]) ~= rv then
        diff = diff + 1
        if diff <= 25 then rows[#rows + 1] = string.format("%s default: real %s, simulator %s", name, show(rv), show(tostring(defaults[name]))) end
      end
    end
    if #missing > 0 then
      table.insert(rows, 1, string.format("%d CVars don't exist in the real client: %s", #missing,
        table.concat(missing, ", ", 1, math.min(#missing, 30)) .. (#missing > 30 and ", ..." or "")))
    end
    if diff > 25 then rows[#rows + 1] = string.format("... %d defaults differ in total", diff) end
    section("CVars", rows, #missing + diff)
  end

  -- probe errors
  do
    local rows = {}
    for k, v in pairs(real.errors or {}) do rows[#rows + 1] = "real client probe '" .. k .. "' failed: " .. tostring(v) end
    for k, v in pairs(simr.errors or {}) do rows[#rows + 1] = "simulator probe '" .. k .. "' failed: " .. tostring(v) end
    table.sort(rows)
    section("Probe failures", rows)
  end
  return sections
end

function M.markdown(sections, real)
  local c = real.client or {}
  local out = { "# SimCheck: real client vs simulator", "",
    string.format("Real client: %s build %s, interface %s, game type %s, recorded %s UTC.", tostring(c.version),
      tostring(c.build), tostring(c.interface), table.concat(c.gameTypes or {}, "/"), tostring(real.meta and real.meta.finished)), "" }
  for _, s in ipairs(sections) do
    out[#out + 1] = string.format("## %s (%d)", s.title, s.count)
    out[#out + 1] = ""
    if s.note then out[#out + 1] = "_" .. s.note .. "_"; out[#out + 1] = "" end
    if #s.rows == 0 then out[#out + 1] = "No differences." end
    for _, r in ipairs(s.rows) do out[#out + 1] = "- " .. r end
    out[#out + 1] = ""
  end
  return table.concat(out, "\n")
end

------------------------------------------------------------------ import
-- Distil the real results into wowapi/data/forever_client.lua.
-- The event names SimCheck asked the client about (its Data.lua).
local function checkedEvents()
  local chunk = loadfile(M.ADDON_DIR .. "/Data.lua")
  if not chunk then return {} end
  local ns = {}
  setfenv(chunk, {})
  chunk("SimCheck", ns)
  return ns.EVENTS or {}
end

function M.importData(real)
  local c = real.client or {}
  local invalid = {}
  for _, e in ipairs((real.events or {}).invalid or {}) do invalid[e] = true end
  local valid = {}
  if real.events then
    for _, e in ipairs(checkedEvents()) do if not invalid[e] then valid[#valid + 1] = e end end
  end
  local data = {
    source = string.format("SimCheck %s, client %s build %s, recorded %s UTC", tostring(real.meta and real.meta.addonVersion),
      tostring(c.version), tostring(c.build), tostring(real.meta and real.meta.finished)),
    client = { version = c.version, build = c.build, date = c.date, interface = c.interface, projectId = c.projectId,
      projects = c.projects, gameType = c.gameTypes and c.gameTypes[1], game = c.game, family = c.family,
      expansion = c.expansion },
    invalidEvents = (real.events or {}).invalid,
    validEvents = valid,
    enums = real.enums,
    constants = real.constants,
    constTables = real.constTables,
    cvars = real.cvars,
    functions = {}, frames = {}, namespaces = {}, mixins = {},
  }
  for name, code in pairs(real.globals or {}) do
    if code == "f" then data.functions[#data.functions + 1] = name
    elseif code:sub(1, 2) == "w:" then data.frames[name] = code:sub(3) end
  end
  table.sort(data.functions)
  for name, members in pairs(real.namespaces or {}) do
    local list = {}
    for m, code in pairs(members) do if code == "f" then list[#list + 1] = m end end
    table.sort(list)
    if name:match("Mixin$") then data.mixins[name] = list else data.namespaces[name] = list end
  end
  return data
end

function M.writeImport(real, path)
  local data = M.importData(real)
  local f = assert(io.open(path or M.IMPORT_PATH, "w"))
  f:write("-- Real WoW: Forever client facts recorded by the SimCheck addon and imported with\n")
  f:write("-- `wowtest compare --import`. The simulator prefers these over its own guesses.\n")
  f:write("return " .. require("wowapi.serialize").serialize(data))
  f:close()
  return data
end

return M
