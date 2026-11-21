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

-- This character's rank in a skill line, from the skill list (the same source as the snapshot).
local function skillLineRank(name)
    local n = num(read("C_SkillInfo.GetNumSkillLines"))
    for i = 1, n or 0 do
        local s = read("C_SkillInfo.GetSkillLineInfo", i)
        if type(s) == "table" and s.name == name and not s.isHeader then
            return num(s.rank) or num(s.skillRank)
        end
    end
    for i = 1, num(read("GetNumSkillLines")) or 0 do
        local lname, isHeader, _, rank = read("GetSkillLineInfo", i)
        if lname == name and not isHeader then
            return num(rank)
        end
    end
end

-- Someone else's profession (a link from chat, a guildmate's, a crafting NPC) shows in the same
-- window. Saved as ours, it made a 95 tailor look like 235 with Runecloth recipes.
local function viewingOthers()
    for _, f in ipairs({ "C_TradeSkillUI.IsTradeSkillLinked", "C_TradeSkillUI.IsTradeSkillGuild",
        "C_TradeSkillUI.IsNPCCrafting", "IsTradeSkillLinked" }) do
        if read(f) == true then
            return true
        end
    end
    return false
end

local warned = {}

local function saveProfession()
    local c = ns.char()
    if not c or viewingOthers() then
        return
    end
    local good, name, p = pcall(classicTradeSkill)
    if not (good and name and p and #p.recipes > 0) then
        good, name, p = pcall(modernTradeSkill)
    end
    if not (good and name and p and #p.recipes > 0) then
        return
    end
    -- The window must agree with the character's own skill list. If it doesn't, it isn't this
    -- character's (or the API reported another line): save nothing, and drop an earlier wrong save.
    local own = skillLineRank(name)
    if own and p.rank and own ~= p.rank then
        if type(c.professions) == "table" and c.professions[name] and c.professions[name].rank ~= own then
            c.professions[name] = nil
            ns.fire("changed")
        end
        if not warned[name] then
            warned[name] = true
            ns.say(string.format("not saving the %s window: it shows %d, your skill is %d.", name, p.rank, own))
        end
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

-- Vendors: everything a merchant sells when you open its window, with the price in gold and any
-- other cost (Honor, marks, tokens), limited stock, and the tooltip's "Requires ..." lines (a PvP
-- rank, a reputation, a level). Account-wide, by vendor name; where you stood when you saw it.
-- ForeverBuddyLiteDB.vendors[name] = { at, by, zone, subzone, x, y, items = { { name, id, price,
--   stack, stock, usable, costs = { { name, count, id } }, requires = { "..." } } } }
local function merchantItem(i)
    local name, price, stack, stock, usable, extended
    local info = read("C_MerchantFrame.GetItemInfo", i)
    if type(info) == "table" then
        name, price, stack, stock, usable, extended = info.name, info.price, info.stackCount, info.numAvailable, info.isUsable,
            info.hasExtendedCost
    else
        local r = ns.pack(read("GetMerchantItemInfo", i))
        name, price, stack, stock, usable, extended = r[1], r[3], r[4], r[5], r[7], r[8]
    end
    if type(name) ~= "string" or name == "" then
        return nil
    end
    local link = read("GetMerchantItemLink", i)
    local item = { name = name, id = ns.itemID(link), price = num(price), stack = num(stack),
        stock = (num(stock) or -1) >= 0 and num(stock) or nil, usable = usable and true or false }
    if extended then
        for j = 1, num(read("GetMerchantItemCostInfo", i)) or 0 do
            local _, value, costLink, currency = read("GetMerchantItemCostItem", i, j)
            local costName = currency or (costLink and costLink:match("%[(.-)%]"))
            if num(value) and costName then
                item.costs = item.costs or {}
                item.costs[#item.costs + 1] = { name = costName, count = num(value), id = costLink and ns.itemID(costLink) or nil }
            end
        end
    end
    local tip = read("C_TooltipInfo.GetMerchantItem", i)
    for _, line in ipairs(type(tip) == "table" and type(tip.lines) == "table" and tip.lines or {}) do
        local text = line.leftText
        if type(text) == "string" and text:find("^Requires ") then
            item.requires = item.requires or {}
            item.requires[#item.requires + 1] = text
        end
    end
    return item
end

local function saveMerchant()
    local db = ns.db
    local n = num(read("GetMerchantNumItems"))
    if not db or not n or n == 0 then
        return
    end
    local items = {}
    for i = 1, n do
        local ok, item = pcall(merchantItem, i)
        if ok and item then
            items[#items + 1] = item
        end
    end
    if #items == 0 then
        return
    end
    local v = { at = now(), by = ns.key, zone = read("GetRealZoneText"), subzone = read("GetSubZoneText"), items = items }
    local map = read("C_Map.GetBestMapForUnit", "player")
    local pos = map and read("C_Map.GetPlayerMapPosition", map, "player")
    if pos and pos.x then
        v.x, v.y = ns.round(pos.x * 100, 1), ns.round(pos.y * 100, 1)
    end
    db.vendors = type(db.vendors) == "table" and db.vendors or {}
    db.vendors[read(UnitName, "npc") or "vendor"] = v
    ns.fire("changed")
end

-- Windows fire their update events in bursts: read once they settle.
local professionSoon, trainerSoon = ns.debounce(0.5, saveProfession), ns.debounce(0.5, saveTrainer)
for _, e in ipairs({ "TRADE_SKILL_SHOW", "TRADE_SKILL_UPDATE", "TRADE_SKILL_LIST_UPDATE" }) do
    ns.on(e, professionSoon)
end
ns.on("TRAINER_SHOW", trainerSoon)
local merchantSoon = ns.debounce(0.5, saveMerchant)
ns.on("MERCHANT_SHOW", merchantSoon)
ns.on("MERCHANT_UPDATE", merchantSoon)
ns.on("TRAINER_UPDATE", trainerSoon)
