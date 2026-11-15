-- Professions and trainers.
-- The profession window, while open: every recipe it lists with its colour
-- now, reagents, how many it makes and the tools it needs (an anvil shows up
-- as a tool). Collapsed headers and the window's filters hide recipes; we
-- leave the window as the player set it.
-- A trainer's list: what each entry needs (skill, level), costs, and whether
-- the visiting character knows it. Account-wide, by trainer name.

local _, ns = ...
local read, num, now = ns.read, ns.num, ns.now

local DIFFICULTY = { optimal = "orange", medium = "yellow", easy = "green", trivial = "grey" }

local function classicTradeSkill()
    local name, rank, max = read("GetTradeSkillLine")
    if type(name) ~= "string" or name == "" or name == "UNKNOWN" then
        return nil
    end
    local recipes = {}
    for i = 1, num(read("GetNumTradeSkills")) or 0 do
        local rname, kind, available = read("GetTradeSkillInfo", i)
        if type(rname) == "string" and kind ~= "header" and kind ~= "subheader" then
            local r = { name = rname, color = DIFFICULTY[kind] or kind, can_make = num(available),
                id = ns.itemID(read("GetTradeSkillItemLink", i)) }
            local lo, hi = read("GetTradeSkillNumMade", i)
            if num(lo) and (lo ~= 1 or hi ~= 1) then
                r.makes = { lo, num(hi) or lo }
            end
            local t = ns.pack(read("GetTradeSkillTools", i)) -- name, have, name, have...
            for k = 1, t.n, 2 do
                if type(t[k]) == "string" and t[k] ~= "" then
                    r.tools = r.tools or {}
                    r.tools[#r.tools + 1] = t[k]
                end
            end
            for j = 1, num(read("GetTradeSkillNumReagents", i)) or 0 do
                local rn, _, count = read("GetTradeSkillReagentInfo", i, j)
                r.reagents = r.reagents or {}
                r.reagents[#r.reagents + 1] = { name = rn, count = num(count),
                    id = ns.itemID(read("GetTradeSkillReagentItemLink", i, j)) }
            end
            recipes[#recipes + 1] = r
        end
    end
    return name, { rank = num(rank), max = num(max), recipes = recipes }
end

-- The modern profession frame, in case Forever uses it.
local MODERN_DIFFICULTY = { [0] = "orange", [1] = "yellow", [2] = "green", [3] = "grey" }

local function modernTradeSkill()
    local base = read("C_TradeSkillUI.GetBaseProfessionInfo")
    local ids = read("C_TradeSkillUI.GetAllRecipeIDs")
    if type(base) ~= "table" or type(base.professionName) ~= "string" or type(ids) ~= "table" then
        return nil
    end
    local recipes = {}
    for _, id in ipairs(ids) do
        local info = read("C_TradeSkillUI.GetRecipeInfo", id)
        if type(info) == "table" and info.learned and info.name then
            local r = { name = info.name, recipe_id = id, color = MODERN_DIFFICULTY[info.relativeDifficulty],
                can_make = num(info.numAvailable) }
            local s = read("C_TradeSkillUI.GetRecipeSchematic", id, false)
            if type(s) == "table" then
                r.id = num(s.outputItemID)
                if num(s.quantityMin) and (s.quantityMin ~= 1 or s.quantityMax ~= 1) then
                    r.makes = { s.quantityMin, num(s.quantityMax) or s.quantityMin }
                end
                for _, slot in ipairs(type(s.reagentSlotSchematics) == "table" and s.reagentSlotSchematics or {}) do
                    local first = slot.reagents and slot.reagents[1]
                    if first and first.itemID then
                        r.reagents = r.reagents or {}
                        r.reagents[#r.reagents + 1] = { id = first.itemID, count = num(slot.quantityRequired),
                            name = read("C_Item.GetItemNameByID", first.itemID) }
                    end
                end
            end
            recipes[#recipes + 1] = r
        end
    end
    return base.professionName, { rank = num(base.skillLevel), max = num(base.maxSkillLevel), recipes = recipes }
end

local function saveProfession()
    local c = ns.char()
    if not c then
        return
    end
    local good, name, p = pcall(classicTradeSkill)
    if not (good and name and p and #p.recipes > 0) then
        good, name, p = pcall(modernTradeSkill)
    end
    if not (good and name and p and #p.recipes > 0) then
        return
    end
    p.at = now()
    c.professions = type(c.professions) == "table" and c.professions or {}
    c.professions[name] = p
    ns.fire("changed")
end

local TRAINER_STATUS = { available = "can learn", unavailable = "not yet", used = "known" }

local function saveTrainer()
    local db = ns.db
    local n = num(read("GetNumTrainerServices"))
    if not db or not n or n == 0 then
        return
    end
    local services = {}
    for i = 1, n do
        -- Classic returns name, subText, type, texture; Forever puts them in
        -- another order (an icon id where the type was). Find the type by value.
        local r = ns.pack(read("GetTrainerServiceInfo", i))
        local name, sub, status = r[1], nil, nil
        for k = 2, r.n do
            local v = r[k]
            if type(v) == "string" and (TRAINER_STATUS[v] or v == "header") then
                status = status or v
            elseif type(v) == "string" and v ~= "" then
                sub = sub or v
            end
        end
        if type(name) == "string" and status ~= "header" then
            local s = { name = name, rank = sub, status = TRAINER_STATUS[status] or status,
                level_req = num(read("GetTrainerServiceLevelReq", i)), cost = num((read("GetTrainerServiceCost", i))),
                id = ns.itemID(read("GetTrainerServiceItemLink", i)) }
            local skill, level = read("GetTrainerServiceSkillReq", i)
            if type(skill) == "string" and skill ~= "" then
                s.skill, s.skill_req = skill, num(level)
            end
            services[#services + 1] = s
        end
    end
    db.trainers = type(db.trainers) == "table" and db.trainers or {}
    db.trainers[read(UnitName, "npc") or "trainer"] = { at = now(), by = ns.key, services = services }
    ns.fire("changed")
end

-- Windows fire their update events in bursts: read once they settle.
local professionSoon, trainerSoon = ns.debounce(0.5, saveProfession), ns.debounce(0.5, saveTrainer)
for _, e in ipairs({ "TRADE_SKILL_SHOW", "TRADE_SKILL_UPDATE", "TRADE_SKILL_LIST_UPDATE" }) do
    ns.on(e, professionSoon)
end
ns.on("TRAINER_SHOW", trainerSoon)
ns.on("TRAINER_UPDATE", trainerSoon)
