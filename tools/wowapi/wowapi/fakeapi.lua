-- Fake-data implementations of widely used global API functions that the
-- client has but Blizzard's documentation doesn't describe (equipment,
-- stats, action bars, instances, position, macros...). Values come from the
-- simulated player so they stay consistent.
local faker = require("wowapi.faker")

local M = {}

local SLOT_NAMES = { "HeadSlot", "NeckSlot", "ShoulderSlot", "ShirtSlot", "ChestSlot", "WaistSlot", "LegsSlot",
  "FeetSlot", "WristSlot", "HandsSlot", "Finger0Slot", "Finger1Slot", "Trinket0Slot", "Trinket1Slot", "BackSlot",
  "MainHandSlot", "SecondaryHandSlot", "RangedSlot", "TabardSlot" }
local SLOT_EQUIP = { "INVTYPE_HEAD", "INVTYPE_NECK", "INVTYPE_SHOULDER", "INVTYPE_BODY", "INVTYPE_CHEST", "INVTYPE_WAIST",
  "INVTYPE_LEGS", "INVTYPE_FEET", "INVTYPE_WRIST", "INVTYPE_HAND", "INVTYPE_FINGER", "INVTYPE_FINGER", "INVTYPE_TRINKET",
  "INVTYPE_TRINKET", "INVTYPE_CLOAK", "INVTYPE_WEAPON", "INVTYPE_WEAPONOFFHAND", "INVTYPE_RANGED", "INVTYPE_TABARD" }

-- Deterministic equipment: slot -> generated item (registered in sim.items).
local function equipment(sim)
  local p = sim.player
  if p.equipment then return p.equipment end
  local eq = {}
  local r = faker.rng("equip:" .. tostring(p.name))
  for slot = 1, 19 do
    if slot ~= 4 and slot ~= 19 and slot ~= 18 then
      -- find a generated item that fits the slot
      for _ = 1, 400 do
        local id = r.int(30000, 220000)
        local it = sim.items[id] or faker.item(id, p.level)
        local fits = it.equipLoc == SLOT_EQUIP[slot] or (slot == 16 and (it.equipLoc == "INVTYPE_2HWEAPON" or it.equipLoc == "INVTYPE_WEAPON"))
        if fits then
          it.quality = math.max(it.quality, 2)
          it.link = faker.itemLink(id, it.name, it.quality)
          sim.items[id] = it
          eq[slot] = { itemID = id, durability = r.int(40, 100), maxDurability = 100 }
          break
        end
      end
    end
  end
  p.equipment = eq
  return eq
end

function M.install(sim, env)
  local function unitData(unit)
    return sim.units[type(unit) == "string" and unit:lower() or ""]
  end
  local function slotItem(unit, slot)
    local u = unitData(unit)
    if u ~= sim.player then return nil end
    local e = equipment(sim)[slot]
    return e and sim.items[e.itemID], e
  end
  local fns = {
    GetInventorySlotInfo = function(name)
      for i, n in ipairs(SLOT_NAMES) do if n:lower() == tostring(name):lower() then return i, 136516, false end end
      error("Invalid inventory slot in GetInventorySlotInfo", 2)
    end,
    GetInventoryItemID = function(unit, slot) local it = slotItem(unit, slot); return it and it.id end,
    GetInventoryItemLink = function(unit, slot) local it = slotItem(unit, slot); return it and it.link end,
    GetInventoryItemTexture = function(unit, slot) local it = slotItem(unit, slot); return it and it.icon end,
    GetInventoryItemQuality = function(unit, slot) local it = slotItem(unit, slot); return it and it.quality end,
    GetInventoryItemCount = function(unit, slot) return slotItem(unit, slot) and 1 or 0 end,
    GetInventoryItemDurability = function(slot)
      local _, e = slotItem("player", slot)
      if e then return e.durability, e.maxDurability end
    end,
    GetInventoryItemBroken = function(unit, slot) local _, e = slotItem(unit, slot); return e ~= nil and e.durability == 0 end,
    GetInventoryItemCooldown = function() return 0, 0, 1 end,
    GetAverageItemLevel = function()
      local total, n = 0, 0
      for _, e in pairs(equipment(sim)) do total = total + (sim.items[e.itemID].itemLevel or 0); n = n + 1 end
      local avg = n > 0 and total / n or sim.player.level
      return avg, avg, avg
    end,
    GetDetailedItemLevelInfo = function(ref)
      local it = type(ref) == "string" and sim.items[tonumber(ref:match("item:(%d+)") or 0)] or sim.items[ref]
      if it then return it.itemLevel, false, it.itemLevel end
    end,
    UnitStat = function(unit, i)
      local u = unitData(unit); if not u then return 0, 0, 0, 0 end
      local base = (u.level or 60) * ({ 2.2, 2.0, 2.6, 1.6, 1.6 })[i or 1]
      return math.floor(base), math.floor(base * 1.3), math.floor(base * 0.3), 0
    end,
    UnitArmor = function(unit) local u = unitData(unit); local a = (u and u.level or 60) * 40; return a, a, a, 0, 0 end,
    UnitAttackPower = function(unit) local u = unitData(unit); return (u and u.level or 60) * 10, 0, 0 end,
    UnitRangedAttackPower = function(unit) local u = unitData(unit); return (u and u.level or 60) * 8, 0, 0 end,
    UnitAttackSpeed = function() return 2.6, 2.6 end,
    UnitDamage = function(unit) local l = (unitData(unit) or {}).level or 60; return l * 3, l * 5, l * 2, l * 3, 0, 0, 1 end,
    GetCritChance = function() return 18.5 end,
    GetRangedCritChance = function() return 17.2 end,
    GetSpellCritChance = function() return 16.8 end,
    GetHaste = function() return 12.4 end,
    GetMeleeHaste = function() return 12.4 end,
    GetRangedHaste = function() return 12.4 end,
    GetMastery = function() return 24.6, 1 end,
    GetMasteryEffect = function() return 24.6, 1 end,
    GetVersatilityBonus = function() return 4.0 end,
    GetCombatRating = function(id) return 400 + (id or 0) * 7 end,
    GetCombatRatingBonus = function(id) return 5 + (id or 0) % 7 end,
    GetDodgeChance = function() return 9.1 end,
    GetParryChance = function() return sim.player.class == "WARRIOR" and 11.4 or 0 end,
    GetBlockChance = function() return 0 end,
    GetSpellBonusDamage = function() return sim.player.level * 12 end,
    GetSpellBonusHealing = function() return sim.player.level * 12 end,
    GetManaRegen = function() return 120, 60 end,
    GetUnitSpeed = function(unit) return 0, 7, 7, 4.72 end,
    GetPlayerFacing = function() return sim.player.facing or 0 end,
    UnitPosition = function(unit)
      local u = unitData(unit); if not u then return nil end
      return (u.worldY or -8900), (u.worldX or -150), 82, 0
    end,
    GetInstanceInfo = function()
      local t = sim.player.instanceType or "none"
      return sim.player.zone, t, t == "none" and 0 or 1, t == "none" and "" or "Normal", t == "raid" and 40 or 5,
        0, false, sim.player.mapID or 0, 0, nil
    end,
    GetDifficultyInfo = function(id)
      local names = { [1] = "Normal", [2] = "Heroic", [8] = "Mythic Keystone", [14] = "Normal", [15] = "Heroic", [16] = "Mythic", [23] = "Mythic" }
      return names[id] or "Normal", id and id >= 14 and "raid" or "party", false, false, false, false, nil
    end,
    GetNumSavedInstances = function() return 0 end,
    GetExpansionLevel = function() return env.LE_EXPANSION_LEVEL_CURRENT or 0 end,
    GetMaxLevelForPlayerExpansion = function() return 60 end,
    GetMaxLevelForLatestExpansion = function() return 60 end,
    GetRealmID = function() return 1 end,
    IsFlyableArea = function() return false end,
    GetNumShapeshiftForms = function() return 0 end,
    GetShapeshiftForm = function() return sim.player.form or 0 end,
    GetShapeshiftFormID = function() return nil end,
    HasAction = function(slot) return sim.actions and sim.actions[slot] ~= nil or (slot >= 1 and slot <= 12) end,
    GetActionInfo = function(slot)
      local a = sim.actions and sim.actions[slot]
      if a then return a.type, a.id, a.subType end
      local own = {}
      for _, k in ipairs(faker.KNOWN_SPELLS) do if k[4] == sim.player.class then own[#own + 1] = k[1] end end
      local id = own[slot]
      if id then return "spell", id, "spell" end
    end,
    GetActionTexture = function(slot)
      local kind, id = env.GetActionInfo(slot)
      if kind == "spell" then return env.C_Spell.GetSpellTexture(id) end
    end,
    GetActionText = function() return nil end,
    GetActionCount = function() return 0 end,
    IsUsableAction = function(slot) return env.HasAction(slot), false end,
    IsActionInRange = function(slot) return true end,
    IsCurrentAction = function() return false end,
    IsAutoRepeatAction = function() return false end,
    IsAttackAction = function() return false end,
    GetNumMacros = function() return #(sim.macros or {}), 0 end,
    GetMacroInfo = function(i) local m = (sim.macros or {})[i]; if m then return m.name, m.icon or 134400, m.body end end,
    GetMacroIndexByName = function(name) for i, m in ipairs(sim.macros or {}) do if m.name == name then return i end end return 0 end,
    GetNumFriends = function() return 0 end,
    BNGetNumFriends = function() return 0, 0 end,
    GetNumGuildMembers = function() return sim.player.guild and 42 or 0, sim.player.guild and 9 or 0, sim.player.guild and 9 or 0 end,
    GetGuildRosterInfo = function(i)
      if not sim.player.guild or i > 42 then return nil end
      local u = faker.character("guild" .. i)
      return u.name .. "-Forever", i == 1 and "Guild Master" or "Member", i == 1 and 0 or 3, u.level, u.class, sim.player.zone,
        "", "", i <= 9, 0, u.class, 0, 0, false, false, 5, u.guid
    end,
    GetGuildInfo = function(unit)
      local u = unitData(unit)
      if u and u.guild then return u.guild, "Member", 3, sim.player.realm end
    end,
    GetTotemInfo = function() return false, "", 0, 0, 0 end,
    GetComboPoints = function() return 0 end,
    GetRuneCooldown = function() return 0, 10, true end,
    UnitThreatSituation = function(unit, mob) return sim.inCombat and 0 or nil end,
    UnitDetailedThreatSituation = function(unit, mob)
      if not sim.inCombat then return nil end
      return true, 3, 100, 100, 10000
    end,
    GetNetStats = function() return 12.5, 30.1, 42, 45 end,
    GetFramerate = function() return 60 end,
    GetGameTime = function()
      local t = os.date("*t", env.GetServerTime())
      return t.hour, t.min
    end,
    GetZonePVPInfo = function() return sim.player.pvpZone or "friendly" end,
    GetNumSpecGroups = function() return sim.player.numSpecGroups or 1 end,
    GetNumTalentGroups = function() return sim.player.numSpecGroups or 1 end,
    GetActiveSpecGroup = function() return 1 end,
    GetActiveTalentGroup = function() return 1 end,
  }
  for name, fn in pairs(fns) do rawset(env, name, fn) end
  rawset(env, "INVENTORY_SLOT_NAMES", SLOT_NAMES)
end

-- Sim helpers
M.SimMethods = {}

-- Set an action bar slot: sim:SetAction(1, "spell", 133)
function M.SimMethods:SetAction(slot, kind, id)
  self.actions = self.actions or {}
  self.actions[slot] = kind and { type = kind, id = id, subType = kind } or nil
  self:FireEvent("ACTIONBAR_SLOT_CHANGED", slot)
end

-- Add a macro: sim:AddMacro("Pull", "/cast Charge", 132337)
function M.SimMethods:AddMacro(name, body, icon)
  self.macros = self.macros or {}
  table.insert(self.macros, { name = name, body = body, icon = icon })
  self:FireEvent("UPDATE_MACROS")
end

-- The player's generated equipment: { [slot] = { itemID, durability } }
function M.SimMethods:Equipment() return equipment(self) end

return M
