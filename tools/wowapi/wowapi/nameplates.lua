-- Nameplates: Blizzard's NamePlateDriverFrame, C_NamePlate lookups and
-- nameplate frames for simulated enemies, with the same event/driver flow
-- as the client (NAME_PLATE_CREATED, NamePlateDriverFrame:OnNamePlateCreated,
-- NAME_PLATE_UNIT_ADDED, NamePlateDriverFrame:OnNamePlateAdded, ...).
local faker = require("wowapi.faker")

local M = {}

function M.install(sim, env)
  sim.nameplates = {}

  local ns = rawget(env, "C_NamePlate") or {}
  ns.GetNamePlates = function()
    local out = {}
    for _, np in ipairs(sim.nameplates) do if np.active then out[#out + 1] = np.frame end end
    return out
  end
  ns.GetNamePlateForUnit = function(unit)
    for _, np in ipairs(sim.nameplates) do
      if np.active and (np.unit == unit or (unit == "target" and sim.units.target == sim.units[np.unit])) then return np.frame end
    end
  end
  rawset(env, "C_NamePlate", ns)
end

-- The Blizzard driver frame (created lazily as a placeholder frame).
function M.decorateDriver(sim, f)
  local noop = function() end
  for _, m in ipairs({ "OnNamePlateCreated", "OnNamePlateAdded", "OnNamePlateRemoved", "OnForbiddenNamePlateCreated",
    "OnUnitAuraUpdate", "OnRaidTargetUpdate", "OnTargetChanged", "OnSoftTargetUpdate", "UpdateNamePlateOptions",
    "SetupClassNameplateBars", "SetClassNameplateBar", "OnSoftTargetUpdate", "ApplyFrameOptions", "UpdateInsetsForType" }) do
    f[m] = noop
  end
  function f:GetNamePlateTypeFromUnit(unit)
    local u = sim.units[unit]
    return u and u.hostile and "enemyNpc" or "friendlyNpc"
  end
  function f:GetClassNameplateManaBar() return nil end
  function f:GetClassNameplateAlternatePowerBar() return nil end
end

local function driver(sim) return sim:Get("NamePlateDriverFrame") end

-- Blizzard's default nameplate unit frame (what addons hide or restyle).
local function defaultUnitFrame(sim, plate, unit)
  local env = sim.env
  local u = sim.units[unit]
  local uf = env.CreateFrame("Button", nil, plate)
  uf:SetAllPoints()
  uf.unit, uf.displayedUnit = unit, unit
  local hb = env.CreateFrame("StatusBar", nil, uf)
  hb:SetPoint("LEFT", 8, -6)
  hb:SetPoint("RIGHT", -8, -6)
  hb:SetHeight(10)
  hb:SetStatusBarTexture("Interface\\TargetingFrame\\UI-TargetingFrame-BarFill")
  hb:SetStatusBarColor(u.hostile and 0.9 or 0.2, u.hostile and 0.1 or 0.9, 0.1)
  hb:SetMinMaxValues(0, u.healthMax or 1)
  hb:SetValue(u.health or 1)
  uf.healthBar = hb
  uf.HealthBarsContainer = hb
  local name = uf:CreateFontString(nil, "OVERLAY", "SystemFont_NamePlate")
  name:SetPoint("BOTTOM", hb, "TOP", 0, 2)
  name:SetText(u.name)
  uf.name = name
  uf.castBar = env.CreateFrame("StatusBar", nil, uf)
  uf.castBar:Hide()
  uf.BuffFrame = env.CreateFrame("Frame", nil, uf)
  uf.RaidTargetFrame = env.CreateFrame("Frame", nil, uf)
  return uf
end

M.SimMethods = {}

-- Spawn `n` enemies with nameplates around the middle of the screen.
-- sim:SpawnNameplates(5, { level = 62 })
function M.SimMethods:SpawnNameplates(n, opts)
  opts = opts or {}
  local env = self.env
  local out = {}
  for i = 1, n or 5 do
    local idx = #self.nameplates + 1
    local unit = "nameplate" .. idx
    local o = {}
    for k, v in pairs(opts) do o[k] = v end
    local u = faker.npc("plate" .. idx .. tostring(opts.seed or ""), o)
    u.health = math.floor(u.healthMax * (0.35 + ((idx * 37) % 60) / 100))
    self.units[unit] = u
    local plate = env.CreateFrame("Button", "NamePlate" .. idx, env.WorldFrame)
    plate:SetSize(160, 44)
    -- spread the plates across the upper middle of the screen, like mobs in view
    local col, row = (idx - 1) % 4, math.floor((idx - 1) / 4)
    plate:SetPoint("CENTER", env.WorldFrame, "BOTTOMLEFT", 560 + col * 260 + (row % 2) * 110, 700 - row * 110)
    plate.namePlateUnitToken = unit
    plate.UnitFrame = defaultUnitFrame(self, plate, unit)
    self.widgetState[plate].strata = "BACKGROUND"
    local np = { unit = unit, frame = plate, active = true }
    table.insert(self.nameplates, np)
    self:FireEvent("NAME_PLATE_CREATED", plate)
    local d = driver(self)
    if d then self:_pcall(d.OnNamePlateCreated, d, plate) end
    self:FireEvent("NAME_PLATE_UNIT_ADDED", unit)
    if d then self:_pcall(d.OnNamePlateAdded, d, unit) end
    out[#out + 1] = plate
  end
  return out
end

function M.SimMethods:RemoveNameplate(index)
  local np = self.nameplates[index]
  if not np or not np.active then return end
  np.active = false
  self:FireEvent("NAME_PLATE_UNIT_REMOVED", np.unit)
  local d = driver(self)
  if d then self:_pcall(d.OnNamePlateRemoved, d, np.unit) end
  np.frame:Hide()
  self.units[np.unit] = nil
end

return M
