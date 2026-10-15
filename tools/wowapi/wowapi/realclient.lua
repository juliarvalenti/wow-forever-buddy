-- Facts recorded from the real WoW: Forever client by the SimCheck addon
-- (imported with `wowtest compare --import` into wowapi/data/forever_client.lua).
-- When present they replace the simulator's own guesses: build and project
-- IDs, game type, enum and constant values, which events exist, CVar
-- defaults, and the names of Blizzard functions, frames and mixin methods.
local M = {}

local cached
function M.data()
  if cached == nil then
    local ok, d = pcall(require, "wowapi.data.forever_client")
    cached = ok and type(d) == "table" and d or false
  end
  return cached or nil
end

-- Use `d` in place of the imported file (tests, or `wowtest compare`
-- previewing an import); nil goes back to the file.
local original
function M.use(d)
  local toc = require("wowapi.toc")
  if original then toc.GAME_TYPE, toc.GAME, toc.FAMILY = original[1], original[2], original[3] end
  if d == nil then cached = nil else cached = d end
  M._globalsApplied = false
  require("wowapi.resources")._resetSets()
end

-- Build info and .toc game variables (once per process).
function M.applyGlobals()
  local d = M.data()
  if not d or M._globalsApplied then return end
  M._globalsApplied = true
  local c = d.client or {}
  local toc = require("wowapi.toc")
  original = original or { toc.GAME_TYPE, toc.GAME, toc.FAMILY }
  if c.gameType then toc.GAME_TYPE = c.gameType end
  if c.game then toc.GAME = c.game end
  if c.family then toc.FAMILY = c.family end
end

function M.build()
  local d = M.data()
  local c = d and d.client
  if not c or not c.build then return nil end
  return { version = c.version, build = tostring(c.build), date = c.date, interface = tonumber(c.interface) }
end

-- Extend the lazy resource sets (resources.sets) with the real client's names.
function M.extendSets(s)
  local d = M.data()
  if not d then return end
  for _, n in ipairs(d.functions or {}) do s.functions[n] = true end
  s.frameTypes = s.frameTypes or {}
  for n, t in pairs(d.frames or {}) do
    s.frames[n] = true
    if t ~= "forbidden" and t ~= "?" then s.frameTypes[n] = t end
  end
  for ns, members in pairs(d.namespaces or {}) do
    local list = s.namespaces[ns] or {}
    local have = {}
    for _, m in ipairs(list) do have[m] = true end
    for _, m in ipairs(members) do if not have[m] then list[#list + 1] = m end end
    s.namespaces[ns] = list
  end
  s.mixinMethods = s.mixinMethods or {}
  for name, methods in pairs(d.mixins or {}) do
    s.mixins[name] = true
    s.mixinMethods[name] = methods
  end
end

-- Values: project ID, enums, constants, constant tables, events, CVars.
function M.install(sim, env)
  local d = M.data()
  if not d then return end
  local c = d.client or {}
  for k, v in pairs(c.projects or {}) do rawset(env, k, v) end
  local enum = rawget(env, "Enum")
  for name, vals in pairs(d.enums or {}) do
    enum[name] = enum[name] or {}
    for k, v in pairs(vals) do enum[name][k] = v end
  end
  for k, v in pairs(d.constants or {}) do
    local cur = rawget(env, k)
    if cur == nil or type(cur) == type(v) then rawset(env, k, v) end
  end
  for k, t in pairs(d.constTables or {}) do
    local cur = rawget(env, k)
    if cur == nil then
      local copy = {}
      for a, b in pairs(t) do copy[a] = b end
      rawset(env, k, copy)
    elseif type(cur) == "table" and not sim.widgetState[cur] then
      for a, b in pairs(t) do cur[a] = b end
    end
  end
  if c.projectId and not sim.opts.projectId then rawset(env, "WOW_PROJECT_ID", c.projectId) end
  for _, e in ipairs(d.validEvents or {}) do sim.knownEvents[e] = true end
  for _, e in ipairs(d.invalidEvents or {}) do sim.knownEvents[e] = nil end
  if d.cvars then
    local defaults = {}
    for k, v in pairs(sim.cvarDefaults or {}) do defaults[k] = v end
    for k, v in pairs(d.cvars) do
      if v == false then defaults[k] = nil else defaults[k] = v end
    end
    sim.cvarDefaults = defaults
  end
end

return M
