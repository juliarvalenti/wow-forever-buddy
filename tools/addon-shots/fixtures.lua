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
    -- A level 34 human rogue in the Arathi Highlands (old enough for Blind),
    -- with a Syndicate Highwayman targeted and three combo points on it. The
    -- bars and keys are Julia's: twelve spells on the main bar and four
    -- Shift binds on bar 2; in stealth, 1 to 5 become Ambush, Garrote, Cheap
    -- Shot, Sap and Pick Pocket.
    rogue = {
        player = { name = "Kessra", class = "ROGUE", race = "Human", level = 34, zone = "Arathi Highlands",
            subZone = "Northfold Manor", health = 1240, healthMax = 1460, powerType = 3, power = 70, powerMax = 100,
            -- Enum.PowerType.ComboPoints = 4
            powers = { [4] = { 3, 5 } } },
        target = { name = "Syndicate Highwayman", level = 33, classification = "normal", creatureType = "Humanoid",
            reaction = 2, health = 690, healthMax = 1150, class = "ROGUE", powerType = 3, power = 100, powerMax = 100,
            guid = "Creature-0-0-0-0-2586-0" },
        -- action slot -> { spell ID, icon }; bar 2 (MultiBarBottomLeft) is
        -- page 6, slots 61 to 72
        actions = {
            [1] = { 1752, icon("Spell_Shadow_RitualOfSacrifice") }, -- Sinister Strike
            [2] = { 53, icon("Ability_BackStab") },
            [3] = { 1776, icon("Ability_Gouge") },
            [4] = { 1943, icon("Ability_Rogue_Rupture") },
            [5] = { 5277, icon("Spell_Shadow_ShadowWard") }, -- Evasion
            [6] = { 2098, icon("Ability_Rogue_Eviscerate") },
            [7] = { 1766, icon("Ability_Kick") },
            [8] = { 5171, icon("Ability_Rogue_SliceDice") },
            [9] = { 1966, icon("Ability_Rogue_Feint") },
            [10] = { 408, icon("Ability_Rogue_KidneyShot") },
            [11] = { 2094, icon("Spell_Shadow_MindSteal") }, -- Blind
            [12] = { 1784, icon("Ability_Stealth") },
            [61] = { 8647, icon("Ability_Warrior_Riposte") }, -- Expose Armor
            [62] = { 1725, icon("Ability_Rogue_Distract") },
            [63] = { 2983, icon("Ability_Rogue_Sprint") },
            [64] = { 1856, icon("Ability_Vanish") },
        },
        -- Stealth's bar (bonus bar 1: slots 73 to 84) changes the first five.
        stealthActions = {
            [1] = { 8676, icon("Ability_Rogue_Ambush") },
            [2] = { 703, icon("Ability_Rogue_Garrote") },
            [3] = { 1833, icon("Ability_CheapShot") },
            [4] = { 6770, icon("Ability_Sap") },
            [5] = { 921, icon("INV_Misc_Bag_11") }, -- Pick Pocket
        },
        -- the bars shown besides the main bar (IsMultibarVisible)
        multibars = { [1] = true },
        -- binding -> key
        keys = {
            ACTIONBUTTON1 = "1", ACTIONBUTTON2 = "2", ACTIONBUTTON3 = "3", ACTIONBUTTON4 = "4", ACTIONBUTTON5 = "5",
            ACTIONBUTTON6 = "Q", ACTIONBUTTON7 = "E", ACTIONBUTTON8 = "R", ACTIONBUTTON9 = "F",
            ACTIONBUTTON10 = "X", ACTIONBUTTON11 = "B", ACTIONBUTTON12 = "`",
            MULTIACTIONBAR1BUTTON1 = "SHIFT-E", MULTIACTIONBAR1BUTTON2 = "SHIFT-F",
            MULTIACTIONBAR1BUTTON3 = "SHIFT-R", MULTIACTIONBAR1BUTTON4 = "SHIFT-V",
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

-- Puts the class's spells on the action bar and binds its keys. Call before
-- the UI loads: the action buttons read their page and slots as they're
-- built. opts.stealth shows the stealth bar (a rogue's bonus bar 1).
function M.mock(sim, class, opts)
    local c = M.CLASSES[class]
    if not c then
        return
    end
    opts = opts or {}
    local actions = {}
    for slot, a in pairs(c.actions or {}) do
        actions[slot] = a
    end
    local bonusBar = 0
    if opts.stealth and c.stealthActions then
        bonusBar = 1
        for slot = 1, 12 do
            actions[72 + slot] = c.stealthActions[slot] or actions[slot]
        end
    end
    for action, key in pairs(c.keys or {}) do
        sim.env.SetBinding(key, action)
    end
    local function has(slot)
        return actions[slot] ~= nil
    end
    local mocks = {
        ["C_ActionBar.GetActionBarPage"] = function() return 1 end,
        ["C_ActionBar.GetBonusBarOffset"] = function() return bonusBar end,
        GetBonusBarOffset = function() return bonusBar end,
        -- Settings' "Action Bar 2" to "Action Bar 8" toggles
        GetActionBarToggles = function()
            local m = c.multibars or {}
            return m[1] == true, m[2] == true, m[3] == true, m[4] == true, m[5] == true, m[6] == true, m[7] == true
        end,
        -- no stance bar (a class with stances lists its forms)
        GetNumShapeshiftForms = function() return c.forms and #c.forms or 0 end,
        -- the bar controller pages the main bar to bonus bar 1 (page 7)
        ["C_ActionBar.HasBonusActionBar"] = function() return bonusBar > 0 end,
        ["C_ActionBar.GetBonusBarIndex"] = function() return bonusBar > 0 and 6 + bonusBar or 0 end,
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
    sim:FireEvent("UPDATE_BONUS_ACTIONBAR")
    sim:FireEvent("UPDATE_BINDINGS")
end

return M
