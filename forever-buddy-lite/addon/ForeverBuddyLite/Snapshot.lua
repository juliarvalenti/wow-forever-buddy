-- Snapshot: this character into ForeverBuddyLiteDB.characters["Name-Realm"].
-- Taken a few seconds after login, a few seconds after bags, money, gear,
-- skills, level, quests or talents change, and at logout. Bank and mail can
-- only be read while they're open, so those are as of the last visit.

local _, ns = ...
local read, num, round, now = ns.read, ns.num, ns.round, ns.now

-- Sections -----------------------------------------------------------------

local function identity()
    local name, realm = read(UnitFullName, "player")
    local _, class = read(UnitClass, "player")
    local _, race = read(UnitRace, "player")
    local guild, rank = read(GetGuildInfo, "player")
    local x, y
    local mapID = read("C_Map.GetBestMapForUnit", "player")
    if mapID then
        local pos = read("C_Map.GetPlayerMapPosition", mapID, "player")
        if type(pos) == "table" and pos.GetXY then
            local ok, px, py = pcall(pos.GetXY, pos)
            if ok then
                x, y = round(px and px * 100, 1), round(py and py * 100, 1)
            end
        end
    end
    local mapInfo = mapID and read("C_Map.GetMapInfo", mapID)
    return {
        name = name,
        realm = realm or read(GetNormalizedRealmName),
        class = class,
        race = race,
        sex = read(UnitSex, "player"),
        faction = read(UnitFactionGroup, "player"),
        level = read(UnitLevel, "player"),
        xp = read(UnitXP, "player"),
        xp_max = read(UnitXPMax, "player"),
        rested = read(GetXPExhaustion),
        resting = read(IsResting),
        money = read(GetMoney),
        guild = guild,
        guild_rank = rank,
        zone = read(GetRealZoneText),
        subzone = read(GetSubZoneText),
        map_id = mapID,
        map_name = type(mapInfo) == "table" and mapInfo.name or nil,
        x = x,
        y = y,
        bind = read(GetBindLocation),
        ilvl = round(read(GetAverageItemLevel), 2),
    }
end

-- Numbers for gear advice: health, mana, base stats, spell power by school,
-- crits, hit, armor.
local function stats()
    local out = {
        health = read(UnitHealthMax, "player"),
        power = read(UnitPowerMax, "player"),
        power_type = select(2, read(UnitPowerType, "player")),
        armor = select(2, read(UnitArmor, "player")),
        stats = {},
        spell_power = {},
        spell_crit = {},
    }
    for i, n in ipairs({ "strength", "agility", "stamina", "intellect", "spirit" }) do
        local _, effective = read(UnitStat, "player", i)
        out.stats[n] = effective
    end
    for i, n in ipairs({ "physical", "holy", "fire", "nature", "frost", "shadow", "arcane" }) do
        if i > 1 then
            out.spell_power[n] = read(GetSpellBonusDamage, i)
            out.spell_crit[n] = round(read(GetSpellCritChance, i), 2)
        end
    end
    out.healing = read(GetSpellBonusHealing)
    out.melee_crit = round(read(GetCritChance), 2)
    out.ranged_crit = round(read(GetRangedCritChance), 2)
    out.dodge = round(read(GetDodgeChance), 2)
    out.parry = round(read(GetParryChance), 2)
    out.block = round(read(GetBlockChance), 2)
    out.spell_hit = round(read(GetSpellHitModifier), 2)
    out.melee_hit = round(read(GetHitModifier), 2)
    local base, pos, neg = read(UnitAttackPower, "player")
    out.attack_power = num(base) and (base + (pos or 0) + (neg or 0)) or nil
    local mainSpeed, offSpeed = read(UnitAttackSpeed, "player")
    out.attack_speed = { main = round(mainSpeed, 2), off = round(offSpeed, 2) }
    local _, regen = read(GetManaRegen)
    out.mana_regen_casting = round(regen and regen * 5, 1) -- per 5 sec
    return out
end

-- Slots 1-19 are gear; 0 is ammo.
local function gear()
    local out = {}
    for slot = 0, 19 do
        local link = read(GetInventoryItemLink, "player", slot)
        if link then
            local cur, max = read(GetInventoryItemDurability, slot)
            out[slot] = {
                link = link,
                tip = ns.statLines(link),
                equip = select(4, read("C_Item.GetItemInfoInstant", link)),
                id = read(GetInventoryItemID, "player", slot),
                durability = num(cur) and num(max) and { cur, max } or nil,
            }
        end
    end
    return out
end

-- One container: { size, free, name, items[slot] = { link, id, count, bound, name } }.
local function container(bag)
    local size = read("C_Container.GetContainerNumSlots", bag)
    if type(size) ~= "number" or size <= 0 then
        return nil
    end
    local out = {
        size = size,
        free = read("C_Container.GetContainerNumFreeSlots", bag),
        name = read("C_Container.GetBagName", bag),
        items = {},
    }
    for slot = 1, size do
        local info = read("C_Container.GetContainerItemInfo", bag, slot)
        if type(info) == "table" and info.hyperlink then
            out.items[slot] = {
                link = info.hyperlink,
                id = info.itemID,
                count = info.stackCount,
                bound = info.isBound,
                name = ns.itemName(info.itemID),
            }
        end
    end
    return out
end

-- The keyring is bag -2 on Forever; 0-4 (and 5 if the client has it) are carried.
local function bags()
    local out = {}
    local last = (num(NUM_BAG_SLOTS) or 4) + (num(NUM_REAGENTBAG_SLOTS) or 0)
    for bag = -2, last do
        if bag ~= -1 then
            out[bag] = container(bag)
        end
    end
    return { at = now(), bags = out }
end

local function bank()
    local ids = { (Enum and Enum.BagIndex and Enum.BagIndex.Bank) or -1 }
    local tabs = read("C_Bank.FetchPurchasedBankTabIDs", Enum and Enum.BankType and Enum.BankType.Character)
    if type(tabs) == "table" and #tabs > 0 then
        for _, id in ipairs(tabs) do
            ids[#ids + 1] = id
        end
    else
        local first = (num(NUM_BAG_SLOTS) or 4) + 1
        for bag = first, first + (num(NUM_BANKBAGSLOTS) or 7) - 1 do
            ids[#ids + 1] = bag
        end
    end
    local out = {}
    for _, id in ipairs(ids) do
        out[id] = container(id)
    end
    return { at = now(), bags = out }
end

local function mail()
    local n = read(GetInboxNumItems)
    if type(n) ~= "number" then
        return nil
    end
    local letters = {}
    for i = 1, math.min(n, 50) do
        local _, _, sender, subject, money, cod, daysLeft, itemCount = read(GetInboxHeaderInfo, i)
        local letter = { sender = sender, subject = subject, money = money, cod = cod,
            days_left = round(daysLeft, 1), items = {} }
        if num(itemCount) and itemCount > 0 then
            for a = 1, (num(ATTACHMENTS_MAX_RECEIVE) or 16) do
                local link = read(GetInboxItemLink, i, a)
                if link then
                    local _, id, _, count = read(GetInboxItem, i, a)
                    letter.items[#letter.items + 1] = { link = link, id = id, count = count }
                    if #letter.items >= itemCount then
                        break
                    end
                end
            end
        end
        letters[#letters + 1] = letter
    end
    return { at = now(), letters = letters }
end

local function quests()
    local log = {}
    for i = 1, read("C_QuestLog.GetNumQuestLogEntries") or 0 do
        local info = read("C_QuestLog.GetInfo", i)
        if type(info) == "table" and not info.isHeader and num(info.questID) then
            local id = info.questID
            local q = {
                id = id,
                title = info.title,
                level = info.level,
                suggested_group = num(info.suggestedGroup) and info.suggestedGroup > 0 and info.suggestedGroup or nil,
                complete = read("C_QuestLog.IsComplete", id) or nil,
                ready = read("C_QuestLog.ReadyForTurnIn", id) or nil,
                failed = info.isFailed or nil,
                -- Reward gold in copper and XP, by quest id (the modern calls; nothing selected in the log).
                money = num(read("GetQuestLogRewardMoney", id)) or nil,
                xp = num(read("GetQuestLogRewardXP", id)) or nil,
                objectives = {},
            }
            for _, o in ipairs(read("C_QuestLog.GetQuestObjectives", id) or {}) do
                if type(o) == "table" then
                    q.objectives[#q.objectives + 1] = { text = o.text, type = o.type, done = o.finished,
                        have = o.numFulfilled, need = o.numRequired }
                end
            end
            log[#log + 1] = q
        end
    end
    local completed = read("C_QuestLog.GetAllCompletedQuestIDs")
    if type(completed) ~= "table" then
        local t = read(GetQuestsCompleted)
        if type(t) == "table" then
            completed = {}
            for id in pairs(t) do
                completed[#completed + 1] = id
            end
        end
    end
    if type(completed) == "table" then
        table.sort(completed)
    end
    return { log = log, completed = completed }
end

-- What the client's skill line table looks like, once a session, so the reader
-- can be fixed if its fields differ.
local skillKeysNoted = false
local function noteSkillKeys()
    local first = read("C_SkillInfo.GetSkillLineInfo", 1)
    if skillKeysNoted or type(first) ~= "table" then
        return
    end
    local keys = {}
    for k in pairs(first) do
        keys[#keys + 1] = tostring(k)
    end
    table.sort(keys)
    ns.debug("skill_info_keys", table.concat(keys, ","))
    skillKeysNoted = true
end

-- Every skill line: weapon skills, Lockpicking, professions, languages.
local function skills()
    local out = {}
    -- Forever replaced the global skill line API with C_SkillInfo (Questie's
    -- QuestieCompat and FlowUp read it this way). Collapsed headers hide
    -- their lines; we don't expand them, so the player's skill window stays as it is.
    local n = read("C_SkillInfo.GetNumSkillLines")
    if num(n) then
        local seen = {}
        for i = 1, n do
            local s = read("C_SkillInfo.GetSkillLineInfo", i)
            -- Professions come twice on Forever (the line and a child line with
            -- another id); keep the first of each name.
            if type(s) == "table" and s.name and not s.isHeader and not seen[s.name] then
                seen[s.name] = true
                local max = num(s.maxRank) or num(s.skillMaxRank) or num(s.maxSkillRank) or num(s.max)
                local bonus = num(s.modifier) or num(s.skillModifier) or num(s.tempRank)
                out[#out + 1] = { name = s.name, id = s.skillID, rank = s.rank, max = max,
                    bonus = (bonus and bonus ~= 0) and bonus or nil, category = s.skillLineCategoryID }
            elseif type(s) == "table" and s.isHeader and s.isCollapsed then
                out[#out + 1] = { name = s.name, collapsed = true }
            end
        end
        noteSkillKeys()
        return out
    end
    for i = 1, read(GetNumSkillLines) or 0 do
        local name, isHeader, _, rank, _, mod, max = read(GetSkillLineInfo, i)
        if name and not isHeader then
            out[#out + 1] = { name = name, rank = rank, bonus = (num(mod) and mod ~= 0) and mod or nil, max = max }
        end
    end
    return out
end

-- Forever's talents are a C_Traits tree (Leatrix Plus and Questie read them
-- the same way). Each node: position, ranks, and the spell it grants. The
-- legacy tree (system 45) holds talents like Diplomat. Trees aren't labelled
-- by the API; the reader splits them by column gaps.
local function traitNodes(configID)
    local config = configID and read("C_Traits.GetConfigInfo", configID)
    if type(config) ~= "table" or type(config.treeIDs) ~= "table" then
        return nil
    end
    local out = {}
    for _, treeID in ipairs(config.treeIDs) do
        for _, nodeID in ipairs(read("C_Traits.GetTreeNodes", treeID) or {}) do
            local node = read("C_Traits.GetNodeInfo", configID, nodeID)
            if type(node) == "table" and num(node.posX) then
                local entryID = node.activeEntry and node.activeEntry.entryID
                    or (type(node.entryIDs) == "table" and node.entryIDs[1])
                local entry = entryID and read("C_Traits.GetEntryInfo", configID, entryID)
                local def = type(entry) == "table" and entry.definitionID
                    and read("C_Traits.GetDefinitionInfo", entry.definitionID)
                local spellID = type(def) == "table" and def.spellID or nil
                out[#out + 1] = {
                    node = nodeID, tree = treeID, x = node.posX, y = node.posY,
                    rank = num(node.ranksPurchased) or num(node.activeRank) or num(node.currentRank) or 0,
                    max = node.maxRanks, spell = spellID, name = ns.spellName(spellID),
                }
            end
        end
    end
    return out
end

local function talents()
    local nodes = traitNodes(read("C_ClassTalents.GetActiveConfigID"))
    if nodes and #nodes > 0 then
        local legacyID = read("C_Traits.GetConfigIDBySystemID", 45)
        return {
            system = "traits",
            unspent = read(UnitCharacterPoints, "player"),
            nodes = nodes,
            legacy = legacyID and traitNodes(legacyID) or nil,
        }
    end
    local tabs = {}
    for t = 1, read(GetNumTalentTabs) or 0 do
        local a, b, c = read(GetTalentTabInfo, t)
        -- Classic returns (name, icon, pointsSpent); newer clients (id, name, ...).
        local name = type(a) == "string" and a or b
        local tab = { name = name, points = type(a) == "string" and num(c) or nil, talents = {} }
        local spent = 0
        for i = 1, read(GetNumTalents, t) or 0 do
            local tname, _, row, col, rank, max = read(GetTalentInfo, t, i)
            if tname then
                tab.talents[#tab.talents + 1] = { name = tname, row = row, col = col, rank = rank, max = max }
                spent = spent + (num(rank) or 0)
            end
        end
        tab.points = tab.points or spent
        tabs[#tabs + 1] = tab
    end
    return { system = "classic", unspent = read(UnitCharacterPoints, "player"), tabs = tabs }
end

-- The spellbook, highest rank of each spell: { name, rank, id }.
local function spells()
    local out = {}
    -- Modern spellbook (C_SpellBook, as Questie uses on Forever).
    local lines = read("C_SpellBook.GetNumSpellBookSkillLines")
    local playerBank = Enum and Enum.SpellBookSpellBank and Enum.SpellBookSpellBank.Player
    if num(lines) and playerBank ~= nil then
        for t = 1, lines do
            local line = read("C_SpellBook.GetSpellBookSkillLineInfo", t)
            if type(line) == "table" then
                local first = num(line.itemIndexOffset) or 0
                for i = first + 1, first + (num(line.numSpellBookItems) or 0) do
                    local info = read("C_SpellBook.GetSpellBookItemInfo", i, playerBank)
                    if type(info) == "table" and info.name then
                        out[#out + 1] = { tab = line.name, name = info.name,
                            rank = info.subName ~= "" and info.subName or nil, id = info.spellID,
                            passive = info.isPassive or nil }
                    end
                end
            end
        end
        if #out > 0 then
            return out
        end
    end
    local book = BOOKTYPE_SPELL or "spell"
    local getTab = ns.resolve("GetSpellTabInfo") or ns.resolve("C_SpellBook.GetSpellBookSkillLineInfo")
    local nTabs = read(GetNumSpellTabs) or read("C_SpellBook.GetNumSpellBookSkillLines") or 0
    for t = 1, nTabs do
        local tabName, offset, count
        if ns.resolve("GetSpellTabInfo") then
            local n, _, o, c = read(GetSpellTabInfo, t)
            tabName, offset, count = n, o, c
        elseif getTab then
            local info = read(getTab, t)
            if type(info) == "table" then
                tabName, offset, count = info.name, info.itemIndexOffset, info.numSpellBookItems
            end
        end
        for i = (offset or 0) + 1, (offset or 0) + (count or 0) do
            local name, sub = read(GetSpellBookItemName, i, book)
            local _, id = read(GetSpellBookItemInfo, i, book)
            if name then
                out[#out + 1] = { tab = tabName, name = name, rank = sub ~= "" and sub or nil, id = id }
            end
        end
    end
    return out
end

local STANDINGS = { "Hated", "Hostile", "Unfriendly", "Neutral", "Friendly", "Honored", "Revered", "Exalted" }

local function reputation()
    local out = {}
    -- Modern API first (Questie uses it on Forever), classic as fallback.
    local n = read("C_Reputation.GetNumFactions")
    if num(n) then
        for i = 1, n do
            local f = read("C_Reputation.GetFactionDataByIndex", i)
            if type(f) == "table" and f.name and (not f.isHeader or f.isHeaderWithRep) then
                local lo, hi, cur = num(f.currentReactionThreshold), num(f.nextReactionThreshold), num(f.currentStanding)
                out[#out + 1] = { name = f.name, id = f.factionID, standing = STANDINGS[f.reaction] or f.reaction,
                    value = lo and cur and (cur - lo) or nil, max = lo and hi and (hi - lo) or nil }
            end
        end
        if #out > 0 then
            return out
        end
    end
    for i = 1, read(GetNumFactions) or 0 do
        local name, _, standing, min, max, value, _, _, isHeader, _, hasRep = read(GetFactionInfo, i)
        if name and (not isHeader or hasRep) then
            out[#out + 1] = { name = name, standing = STANDINGS[standing] or standing,
                value = num(value) and num(min) and (value - min) or nil,
                max = num(max) and num(min) and (max - min) or nil }
        end
    end
    return out
end

local function lockouts()
    local out = {}
    for i = 1, read(GetNumSavedInstances) or 0 do
        local name, _, reset, difficulty, locked, _, _, _, players, diffName, bosses, killed = read(GetSavedInstanceInfo, i)
        if name then
            out[#out + 1] = { name = name, resets_at = num(reset) and (now() + reset) or nil, locked = locked,
                difficulty = diffName or difficulty, players = players, bosses = bosses, killed = killed }
        end
    end
    return out
end

local function buffs()
    local out = {}
    for i = 1, 40 do
        local name, count, expires, spellId
        local aura = read("C_UnitAuras.GetAuraDataByIndex", "player", i, "HELPFUL")
        if type(aura) == "table" then
            name, count, expires, spellId = aura.name, aura.applications, aura.expirationTime, aura.spellId
        else
            local _
            name, _, count, _, _, expires, _, _, _, spellId = read(UnitBuff, "player", i)
        end
        if not name then
            break
        end
        local left = num(expires) and expires > 0 and read(GetTime) and round(expires - GetTime(), 0) or nil
        out[#out + 1] = { name = name, id = spellId, stacks = (num(count) and count > 1) and count or nil, left = left }
    end
    return out
end

-- What a section looks like when the client has already torn it down ------
-- A live character always has gear or skills, health, and bank bags while the
-- bank is open; a section that comes back like this never replaces a good one.

local function noItems(r)
    for _, b in pairs(r.bags or {}) do
        if type(b.items) == "table" and next(b.items) then
            return false
        end
    end
    return true
end

local function empty(r)
    return not next(r)
end

local BLANK = {
    bags = noItems,
    bank = function(r) return not next(r.bags or {}) end, -- no bank bags at all: the bank wasn't readable
    stats = function(r) return (r.health or 0) == 0 end,
    identity = function(r) return (r.ilvl or 0) == 0 and (r.xp_max or 0) == 0 end,
    talents = function(r) return not next(r.tabs or {}) and not next(r.nodes or {}) end,
    gear = empty, skills = empty, spells = empty, reputation = empty,
}

local SECTIONS = {
    { "identity", identity }, { "stats", stats }, { "gear", gear }, { "bags", bags },
    { "quests", quests }, { "skills", skills }, { "talents", talents }, { "spells", spells },
    { "reputation", reputation }, { "lockouts", lockouts }, { "buffs", buffs },
}
ns.SECTION_COUNT = #SECTIONS

-- The snapshot -------------------------------------------------------------

local bankOpen, mailOpen = false, false

local function capture(c, name, fn)
    local good, result = pcall(fn)
    if good and ns.commit(c, name, result, BLANK[name]) then
        if c.errors then
            c.errors[name] = nil
        end
        return true
    end
    c.errors = c.errors or {}
    c.errors[name] = good and "nil" or tostring(result)
    return false
end

-- Returns how many sections were good.
function ns.snapshot()
    local c = ns.char()
    if not c then
        return 0
    end
    c.errors = nil -- only this snapshot's
    local ok = 0
    for _, s in ipairs(SECTIONS) do
        if capture(c, s[1], s[2]) then
            ok = ok + 1
        end
    end
    if bankOpen then
        capture(c, "bank", bank)
    end
    if mailOpen then
        capture(c, "mail", mail)
    end
    c.saved_at = now()
    c.addon_version = ns.version
    ns.fire("changed")
    return ok
end

-- During play only what changed is read again: looting re-reads bags, not the
-- spellbook, reputations and every gear tooltip. Changes collect for a few
-- seconds and are read together, never in combat.
local SECTION = { bank = bank, mail = mail }
for _, s in ipairs(SECTIONS) do
    SECTION[s[1]] = s[2]
end

local dirty, waiting = {}, false

local function flush()
    if InCombatLockdown() then
        waiting = "combat" -- PLAYER_REGEN_ENABLED flushes
        return
    end
    waiting = false
    local c = ns.char()
    if not c or not next(dirty) then
        return
    end
    for name in pairs(dirty) do
        if (name ~= "bank" or bankOpen) and (name ~= "mail" or mailOpen) then
            capture(c, name, SECTION[name])
        end
    end
    dirty = {}
    c.saved_at = now()
    ns.fire("changed")
end

local function refresh(...)
    for i = 1, select("#", ...) do
        dirty[select(i, ...)] = true
    end
    if not waiting then
        waiting = "timer"
        C_Timer.After(5, flush)
    end
end

ns.on("PLAYER_REGEN_ENABLED", function()
    if waiting == "combat" then
        flush()
    end
end)

ns.on("PLAYER_ENTERING_WORLD", function()
    C_Timer.After(3, ns.snapshot) -- bags and quests fill in a moment after login
end)

ns.on("PLAYER_LOGOUT", function()
    -- Also fires on /reload, right before the file is written. /fbl r has just
    -- saved; on a real logout the blank guard keeps what play saved.
    if not ns.reloading then
        ns.snapshot()
    end
end)

-- Keep the save fresh during play, so a logout loses nothing: each event
-- re-reads only the sections it can change.
local CHANGES = {
    BAG_UPDATE_DELAYED = { "bags", "bank" }, -- bank only while it's open
    PLAYER_MONEY = { "identity" },
    PLAYER_EQUIPMENT_CHANGED = { "gear", "stats", "identity" },
    SKILL_LINES_CHANGED = { "skills" },
    PLAYER_LEVEL_UP = { "identity", "stats", "spells", "talents", "skills" },
    QUEST_LOG_UPDATE = { "quests" },
    QUEST_TURNED_IN = { "quests", "identity" },
    LEARNED_SPELL_IN_TAB = { "spells" },
    CHARACTER_POINTS_CHANGED = { "talents" },
    PLAYER_TALENT_UPDATE = { "talents" },
    PLAYERBANKSLOTS_CHANGED = { "bank" },
}
for event, sections in pairs(CHANGES) do
    ns.on(event, function()
        refresh(unpack(sections))
    end)
end

-- Gold over time: each change with when it happened, for the reader's wealth chart. Changes within
-- a minute of each other merge into one entry (looting a pack, vendoring a bag); the newest 2000
-- are kept, which is weeks of play.
local MONEY_LOG_MAX, MONEY_MERGE_SECONDS = 2000, 60

local function logMoney()
    local c = ns.char()
    local money = num(read(GetMoney))
    if not c or not money then
        return
    end
    c.money_log = type(c.money_log) == "table" and c.money_log or {}
    local log, t = c.money_log, now()
    local last = log[#log]
    if last and last.m == money then
        return
    end
    if last and #log > 1 and t - last.t < MONEY_MERGE_SECONDS then
        last.t, last.m = t, money
    else
        log[#log + 1] = { t = t, m = money }
    end
    while #log > MONEY_LOG_MAX do
        table.remove(log, 1)
    end
end

ns.on("PLAYER_MONEY", logMoney)
ns.on("PLAYER_ENTERING_WORLD", function()
    C_Timer.After(2, logMoney) -- where the log starts, and anything earned while logged out
end)

ns.on("BANKFRAME_OPENED", function()
    bankOpen = true
    refresh("bank", "bags")
end)
ns.on("BANKFRAME_CLOSED", function()
    local c = ns.char()
    if c then
        capture(c, "bank", bank)
        ns.fire("changed")
    end
    bankOpen = false
end)

ns.on("MAIL_SHOW", function()
    mailOpen = true
end)
ns.on("MAIL_INBOX_UPDATE", function()
    if mailOpen then
        refresh("mail")
    end
end)
ns.on("MAIL_CLOSED", function()
    mailOpen = false
end)

-- Commands -----------------------------------------------------------------

local function report(ok)
    local c = ns.char()
    local items = 0
    for _, b in pairs(c and c.bags and c.bags.bags or {}) do
        items = items + ns.count(b.items)
    end
    ns.say(string.format("%d/%d sections, %d gear, %d bag items, %d quests, %d completed%s%s.",
        ok, ns.SECTION_COUNT, ns.count(c and c.gear), items, ns.count(c and c.quests and c.quests.log),
        ns.count(c and c.quests and c.quests.completed),
        c and c.bank and ", bank" or "", c and c.mail and ", mail" or ""))
end

ns.command({ name = "snap", aliases = { "s" }, help = "snapshot now and print what was captured",
    fn = function()
        report(ns.snapshot())
        ns.say("/fbl r saves it to disk (reloads the UI).")
    end })

-- /fbl inspect [tree]: save your target's equipped gear (with stats from the tooltips), class, level
-- and race, so the reader can find them upgrades (fbl-rs upgrades <name>). For a friend you group
-- with; kept on this machine only. The talent tree isn't readable from an inspect here, so name it
-- ("/fbl inspect discipline"): it decides how stats are weighed (a Discipline priest heals).
-- ForeverBuddyLiteDB.inspected["Name-Realm"] = { saved_at, inspected = true, identity, gear, talents, skills }

-- What each class can wear and wield (Classic proficiencies), as skill-line names: the reader's
-- upgrade check reads armor and weapon types from these.
local PROFICIENCY = {
    PRIEST = { "Cloth", "Daggers", "Maces", "Staves", "Wands" },
    MAGE = { "Cloth", "Daggers", "Swords", "Staves", "Wands" },
    WARLOCK = { "Cloth", "Daggers", "Swords", "Staves", "Wands" },
    ROGUE = { "Cloth", "Leather", "Daggers", "Swords", "Maces", "Fist Weapons", "Bows", "Guns", "Crossbows", "Thrown" },
    DRUID = { "Cloth", "Leather", "Daggers", "Maces", "Two-Handed Maces", "Staves", "Fist Weapons", "Polearms" },
    HUNTER = { "Cloth", "Leather", "Mail", "Daggers", "Swords", "Two-Handed Swords", "Axes", "Two-Handed Axes", "Fist Weapons",
        "Polearms", "Staves", "Bows", "Guns", "Crossbows", "Thrown" },
    SHAMAN = { "Cloth", "Leather", "Mail", "Shield", "Daggers", "Maces", "Two-Handed Maces", "Axes", "Two-Handed Axes", "Staves",
        "Fist Weapons" },
    PALADIN = { "Cloth", "Leather", "Mail", "Plate Mail", "Shield", "Swords", "Two-Handed Swords", "Maces", "Two-Handed Maces",
        "Axes", "Two-Handed Axes", "Polearms" },
    WARRIOR = { "Cloth", "Leather", "Mail", "Plate Mail", "Shield", "Daggers", "Swords", "Two-Handed Swords", "Maces",
        "Two-Handed Maces", "Axes", "Two-Handed Axes", "Fist Weapons", "Polearms", "Staves", "Bows", "Guns", "Crossbows", "Thrown" },
}

local inspecting -- { guid, tree } while waiting for the server

local function saveInspect()
    local unit = "target"
    if not inspecting or read(UnitGUID, unit) ~= inspecting.guid then
        return
    end
    local name, realm = read(UnitName, unit)
    realm = (realm and realm ~= "") and realm or read(GetRealmName)
    local _, class = read(UnitClass, unit)
    local gearOut = {}
    for slot = 0, 19 do
        local link = read(GetInventoryItemLink, unit, slot)
        if link then
            gearOut[slot] = { link = link, tip = ns.statLines(link), equip = select(4, read("C_Item.GetItemInfoInstant", link)),
                id = read(GetInventoryItemID, unit, slot) }
        end
    end
    local skills = {}
    for _, s in ipairs(PROFICIENCY[class or ""] or {}) do
        skills[#skills + 1] = { name = s, rank = 1, max = 1 }
    end
    local tree = inspecting.tree
    local db = ns.db
    db.inspected = type(db.inspected) == "table" and db.inspected or {}
    local key = tostring(name) .. "-" .. tostring(realm)
    db.inspected[key] = {
        saved_at = now(),
        inspected = true,
        identity = { name = name, realm = realm, class = class, race = select(2, read(UnitRace, unit)),
            level = read(UnitLevel, unit), faction = read(UnitFactionGroup, unit) },
        gear = gearOut,
        skills = skills,
        talents = tree and { system = "tabs", tabs = { { name = tree, points = 1 } } } or nil,
    }
    inspecting = nil
    if ClearInspectPlayer then
        pcall(ClearInspectPlayer)
    end
    local n = 0
    for _ in pairs(gearOut) do
        n = n + 1
    end
    ns.say(string.format("saved %s's gear (%d items%s). /fbl r, then ask for their upgrades.", tostring(name), n,
        tree and (", " .. tree) or ""))
    ns.fire("changed")
end

ns.on("INSPECT_READY", function(guid)
    if inspecting and guid == inspecting.guid then
        -- Item links can arrive a moment after the event; read once they've settled.
        C_Timer.After(0.5, saveInspect)
    end
end)

ns.command({ name = "inspect", args = "[talent tree]",
    help = "save your target's gear for upgrade suggestions (e.g. /fbl inspect discipline)",
    fn = function(rest)
        local unit = "target"
        if not read(UnitIsPlayer, unit) then
            ns.say("target a player first.")
            return
        end
        if read(CanInspect, unit) == false then
            ns.say("can't inspect them from here: get closer.")
            return
        end
        local tree = rest:match("^%s*(.-)%s*$")
        tree = tree ~= "" and (tree:sub(1, 1):upper() .. tree:sub(2):lower()) or nil
        if tree == "Feral" then
            tree = "Feral Combat"
        end
        inspecting = { guid = read(UnitGUID, unit), tree = tree }
        if UnitIsUnit and read(UnitIsUnit, unit, "player") then
            ns.say("that's you: your own gear is saved anyway.")
            inspecting = nil
            return
        end
        pcall(NotifyInspect, unit)
        ns.say("inspecting...")
    end })

ns.command({ name = "r", aliases = { "reload", "save" }, help = "snapshot, then reload the UI, which writes the file",
    fn = function()
        report(ns.snapshot())
        ns.reloading = true
        ReloadUI()
    end })
