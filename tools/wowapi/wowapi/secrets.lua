-- 12.x "secret values" (opt-in: WoW.new({ secretValues = true })).
--
-- While restrictions are active (in combat, or sim:SetSecretRestrictions(true)),
-- API functions the documentation flags as secret (SecretReturns,
-- SecretWhenInCombat, SecretWhenUnit*Restricted, ...) return secret values:
--   * arithmetic, ordering comparisons, concatenation and indexing error
--   * type() still reports the real type, issecretvalue() returns true
--   * widget setters (SetText, SetValue, SetMinMaxValues, ...) accept them
--   * scrubsecretvalues() replaces them with nil
-- The player's own unit is not restricted.
local M = {}

local SECRET_FLAGS = { SecretReturns = true, SecretWhenInCombat = true, SecretWhenUnitStatsRestricted = true,
  SecretWhenUnitIdentityRestricted = true, SecretWhenUnitAuraRestricted = true,
  SecretWhenUnitSpellCastRestricted = true, SecretWhenCooldownsRestricted = true,
  SecretWhenUnitPowerRestricted = true }
local UNIT_FLAGS = { SecretWhenUnitStatsRestricted = true, SecretWhenUnitIdentityRestricted = true,
  SecretWhenUnitAuraRestricted = true, SecretWhenUnitSpellCastRestricted = true, SecretWhenUnitPowerRestricted = true }

local registry = setmetatable({}, { __mode = "k" })

local function secretError(what)
  return function() error("attempt to " .. what .. " a secret value", 2) end
end

local mt = {
  __add = secretError("perform arithmetic on"), __sub = secretError("perform arithmetic on"),
  __mul = secretError("perform arithmetic on"), __div = secretError("perform arithmetic on"),
  __mod = secretError("perform arithmetic on"), __pow = secretError("perform arithmetic on"),
  __unm = secretError("perform arithmetic on"), __concat = secretError("concatenate"),
  __lt = secretError("compare"), __le = secretError("compare"),
  __index = secretError("index"), __newindex = secretError("index"),
  __call = secretError("call"), __len = secretError("get length of"),
  __tostring = function() return "<secret>" end,
  __metatable = false,
}

function M.wrap(v)
  if v == nil then return nil end
  if registry[v] ~= nil then return v end
  local s = setmetatable({}, mt)
  registry[s] = { v }
  return s
end

function M.isSecret(v) return type(v) == "table" and registry[v] ~= nil end
function M.unwrap(v)
  local r = type(v) == "table" and registry[v]
  if r then return r[1] end
  return v
end

-- Should a documented function's returns be secret right now?
function M.shouldWrap(sim, doc, firstArg)
  if not sim.secretRestrictions then return false end
  local flagged, unitFlag = false, false
  for _, f in ipairs(doc.f or {}) do
    if f == "ReturnsNeverSecret" then return false end
    if SECRET_FLAGS[f] then flagged = true end
    if UNIT_FLAGS[f] then unitFlag = true end
  end
  if not flagged then return false end
  if unitFlag and type(firstArg) == "string" and firstArg:lower() == "player" then return false end
  return true
end

function M.install(sim, env)
  sim.secretValuesEnabled = sim.opts.secretValues and true or false
  sim.secretRestrictions = false
  local realType = type
  rawset(env, "type", function(v)
    if registry[v] ~= nil then return realType(registry[v][1]) end
    return realType(v)
  end)
  rawset(env, "issecretvalue", function(v) return M.isSecret(v) end)
  rawset(env, "issecrettable", function(t) return M.isSecret(t) end)
  rawset(env, "canaccesssecrets", function() return not sim.secretRestrictions end)
  rawset(env, "canaccessvalue", function(v) return not M.isSecret(v) end)
  rawset(env, "hasanysecretvalues", function(...)
    for i = 1, select("#", ...) do if M.isSecret((select(i, ...))) then return true end end
    return false
  end)
  rawset(env, "scrubsecretvalues", function(...)
    local t = { n = select("#", ...), ... }
    for i = 1, t.n do if M.isSecret(t[i]) then t[i] = nil end end
    return (table.unpack or unpack)(t, 1, t.n)
  end)
  rawset(env, "secretwrap", function(...)
    local t = { n = select("#", ...), ... }
    for i = 1, t.n do t[i] = M.wrap(t[i]) end
    return (table.unpack or unpack)(t, 1, t.n)
  end)
  rawset(env, "dropsecretaccess", function() end)
  -- tostring of a secret is allowed and yields a (secret) string in game;
  -- here it yields "<secret>".
  local ns = rawget(env, "C_Secrets")
  if ns then
    local function restricted(unit)
      if not sim.secretRestrictions then return false end
      if type(unit) == "string" and unit:lower() == "player" then return false end
      return true
    end
    for k in pairs(ns) do
      if k:match("^Should") or k == "HasSecretRestrictions" then
        local orig = ns[k]
        ns[k] = function(unit, ...)
          orig(unit, ...) -- keep argument checks / call log
          return restricted(unit)
        end
      end
    end
    ns.CanCompareUnitTokens = function() return not sim.secretRestrictions end
  end
end

-- Recompute whether restrictions apply (called on combat changes).
function M.update(sim)
  if not sim.secretValuesEnabled then sim.secretRestrictions = false; return end
  if sim.forcedSecretRestrictions ~= nil then sim.secretRestrictions = sim.forcedSecretRestrictions
  else sim.secretRestrictions = sim.lockdown end
end

return M
