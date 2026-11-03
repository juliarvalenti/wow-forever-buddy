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
        -- spell ID -> name, for the spells above (their icons are there)
        names = {
            [1752] = "Sinister Strike", [53] = "Backstab", [1776] = "Gouge", [1943] = "Rupture", [5277] = "Evasion",
            [2098] = "Eviscerate", [1766] = "Kick", [5171] = "Slice and Dice", [1966] = "Feint",
            [408] = "Kidney Shot", [2094] = "Blind", [1784] = "Stealth", [8647] = "Expose Armor",
            [1725] = "Distract", [2983] = "Sprint", [1856] = "Vanish", [8676] = "Ambush", [703] = "Garrote",
            [1833] = "Cheap Shot", [6770] = "Sap", [921] = "Pick Pocket",
        },
        -- auras at login: unit -> { spell ID, seconds it lasts, seconds left,
        -- harmful }. Slice and Dice up; Rupture and Expose Armor on the target.
        auras = {
            player = { { 5171, 21, 14 } },
            target = { { 1943, 16, 9, true }, { 8647, 30, 22, true } },
        },
        -- the quest log, every quest watched: { quest ID, title, level,
        -- objectives = { { text, have, need } } }. Believable for an Alliance
        -- rogue of 34 in Arathi, not copied from the game's data.
        quests = {
            { 681, "Northfold Manor", 34, { { "Syndicate Highwayman slain", 7, 10 },
                { "Syndicate Conjuror slain", 4, 6 } } },
            { 658, "Hints of a New Plague?", 35, { { "Courier's Satchel", 0, 1 } } },
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
    -- its own powers, so a session changing them leaves the fixture alone
    if c.player.powers then
        p.powers = {}
        for t, v in pairs(c.player.powers) do
            p.powers[t] = { v[1], v[2] }
        end
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
    -- the class's spells, by ID and name, with their icons (GetSpellInfo,
    -- tooltips, auras)
    for _, list in ipairs({ c.actions or {}, c.stealthActions or {} }) do
        for _, a in pairs(list) do
            local name = (c.names or {})[a[1]]
            if name and not sim.spells[a[1]] then
                sim:AddSpell(a[1], { name = name, icon = a[2] })
            end
        end
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
            local me = sim.units.player or {}
            local cp = me.powers and me.powers[4]
            return (cp and unit == "player" and target == "target" and sim.units.target) and cp[1] or 0
        end,
        GetActionCount = function() return 0 end,
        GetActionCooldown = function() return 0, 0, 1, 1 end,
        IsUsableAction = function(slot) return has(slot), false end,
    }
    for name, fn in pairs(mocks) do
        sim:Mock(name, fn)
    end
    -- the session's own copy, which addon-live's `quest` moves along
    local quests = {}
    for i, q in ipairs(c.quests or {}) do
        local objectives = {}
        for k, o in ipairs(q[4]) do
            objectives[k] = { o[1], o[2], o[3] }
        end
        quests[i] = { q[1], q[2], q[3], objectives }
    end
    sim.fixtureQuests = quests
    M.mockQuests(sim, quests)
end

-- The quest log as the objective tracker reads it, every quest watched.
-- Objective text in Forever's style ("Syndicate Highwayman slain: 7/10").
function M.mockQuests(sim, quests)
    local byID = {}
    for i, q in ipairs(quests) do
        byID[q[1]] = i
    end
    local function done(q)
        for _, o in ipairs(q[4]) do
            if o[2] < o[3] then
                return false
            end
        end
        return true
    end
    local function objective(q, k)
        local o = q[4][k]
        if o then
            return ("%s: %d/%d"):format(o[1], o[2], o[3]), "monster", o[2] >= o[3], o[2], o[3]
        end
    end
    local Q = {
        GetNumQuestWatches = function() return #quests end,
        GetQuestIDForQuestWatchIndex = function(i) return quests[i] and quests[i][1] end,
        GetNumQuestLogEntries = function() return #quests, #quests end,
        GetLogIndexForQuestID = function(id) return byID[id] end,
        GetQuestIDForLogIndex = function(i) return quests[i] and quests[i][1] end,
        GetTitleForQuestID = function(id) return byID[id] and quests[byID[id]][2] end,
        IsComplete = function(id) return byID[id] and done(quests[byID[id]]) or false end,
        IsFailed = function() return false end,
        IsOnMap = function(id) return byID[id] ~= nil, false end,
        IsQuestFlaggedCompleted = function() return false end,
        GetTimeAllowed = function() return nil end,
        GetInfo = function(i)
            local q = quests[i]
            if not q then
                return nil
            end
            return { title = q[2], questLogIndex = i, questID = q[1], campaignID = nil, level = q[3],
                difficultyLevel = q[3], suggestedGroup = 0, frequency = 0, isHeader = false, isCollapsed = false,
                startEvent = false, isTask = false, isBounty = false, isStory = false, isScaling = false,
                isOnMap = true, hasLocalPOI = false, isHidden = false, isAutoComplete = false,
                overridesSortOrder = false, readyForTranslation = true, isInternalOnly = false,
                isAbandonOnDisable = false, isComplete = done(q) }
        end,
        GetQuestObjectives = function(id)
            local q = byID[id] and quests[byID[id]]
            local out = {}
            for k in ipairs(q and q[4] or {}) do
                local text, kind, finished, have, need = objective(q, k)
                out[k] = { text = text, type = kind, finished = finished, numFulfilled = have, numRequired = need }
            end
            return out
        end,
    }
    for name, fn in pairs(Q) do
        sim:Mock("C_QuestLog." .. name, fn)
    end
    -- the quests' data is loaded (QuestCache waits on it for the title), and
    -- none of them is a world task (whose name would win, even "")
    sim:Mock("C_QuestLog.HaveQuestData", function(id) return byID[id] ~= nil end)
    -- asked for, a quest's data arrives a moment later, as from the server
    sim:Mock("C_QuestLog.RequestLoadQuestByID", function(id)
        sim:_addTimer(0, function()
            sim:FireEvent("QUEST_DATA_LOAD_RESULT", id, byID[id] ~= nil)
        end)
    end)
    -- the "[34]" before a title, and its colour against the player's level
    sim:Mock("C_QuestLog.GetQuestDifficultyLevel", function(id) return byID[id] and quests[byID[id]][3] or 0 end)
    sim:Mock("C_TaskQuest.GetQuestInfoByQuestID", function() return nil end)
    sim:Mock("GetNumQuestLeaderBoards", function(i) return quests[i] and #quests[i][4] or 0 end)
    sim:Mock("GetQuestLogLeaderBoard", function(k, i)
        if quests[i] then
            local text, kind, finished = objective(quests[i], k)
            return text, kind, finished
        end
    end)
    -- ordinary quests, not campaign or "important" ones
    sim:Mock("C_QuestInfoSystem.GetQuestClassification", function()
        local e = sim.env.Enum.QuestClassification
        return e and e.Normal or nil
    end)
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
    for _, unit in ipairs({ "player", "target" }) do
        for _, a in ipairs((c.auras or {})[unit] or {}) do
            if sim.units[unit] then
                sim:AddAura(unit, a[1], { duration = a[2], expirationTime = sim.time + a[3], harmful = a[4] })
            end
        end
    end
    sim:FireEvent("ACTIONBAR_SLOT_CHANGED", 0)
    sim:FireEvent("UPDATE_SHAPESHIFT_FORM")
    sim:FireEvent("UPDATE_BONUS_ACTIONBAR")
    sim:FireEvent("UPDATE_BINDINGS")
    if c.quests then
        sim:FireEvent("QUEST_LOG_UPDATE")
        sim:FireEvent("QUEST_WATCH_LIST_CHANGED")
    end
end

return M
