-- Characters for the login scene (HUD2): one per class, with what a player
-- of that class and level has on screen at login (spells on the action
-- bar, their power, a target). Shared by hud.lua and addon-live's scene.
--
-- Icons are named as in the game (Interface\Icons\...); the art comes from
-- the CASC extract, so with WOWAPI_ART set they draw as the real icons.

local M = {}

local function icon(name)
    return "Interface\\Icons\\" .. name
end

M.CLASSES = {
    -- A level 20 human rogue in Westfall, stealthed out of combat, with a
    -- Defias Pillager targeted and three combo points on it.
    rogue = {
        player = { name = "Kessra", class = "ROGUE", race = "Human", level = 20, zone = "Westfall", subZone = "Moonbrook",
            health = 595, healthMax = 700, powerType = 3, power = 70, powerMax = 100,
            -- Enum.PowerType.ComboPoints = 4
            powers = { [4] = { 3, 5 } } },
        target = { name = "Defias Pillager", level = 19, classification = "normal", creatureType = "Humanoid",
            reaction = 2, health = 310, healthMax = 520, class = "MAGE", powerType = 0, power = 340, powerMax = 600,
            guid = "Creature-0-0-0-0-589-0" },
        -- action slot -> { spell ID, icon }
        actions = {
            [1] = { 1752, icon("Spell_Shadow_RitualOfSacrifice") }, -- Sinister Strike
            [2] = { 2098, icon("Ability_Rogue_Eviscerate") },
            [3] = { 5171, icon("Ability_Rogue_SliceDice") },
            [4] = { 53, icon("Ability_BackStab") },
            [5] = { 1776, icon("Ability_Gouge") },
            [6] = { 1766, icon("Ability_Kick") },
            [7] = { 1833, icon("Ability_CheapShot") },
            [8] = { 408, icon("Ability_Rogue_KidneyShot") },
            [9] = { 2983, icon("Ability_Rogue_Sprint") },
            [10] = { 5277, icon("Spell_Shadow_ShadowWard") }, -- Evasion
            [11] = { 1784, icon("Ability_Stealth") },
            [12] = { 6770, icon("Ability_Sap") },
        },
    },
}

-- The player options for WoW.new, or nil for an unknown class.
function M.player(class, level)
    local c = M.CLASSES[class]
    if not c then
        return nil
    end
    local p = {}
    for k, v in pairs(c.player) do
        p[k] = v
    end
    p.level = level or p.level
    return p
end

-- Puts the class's spells on the action bar. Call before the UI loads: the
-- action buttons read their page and slots as they're built.
function M.mock(sim, class)
    local c = M.CLASSES[class]
    if not c then
        return
    end
    local actions = c.actions or {}
    local function has(slot)
        return actions[slot] ~= nil
    end
    local mocks = {
        ["C_ActionBar.GetActionBarPage"] = function() return 1 end,
        ["C_ActionBar.GetBonusBarOffset"] = function() return 0 end,
        ["C_ActionBar.GetExtraBarIndex"] = function() return 0 end,
        ["C_ActionBar.GetMultiCastBarIndex"] = function() return 0 end,
        ["C_ActionBar.HasAction"] = has,
        ["C_ActionBar.GetActionTexture"] = function(slot) return actions[slot] and actions[slot][2] end,
        ["C_ActionBar.IsUsableAction"] = function(slot) return has(slot), false end,
        ["C_ActionBar.IsCurrentAction"] = function() return false end,
        ["C_ActionBar.IsAutoRepeatAction"] = function() return false end,
        ["C_ActionBar.IsAttackAction"] = function() return false end,
        ["C_ActionBar.GetActionText"] = function() return nil end,
        HasAction = has,
        GetActionTexture = function(slot) return actions[slot] and actions[slot][2] end,
        GetActionInfo = function(slot)
            if actions[slot] then
                return "spell", actions[slot][1], "spell"
            end
        end,
        -- Forever keeps combo points on the target (Camelot ComboFrame).
        GetComboPoints = function(unit, target)
            local cp = c.player.powers and c.player.powers[4]
            return (cp and unit == "player" and target == "target" and c.target) and cp[1] or 0
        end,
        GetActionCount = function() return 0 end,
        GetActionCooldown = function() return 0, 0, 1, 1 end,
        IsUsableAction = function(slot) return has(slot), false end,
    }
    for name, fn in pairs(mocks) do
        sim:Mock(name, fn)
    end
end

-- Puts the class's target in front of the player and tells the bars their
-- slots changed. Call after login (these fire events).
function M.apply(sim, class)
    local c = M.CLASSES[class]
    if not c then
        return
    end
    if c.target then
        sim:SetTarget(c.target)
    end
    sim:FireEvent("ACTIONBAR_SLOT_CHANGED", 0)
    sim:FireEvent("UPDATE_SHAPESHIFT_FORM")
end

return M
