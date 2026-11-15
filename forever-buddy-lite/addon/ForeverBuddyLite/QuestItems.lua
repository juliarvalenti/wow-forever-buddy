-- /fbl items [level]: quest rewards this character could still earn, and
-- dungeon boss drops, from a few levels below it up to [level] (default: six
-- above), found through QuestieDB's public API (ItemDB / QuestDB / NpcDB,
-- Forever corrections applied), with stats from each item's tooltip, the same
-- lines as gear. The reader scores them like auction listings
-- (fbl upgrades source=items). The list is built aside and saved whole at the
-- end, so a reload mid-search keeps the last one.

local _, ns = ...
local read, num, now, say = ns.read, ns.num, ns.now, ns.say

local BELOW, ABOVE = 3, 6 -- required level window around the player's

-- Dungeons by AreaTable id: an item a few NPCs here drop is a boss or named drop.
local DUNGEONS = { [1581] = "Deadmines", [718] = "Wailing Caverns", [209] = "Shadowfang Keep", [719] = "Blackfathom Deeps",
    [717] = "Stockade", [721] = "Gnomeregan", [491] = "Razorfen Kraul", [796] = "Scarlet Monastery", [722] = "Razorfen Downs",
    [1337] = "Uldaman", [1176] = "Zul'Farrak", [2100] = "Maraudon", [1477] = "Sunken Temple", [1584] = "Blackrock Depths",
    [1583] = "Blackrock Spire", [2557] = "Dire Maul", [2017] = "Stratholme", [2057] = "Scholomance", [2437] = "Ragefire Chasm" }

local searching = false

-- Stats for each found item, retrying the ones the client hasn't loaded.
local function loadStats(found, list, tries)
    local left = {}
    for id, src in pairs(found) do
        local _, itemType, subType, equipLoc, _, classID, subClassID = read("C_Item.GetItemInfoInstant", id)
        local tip = ns.statLines(nil, id)
        local name, _, quality, _, minLevel = ns.itemInfo(id)
        if read("C_Item.DoesItemExistByID", id) == false then
            -- in Questie's data but not in this client: nothing to load
        elseif not tip or #tip == 0 or not name then
            read("C_Item.RequestLoadItemDataByID", id)
            left[id] = src
        elseif src.quests or (num(quality) or 0) >= 2 then -- drops: green and better only
            list[tostring(id)] = { id = id, link = "item:" .. id, name = name, quality = quality, slot = equipLoc,
                type = itemType, subtype = subType, class = classID, subclass = subClassID, req = num(minLevel), tip = tip,
                quests = src.quests, drops = src.drops, at = now() }
        end
    end
    if next(left) and tries < 10 then
        C_Timer.After(2, function()
            loadStats(left, list, tries + 1)
        end)
        return
    end
    -- Why the rest failed, for the reader: name loaded? raw tooltip lines?
    local missed, why = 0, {}
    for id in pairs(left) do
        missed = missed + 1
        if missed <= 15 then
            local data = read("C_TooltipInfo.GetItemByID", id)
            local raw = {}
            for _, line in ipairs(type(data) == "table" and type(data.lines) == "table" and data.lines or {}) do
                raw[#raw + 1] = type(line) == "table" and line.leftText or "?"
            end
            why[tostring(id)] = { name = (ns.itemInfo(id)), exists = read("C_Item.DoesItemExistByID", id),
                cached = read("C_Item.IsItemDataCachedByID", id), tooltip = data ~= nil, lines = raw }
        end
    end
    ns.debug("items_missed", why)
    ns.db.items = list
    searching = false
    say(string.format("stats saved for %d quest rewards and dungeon drops%s. /fbl r to hand them over.", ns.count(list),
        missed > 0 and (" (" .. missed .. " wouldn't load)") or ""))
    ns.fire("changed")
end

local function search(maxLevel)
    local Q, I, N = _G.QuestDB, _G.ItemDB, _G.NpcDB
    if type(Q) ~= "table" or type(I) ~= "table" or not I.GetAllIds then
        say("/fbl items needs Questie (QuestieDB) loaded.")
        return
    end
    if searching then
        say("already looking; it'll say when it's done.")
        return
    end
    searching = true
    local level = num(read(UnitLevel, "player")) or 1
    local top = maxLevel or level + ABOVE
    local classBit = bit.lshift(1, (select(3, read(UnitClass, "player")) or 1) - 1)
    local masks = _G.LibQuestieDB and LibQuestieDB.Enum and LibQuestieDB.Enum.factionRaceMasks or {}
    local raceMask = masks[read(UnitFactionGroup, "player")]
    -- No mask (or 0) means anyone; otherwise the bits must overlap.
    local function allows(mask, want)
        return not num(mask) or mask == 0 or not want or bit.band(mask, want) ~= 0
    end
    local ids = I.GetAllIds()
    local found, i = {}, 0
    say(string.format("looking through Questie's quest rewards and dungeon drops, level %d-%d...", level - BELOW, top))
    local function step()
        local stop = math.min(#ids, i + 1000) -- a thousand items a frame
        for k = i + 1, stop do
            local id = ids[k]
            local _, _, _, equipLoc, _, classID = read("C_Item.GetItemInfoInstant", id)
            if ns.isGear(equipLoc, classID) then
                local rewards = read(I.Get, id, "questRewards")
                local npcDrops = read(I.Get, id, "npcDrops")
                local hasRewards = type(rewards) == "table" and #rewards > 0
                -- A handful of droppers: a boss or named mob, not a world drop.
                local hasDrops = N and type(npcDrops) == "table" and #npcDrops > 0 and #npcDrops <= 8
                if hasRewards or hasDrops then
                    local req = num(read(I.Get, id, "requiredLevel"))
                    if not req or req == 0 then
                        local ilvl = num(read(I.Get, id, "itemLevel"))
                        req = ilvl and ilvl - 5
                    end
                    if req and req >= level - BELOW and req <= top then
                        local drops
                        for _, npc in ipairs(hasDrops and npcDrops or {}) do
                            local where = DUNGEONS[num(read(N.Get, npc, "zoneID")) or 0]
                            if where then
                                drops = drops or {}
                                drops[#drops + 1] = { npc = read(N.Get, npc, "name"), dungeon = where }
                            end
                        end
                        local quests = {}
                        for _, q in ipairs(hasRewards and rewards or {}) do
                            local done = read("C_QuestLog.IsQuestFlaggedCompleted", q)
                            if not done and allows(read(Q.Get, q, "requiredClasses"), classBit)
                                and allows(read(Q.Get, q, "requiredRaces"), raceMask) then
                                quests[#quests + 1] = { id = q, name = read(Q.Get, q, "name"), level = read(Q.Get, q, "questLevel") }
                            end
                        end
                        if #quests > 0 or drops then
                            found[id] = { quests = #quests > 0 and quests or nil, drops = drops }
                        end
                    end
                end
            end
        end
        i = stop
        if i < #ids then
            C_Timer.After(0, step)
        else
            loadStats(found, {}, 0)
        end
    end
    step()
end

ns.command({ name = "items", args = "[level]",
    help = "quest rewards and dungeon drops near your level (or up to [level]), with stats (needs Questie)",
    fn = function(rest)
        if ns.db then
            search(tonumber(rest:match("^(%d+)")))
        end
    end })
