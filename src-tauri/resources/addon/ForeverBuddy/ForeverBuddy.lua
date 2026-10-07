-- ForeverBuddy: notes down each character at logout for WoW Forever Buddy
-- (docs/specs/v0.2-addon.md §2). Everything goes into ForeverBuddyDB, this
-- character's SavedVariables, which the app reads after the game closes.
--
-- The rules this file keeps (spec §2, "Hard rules"):
-- * Every API call goes through read(): a missing API, an error or a secret
--   value gives nil, and nil is never written as a zero.
-- * Every RegisterEvent is pcall'd. Events this client doesn't know are
--   listed in _meta.missing_events, and the rest still work.
-- * This session is built from live APIs. What the client loaded from disk
--   is only trusted if it validates, and only old sessions come from it.
-- * Nothing visible but lines added to the game's own item tooltip, through
--   TooltipDataProcessor (bridge spec §5), the plan frame the player opens
--   with /fb plan (INGAME §7), and the login briefing: at most two chat
--   lines once per login, and /fb's reply (INGAME §9). No popups, no
--   Blizzard function replaced, nothing protected, no automation.
-- * Bounded: at most 10 sessions and 2,000 events per session in the file.
--
-- The app → addon bridge (docs/specs/bridge-v0.4.md): the app writes data
-- files into Data/, listed in the TOC before this file. Their values are
-- data only and never run, passed to a macro, a secure attribute or a frame
-- name; this file reads their header and saves a receipt so the app knows
-- what the game has seen.
--
-- tools/addon-test/run.lua runs this file against a fake client.

local ADDON_NAME = ...

local SCHEMA = 1
local VERSION = "0.8.0"
local MAX_SESSIONS = 10
local MAX_EVENTS = 2000

local loaded -- the file the client read back, if it validates
local character -- identity, taken at login and refreshed at logout
local session -- this session, from PLAYER_LOGIN on
local entered -- PLAYER_ENTERING_WORLD has been seen once
local items = {} -- static info for items seen this session
local secretHits = 0
local truncated = false
local missingEvents = {}
local errors -- handler errors by event, if any
local briefed = {} -- login note id -> when the briefing showed it (B1)
local MAX_BRIEFED = 20

-- Guarded reads ----------------------------------------------------------------

local function pack(...)
    return { n = select("#", ...), ... }
end

local function isSecret(v)
    if issecretvalue and issecretvalue(v) then
        return true
    end
    return type(v) == "table" and issecrettable ~= nil and issecrettable(v) == true
end

-- "C_Item.GetItemInfo" -> that function, or nil if any part is missing.
local function resolve(fn)
    if type(fn) == "function" then
        return fn
    end
    if type(fn) ~= "string" then
        return nil
    end
    local value = _G
    for part in string.gmatch(fn, "[^%.]+") do
        if type(value) ~= "table" then
            return nil
        end
        value = value[part]
    end
    if type(value) == "function" then
        return value
    end
    return nil
end

-- Calls an API and returns its results, or nil for anything that can't be
-- trusted: a missing API or one that errors returns nothing, and each secret
-- value becomes nil (counted in _meta.secret_hits). `fn` is a function or a
-- dotted name, so read("C_Item.GetItemInfo", id) is safe without C_Item.
local function read(fn, ...)
    local f = resolve(fn)
    if not f then
        return nil
    end
    local r = pack(pcall(f, ...))
    if not r[1] then
        return nil
    end
    for i = 2, r.n do
        if isSecret(r[i]) then
            r[i] = nil
            secretHits = secretHits + 1
        end
    end
    return unpack(r, 2, r.n)
end

local function now()
    return read(GetServerTime) or read(time)
end

-- Bridge slots -------------------------------------------------------------------

local SLOT_SCHEMA = 1 -- the slot format this version reads
local SLOT_NAMES = { "Tooltip1", "Tooltip2", "Plan", "Briefing", "Lists", "Cleanup" }
local slots = {} -- name -> the slot's table, when its schema is one we read
local receipts -- name -> { stamp, schema, seen }, saved as ForeverBuddyDB.bridge

-- What each slot file set as its global. A slot from a newer app (another
-- schema) gets a receipt, so the app can tell, but its data isn't used.
local function loadSlots()
    receipts = {}
    for _, name in ipairs(SLOT_NAMES) do
        local data = _G["ForeverBuddyData_" .. name]
        if type(data) == "table" then
            local schema = type(data.schema) == "number" and data.schema or nil
            receipts[name] = {
                stamp = type(data.stamp) == "number" and data.stamp or nil,
                schema = schema,
                seen = now(),
            }
            if schema == SLOT_SCHEMA then
                slots[name] = data
            end
        end
    end
end

-- The file -----------------------------------------------------------------------

local function count(t)
    local n = 0
    if type(t) == "table" then
        for _ in pairs(t) do
            n = n + 1
        end
    end
    return n
end

-- The integrity numbers (spec §2): each is the number of entries in the
-- tables it names. They're written last and recounted by the app, so a
-- half-applied write is caught.
local function counts(db)
    local events, bagItems = 0, 0
    for _, s in pairs(db.sessions) do
        events = events + count(s.events)
    end
    local bags = type(db.snapshot) == "table" and db.snapshot.bags
    if type(bags) == "table" then
        for _, bag in pairs(bags) do
            if type(bag) == "table" then
                bagItems = bagItems + count(bag.items)
            end
        end
    end
    return {
        sessions = count(db.sessions),
        events = events,
        items = count(db.items),
        bag_items = bagItems,
        -- Only in files that have the list (0.4.0 on), so a file from an
        -- older version still validates and carries forward.
        quests_done = type(db.snapshot) == "table" and type(db.snapshot.quests_done) == "table"
            and count(db.snapshot.quests_done) or nil,
    }
end

-- The file the client loaded, if it's one this version wrote and it's whole:
-- the same schema, and counts that still add up. Anything else is ignored
-- and this login starts over from live data.
local function validate(db)
    if type(db) ~= "table" or type(db._meta) ~= "table" or type(db.sessions) ~= "table" then
        return nil
    end
    local want = db._meta.counts
    if db._meta.schema ~= SCHEMA or type(want) ~= "table" then
        return nil
    end
    local ok, got = pcall(counts, db)
    if not ok then
        return nil
    end
    for k, v in pairs(got) do
        if want[k] ~= v then
            return nil
        end
    end
    return db
end

-- On Forever, UnitName's second return is the surname (probe run 1:
-- "Ellygie", "Vargur"), and the WTF folder is "Ellygie-Vargur"; on retail it's
-- the realm of a player from another realm, and nil for yourself.
local function identity()
    local name, surname = read(UnitName, "player")
    local _, class = read(UnitClass, "player")
    local _, race = read(UnitRace, "player")
    local guild, rank = read(GetGuildInfo, "player")
    return {
        name = name,
        surname = surname ~= "" and surname or nil,
        realm = read(GetRealmName),
        guid = read(UnitGUID, "player"),
        class = class,
        race = race,
        sex = read(UnitSex, "player"),
        faction = read(UnitFactionGroup, "player"),
        level = read(UnitLevel, "player"),
        guild = guild and { name = guild, rank = rank } or nil,
    }
end

-- Events -------------------------------------------------------------------------

local handlers = {}

handlers.ADDON_LOADED = function(name)
    if name == ADDON_NAME then
        loaded = validate(ForeverBuddyDB)
        loadSlots()
        -- The account-wide settings (ForeverBuddySettings): only the
        -- briefing toggle, and only what we expect.
        if type(ForeverBuddySettings) ~= "table" then
            ForeverBuddySettings = {}
        end
        -- Notes already shown keep their receipt until the app has read it.
        local prev = loaded and loaded.briefed
        if type(prev) == "table" then
            for id, t in pairs(prev) do
                if type(id) == "number" and type(t) == "number" then
                    briefed[id] = t
                end
            end
        end
    end
end

-- Session events (spec §2, "What it captures") ------------------------------

local MONEY_WINDOW = 60
local lastMoney -- the newest money event, which later changes within a minute update
local lastZone
local merchantOpen, bankOpen, mailOpen
local repairCost -- at the open merchant, last we looked
local inventory -- item counts carried, from the last bag scan

-- An event argument, or nil if it's secret.
local function arg(v)
    if isSecret(v) then
        secretHits = secretHits + 1
        return nil
    end
    return v
end

-- Appends to this session's log. Past the cap the oldest go, and the file
-- says so.
local function addEvent(kind, fields)
    if not session then
        return nil
    end
    fields.kind = kind
    fields.t = now()
    local events = session.events
    events[#events + 1] = fields
    if #events > MAX_EVENTS then
        table.remove(events, 1)
        truncated = true
    end
    return fields
end

local function zoneNow()
    local zone = read(GetRealZoneText)
    if zone ~= "" then
        return zone
    end
    return nil
end

-- Item counts by id across the bags and everything worn, or nil if any of it
-- couldn't be read (a secret anywhere would make items seem to vanish).
local function carried()
    local before = secretHits
    local counts = {}
    for bag = 0, 5 do
        local size = read("C_Container.GetContainerNumSlots", bag)
        for slot = 1, type(size) == "number" and size or 0 do
            local info = read("C_Container.GetContainerItemInfo", bag, slot)
            if type(info) == "table" then
                local id, n = arg(info.itemID), arg(info.stackCount)
                if type(id) ~= "number" or type(n) ~= "number" then
                    return nil
                end
                counts[id] = (counts[id] or 0) + n
            end
        end
    end
    -- 1-19 is gear, 20-23 the bags themselves: equipping isn't losing.
    for slot = 1, 23 do
        local id = read(GetInventoryItemID, "player", slot)
        if type(id) == "number" then
            counts[id] = (counts[id] or 0) + 1
        end
    end
    if secretHits > before then
        return nil
    end
    return counts
end

-- Snapshot (spec §2, "What it captures") -------------------------------------

local MAX_LETTERS = 50 -- the inbox shows at most 50
local ATTACHMENTS = 16 -- ATTACHMENTS_MAX_RECEIVE
local played -- { total, level, at } from TIME_PLAYED_MSG, or the file after /reload
local bank, mail -- this session's visits, if there were any
local lockouts -- from the latest UPDATE_INSTANCE_INFO
-- Item ids whose info is loading (true), or that loaded without info (false).
local pendingItems = {}

-- "|cnIQ1:|Hitem:6948::...|h[Hearthstone]|h|r" -> 6948
local function linkItemID(link)
    if type(link) ~= "string" then
        return nil
    end
    return tonumber(string.match(link, "|Hitem:(%d+)"))
end

local function fillItem(id)
    local name, _, quality, ilvl, _, _, _, _, _, icon, sell, class, subclass =
        read("C_Item.GetItemInfo", id)
    if not name then
        return false
    end
    items[id] = {
        name = name,
        quality = quality,
        ilvl = ilvl,
        icon = icon,
        sell = sell,
        class = class,
        subclass = subclass,
    }
    return true
end

-- Static info for every item seen (row 19), once. If the client hasn't
-- cached it yet it's requested, and ITEM_DATA_LOAD_RESULT fills it in.
local function noteItem(id)
    if type(id) ~= "number" or items[id] or pendingItems[id] ~= nil then
        return
    end
    if not fillItem(id) then
        pendingItems[id] = true
        read("C_Item.RequestLoadItemDataByID", id)
    end
end

-- A container: { size, free, name, items[slot] = { link, count } }, or nil
-- for an empty bag slot or a bank bag that can't be read away from the bank.
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
        if type(info) == "table" then
            local link = arg(info.hyperlink)
            if link then
                -- bound (B3): the app won't offer to mail a soulbound item.
                out.items[slot] = { link = link, count = arg(info.stackCount), bound = arg(info.isBound) == true or nil }
                noteItem(arg(info.itemID) or linkItemID(link))
            end
        end
    end
    return out
end

-- The bank as of now: the main bank plus each purchased tab (row 14). Only
-- readable while the bank is open.
local function scanBank()
    local ids = { (Enum and Enum.BagIndex and Enum.BagIndex.Bank) or -1 }
    local tabs = read("C_Bank.FetchPurchasedBankTabIDs", Enum and Enum.BankType and Enum.BankType.Character)
    if type(tabs) == "table" then
        for _, id in ipairs(tabs) do
            ids[#ids + 1] = arg(id)
        end
    end
    local bags = {}
    for _, id in ipairs(ids) do
        bags[id] = container(id)
    end
    bank = { at = now(), bags = bags }
end

-- The mailbox as of now (row 15). Senders and subjects are other players'
-- words: the app keeps them locally and never exports or logs them.
local function scanMail()
    local n = read(GetInboxNumItems)
    if type(n) ~= "number" then
        return
    end
    local letters = {}
    for i = 1, math.min(n, MAX_LETTERS) do
        local _, _, sender, subject, money, cod, daysLeft, itemCount = read(GetInboxHeaderInfo, i)
        local letter = {
            sender = sender,
            subject = subject,
            money = money,
            cod = cod,
            days_left = daysLeft,
            items = {},
        }
        if type(itemCount) == "number" and itemCount > 0 then
            for a = 1, ATTACHMENTS do
                local link = read(GetInboxItemLink, i, a)
                if link then
                    local _, id, _, count = read(GetInboxItem, i, a)
                    letter.items[#letter.items + 1] = { link = link, count = count }
                    noteItem(id or linkItemID(link))
                end
            end
        end
        letters[#letters + 1] = letter
    end
    mail = { at = now(), items = letters }
end

-- Row 16. GetProfessions returns up to five indices, any of them nil.
local function professions()
    local out = {}
    local p = pack(read(GetProfessions))
    for i = 1, p.n do
        if p[i] then
            local name, _, skill, max, _, _, line, _, spec = read(GetProfessionInfo, p[i])
            if name then
                out[#out + 1] = {
                    name = name,
                    skill = skill,
                    max = max,
                    line = line,
                    spec = type(spec) == "number" and spec >= 0 and spec or nil,
                }
            end
        end
    end
    return out
end

-- Played time at `t`: the last TIME_PLAYED_MSG plus the seconds since.
local function playedAt(t)
    if not played or not t or not played.at then
        return nil
    end
    local since = t - played.at
    return {
        total = played.total + since,
        level = played.level and played.level + since or nil,
    }
end

-- Known recipes (C1): what each of this character's professions can make,
-- read when its profession window is open (the only time the game lists
-- them). Your own window only, never a linked or guild crafter's, and
-- never in combat. Item ids and skill only.
local R = { scanned = nil } -- scanned: profession name -> { skill, max, at, made, mats }
-- Bag cleanup marks (B3), filled in by the cleanup section further down
-- (the tooltip hook and the mailbox's errands use it).
local C = {}
do
    local MAX_MADE = 1000
    -- C2: a recipe's required reagents, { reagentID, qty, ... }. Basic
    -- slots only: optional and finishing reagents aren't needed to craft.
    local MAX_REAGENTS = 8

    local function reagents(schematic)
        local basic = type(Enum) == "table" and type(Enum.CraftingReagentType) == "table"
            and Enum.CraftingReagentType.Basic or 1
        local out = {}
        local slots = type(schematic.reagentSlotSchematics) == "table" and schematic.reagentSlotSchematics or {}
        for _, s in ipairs(slots) do
            local r = type(s) == "table" and s.reagentType == basic and type(s.reagents) == "table" and s.reagents[1]
            local id = type(r) == "table" and r.itemID
            local qty = s.quantityRequired
            if type(id) == "number" and id > 0 and type(qty) == "number" and qty > 0 and #out < MAX_REAGENTS * 2 then
                out[#out + 1] = id
                out[#out + 1] = qty
            end
        end
        return #out > 0 and out or nil
    end

    function R.scan()
        if read("InCombatLockdown") or read("C_TradeSkillUI.IsTradeSkillLinked")
            or read("C_TradeSkillUI.IsTradeSkillGuild") or read("C_TradeSkillUI.IsNPCCrafting") then
            return
        end
        local info = read("C_TradeSkillUI.GetBaseProfessionInfo")
        local name = type(info) == "table" and info.professionName
        local ids = read("C_TradeSkillUI.GetAllRecipeIDs")
        if type(name) ~= "string" or name == "" or type(ids) ~= "table" then
            return
        end
        local made, seen, mats = {}, {}, {}
        for _, id in ipairs(ids) do
            local r = type(id) == "number" and read("C_TradeSkillUI.GetRecipeInfo", id)
            if type(r) == "table" and r.learned == true and #made < MAX_MADE then
                local schematic = read("C_TradeSkillUI.GetRecipeSchematic", id, false)
                local out = type(schematic) == "table" and schematic.outputItemID
                if type(out) == "number" and out > 0 and not seen[out] then
                    seen[out] = true
                    made[#made + 1] = out
                    mats[out] = reagents(schematic)
                end
            end
        end
        table.sort(made)
        R.scanned = R.scanned or {}
        R.scanned[name] = {
            skill = type(info.skillLevel) == "number" and info.skillLevel or nil,
            max = type(info.maxSkillLevel) == "number" and info.maxSkillLevel or nil,
            at = now(),
            made = made,
            mats = next(mats) and mats or nil,
        }
    end

    -- A probe for C1b (INGAME §12 (b)): can a recipe item be mapped to its
    -- recipe? For up to 10 recipe items seen this session (item class 9),
    -- what C_Item.GetItemSpell and the item's tooltip data report. Numbers
    -- only; the app doesn't read it, Julia's file answers the question.
    local RECIPE_CLASS, PROBES = 9, 10
    function R.probe(seen)
        local out, n = {}, 0
        for id, info in pairs(seen) do
            if n >= PROBES then
                break
            end
            if type(info) == "table" and info.class == RECIPE_CLASS then
                local _, spell = read("C_Item.GetItemSpell", id)
                local data = read("C_TooltipInfo.GetItemByID", id)
                local kinds = {}
                for i, line in ipairs(type(data) == "table" and type(data.lines) == "table" and data.lines or {}) do
                    if type(line) == "table" and type(line.type) == "number" then
                        kinds[i] = line.type
                    end
                end
                out[id] = { spell = type(spell) == "number" and spell or nil, lines = kinds }
                n = n + 1
            end
        end
        return n > 0 and out or nil
    end

    -- At logout: this session's scans over the last file's, for the
    -- professions the character still has. nil when there's none.
    function R.merged(prior, has)
        local out = {}
        for name, p in pairs(type(prior) == "table" and prior or {}) do
            if type(p) == "table" and has[name] then
                out[name] = p
            end
        end
        for name, p in pairs(R.scanned or {}) do
            out[name] = p
        end
        return next(out) and out or nil
    end
end

-- A table from the loaded file, if it's one.
local function prior(key)
    local s = loaded and loaded.snapshot
    if type(s) == "table" and type(s[key]) == "table" then
        return s[key]
    end
    return nil
end

-- Every quest this character has completed, as sorted ids (quest data
-- spike, #94): what the app's planner starts from. nil when the client has
-- no such API, never an empty list in its place.
local function questsDone()
    local ids = read("C_QuestLog.GetAllCompletedQuestIDs")
    if type(ids) ~= "table" then
        return nil
    end
    local out = {}
    for _, id in ipairs(ids) do
        -- arg(): a secret id is dropped and counted in secret_hits.
        local v = arg(id)
        if type(v) == "number" then
            out[#out + 1] = v
        end
    end
    table.sort(out)
    return out
end

local function snapshot(t)
    local s = { at = t }
    s.money = read(GetMoney)
    s.xp = read(UnitXP, "player")
    s.xp_max = read(UnitXPMax, "player")
    s.rested = read(GetXPExhaustion)
    local _, restState = read(GetRestState)
    s.rest_state = restState
    local avg, equipped = read(GetAverageItemLevel)
    s.ilvl = { avg = avg, equipped = equipped }
    s.played = playedAt(t)
    local subzone = read(GetSubZoneText)
    s.zone = {
        zone = zoneNow(),
        subzone = subzone ~= "" and subzone or nil,
        map = read("C_Map.GetBestMapForUnit", "player"),
    }
    s.equipped = {}
    for slot = 1, 19 do
        local link = read(GetInventoryItemLink, "player", slot)
        if link then
            s.equipped[slot] = link
            noteItem(read(GetInventoryItemID, "player", slot) or linkItemID(link))
        end
    end
    s.bags = {}
    for bag = 0, 5 do
        s.bags[bag] = container(bag)
    end
    s.professions = professions()
    -- Recipes are readable only with the profession window open: without a
    -- look this session, the last scan carries forward.
    local has = {}
    for _, p in ipairs(s.professions) do
        has[p.name] = true
    end
    s.recipes = R.merged(prior("recipes"), has)
    s.recipe_probe = R.probe(items)
    s.lockouts = lockouts or prior("lockouts")
    -- Only readable at the banker and the mailbox: without a visit this
    -- session, the last one carries forward (a relog mustn't wipe it).
    s.bank = bank or prior("bank")
    s.mail = mail or prior("mail")
    s.quests_done = questsDone()
    return s
end

local function sortedKeys(t)
    local keys = {}
    for k in pairs(t) do
        keys[#keys + 1] = k
    end
    table.sort(keys)
    return keys
end

-- Compares what's carried now with the last scan and logs the difference:
-- `gain` and `lose`, with how when it's known. Items moved to or from the
-- bank aren't gained or lost, so a bank visit only takes a new baseline.
local function scanBags()
    local now_ = carried()
    if not now_ then
        return
    end
    if inventory and not bankOpen then
        local gainHow = (merchantOpen and "bought") or (mailOpen and "mail") or nil
        local loseHow = (merchantOpen and "sold") or (mailOpen and "mailed") or "used"
        for _, id in ipairs(sortedKeys(now_)) do
            local d = now_[id] - (inventory[id] or 0)
            if d > 0 then
                addEvent("gain", { item = id, count = d, how = gainHow })
                noteItem(id)
            end
        end
        for _, id in ipairs(sortedKeys(inventory)) do
            local d = inventory[id] - (now_[id] or 0)
            if d > 0 then
                addEvent("lose", { item = id, count = d, how = loseHow })
            end
        end
    end
    inventory = now_
end

local function repairCostNow()
    local cost = read(GetRepairAllCost)
    if type(cost) == "number" then
        return cost
    end
    return nil
end

-- Alt-aware tooltips (bridge spec §5) ---------------------------------------------
--
-- Lines added to the game's own item tooltip from the tooltip index: which
-- alts hold the item, where, and its last scan price. Read-only: the index
-- is looked up, never run, and the whole callback is pcall'd, so a bad
-- entry drops our lines rather than raising an error.

local MAX_ROWS = 8 -- in the Shift view
local COMPACT_NAMES = 3 -- in the default line
local PLACES = { "bags", "bank", "mail", "worn" }
local tooltipErrors = 0

-- TIP2 (INGAME §8): grey for stale data, white for the hint's numbers.
local GREY, WHITE = "|cff808080", "|cffffffff"
local STALE = 7 * 86400 -- older than a week reads grey
local MIN_GAIN = 5 -- the upgrade hint ignores sidegrades
local UPGRADE_NAMES = 2

-- Inventory slots an equip location is compared against: armour and
-- jewellery (weapons and off-hand items are TIP3's `HAND` below). Rings and
-- trinkets take the lower of their two slots.
local SLOTS_FOR = {
    INVTYPE_HEAD = { 1 }, INVTYPE_NECK = { 2 }, INVTYPE_SHOULDER = { 3 },
    INVTYPE_CHEST = { 5 }, INVTYPE_ROBE = { 5 }, INVTYPE_WAIST = { 6 },
    INVTYPE_LEGS = { 7 }, INVTYPE_FEET = { 8 }, INVTYPE_WRIST = { 9 },
    INVTYPE_HAND = { 10 }, INVTYPE_CLOAK = { 15 },
    INVTYPE_FINGER = { 11, 12 }, INVTYPE_TRINKET = { 13, 14 },
}
local ARMOR = 4 -- item class; its subclasses 0 misc, 1 cloth, 2 leather, 3 mail, 4 plate
local NO_LEATHER = { MAGE = true, PRIEST = true, WARLOCK = true }
local MAIL_ALWAYS = { WARRIOR = true, PALADIN = true }
local MAIL_AT_40 = { HUNTER = true, SHAMAN = true }

-- Weapons (TIP3 (a), INGAME §15): what each class can train, Classic 1.x
-- rules. We can't see what an alt has trained, so this is "can use". One
-- table, so a Forever change is a one-line fix. Weapon subclasses: 0 axe,
-- 1 two-handed axe, 2 bow, 3 gun, 4 mace, 5 two-handed mace, 6 polearm,
-- 7 sword, 8 two-handed sword, 10 staff, 13 fist, 15 dagger, 16 thrown,
-- 18 crossbow, 19 wand.
-- One table, not a local each: the file is near Lua's 200-locals limit.
local Wp = { WEAPON = 2, SHIELD = 6 } -- an item class; an armour subclass
do
    local function set(list)
        local s = {}
        for _, v in ipairs(list) do
            s[v] = true
        end
        return s
    end
    Wp.CAN = {
        WARRIOR = set({ 0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 13, 15, 16, 18 }),
        PALADIN = set({ 0, 1, 4, 5, 6, 7, 8 }),
        HUNTER = set({ 0, 1, 2, 3, 6, 7, 8, 10, 13, 15, 16, 18 }),
        ROGUE = set({ 2, 3, 4, 7, 13, 15, 16, 18 }),
        SHAMAN = set({ 0, 1, 4, 5, 10, 13, 15 }),
        DRUID = set({ 4, 5, 10, 13, 15 }),
        PRIEST = set({ 4, 10, 15, 19 }),
        MAGE = set({ 7, 10, 15, 19 }),
        WARLOCK = set({ 7, 10, 15, 19 }),
    }
    Wp.SHIELDS = set({ "WARRIOR", "PALADIN", "SHAMAN" })
    Wp.DUAL_WIELD = set({ "WARRIOR", "ROGUE", "HUNTER" })
end
-- A weapon's equip location, and what it's compared with: inventory slots
-- 16 main hand, 17 off hand, 18 ranged. Relics, shirts and tabards are out.
Wp.HAND = {
    INVTYPE_2HWEAPON = "two",
    INVTYPE_WEAPON = "one",
    INVTYPE_WEAPONMAINHAND = "main",
    INVTYPE_WEAPONOFFHAND = "off",
    INVTYPE_SHIELD = "held",
    INVTYPE_HOLDABLE = "held",
    INVTYPE_RANGED = "ranged",
    INVTYPE_RANGEDRIGHT = "ranged",
    INVTYPE_THROWN = "ranged",
}

-- Slot text shown as text: every "|" doubled, so a name can't carry an item
-- link, a texture or a colour code.
local function plain(s)
    return (string.gsub(tostring(s), "|", "||"))
end

-- "1g 12s", "45s", "8c".
local function coins(copper)
    local g, s, c = math.floor(copper / 10000), math.floor(copper / 100) % 100, copper % 100
    if g > 0 then
        return s > 0 and (g .. "g " .. s .. "s") or (g .. "g")
    elseif s > 0 then
        return c > 0 and (s .. "s " .. c .. "c") or (s .. "s")
    end
    return c .. "c"
end

-- "today", "1 day ago", "3 days ago".
local function ago(t)
    local days = math.floor(((now() or t) - t) / 86400)
    if days < 1 then
        return "today"
    end
    return days == 1 and "1 day ago" or (days .. " days ago")
end

local function isMe(alt)
    return character ~= nil
        and alt.name == character.name
        and (alt.surname or "") == (character.surname or "")
end

local function classColor(class)
    return RAID_CLASS_COLORS and RAID_CLASS_COLORS[class or ""]
end

-- A name in its class colour: our own code around the escaped name, so the
-- line's own colour comes back after it.
local function colorName(name, class)
    local c = classColor(class)
    if not c then
        return plain(name)
    end
    return string.format("|cff%02x%02x%02x", math.floor(c.r * 255 + 0.5),
        math.floor(c.g * 255 + 0.5), math.floor(c.b * 255 + 0.5)) .. plain(name) .. "|r"
end

-- An alt's place for the compact line: where most of its stack is.
local function mainPlace(row)
    local best = 1
    for p = 2, 4 do
        if row.places[p] > row.places[best] then
            best = p
        end
    end
    return PLACES[best]
end

-- When a place was last seen: bags and worn as of the last logout, the bank
-- and mail as of their last visit.
local function placeTime(alt, place)
    if place == "bank" then
        return alt.bank
    elseif place == "mail" then
        return alt.mail
    end
    return alt.seen
end

local function stale(t)
    local n = now()
    return type(t) == "number" and type(n) == "number" and n - t > STALE
end

-- "5 Oct": day without a leading zero.
local function shortDate(t)
    local s = read("date", "%d %b", t)
    return type(s) == "string" and (string.gsub(s, "^0", "")) or "?"
end

-- Can a class wear this? Jewellery, cloaks and cloth: anyone. Leather: all
-- but the cloth classes. Mail and plate open up at 40 (INGAME §8 (b)).
local function canWear(class, level, classID, subclassID, required)
    if classID ~= ARMOR or subclassID == 0 or subclassID == 1 then
        return true
    end
    local at40 = (tonumber(level) or 0) >= 40 or (tonumber(required) or 0) >= 40
    if subclassID == 2 then
        return type(class) == "string" and class ~= "" and not NO_LEATHER[class]
    elseif subclassID == 3 then
        return MAIL_ALWAYS[class] == true or (MAIL_AT_40[class] == true and at40)
    elseif subclassID == 4 then
        return MAIL_ALWAYS[class] == true and at40
    end
    return false
end

-- The lower item level across `slots`, an empty slot counting as 0.
local function lowest(slots, ilvlIn)
    local low
    for _, s in ipairs(slots) do
        local n = tonumber(ilvlIn(s)) or 0
        if not low or n < low then
            low = n
        end
    end
    return low or 0
end

-- A soulbound or bind-on-pickup item can't reach another character: the
-- tooltip's own lines say which, in the game's own words.
local function isBound(data)
    local lines = type(data) == "table" and data.lines
    if type(lines) ~= "table" then
        return false
    end
    for _, line in ipairs(lines) do
        local text = type(line) == "table" and line.leftText
        if type(text) == "string" and not isSecret(text)
            and (text == ITEM_SOULBOUND or text == ITEM_BIND_ON_PICKUP) then
            return true
        end
    end
    return false
end

-- What a hand item would gain a character by base item level, and what it
-- was set against when that isn't one plain slot ("main and off hand", "the
-- off hand"). nil when the class can't use it or the comparison is skipped
-- (a one-hander or off-hand item while a two-hander is worn). `worn(s)` is
-- the item level in inventory slot s; `twoHanded` says whether the main hand
-- holds a two-hander, nil when that's unknown (then no hint).
function Wp.gain(kind, ilvl, class, classID, subclassID, worn, twoHanded)
    local c = type(class) == "string" and class or ""
    if classID == Wp.WEAPON and not (Wp.CAN[c] and Wp.CAN[c][subclassID]) then
        return nil
    elseif classID == ARMOR and subclassID == Wp.SHIELD and not Wp.SHIELDS[c] then
        return nil
    end
    local function w(s)
        return tonumber(worn(s)) or 0
    end
    if kind == "ranged" then
        return ilvl - w(18)
    elseif twoHanded == nil then
        return nil
    elseif kind == "two" then
        -- Against a two-hander, or a main hand with nothing in the off hand;
        -- the average only when an off hand is worn (§15).
        if twoHanded or w(17) == 0 then
            return ilvl - w(16)
        end
        return ilvl - (w(16) + w(17)) / 2, "main and off hand"
    elseif twoHanded then
        return nil
    elseif kind == "main" then
        return ilvl - w(16)
    end
    -- Off-hand comparisons only against an off hand that's worn: an empty one
    -- is usually a choice, and "+63 over nothing" reads like noise (§15).
    local offWorn = w(17) > 0
    if kind == "held" then
        return offWorn and ilvl - w(17) or nil, "the off hand"
    elseif kind == "off" then
        if not (Wp.DUAL_WIELD[c] and offWorn) then
            return nil
        end
        return ilvl - w(17), "the off hand"
    end
    -- A one-hander: the main hand, or for a dual wielder a worn off hand if
    -- that gains more.
    local gain = ilvl - w(16)
    local off = ilvl - w(17)
    if Wp.DUAL_WIELD[c] and offWorn and off > gain then
        return off, "the off hand"
    end
    return gain
end

-- Whether item `id` is a two-hander: nil when the client doesn't know it yet.
function Wp.twoHanded(id)
    if type(id) ~= "number" or id <= 0 then
        return false
    end
    local loc = select(9, read("C_Item.GetItemInfo", id))
    if type(loc) ~= "string" then
        return nil
    end
    return loc == "INVTYPE_2HWEAPON"
end

-- Which other characters the item would upgrade, by base item level: best
-- first (ties to the higher level), at most two, none when this character
-- is the best fit (the game's own comparison covers that).
local function upgrades(id, data, alts)
    local _, _, _, ilvl, required, _, _, _, equipLoc, _, _, classID, subclassID =
        read("C_Item.GetItemInfo", id)
    local slotsFor = type(equipLoc) == "string" and SLOTS_FOR[equipLoc]
    local hand = type(equipLoc) == "string" and Wp.HAND[equipLoc]
    if not (slotsFor or hand) or type(ilvl) ~= "number" or isBound(data) then
        return {}
    end
    required = tonumber(required) or 0
    local function liveIlvl(s)
        local worn = read(GetInventoryItemID, "player", s)
        return type(worn) == "number" and select(4, read("C_Item.GetItemInfo", worn)) or 0
    end
    local mine = -math.huge
    local myLevel = read(UnitLevel, "player")
    if character and hand then
        mine = Wp.gain(hand, ilvl, character.class, classID, subclassID, liveIlvl,
            Wp.twoHanded(read(GetInventoryItemID, "player", 16))) or -math.huge
    elseif character and canWear(character.class, myLevel, classID, subclassID, required) then
        mine = ilvl - lowest(slotsFor, liveIlvl)
    end
    local found = {}
    for _, alt in ipairs(alts) do
        if type(alt) == "table" and not isMe(alt) and type(alt.worn) == "table" then
            local level = tonumber(alt.level)
            local gain, over
            if hand then
                -- An index from before TIP3 has no hands: an empty main hand
                -- is still known to hold no two-hander.
                local two
                if type(alt.hands) == "table" then
                    two = Wp.twoHanded(alt.hands[1])
                elseif (tonumber(alt.worn[16]) or 0) == 0 then
                    two = false
                end
                gain, over = Wp.gain(hand, ilvl, alt.class, classID, subclassID, function(s)
                    return alt.worn[s]
                end, two)
            elseif canWear(alt.class, level, classID, subclassID, required) then
                gain = ilvl - lowest(slotsFor, function(s)
                    return alt.worn[s]
                end)
            end
            gain = gain and math.floor(gain)
            if gain and gain >= MIN_GAIN then
                found[#found + 1] = {
                    alt = alt,
                    gain = gain,
                    over = over,
                    level = level or 0,
                    under = level and level < required and required or nil,
                }
            end
        end
    end
    table.sort(found, function(a, b)
        if a.gain ~= b.gain then
            return a.gain > b.gain
        end
        return a.level > b.level
    end)
    if not found[1] or mine >= found[1].gain then
        return {}
    end
    while #found > UPGRADE_NAMES do
        table.remove(found)
    end
    return found
end

-- "Upgrade for Kaelor (+9 item level, once level 58) · Sela (+6)": gold
-- lead, class-coloured names, white numbers, the level note grey.
local function upgradeLine(found)
    local parts = {}
    for k, u in ipairs(found) do
        local text = colorName(u.alt.name, u.alt.class) .. WHITE .. " (+" .. u.gain
            .. (k == 1 and " item level" or "") .. "|r"
        if u.under then
            text = text .. GREY .. ", once level " .. u.under .. "|r"
        end
        parts[k] = text .. WHITE .. ")|r"
    end
    return "Upgrade for " .. table.concat(parts, " · ")
end

-- Can make (C1, INGAME §12): the other characters whose recipes make an
-- item, from the index's `makes` and each alt's `prof`.
do
    function R.makers(slot, id)
        local out = {}
        local m = type(slot.makes) == "table" and slot.makes[id]
        local alts = type(slot.alts) == "table" and slot.alts or {}
        if type(m) ~= "table" then
            return out
        end
        for k = 1, #m - 1, 2 do
            local alt, profession = alts[m[k]], m[k + 1]
            if type(alt) == "table" and not isMe(alt) and type(profession) == "string" then
                local p = type(alt.prof) == "table" and alt.prof[profession]
                out[#out + 1] = {
                    alt = alt,
                    profession = profession,
                    skill = type(p) == "table" and type(p.skill) == "number" and p.skill or nil,
                    at = type(p) == "table" and p.at or nil,
                }
            end
        end
        return out
    end

    -- "Sela can make this" · "Sela and Kaelor …" · "Sela, Kaelor and
    -- Velyra …" · "Sela, Kaelor, Velyra +2 …", names in class colour on a
    -- gold line; a crafter whose recipes were read over a week ago is grey
    -- (its date is in the Shift view).
    function R.line(makers)
        local names = {}
        for k = 1, math.min(#makers, COMPACT_NAMES) do
            local m = makers[k]
            names[k] = stale(m.at) and (GREY .. plain(m.alt.name) .. "|r") or colorName(m.alt.name, m.alt.class)
        end
        local who
        if #makers > COMPACT_NAMES then
            who = table.concat(names, ", ") .. " +" .. (#makers - COMPACT_NAMES)
        elseif #names == 1 then
            who = names[1]
        else
            who = table.concat(names, ", ", 1, #names - 1) .. " and " .. names[#names]
        end
        return who .. " can make this"
    end

    -- Shift: "Sela | Tailoring 285", grey with "· as of 21 Sep" when the
    -- recipes were read over a week ago.
    function R.rows(tooltip, makers)
        for _, m in ipairs(makers) do
            local right = plain(m.profession) .. (m.skill and (" " .. m.skill) or "")
            if stale(m.at) then
                right = GREY .. right .. " · as of " .. shortDate(m.at) .. "|r"
            end
            local c = classColor(m.alt.class)
            tooltip:AddDoubleLine(plain(m.alt.name), right, c and c.r or 1, c and c.g or 1, c and c.b or 1, 1, 1, 1)
        end
    end

    -- Materials (C2, INGAME §12 (c)): what one craft takes, from the index's
    -- `mats`, against what every character holds of each reagent in bags,
    -- bank and mail. This character's bags and bank are live; the rest is
    -- as of each alt's last logout. { id, need, have, who, place } each.
    local MAX_MATS = 8
    function R.mats(slot, id)
        local m = type(slot.mats) == "table" and slot.mats[id]
        if type(m) ~= "table" then
            return nil
        end
        local out = {}
        for k = 1, math.min(#m - 1, MAX_MATS * 2), 2 do
            local rid, need = m[k], m[k + 1]
            if type(rid) == "number" and type(need) == "number" and need > 0 then
                local half = slots[(rid % 2 == 0) and "Tooltip1" or "Tooltip2"]
                local entry = type(half) == "table" and type(half.items) == "table" and half.items[rid]
                local alts = type(half) == "table" and type(half.alts) == "table" and half.alts or {}
                local mine = read("C_Item.GetItemCount", rid, true)
                local have = type(mine) == "number" and mine or 0
                local r = { id = rid, need = need, have = 0, best = have, who = have > 0 and "me" or nil, place = "on you" }
                if type(entry) == "table" then
                    for i = 2, #entry, 5 do
                        local alt = alts[entry[i]]
                        if type(alt) == "table" then
                            -- This character's bags and bank are live above;
                            -- only its mail comes from the index.
                            for p = isMe(alt) and 3 or 1, 3 do
                                local n = tonumber(entry[i + p]) or 0
                                have = have + n
                                if n > r.best and not isMe(alt) then
                                    r.best, r.who, r.place = n, alt, PLACES[p]
                                end
                            end
                        end
                    end
                end
                r.have = have
                out[#out + 1] = r
            end
        end
        return #out > 0 and out or nil
    end

    -- "· materials 4 of 6", or "· all materials on hand" in green.
    function R.matsLine(mats)
        local enough = 0
        for _, r in ipairs(mats) do
            if r.have >= r.need then
                enough = enough + 1
            end
        end
        if enough == #mats then
            return " · |cff40ff40all materials on hand|r"
        end
        return " · materials " .. enough .. " of " .. #mats
    end

    -- Shift: "Materials for one", then "Mooncloth | 2 of 2 · Sela bags",
    -- grey where there isn't enough ("Rune Thread | 0 of 1").
    function R.matsRows(tooltip, mats)
        tooltip:AddLine("Materials for one", 1, 0.82, 0)
        for _, r in ipairs(mats) do
            local name = read("C_Item.GetItemNameByID", r.id) or read("C_Item.GetItemInfo", r.id)
            name = type(name) == "string" and plain(name) or ("Item " .. r.id)
            local right = math.min(r.have, r.need) .. " of " .. r.need
            if r.who == "me" then
                right = right .. " · on you"
            elseif r.who then
                right = right .. " · " .. plain(r.who.name) .. " " .. r.place
            end
            if r.have >= r.need then
                tooltip:AddDoubleLine(name, right, 1, 1, 1, 1, 1, 1)
            else
                tooltip:AddDoubleLine(name, right, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5)
            end
        end
    end
end

-- The default view (INGAME §8): "Your alts: Coinpurse 340 bank · …" in gold
-- with names in class colour, at most three, then the price, the upgrade
-- hint, who can make it and a hint about Shift. An alt whose place is stale
-- is grey, dated.
local function compactLines(tooltip, slot, others, price, found, makers, mats)
    tooltip:AddLine(" ")
    if #others > 0 then
        local parts = {}
        for k = 1, math.min(#others, COMPACT_NAMES) do
            local row = others[k]
            local place = mainPlace(row)
            local t = placeTime(row.alt, place)
            if stale(t) then
                parts[#parts + 1] = GREY .. plain(row.name) .. " " .. row.total .. " " .. place
                    .. " (as of " .. shortDate(t) .. ")|r"
            else
                parts[#parts + 1] = colorName(row.name, row.class) .. " " .. row.total .. " " .. place
            end
        end
        if #others > COMPACT_NAMES then
            parts[#parts + 1] = "+" .. (#others - COMPACT_NAMES) .. " more"
        end
        tooltip:AddLine("Your alts: " .. table.concat(parts, " · "), 1, 0.82, 0)
    end
    if price > 0 then
        -- "~", not "≈": the game's fonts may not have the glyph.
        local scan = slot.scanAt
        local old = stale(scan) and (GREY .. " · " .. ago(scan) .. "|r") or ""
        tooltip:AddLine("~" .. coins(price) .. " each at your last scan" .. old, 1, 1, 1)
    end
    if #found > 0 then
        tooltip:AddLine(upgradeLine(found), 1, 0.82, 0)
    end
    if #makers > 0 then
        tooltip:AddLine(R.line(makers) .. (mats and R.matsLine(mats) or ""), 1, 0.82, 0)
    end
    -- Only when Shift has more to show than this.
    if #others > 0 or #makers > 0 then
        tooltip:AddLine("Shift for details", 0.5, 0.6, 0.8)
    end
end

-- The Shift view: a head, this character first with its live count, then
-- each alt by place and date, the total, the scan and the upgrade hint.
local function fullLines(tooltip, slot, id, others, price, found, makers, mats)
    local rows, total = {}, 0
    local mine = read("C_Item.GetItemCount", id, true)
    if type(mine) == "number" and mine > 0 and character and character.name then
        rows[1] = { name = character.name, class = character.class, total = mine, live = true }
        total = mine
    end
    for _, row in ipairs(others) do
        rows[#rows + 1] = row
        total = total + row.total
    end
    tooltip:AddLine(" ")
    tooltip:AddLine("Forever Buddy", 1, 0.82, 0)
    for k = 1, math.min(#rows, MAX_ROWS) do
        local row = rows[k]
        local right
        if row.live then
            right = row.total .. " · on you"
        else
            local where = {}
            for p = 1, 4 do
                if row.places[p] > 0 then
                    where[#where + 1] = row.places[p] .. " " .. PLACES[p]
                end
            end
            local t = placeTime(row.alt, mainPlace(row))
            if stale(t) then
                right = GREY .. table.concat(where, ", ") .. " · " .. ago(t) .. "|r"
            else
                right = table.concat(where, ", ") .. " · " .. ago(row.alt.seen or 0)
            end
        end
        local c = classColor(row.class)
        if c then
            tooltip:AddDoubleLine(plain(row.name), right, c.r, c.g, c.b, 1, 1, 1)
        else
            tooltip:AddDoubleLine(plain(row.name), right, 1, 1, 1, 1, 1, 1)
        end
    end
    if #rows > MAX_ROWS then
        tooltip:AddLine("+" .. (#rows - MAX_ROWS) .. " more", 0.6, 0.6, 0.6)
    end
    if #rows >= 2 then
        tooltip:AddDoubleLine("All characters", tostring(total), 1, 0.82, 0, 1, 1, 1)
    end
    if price > 0 then
        local scan = type(slot.scanAt) == "number" and (" · " .. ago(slot.scanAt)) or ""
        local right = "~" .. coins(price) .. " each" .. scan
        if stale(slot.scanAt) then
            right = GREY .. right .. "|r"
        end
        tooltip:AddDoubleLine("Last scan", right, 1, 0.82, 0, 1, 1, 1)
    end
    if #found > 0 then
        local parts = {}
        for k, u in ipairs(found) do
            -- TIP3: what a weapon was set against, when not one plain slot.
            parts[k] = colorName(u.alt.name, u.alt.class) .. " +" .. u.gain
                .. (u.over and (" " .. GREY .. "(over " .. u.over .. ")|r") or "")
                .. (u.under and (" " .. GREY .. "(level " .. u.under .. ")|r") or "")
        end
        tooltip:AddDoubleLine("Upgrade for", table.concat(parts, " · "), 1, 0.82, 0, 1, 1, 1)
    end
    R.rows(tooltip, makers)
    if mats then
        R.matsRows(tooltip, mats)
    end
    tooltip:AddLine("As of each alt's last logout", 0.5, 0.5, 0.5)
end

local function addItemLines(tooltip, data)
    local id = type(data) == "table" and data.id
    if type(id) ~= "number" or isSecret(id) or read("InCombatLockdown") then
        return
    end
    local name = (id % 2 == 0) and "Tooltip1" or "Tooltip2"
    local slot = slots[name]
    if not slot then
        local r = receipts and receipts[name]
        if r and r.schema and r.schema ~= SLOT_SCHEMA then
            tooltip:AddLine(" ")
            tooltip:AddLine("From a newer Forever Buddy. Update the addon from the app.", 0.6, 0.6, 0.6)
        end
        return
    end
    if slot.tooLarge then
        tooltip:AddLine(" ")
        tooltip:AddLine("Forever Buddy", 1, 0.82, 0)
        tooltip:AddLine("Alt data too large to send", 0.6, 0.6, 0.6)
        return
    end
    local entry = type(slot.items) == "table" and slot.items[id]
    local alts = type(slot.alts) == "table" and slot.alts or {}
    -- The other alts holding it, from the index.
    local others = {}
    if type(entry) == "table" then
        for i = 2, #entry, 5 do
            local alt = alts[entry[i]]
            if type(alt) == "table" and not isMe(alt) then
                local row = { name = alt.name, class = alt.class, total = 0, places = {}, alt = alt }
                for p = 1, 4 do
                    local n = tonumber(entry[i + p]) or 0
                    row.places[p] = n
                    row.total = row.total + n
                end
                if row.total > 0 then
                    others[#others + 1] = row
                end
            end
        end
    end
    -- The upgrade hint stands on its own: vendors, the AH and loot are where
    -- nobody holds the item yet.
    local found = upgrades(id, data, alts)
    -- Who else can make it (C1): it stands on its own too.
    local makers = R.makers(slot, id)
    -- Nothing when only this character has it or nobody does, and there's
    -- no hint: never an empty head (INGAME §8).
    if #others == 0 and #found == 0 and #makers == 0 then
        return
    end
    local price = type(entry) == "table" and tonumber(entry[1]) or 0
    -- What one craft takes (C2), only alongside who can make it.
    local mats = #makers > 0 and R.mats(slot, id) or nil
    if (#others > 0 or #makers > 0) and read("IsShiftKeyDown") then
        fullLines(tooltip, slot, id, others, price, found, makers, mats)
    else
        compactLines(tooltip, slot, others, price, found, makers, mats)
    end
end

local hooked = false
local function hookTooltips()
    if hooked or not (TooltipDataProcessor and Enum and Enum.TooltipDataType) then
        return
    end
    hooked = true
    TooltipDataProcessor.AddTooltipPostCall(Enum.TooltipDataType.Item, function(tooltip, data)
        if not pcall(addItemLines, tooltip, data) then
            tooltipErrors = tooltipErrors + 1
        end
        -- B3: a marked item's line, after ours (C is filled in further down).
        if C.tooltip and not pcall(C.tooltip, tooltip, data) then
            tooltipErrors = tooltipErrors + 1
        end
    end)
end

-- Lists and errands: the data (B2, INGAME §10) ---------------------------------------
--
-- The Lists slot carries every list, what each alt held of its items at its
-- last logout, and the errands the app worked out (an alt that can send
-- what the list's character is short of). The one live part is this
-- character: its own counts come from the game as they are now. Nothing
-- here acts; the panels further down only show it.

local listErrors = 0

local function num(v)
    return type(v) == "number" and v or 0
end

local function listsSlot()
    local s = slots.Lists
    if type(s) == "table" and type(s.lists) == "table" and type(s.alts) == "table" then
        return s
    end
    return nil
end

-- This character's index in the slot's alts, if it's there.
local function myAlt(s)
    for i, a in ipairs(s.alts) do
        if type(a) == "table" and isMe(a) then
            return i
        end
    end
    return nil
end

-- What this character holds of `id` right now: in its bags, and in its bank
-- (the client keeps the bank's count once it's been opened).
local function liveCount(id)
    local bags = num(read("C_Item.GetItemCount", id))
    local all = read("C_Item.GetItemCount", id, true)
    return bags, math.max(0, (type(all) == "number" and all or bags) - bags)
end

-- An item's holdings by alt index, { bags, bank, mail, asOf, live }; this
-- character's from the game.
local function holdings(item, me)
    local out = {}
    local held = type(item.held) == "table" and item.held or {}
    for k = 1, #held - 4, 5 do
        if type(held[k]) == "number" then
            out[held[k]] = { bags = num(held[k + 1]), bank = num(held[k + 2]), mail = num(held[k + 3]), asOf = held[k + 4] }
        end
    end
    if me and type(item.id) == "number" then
        local bags, bank = liveCount(item.id)
        out[me] = { bags = bags, bank = bank, mail = out[me] and out[me].mail or 0, live = true }
    end
    return out
end

local function total(h)
    return h.bags + h.bank + h.mail
end

local function heldIn(h)
    if h.bags >= h.bank and h.bags >= h.mail then
        return "bags"
    end
    return h.bank >= h.mail and "bank" or "mail"
end

local function altName(s, i)
    local a = s.alts[i]
    return type(a) == "table" and colorName(a.name, a.class) or "?"
end

-- This character's errands: what it can send to whom, from every list for
-- another character. { list, item, to, count }.
local function myErrands()
    local s = listsSlot()
    local me = s and myAlt(s)
    local out = {}
    if not me then
        return out
    end
    for _, list in ipairs(s.lists) do
        local to = type(list) == "table" and list["for"]
        if type(to) == "number" and to ~= me and type(list.items) == "table" then
            for _, item in ipairs(list.items) do
                local e = type(item) == "table" and type(item.errands) == "table" and item.errands or {}
                for k = 1, #e - 2, 3 do
                    if e[k] == me and num(e[k + 1]) > 0 then
                        out[#out + 1] = { list = list, item = item, to = to, count = e[k + 1] }
                    end
                end
            end
        end
    end
    return out
end

-- Login briefing (B1, INGAME §9) ---------------------------------------------------
--
-- One chat line at login, only when there's something to say: quests ready
-- to hand in and repairs (live), an alt's waiting mail (from the Briefing
-- slot), errands (the Lists slot), then this character's note on a line of
-- its own. Nothing acts:
-- it's text in the chat frame, every slot string escaped with plain().

local GOLD_PREFIX = "|cffffd100Forever Buddy:|r "
local BRIEF_FACTS = 4
local REPAIR_AT = 0.30
local lastBrief -- { facts, note } of this login, for /fb brief
-- Set at login when this character's plan is one it hasn't seen (the plan
-- section below): the briefing mentions it, P1 itself prints nothing.
local planIsNew = false

local function say(text)
    local f = DEFAULT_CHAT_FRAME
    if type(f) == "table" and type(f.AddMessage) == "function" then
        pcall(f.AddMessage, f, text)
    end
end

local function briefingOn()
    return type(ForeverBuddySettings) ~= "table" or ForeverBuddySettings.briefing ~= false
end

local function setBriefing(on)
    if type(ForeverBuddySettings) ~= "table" then
        ForeverBuddySettings = {}
    end
    -- Saved only when off, so the file stays empty for most players.
    if on then
        ForeverBuddySettings.briefing = nil
    else
        ForeverBuddySettings.briefing = false
    end
end

-- Quests in the log whose objectives are done.
local function questsReady()
    local n = read("C_QuestLog.GetNumQuestLogEntries")
    if type(n) ~= "number" then
        return 0
    end
    local ready = 0
    for i = 1, math.min(n, 100) do
        local info = read("C_QuestLog.GetInfo", i)
        local id = type(info) == "table" and not info.isHeader and info.questID
        if type(id) == "number" and (read("C_QuestLog.ReadyForTurnIn", id) == true
            or read("C_QuestLog.IsComplete", id) == true) then
            ready = ready + 1
        end
    end
    return ready
end

-- The lowest durability across worn gear, 0 to 1, or nil.
local function lowestDurability()
    local low
    for slot = 1, 19 do
        local cur, max = read(GetInventoryItemDurability, slot)
        if type(cur) == "number" and type(max) == "number" and max > 0 then
            local p = cur / max
            if not low or p < low then
                low = p
            end
        end
    end
    return low
end

-- Every fact that's true now, in §9's order, and this character's note.
local function briefing()
    local facts = {}
    local ready = questsReady()
    if ready > 0 then
        facts[#facts + 1] = ready .. (ready == 1 and " quest" or " quests") .. " ready to hand in"
    end
    local low = lowestDurability()
    if low and low < REPAIR_AT then
        facts[#facts + 1] = "repair due (" .. math.floor(low * 100 + 0.5) .. "%)"
    end
    local slot = slots.Briefing
    if type(slot) == "table" and type(slot.mail) == "table" then
        -- The app sends them soonest-expiring first; the first other alt.
        for _, m in ipairs(slot.mail) do
            if type(m) == "table" and type(m.letters) == "number" and m.letters > 0 and not isMe(m) then
                facts[#facts + 1] = colorName(m.name, m.class) .. " has " .. m.letters
                    .. (m.letters == 1 and " letter" or " letters") .. " waiting"
                break
            end
        end
    end
    local ok, errands = pcall(myErrands)
    if ok and #errands > 0 then
        facts[#facts + 1] = #errands .. (#errands == 1 and " errand" or " errands") .. " at the mailbox"
    end
    if planIsNew then
        facts[#facts + 1] = "tonight's plan is ready"
    end
    local note
    if type(slot) == "table" and type(slot.notes) == "table" then
        for _, n in ipairs(slot.notes) do
            if type(n) == "table" and isMe(n) and type(n.text) == "string" and type(n.id) == "number"
                and not (n.once == true and briefed[n.id]) then
                note = n
                break
            end
        end
    end
    return facts, note
end

-- The two lines; `all` lists every fact (for /fb brief) instead of four.
local function show(facts, note, all)
    if #facts > 0 then
        local shown = facts
        if not all and #facts > BRIEF_FACTS then
            shown = {}
            for k = 1, BRIEF_FACTS do
                shown[k] = facts[k]
            end
            shown[#shown + 1] = "and more: /fb brief"
        end
        say(GOLD_PREFIX .. table.concat(shown, " · "))
    end
    if note then
        say("|cffffd100Note:|r \"" .. plain(note.text) .. "\"")
    end
end

local function briefNow()
    if not briefingOn() or read("InCombatLockdown") then
        return
    end
    local facts, note = briefing()
    lastBrief = { facts = facts, note = note }
    show(facts, note, false)
    if note then
        briefed[note.id] = now()
    end
end

-- The note receipts to save: the newest MAX_BRIEFED, so the table can't grow.
local function briefedKept()
    local ids = {}
    for id in pairs(briefed) do
        ids[#ids + 1] = id
    end
    table.sort(ids, function(a, b)
        return briefed[a] > briefed[b]
    end)
    local kept = {}
    for k = 1, math.min(#ids, MAX_BRIEFED) do
        kept[ids[k]] = briefed[ids[k]]
    end
    return kept
end

-- Tonight's plan (P1, INGAME §7) -----------------------------------------------------
--
-- This character's quest plan from the Plan slot, in a small movable frame
-- of our own, shown with /fb plan and never by itself. Accept and hand-in
-- steps tick off when the game says that quest was taken or handed in;
-- other steps are ticked by clicking them. Progress is saved per character.
-- A step's text, zone and giver came from outside the game, so they're
-- shown through plain(); its waypoint is only ever handed to the game's own
-- map pin (or TomTom), never super-tracked and never a click on anything.

local plan -- this character's plan, if the slot has one
local progress -- { id = plan id, done = { [step] = true } }, saved in ForeverBuddyDB.plan
local planFrame
local planErrors = 0

local function myPlan()
    local slot = slots.Plan
    if type(slot) ~= "table" or type(slot.plans) ~= "table" or not character then
        return nil
    end
    for _, p in ipairs(slot.plans) do
        if type(p) == "table" and type(p.steps) == "table" and p.name == character.name
            and (p.surname or "") == (character.surname or "") then
            return p
        end
    end
    return nil
end

-- The first step not done yet: the one the frame marks.
local function currentStep()
    for i = 1, #plan.steps do
        if not progress.done[i] then
            return i
        end
    end
    return nil
end

local function doneCount()
    local n = 0
    for i = 1, #plan.steps do
        if progress.done[i] then
            n = n + 1
        end
    end
    return n
end

local waypointStep -- the step whose game waypoint we set, if any

local function hasPosition(step)
    return type(step.map) == "number" and type(step.x) == "number" and type(step.y) == "number"
end

-- The game's own waypoint for a step, or TomTom's when it's installed.
local function setWaypoint(i, step)
    if not hasPosition(step) then
        return
    end
    local tomtom = rawget(_G, "TomTom")
    if type(tomtom) == "table" and type(tomtom.AddWaypoint) == "function" then
        pcall(tomtom.AddWaypoint, tomtom, step.map, step.x, step.y, { title = plain(step.text), from = "Forever Buddy" })
        return
    end
    local point = read("UiMapPoint.CreateFromCoordinates", step.map, step.x, step.y)
    if point then
        read("C_Map.SetUserWaypoint", point)
        waypointStep = i
    end
end

local renderPlan

-- After any tick: the ticked step's waypoint goes, and finishing the plan
-- gets its one chat line.
local function afterTick()
    if waypointStep and progress.done[waypointStep] then
        read("C_Map.ClearUserWaypoint")
        waypointStep = nil
    end
    if doneCount() == #plan.steps and not progress.finished then
        progress.finished = true
        say(GOLD_PREFIX .. "tonight's plan is done.")
    end
    if planFrame and planFrame:IsShown() then
        renderPlan()
    end
end

function renderPlan()
    if not planFrame then
        return
    end
    local f = planFrame
    for _, row in ipairs(f.rows) do
        row:Hide()
    end
    if not plan then
        f.title:SetText("Tonight's plan")
        f.footer:SetText("No plan for this character yet.")
        return
    end
    local zone = plan.steps[1] and plan.steps[1].zone
    f.title:SetText("Tonight's plan" .. (zone and (" · " .. plain(zone)) or ""))
    local current = currentStep()
    local here = read("C_Map.GetBestMapForUnit", "player")
    for i, step in ipairs(plan.steps) do
        local row = f.rows[i]
        if not row then
            row = CreateFrame("Button", nil, f)
            row:SetSize(260, 30)
            row.label = row:CreateFontString(nil, "OVERLAY", "GameFontNormal")
            row.label:SetPoint("TOPLEFT", 10, -2)
            row.detail = row:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
            row.detail:SetPoint("TOPLEFT", row.label, "BOTTOMLEFT", 0, -1)
            row.go = CreateFrame("Button", nil, row)
            row.go:SetSize(18, 18)
            row.go:SetPoint("RIGHT", -4, 0)
            row.go:SetText("→")
            f.rows[i] = row
        end
        row:SetPoint("TOPLEFT", f, "TOPLEFT", 6, -26 - (i - 1) * 32)
        local done = progress.done[i]
        local where = {}
        if type(step.zone) == "string" then
            where[#where + 1] = plain(step.zone)
        end
        if type(step.giver) == "string" then
            where[#where + 1] = plain(step.giver)
        end
        row.label:SetText(i .. ". " .. plain(step.text))
        if done then
            row.label:SetTextColor(0.5, 0.5, 0.5)
        elseif i == current then
            row.label:SetTextColor(1, 0.82, 0)
        else
            row.label:SetTextColor(1, 1, 1)
        end
        row.detail:SetText(table.concat(where, " · "))
        -- The one manual tick: a step with no quest id, which the game can't
        -- see finish. Clicking marks it done or undone.
        row:SetScript("OnClick", function()
            if not step.quest then
                progress.done[i] = not progress.done[i] or nil
                afterTick()
            end
        end)
        local canGo = hasPosition(step) and step.map == here and not done
        row.go:SetEnabled(canGo)
        if not hasPosition(step) then
            row.go.tip = "No position recorded for this quest yet."
        elseif canGo then
            row.go.tip = "Click the map pin to track it."
        elseif type(step.zone) == "string" and not done then
            row.go.tip = "Go to " .. plain(step.zone) .. " first."
        else
            row.go.tip = nil
        end
        row.go:SetScript("OnClick", function()
            if canGo then
                setWaypoint(i, step)
            end
        end)
        row:Show()
    end
    local n = #plan.steps
    if doneCount() == n then
        f.footer:SetText("All " .. n .. " done · from Forever Buddy")
        f.footer:SetTextColor(0.25, 1, 0.25)
    else
        local at = type(slots.Plan.stamp) == "number" and read(date, "%H:%M", slots.Plan.stamp)
        f.footer:SetText("From Forever Buddy" .. (at and (" · " .. at) or "") .. " · " .. doneCount()
            .. " of " .. n .. " done")
        f.footer:SetTextColor(0.5, 0.5, 0.5)
    end
end

local function togglePlan()
    if not planFrame then
        local f = CreateFrame("Frame", "ForeverBuddyPlanFrame", UIParent, "BackdropTemplate")
        f:SetSize(280, 80)
        f:SetPoint("RIGHT", UIParent, "RIGHT", -60, 80)
        f:SetMovable(true)
        f:EnableMouse(true)
        f:RegisterForDrag("LeftButton")
        f:SetScript("OnDragStart", f.StartMoving)
        f:SetScript("OnDragStop", f.StopMovingOrSizing)
        f.title = f:CreateFontString(nil, "OVERLAY", "GameFontNormal")
        f.title:SetPoint("TOPLEFT", 10, -8)
        f.footer = f:CreateFontString(nil, "OVERLAY", "GameFontDisableSmall")
        f.footer:SetPoint("BOTTOMLEFT", 10, 8)
        f.rows = {}
        f:Hide()
        planFrame = f
    end
    if planFrame:IsShown() then
        planFrame:Hide()
    else
        renderPlan()
        planFrame:SetHeight(48 + (plan and #plan.steps or 1) * 32)
        planFrame:Show()
    end
end

-- At login: this character's plan and its saved progress.
local function loadPlan()
    plan = myPlan()
    local saved = loaded and type(loaded.plan) == "table" and loaded.plan or nil
    if not plan then
        progress = nil
        return
    end
    if saved and saved.id == plan.id and type(saved.done) == "table" then
        progress = saved
        return
    end
    progress = { id = plan.id, done = {} }
    planIsNew = true
end

-- The game saw something happen to quest `id`: tick the first unfinished
-- step it completes. `kind` is accept, turn_in, or objective (the quest is
-- complete in the log). Ticks are never undone by the game.
local function planTick(kind, id)
    if not plan or not id then
        return
    end
    for i, step in ipairs(plan.steps) do
        if not progress.done[i] and step.quest == id and step.kind == kind then
            progress.done[i] = true
            afterTick()
            return
        end
    end
end

-- QUEST_LOG_UPDATE: objective steps whose quest is now complete.
local function planObjectives()
    if not plan then
        return
    end
    for i, step in ipairs(plan.steps) do
        if not progress.done[i] and step.kind == "objective" and step.quest
            and read("C_QuestLog.IsComplete", step.quest) then
            planTick("objective", step.quest)
        end
    end
end

-- Lists and errands: the panels (B2, INGAME §10) -------------------------------------
--
-- "Your list" docks beside the merchant and the auction house while they're
-- open (and opens anywhere with /fb list); "Errands" docks beside the
-- mailbox. They read item ids off the game's own frames to say what's here,
-- and the one button only types a name into the To field. Nothing is
-- bought, attached, clicked or sent.

local LIST_ROWS = 12
local listFrame, errandFrame
local listDocked = false -- beside a vendor or the AH, rather than from /fb list
local here = {} -- item id or lower-case name -> true: sold here, or in the AH results
local under = {} -- item id -> how many of the first AH rows are under the last scan
local WORDS = { "one", "two", "three", "four", "five" }

local function itemName(item)
    return type(item.name) == "string" and item.name or ("Item " .. num(item.id))
end

local function isHere(item)
    return (type(item.id) == "number" and here[item.id])
        or (type(item.name) == "string" and here[string.lower(item.name)]) or false
end

-- An item's row: its name ("· here" in gold when it's here), what's needed
-- on the right, and a grey line saying where your characters stand.
local function itemRow(s, list, item, me)
    local hold = holdings(item, me)
    local need = num(item.need)
    local target = type(list["for"]) == "number" and list["for"] or nil
    local have = 0
    for alt, h in pairs(hold) do
        if not target or alt == target then
            have = have + total(h)
        end
    end
    local done = have >= need
    local label = plain(itemName(item)) .. (isHere(item) and " |cffffd100· here|r" or "")
    local right = done and "done" or ("need " .. (need - have))
    -- On the list's own character, an errand reads as what's coming.
    local e = type(item.errands) == "table" and item.errands or {}
    if not done and target == me and type(e[1]) == "number" and num(e[2]) > 0 then
        right = plain(s.alts[e[1]] and s.alts[e[1]].name or "?") .. " can send " .. num(e[2])
    end
    if done then
        return label, right, "done", true
    end
    local parts = {}
    local short = need - have
    local own = target and hold[target]
    if own and total(own) > 0 then
        parts[1] = (target == me and "you have " or (altName(s, target) .. " has ")) .. total(own) .. " in "
            .. heldIn(own) .. ", still " .. short .. " short"
    else
        local best, bestAlt
        for alt = 1, #s.alts do
            local h = hold[alt]
            if h and alt ~= target and total(h) > 0 and (not best or total(h) > total(best)) then
                best, bestAlt = h, alt
            end
        end
        if best then
            parts[1] = bestAlt == me and ("you have " .. total(best) .. " in " .. heldIn(best))
                or (altName(s, bestAlt) .. " has " .. total(best) .. " in " .. heldIn(best))
            if not best.live and stale(best.asOf) then
                parts[1] = parts[1] .. " (as of " .. shortDate(best.asOf) .. ")"
            end
        else
            parts[1] = "your alts have 0"
        end
    end
    local price = num(item.price)
    if price > 0 then
        local n = type(item.id) == "number" and under[item.id]
        if n and n > 0 then
            parts[#parts + 1] = n == 1 and "the first row is under it"
                or ("first " .. (WORDS[n] or n) .. " rows are under it")
        else
            parts[#parts + 1] = "~" .. coins(price) .. " at last scan"
        end
    end
    return label, right, table.concat(parts, " · "), false
end

local function panel(name, title)
    local f = CreateFrame("Frame", name, UIParent, "DefaultPanelFlatTemplate")
    f:SetSize(300, 80)
    f:SetFrameStrata("HIGH")
    f.heading = f:CreateFontString(nil, "OVERLAY", "GameFontNormal")
    f.heading:SetPoint("TOPLEFT", 12, -8)
    f.heading:SetText(title)
    f.meta = f:CreateFontString(nil, "OVERLAY", "GameFontDisableSmall")
    f.meta:SetPoint("TOPRIGHT", -12, -10)
    f.footer = f:CreateFontString(nil, "OVERLAY", "GameFontDisableSmall")
    f.footer:SetPoint("BOTTOMLEFT", 12, 8)
    f.rows = {}
    f:Hide()
    return f
end

-- A row: name, what's needed on the right, a grey line under; with
-- `height` 56, an errand row, which also gets its "Fill recipient" button.
local function listRow(f, i, height)
    height = height or 32
    local row = f.rows[i]
    if not row then
        row = CreateFrame("Frame", nil, f)
        row:SetSize(280, height - 2)
        row.label = row:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
        row.label:SetPoint("TOPLEFT", 0, 0)
        row.right = row:CreateFontString(nil, "OVERLAY", "GameFontNormal")
        row.right:SetPoint("TOPRIGHT", 0, 0)
        row.detail = row:CreateFontString(nil, "OVERLAY", "GameFontDisableSmall")
        row.detail:SetPoint("TOPLEFT", row.label, "BOTTOMLEFT", 0, -1)
        if height > 32 then
            local fill = CreateFrame("Button", nil, row, "UIPanelButtonTemplate")
            fill:SetSize(110, 20)
            fill:SetPoint("TOPLEFT", row.detail, "BOTTOMLEFT", 0, -3)
            fill:SetText("Fill recipient")
            fill:SetScript("OnEnter", function(b)
                if type(GameTooltip) == "table" then
                    GameTooltip:SetOwner(b, "ANCHOR_RIGHT")
                    GameTooltip:SetText(b.tip, 1, 1, 1, 1, true)
                    GameTooltip:Show()
                end
            end)
            fill:SetScript("OnLeave", function()
                if type(GameTooltip) == "table" then
                    GameTooltip:Hide()
                end
            end)
            row.fill = fill
        end
        f.rows[i] = row
    end
    row:ClearAllPoints()
    row:SetPoint("TOPLEFT", f, "TOPLEFT", 12, -28 - (i - 1) * height)
    row:Show()
    return row
end

-- Fills the list panel. Docked (at a vendor or the AH), lists with
-- something here come first and the rest fold into "+N lists"; undocked,
-- every list shows.
local function renderLists(docked)
    local f = listFrame
    for _, row in ipairs(f.rows) do
        row:Hide()
    end
    local s = listsSlot()
    if not s or #s.lists == 0 then
        f.meta:SetText("")
        f.footer:SetText("No lists yet. Make one in Forever Buddy.")
        f:SetHeight(56)
        return
    end
    local me = myAlt(s)
    local shown, folded = {}, 0
    for _, list in ipairs(s.lists) do
        if type(list) == "table" and type(list.items) == "table" then
            local anyHere = false
            for _, item in ipairs(list.items) do
                anyHere = anyHere or (type(item) == "table" and isHere(item))
            end
            if anyHere or not docked then
                shown[#shown + 1] = list
            else
                folded = folded + 1
            end
        end
    end
    if #shown == 0 then
        -- Nothing on any list is here: show them all rather than nothing.
        for _, list in ipairs(s.lists) do
            if type(list) == "table" and type(list.items) == "table" then
                shown[#shown + 1] = list
            end
        end
        folded = 0
    end
    local names = {}
    for _, list in ipairs(shown) do
        names[#names + 1] = plain(list.name or "")
    end
    f.meta:SetText(table.concat(names, " · "))
    local n = 0
    for _, list in ipairs(shown) do
        for _, item in ipairs(list.items) do
            if type(item) == "table" and n < LIST_ROWS then
                n = n + 1
                local row = listRow(f, n)
                local label, right, detail, done = itemRow(s, list, item, me)
                row.label:SetText(label)
                row.right:SetText(right)
                row.detail:SetText(detail)
                if done then
                    row.label:SetTextColor(0.5, 0.5, 0.5)
                    row.right:SetTextColor(0.5, 0.5, 0.5)
                else
                    row.label:SetTextColor(1, 1, 1)
                    row.right:SetTextColor(1, 0.82, 0)
                end
            end
        end
    end
    local at = type(s.stamp) == "number" and read(date, "%H:%M", s.stamp)
    local foot = "From Forever Buddy" .. (at and (" · " .. at) or "") .. " · /fb list"
    if folded > 0 then
        foot = "+" .. folded .. (folded == 1 and " list" or " lists") .. " · " .. foot
    end
    f.footer:SetText(foot)
    f:SetHeight(48 + n * 32)
end

local function showLists(anchor)
    if not listFrame then
        listFrame = panel("ForeverBuddyListFrame", "Your list")
    end
    listFrame:ClearAllPoints()
    if anchor then
        listFrame:SetPoint("TOPLEFT", anchor, "TOPRIGHT", 6, 0)
    else
        listFrame:SetPoint("TOPRIGHT", UIParent, "TOPRIGHT", -60, -160)
    end
    listDocked = anchor ~= nil
    renderLists(listDocked)
    listFrame:Show()
end

local listPinned = false -- opened with /fb list: stays after a vendor closes

-- The vendor or AH it docked beside closed: back to where /fb list had it,
-- or gone.
local function undock()
    if not listFrame or not listDocked then
        return
    end
    if listPinned then
        showLists(nil)
    else
        listFrame:Hide()
    end
end

-- Every listed item id and name, for the highlights.
local function wanted()
    local s = listsSlot()
    local ids, names = {}, {}
    for _, list in ipairs(s and s.lists or {}) do
        for _, item in ipairs(type(list) == "table" and type(list.items) == "table" and list.items or {}) do
            if type(item) == "table" then
                if type(item.id) == "number" then
                    ids[item.id] = true
                elseif type(item.name) == "string" then
                    names[string.lower(item.name)] = true
                end
            end
        end
    end
    return ids, names
end

-- A merchant item button on a list: a gold edge and a small "list" tag.
-- Kept here by button, so nothing is written onto the game's own frames.
local marks = setmetatable({}, { __mode = "k" })

local function mark(button, on)
    local m = marks[button]
    if not m then
        if not on then
            return
        end
        local glow = button:CreateTexture(nil, "OVERLAY")
        glow:SetAllPoints()
        glow:SetColorTexture(1, 0.82, 0, 0.25)
        local tag = button:CreateFontString(nil, "OVERLAY", "GameFontNormalSmall")
        tag:SetPoint("BOTTOMRIGHT", -1, 1)
        tag:SetText("list")
        m = { glow = glow, tag = tag }
        marks[button] = m
    end
    m.glow:SetShown(on)
    m.tag:SetShown(on)
end

-- What this vendor sells, and which of its buttons on this page to mark.
local function merchantUpdate()
    here = {}
    local ids, names = wanted()
    for index = 1, num(read("GetMerchantNumItems")) do
        local id = read("GetMerchantItemID", index)
        if type(id) == "number" then
            here[id] = true
            local name = read("C_Item.GetItemInfo", id)
            if type(name) == "string" then
                here[string.lower(name)] = true
            end
        end
    end
    local perPage = num(rawget(_G, "MERCHANT_ITEMS_PER_PAGE"))
    local page = type(MerchantFrame) == "table" and num(MerchantFrame.page) or 1
    for i = 1, perPage do
        local button = rawget(_G, "MerchantItem" .. i .. "ItemButton")
        if type(button) == "table" then
            local id = read("GetMerchantItemID", (math.max(page, 1) - 1) * perPage + i)
            local name = type(id) == "number" and read("C_Item.GetItemInfo", id)
            mark(button, (type(id) == "number" and ids[id])
                or (type(name) == "string" and names[string.lower(name)]) or false)
        end
    end
    if listFrame and listFrame:IsShown() then
        renderLists(listDocked)
    end
end

local merchantHooked = false

local function atMerchant()
    if not merchantHooked and type(hooksecurefunc) == "function" and rawget(_G, "MerchantFrame_Update") then
        merchantHooked = pcall(hooksecurefunc, "MerchantFrame_Update", function()
            if not pcall(merchantUpdate) then
                listErrors = listErrors + 1
            end
        end)
    end
    merchantUpdate()
    if listsSlot() then
        showLists(MerchantFrame)
    end
end

-- The AH's browse results: which listed items show there.
local function auctionBrowse()
    here = {}
    local results = read("C_AuctionHouse.GetBrowseResults")
    for _, r in ipairs(type(results) == "table" and results or {}) do
        local id = type(r) == "table" and type(r.itemKey) == "table" and r.itemKey.itemID
        if type(id) == "number" then
            here[id] = true
        end
    end
    if listFrame and listFrame:IsShown() then
        renderLists(true)
    end
end

-- One item's AH rows (cheapest first): how many lead under the last scan.
local function auctionItem(itemKey)
    local id = type(itemKey) == "table" and itemKey.itemID
    if type(id) ~= "number" then
        return
    end
    local s = listsSlot()
    local price
    for _, list in ipairs(s and s.lists or {}) do
        for _, item in ipairs(type(list) == "table" and type(list.items) == "table" and list.items or {}) do
            if type(item) == "table" and item.id == id and num(item.price) > 0 then
                price = item.price
            end
        end
    end
    if not price then
        return
    end
    local n = 0
    for i = 1, num(read("C_AuctionHouse.GetNumItemSearchResults", itemKey)) do
        local r = read("C_AuctionHouse.GetItemSearchResultInfo", itemKey, i)
        if type(r) ~= "table" or type(r.buyoutAmount) ~= "number" or r.buyoutAmount >= price then
            break
        end
        n = n + 1
    end
    under[id] = n
    here[id] = true
    if listFrame and listFrame:IsShown() then
        renderLists(true)
    end
end

-- Who an errand goes to: a list's character (Lists slot) or a marked
-- send's (B3, which carries its own name and class).
function C.recipient(e, s)
    if e.marked then
        return e.toName, e.toClass
    end
    local to = s and s.alts[e.to]
    return type(to) == "table" and type(to.name) == "string" and to.name or "?", type(to) == "table" and to.class
end

-- The errands panel: one row per errand, a "Fill recipient" button that
-- only types the name, disabled when the goods are in the bank. Marked
-- sends (B3) are ordinary rows: "Truestrike Shoulders to Kaelor".
local function renderErrands(errands, s, me)
    local f = errandFrame
    for _, row in ipairs(f.rows) do
        row:Hide()
    end
    f.meta:SetText("from " .. (s and me and altName(s, me)
        or colorName(character and character.name or "?", character and character.class)))
    for i, e in ipairs(errands) do
        local row = listRow(f, i, 56)
        local toName, toClass = C.recipient(e, s)
        local name = itemName(e.item)
        row.label:SetText(plain(name) .. (e.count and (" ×" .. e.count) or "") .. " to " .. colorName(toName, toClass))
        row.right:SetText("")
        local bags, bank = 0, 0
        if type(e.item.id) == "number" then
            bags, bank = liveCount(e.item.id)
        end
        local whose = e.marked and "marked in Forever Buddy"
            or (plain(toName) .. "'s " .. plain(e.list.name or "") .. " list")
        if bags > 0 then
            row.detail:SetText((e.marked and "in your bags · " or ("you have " .. bags .. " in bags · ")) .. whose)
        elseif bank > 0 then
            row.detail:SetText(bank .. " in your bank · visit the bank first")
        else
            row.detail:SetText("none in your bags now · " .. whose)
        end
        row.fill:SetEnabled(bags > 0)
        row.fill.tip = "Types \"" .. plain(toName) .. "\" in the To field. Attach the "
            .. plain(name) .. " yourself, then press Send."
        row.fill:SetScript("OnClick", function()
            local box = rawget(_G, "SendMailNameEditBox")
            if bags > 0 and type(box) == "table" then
                box:SetText(toName)
            end
        end)
    end
    f.footer:SetText("Nothing is attached or sent for you.")
    f:SetHeight(48 + #errands * 56)
end

-- The list errands, then the marked sends.
function C.errands()
    local errands = myErrands()
    local ok, sends = pcall(C.sends)
    for _, e in ipairs(ok and sends or {}) do
        errands[#errands + 1] = e
    end
    return errands
end

local function atMailbox()
    local errands = C.errands()
    if #errands == 0 then
        return
    end
    local s = listsSlot()
    if not errandFrame then
        errandFrame = panel("ForeverBuddyErrandFrame", "Errands")
    end
    errandFrame:ClearAllPoints()
    errandFrame:SetPoint("TOPLEFT", MailFrame, "TOPRIGHT", 6, 0)
    renderErrands(errands, s, s and myAlt(s))
    errandFrame:Show()
end

local function toggleLists()
    if listFrame and listFrame:IsShown() then
        listPinned = false
        listFrame:Hide()
    else
        listPinned = true
        showLists(nil)
    end
end

-- /fb errands: the same, as text, anywhere.
local function sayErrands()
    local errands = C.errands()
    if #errands == 0 then
        say(GOLD_PREFIX .. "no errands for this character.")
        return
    end
    local s = listsSlot()
    for _, e in ipairs(errands) do
        local toName, toClass = C.recipient(e, s)
        local bags = type(e.item.id) == "number" and (liveCount(e.item.id)) or 0
        local whose = e.marked and "marked in Forever Buddy"
            or (plain(toName) .. "'s " .. plain(e.list.name or "") .. " list")
        say(GOLD_PREFIX .. plain(itemName(e.item)) .. (e.count and (" ×" .. e.count) or "") .. " to "
            .. colorName(toName, toClass) .. " (" .. bags .. " in bags) · " .. whose)
    end
end

local function guarded(fn, ...)
    if not pcall(fn, ...) then
        listErrors = listErrors + 1
    end
end

-- Bag cleanup (B3, INGAME §14) ---------------------------------------------------------
--
-- Items marked in the app to sell or to send to another character (the
-- Cleanup slot). A corner tag on their buttons in Blizzard's bags, a line
-- on their tooltip, one grey line at a vendor, and marked sends join the
-- mailbox's errands. Nothing sells, attaches or sends.

do
    local COIN = "Interface\\MoneyFrame\\UI-GoldIcon"
    local LETTER = "Interface\\Minimap\\Tracking\\Mailbox"
    local tags = setmetatable({}, { __mode = "k" }) -- our own textures, by button
    local vendorFrame

    -- This character's marks: item id -> { sell = true } or { to = alt }.
    function C.mine()
        local s = slots.Cleanup
        local out = {}
        if type(s) ~= "table" or type(s.alts) ~= "table" or type(s.marks) ~= "table" then
            return out
        end
        local me
        for i, a in ipairs(s.alts) do
            if type(a) == "table" and isMe(a) then
                me = i
            end
        end
        local flat = me and s.marks[me]
        for k = 1, (type(flat) == "table" and #flat or 0) - 2, 3 do
            local id, action, to = flat[k], flat[k + 1], flat[k + 2]
            if type(id) == "number" then
                if action == "sell" then
                    out[id] = { sell = true }
                elseif action == "send" and type(s.alts[to]) == "table" then
                    out[id] = { to = s.alts[to] }
                end
            end
        end
        return out
    end

    local function sellPrice(id)
        local p = select(11, read("C_Item.GetItemInfo", id))
        return type(p) == "number" and p > 0 and p or nil
    end

    local function tag(button, mark)
        local t = tags[button]
        if not mark then
            if t then
                t:Hide()
            end
            return
        end
        if not t then
            t = button:CreateTexture(nil, "OVERLAY")
            t:SetSize(12, 12)
            t:SetPoint("TOPLEFT", 1, -1)
            tags[button] = t
        end
        t:SetTexture(mark.sell and COIN or LETTER)
        t:Show()
    end

    -- Every item button in Blizzard's bag frames (combined or separate).
    function C.tagBags()
        local each = rawget(_G, "ContainerFrameUtil_EnumerateContainerFrames")
        if type(each) ~= "function" then
            return
        end
        local marks = C.mine()
        for _, frame in each() do
            if type(frame) == "table" and type(frame.EnumerateValidItems) == "function" then
                for _, button in frame:EnumerateValidItems() do
                    local bag = type(button.GetBagID) == "function" and button:GetBagID()
                    local id = bag and read("C_Container.GetContainerItemID", bag, button:GetID())
                    tag(button, type(id) == "number" and marks[id] or nil)
                end
            end
        end
    end

    -- Re-tag when a bag opens (the game builds its buttons then).
    function C.hook()
        if type(hooksecurefunc) ~= "function" then
            return
        end
        for _, name in ipairs({ "OpenAllBags", "ToggleAllBags", "OpenBag", "ToggleBag", "ToggleBackpack" }) do
            if type(rawget(_G, name)) == "function" then
                pcall(hooksecurefunc, name, function()
                    read("C_Timer.After", 0, function()
                        pcall(C.tagBags)
                    end)
                end)
            end
        end
    end

    -- The item tooltip's line, for an item this character carries.
    function C.tooltip(tooltip, data)
        local id = type(data) == "table" and data.id
        if type(id) ~= "number" or isSecret(id) or read("InCombatLockdown") then
            return
        end
        local m = C.mine()[id]
        if not m or num(read("C_Item.GetItemCount", id)) == 0 then
            return
        end
        if m.sell then
            local p = sellPrice(id)
            tooltip:AddLine("Marked to sell in Forever Buddy"
                .. (p and (WHITE .. " · " .. coins(p) .. " each at a vendor|r") or ""), 1, 0.82, 0)
        else
            tooltip:AddLine("Marked to send to " .. colorName(m.to.name, m.to.class)
                .. (isBound(data) and (GREY .. ", but it's soulbound|r") or ""), 1, 0.82, 0)
        end
    end

    -- What's marked to sell in the bags now: items, copper, whether priced.
    local function sells()
        local n, total, priced = 0, 0, false
        for id, m in pairs(C.mine()) do
            local bags = m.sell and num(read("C_Item.GetItemCount", id)) or 0
            if bags > 0 then
                n = n + bags
                local p = sellPrice(id)
                if p then
                    total, priced = total + p * bags, true
                end
            end
        end
        return n, total, priced
    end

    -- At a vendor: "5 marked to sell · ~1g 20s at the vendor", under the
    -- list panel or on its own. No button.
    function C.atVendor()
        local n, total, priced = sells()
        if n == 0 then
            C.leaveVendor()
            return
        end
        if not vendorFrame then
            vendorFrame = CreateFrame("Frame", "ForeverBuddyCleanupFrame", UIParent, "DefaultPanelFlatTemplate")
            vendorFrame:SetSize(300, 28)
            vendorFrame.text = vendorFrame:CreateFontString(nil, "OVERLAY", "GameFontDisableSmall")
            vendorFrame.text:SetPoint("LEFT", 12, 0)
        end
        vendorFrame:ClearAllPoints()
        if listFrame and listFrame:IsShown() then
            vendorFrame:SetPoint("TOPLEFT", listFrame, "BOTTOMLEFT", 0, -4)
        else
            vendorFrame:SetPoint("TOPLEFT", MerchantFrame, "TOPRIGHT", 6, 0)
        end
        vendorFrame.text:SetText(n .. " marked to sell" .. (priced and (" · ~" .. coins(total) .. " at the vendor") or ""))
        vendorFrame:Show()
    end

    function C.leaveVendor()
        if vendorFrame then
            vendorFrame:Hide()
        end
    end

    -- Marked sends still carried (bags or bank), as errand rows.
    function C.sends()
        local out = {}
        for id, m in pairs(C.mine()) do
            local bags, bank = liveCount(id)
            if m.to and bags + bank > 0 then
                local name = read("C_Item.GetItemInfo", id)
                out[#out + 1] = {
                    marked = true,
                    item = { id = id, name = type(name) == "string" and name or nil },
                    toName = type(m.to.name) == "string" and m.to.name or "?",
                    toClass = m.to.class,
                }
            end
        end
        table.sort(out, function(a, b)
            return a.item.id < b.item.id
        end)
        return out
    end

    -- /fb cleanup: "5 marked to sell, 2 to send." or "nothing marked on
    -- Thrandor."
    function C.say()
        local sell, send = 0, 0
        for id, m in pairs(C.mine()) do
            if num(read("C_Item.GetItemCount", id, true)) > 0 then
                if m.sell then
                    sell = sell + 1
                else
                    send = send + 1
                end
            end
        end
        if sell + send == 0 then
            say(GOLD_PREFIX .. "nothing marked on " .. plain(character and character.name or "this character") .. ".")
            return
        end
        local parts = {}
        if sell > 0 then
            parts[#parts + 1] = sell .. " marked to sell"
        end
        if send > 0 then
            parts[#parts + 1] = send .. (sell > 0 and " to send" or " marked to send")
        end
        say(GOLD_PREFIX .. table.concat(parts, ", ") .. ".")
    end
end

-- This session: the coach and the logout card (S2, INGAME §8) -------------------------
--
-- Both read the session the addon already keeps (it carries over a /reload):
-- money against the session's start, looted gains from the bag diff, quest
-- hand-ins, level-ups. XP per hour is counted here from PLAYER_XP_UPDATE.
-- The coach is a small strip, off by default; the card shows during the
-- logout countdown. Neither acts or blocks anything.

-- The section's functions live in a do-block (a chunk can hold only 200
-- locals); what the rest of the file uses is exported through S.
local S = { errors = 0 }
do
local COACH_EVERY = 5 -- seconds between coach refreshes while it's shown
local QUALITY_HEX = { [0] = "9d9d9d", "ffffff", "1eff00", "0070dd", "a335ee", "ff8000", "e6cc80", "00ccff" }
local coachFrame, cardFrame
local xp -- { t, gained, last, max }: XP counted since `t`

local function settings()
    if type(ForeverBuddySettings) ~= "table" then
        ForeverBuddySettings = {}
    end
    return ForeverBuddySettings
end

local function coachOn()
    return type(ForeverBuddySettings) == "table" and ForeverBuddySettings.coach == true
end

local function coachHidesInCombat()
    return type(ForeverBuddySettings) == "table" and ForeverBuddySettings.coachCombat == true
end

local function cardOn()
    return type(ForeverBuddySettings) ~= "table" or ForeverBuddySettings.card ~= false
end

-- "1,234,567".
local function thousands(n)
    local s = tostring(math.floor(n + 0.5))
    local out
    repeat
        s, out = string.gsub(s, "^(%d+)(%d%d%d)", "%1,%2")
    until out == 0
    return s
end

-- Whole gold once there's a gold: "312g", else "45s", "8c".
local function money(copper)
    local c = math.floor(math.abs(copper) + 0.5)
    if c >= 10000 then
        return thousands(math.floor(c / 10000)) .. "g"
    end
    return coins(c)
end

-- "1h 42m", "22m".
local function span(seconds)
    local m = math.max(0, math.floor(seconds / 60 + 0.5))
    return m < 60 and (m .. "m") or (math.floor(m / 60) .. "h " .. (m % 60) .. "m")
end

-- The last-scan price of `id` from the tooltip index, or nil.
local function scanPrice(id)
    local slot = slots[(id % 2 == 0) and "Tooltip1" or "Tooltip2"]
    local entry = type(slot) == "table" and type(slot.items) == "table" and slot.items[id]
    local p = type(entry) == "table" and entry[1]
    return type(p) == "number" and p > 0 and p or nil
end

-- Where XP is counted from: the session's start if the level hasn't changed
-- (so a /reload keeps the count), else from now.
local function xpBaseline()
    local cur, max = read(UnitXP, "player"), read(UnitXPMax, "player")
    local start = session and session.start or {}
    local level = read(UnitLevel, "player")
    if type(cur) ~= "number" then
        xp = nil
        return
    end
    if start.level == level and type(start.xp) == "number" and cur >= start.xp then
        xp = { t = session.login, gained = cur - start.xp, last = cur, max = max }
    else
        xp = { t = now(), gained = 0, last = cur, max = max }
    end
end

-- PLAYER_XP_UPDATE: what was gained since the last one, across a level-up
-- (the rest of the old level, then the new level's XP).
local function countXp()
    local cur, max = read(UnitXP, "player"), read(UnitXPMax, "player")
    if not xp or type(cur) ~= "number" then
        return
    end
    if cur >= xp.last then
        xp.gained = xp.gained + (cur - xp.last)
    elseif type(xp.max) == "number" then
        xp.gained = xp.gained + math.max(0, xp.max - xp.last) + cur
    end
    xp.last, xp.max = cur, max
end

-- What the session adds up to now.
local function tally()
    local t = now() or 0
    local s = session or { login = t, start = {}, events = {} }
    local out = { length = t - (s.login or t), quests = 0, loot = 0, worth = 0, priced = false }
    local m = read(GetMoney)
    if type(m) == "number" and type(s.start.money) == "number" then
        out.gold = m - s.start.money
    end
    local level = read(UnitLevel, "player")
    if type(level) == "number" and type(s.start.level) == "number" and level > s.start.level then
        out.ding = level
    end
    for _, e in ipairs(s.events or {}) do
        if e.kind == "quest" then
            out.quests = out.quests + 1
        elseif e.kind == "gain" and e.how == nil and type(e.item) == "number" then
            local n = num(e.count)
            out.loot = out.loot + n
            local p = scanPrice(e.item)
            if p then
                out.worth, out.priced = out.worth + p * n, true
            end
            local info = items[e.item]
            local q = info and num(info.quality) or 0
            if info and q >= 2 and (not out.best or q > out.best.q
                or (q == out.best.q and num(info.ilvl) > num(out.best.ilvl))) then
                out.best = { name = info.name, q = q, ilvl = info.ilvl }
            end
        end
    end
    if xp and out.length > 0 then
        local hours = math.max(t - xp.t, 60) / 3600
        out.xpHour = xp.gained / hours
        local cur, max = read(UnitXP, "player"), read(UnitXPMax, "player")
        local cap = read(GetMaxPlayerLevel)
        if out.xpHour > 0 and type(cur) == "number" and type(max) == "number" and type(level) == "number"
            and (type(cap) ~= "number" or level < cap) then
            out.nextLevel = level + 1
            out.toLevel = (max - cur) / out.xpHour * 3600
        end
    end
    return out
end

local function coachRow(f, i)
    local row = f.rows[i]
    if not row then
        row = CreateFrame("Frame", nil, f)
        row:SetSize(186, 16)
        row.label = row:CreateFontString(nil, "OVERLAY", "GameFontNormalSmall")
        row.label:SetPoint("LEFT", 0, 0)
        row.value = row:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
        row.value:SetPoint("RIGHT", 0, 0)
        f.rows[i] = row
    end
    row:ClearAllPoints()
    row:SetPoint("TOPLEFT", f, "TOPLEFT", 12, -24 - (i - 1) * 17)
    row:Show()
    return row
end

-- The coach's rows: Gold, Experience, "Level 61 in" while levelling, Loot.
local function renderCoach()
    local f = coachFrame
    local s = tally()
    f.meta:SetText(span(s.length))
    for _, row in ipairs(f.rows) do
        row:Hide()
    end
    local lines = {}
    if s.gold then
        local hours = math.max(s.length, 60) / 3600
        lines[#lines + 1] = { "Gold", (s.gold < 0 and "-" or "+") .. money(s.gold) .. " · "
            .. money(math.max(0, s.gold) / hours) .. "/hr" }
    end
    if s.xpHour then
        lines[#lines + 1] = { "Experience", thousands(s.xpHour) .. "/hr" }
    end
    if s.toLevel then
        lines[#lines + 1] = { "Level " .. s.nextLevel .. " in", "~" .. span(s.toLevel) }
    end
    lines[#lines + 1] = { "Loot", s.loot .. (s.loot == 1 and " item" or " items")
        .. (s.priced and (" · ~" .. money(s.worth)) or "") }
    for i, l in ipairs(lines) do
        local row = coachRow(f, i)
        row.label:SetText(l[1])
        row.value:SetText(l[2])
    end
    f:SetHeight(32 + #lines * 17)
end

local function coachTick()
    if not coachFrame or not coachFrame:IsShown() then
        return
    end
    if not read("InCombatLockdown") then
        if not pcall(renderCoach) then
            S.errors = S.errors + 1
        end
    end
    read("C_Timer.After", COACH_EVERY, coachTick)
end

local function showCoach()
    if not coachFrame then
        local f = CreateFrame("Frame", "ForeverBuddyCoachFrame", UIParent, "DefaultPanelFlatTemplate")
        f:SetSize(210, 100)
        f:SetMovable(true)
        f:EnableMouse(true)
        f:RegisterForDrag("LeftButton")
        f:SetScript("OnDragStart", f.StartMoving)
        f:SetScript("OnDragStop", function(frame)
            frame:StopMovingOrSizing()
            local ok, point, _, _, x, y = pcall(frame.GetPoint, frame)
            if ok and type(point) == "string" and type(x) == "number" and type(y) == "number" then
                settings().coachAt = { point, x, y }
            end
        end)
        f.heading = f:CreateFontString(nil, "OVERLAY", "GameFontNormal")
        f.heading:SetPoint("TOPLEFT", 10, -6)
        f.heading:SetText("This session")
        f.meta = f:CreateFontString(nil, "OVERLAY", "GameFontDisableSmall")
        f.meta:SetPoint("TOPRIGHT", -10, -8)
        f.rows = {}
        f:Hide() -- new frames start shown; the timer starts on the first Show
        coachFrame = f
    end
    local at = type(ForeverBuddySettings) == "table" and ForeverBuddySettings.coachAt
    coachFrame:ClearAllPoints()
    if type(at) == "table" and type(at[1]) == "string" and type(at[2]) == "number" and type(at[3]) == "number" then
        coachFrame:SetPoint(at[1], UIParent, at[1], at[2], at[3])
    else
        coachFrame:SetPoint("TOPLEFT", UIParent, "TOPLEFT", 40, -200)
    end
    if coachHidesInCombat() and read("InCombatLockdown") then
        return
    end
    renderCoach()
    local wasShown = coachFrame:IsShown()
    coachFrame:Show()
    if not wasShown then
        read("C_Timer.After", COACH_EVERY, coachTick)
    end
end

local function hideCoach()
    if coachFrame then
        coachFrame:Hide()
    end
end

-- Saved only when on (and the combat option only when set), so the file
-- stays empty for most players.
local function setCoach(on)
    settings().coach = on or nil
    if on then
        showCoach()
    else
        hideCoach()
    end
end

local function setCoachCombat(hide)
    settings().coachCombat = hide or nil
end

local function setCard(on)
    if on then
        settings().card = nil
    else
        settings().card = false
    end
end

-- "Thrandor's evening", from the local clock.
local function cardTitle()
    local h = tonumber(read(date, "%H", now())) or 20
    local part = (h >= 5 and h < 12 and "morning") or (h >= 12 and h < 17 and "afternoon")
        or (h >= 17 and h < 22 and "evening") or "night"
    return plain(character and character.name or "Your") .. "'s " .. part
end

local SHORT = 5 * 60 -- a session this short with nothing but Played gets no card

-- The logout card: shown during the countdown, above the game's dialog;
-- only its × takes the mouse.
local function showCard()
    if not cardOn() or not session then
        return
    end
    local s = tally()
    local lines = { { "Played", span(s.length) } }
    if s.gold and s.gold ~= 0 then
        -- A gain in green; a loss stays white (§11).
        lines[#lines + 1] = { "Gold", s.gold > 0 and ("|cff1eff00+" .. money(s.gold) .. "|r") or ("-" .. money(s.gold)) }
    end
    if s.best then
        lines[#lines + 1] = { "Best find", "|cff" .. (QUALITY_HEX[s.best.q] or "ffffff") .. plain(s.best.name) .. "|r" }
    end
    if s.quests > 0 then
        lines[#lines + 1] = { "Quests", tostring(s.quests) }
    end
    if s.length < SHORT and #lines == 1 and not s.ding then
        return
    end
    if not cardFrame then
        local f = CreateFrame("Frame", "ForeverBuddyCardFrame", UIParent, "DefaultPanelFlatTemplate")
        f:SetSize(270, 120)
        f:SetFrameStrata("DIALOG")
        f:SetPoint("TOP", UIParent, "TOP", 0, -40)
        f:EnableMouse(false)
        f.heading = f:CreateFontString(nil, "OVERLAY", "GameFontNormal")
        f.heading:SetPoint("TOPLEFT", 10, -6)
        f.close = CreateFrame("Button", nil, f, "UIPanelCloseButton")
        f.close:SetPoint("TOPRIGHT", 2, 2)
        f.close:SetScript("OnClick", function()
            f:Hide()
        end)
        f.ding = f:CreateFontString(nil, "OVERLAY", "GameFontNormalLarge")
        f.ding:SetPoint("TOPLEFT", 12, -28)
        f.footer = f:CreateFontString(nil, "OVERLAY", "GameFontDisableSmall")
        f.footer:SetPoint("BOTTOMLEFT", 12, 8)
        f.footer:SetText("In Adventures after you close WoW")
        f.rows = {}
        cardFrame = f
    end
    local f = cardFrame
    f.heading:SetText(cardTitle())
    f.ding:SetText(s.ding and ("Ding! Level " .. s.ding) or "")
    f.ding:SetShown(s.ding ~= nil)
    for _, row in ipairs(f.rows) do
        row:Hide()
    end
    local top = s.ding and 50 or 28
    for i, l in ipairs(lines) do
        local row = coachRow(f, i)
        row:SetSize(246, 16)
        row:ClearAllPoints()
        row:SetPoint("TOPLEFT", f, "TOPLEFT", 12, -top - (i - 1) * 18)
        row.label:SetText(l[1])
        row.value:SetText(l[2])
    end
    f:SetHeight(top + #lines * 18 + 28)
    f:Show()
end

local function hideCard()
    if cardFrame then
        cardFrame:Hide()
    end
end

local function sessionGuarded(fn, ...)
    if not pcall(fn, ...) then
        S.errors = S.errors + 1
    end
end

S.guarded, S.xpBaseline, S.countXp = sessionGuarded, xpBaseline, countXp
S.coachOn, S.setCoach, S.showCoach, S.hideCoach = coachOn, setCoach, showCoach, hideCoach
S.hidesInCombat, S.setCoachCombat = coachHidesInCombat, setCoachCombat
S.cardOn, S.setCard, S.showCard, S.hideCard = cardOn, setCard, showCard, hideCard
end

-- Lockouts at the entrance (L2, INGAME §13) ------------------------------------------
--
-- Entering a dungeon or raid: one chat line naming your other characters
-- saved there (the Briefing slot's lockouts, F3's data), with the reset.
-- Once per instance per session, never after /reload, and in combat it
-- waits until combat ends. Text in the chat frame and nothing else.

local L = { said = {} } -- said: "instance|difficulty" -> true, this session
do
    -- Difficulties that are "the normal one" and go unnamed: normal and
    -- 10/25-player dungeons and raids, 40-player raids.
    local NORMAL = { [1] = true, [3] = true, [4] = true, [9] = true, [14] = true }

    function L.on()
        return type(ForeverBuddySettings) ~= "table" or ForeverBuddySettings.lockouts ~= false
    end

    -- Written only when off.
    function L.set(on)
        if type(ForeverBuddySettings) ~= "table" then
            ForeverBuddySettings = {}
        end
        ForeverBuddySettings.lockouts = (not on) and false or nil
    end

    -- "resets Tue", or "resets in 5 h" under a day.
    local function resets(at)
        local left = at - (now() or at)
        if left < 86400 then
            return "resets in " .. math.max(1, math.floor(left / 3600 + 0.5)) .. " h"
        end
        return "resets " .. (read(date, "%a", at) or "?")
    end

    -- "Velyra", "Velyra and Kaelor", "Velyra, Kaelor and Sela",
    -- "Velyra, Kaelor, Sela +2".
    local function names(who)
        local shown = {}
        for k = 1, math.min(#who, 3) do
            shown[k] = colorName(who[k].name, who[k].class)
        end
        if #who > 3 then
            return table.concat(shown, ", ") .. " +" .. (#who - 3)
        elseif #shown == 1 then
            return shown[1]
        end
        return table.concat(shown, ", ", 1, #shown - 1) .. " and " .. shown[#shown]
    end

    -- The line for where we are, if any other character is saved here.
    function L.line()
        local inside, kind = read(IsInInstance)
        if not inside or (kind ~= "party" and kind ~= "raid") then
            return nil
        end
        local name, _, difficultyID, difficulty = read(GetInstanceInfo)
        local slot = slots.Briefing
        if type(name) ~= "string" or type(slot) ~= "table" or type(slot.lockouts) ~= "table" then
            return nil
        end
        difficulty = type(difficulty) == "string" and difficulty or ""
        local key = name .. "|" .. difficulty
        if L.said[key] then
            return nil
        end
        L.said[key] = true
        local t, who, soonest = now() or 0, {}, nil
        for _, alt in ipairs(slot.lockouts) do
            local saves = type(alt) == "table" and not isMe(alt) and type(alt.saves) == "table" and alt.saves or {}
            for k = 1, #saves - 2, 3 do
                local reset = saves[k + 2]
                if saves[k] == name and saves[k + 1] == difficulty and type(reset) == "number" and reset > t then
                    who[#who + 1] = alt
                    soonest = (not soonest or reset < soonest) and reset or soonest
                    break
                end
            end
        end
        if #who == 0 then
            return nil
        end
        local where = plain(name) .. ((NORMAL[difficultyID] or difficulty == "") and "" or (" (" .. plain(difficulty) .. ")"))
        return GOLD_PREFIX .. names(who) .. (#who == 1 and " is" or " are") .. " saved to " .. where
            .. " (" .. resets(soonest) .. ")."
    end

    -- PLAYER_ENTERING_WORLD (not a /reload) and PLAYER_REGEN_ENABLED.
    function L.check()
        if not L.on() then
            return
        end
        if read("InCombatLockdown") then
            L.waiting = true
            return
        end
        L.waiting = false
        local line = L.line()
        if line then
            say(line)
        end
    end
end

-- /fb ------------------------------------------------------------------------------
--
-- /fb plan opens the plan frame; /fb list the list panel; /fb errands says
-- this character's errands; /fb brief repeats this login's briefing in
-- full; /fb brief off|on.
local function slash(msg)
    local cmd = type(msg) == "string" and string.lower(string.match(msg, "^%s*(.-)%s*$")) or ""
    if cmd == "plan" then
        if not pcall(togglePlan) then
            planErrors = planErrors + 1
        end
    elseif cmd == "brief" then
        local b = lastBrief
        if not b then
            local facts, note = briefing()
            b = { facts = facts, note = note }
        end
        if #b.facts == 0 and not b.note then
            say(GOLD_PREFIX .. "nothing to report.")
        else
            show(b.facts, b.note, true)
        end
    elseif cmd == "brief off" or cmd == "brief on" then
        setBriefing(cmd == "brief on")
        say(GOLD_PREFIX .. "login briefing " .. (briefingOn() and "on." or "off."))
    elseif cmd == "list" then
        guarded(toggleLists)
    elseif cmd == "errands" then
        guarded(sayErrands)
    elseif cmd == "cleanup" then
        guarded(C.say)
    elseif cmd == "coach" then
        S.guarded(S.setCoach, not S.coachOn())
        say(GOLD_PREFIX .. "session coach " .. (S.coachOn() and "on." or "off."))
    elseif cmd == "coach combat" then
        S.setCoachCombat(not S.hidesInCombat())
        say(GOLD_PREFIX .. "session coach " .. (S.hidesInCombat() and "hides in combat." or "stays in combat."))
    elseif cmd == "card off" or cmd == "card on" then
        S.setCard(cmd == "card on")
        say(GOLD_PREFIX .. "session card at logout " .. (S.cardOn() and "on." or "off."))
    else
        say(GOLD_PREFIX .. "/fb plan shows tonight's plan; /fb list your lists; /fb errands what to send; "
            .. "/fb cleanup what's marked; /fb coach this session's strip; /fb card off hides the logout card; "
            .. "/fb brief repeats the login briefing; /fb brief off turns it off.")
    end
end

if type(SlashCmdList) == "table" then
    SLASH_FOREVERBUDDY1 = "/fb"
    SlashCmdList.FOREVERBUDDY = slash
end

-- The addon compartment (the TOC's AddonCompartmentFunc): a menu with the
-- briefing, coach and card toggles, or a plain briefing toggle where the
-- menu API isn't there.
function ForeverBuddy_OnAddonCompartmentClick(_, _, owner)
    local toggle = function()
        setBriefing(not briefingOn())
    end
    if type(MenuUtil) == "table" and type(MenuUtil.CreateContextMenu) == "function" then
        local ok = pcall(MenuUtil.CreateContextMenu, owner, function(_, root)
            root:CreateTitle("Forever Buddy")
            root:CreateCheckbox("Login briefing", briefingOn, toggle)
            root:CreateCheckbox("Session coach", S.coachOn, function()
                S.guarded(S.setCoach, not S.coachOn())
            end)
            root:CreateCheckbox("Session card at logout", S.cardOn, function()
                S.setCard(not S.cardOn())
            end)
            root:CreateCheckbox("Lockouts at the entrance", L.on, function()
                L.set(not L.on())
            end)
        end)
        if ok then
            return
        end
    end
    toggle()
    say(GOLD_PREFIX .. "login briefing " .. (briefingOn() and "on." or "off."))
end

handlers.PLAYER_LOGIN = function()
    local t = now()
    character = identity()
    hookTooltips()
    pcall(C.hook)
    if not pcall(loadPlan) then
        planErrors = planErrors + 1
    end
    session = {
        id = t,
        login = t,
        start = {
            money = read(GetMoney),
            xp = read(UnitXP, "player"),
            level = read(UnitLevel, "player"),
            zone = zoneNow(),
        },
        events = {},
    }
    lastZone = session.start.zone
end

-- After /reload the client has just written the file and read it back, and
-- PLAYER_LOGIN fired again: carry on with the session that was running
-- instead of starting another. If the file didn't load, the reload starts a
-- new session; the two just aren't joined.
handlers.PLAYER_ENTERING_WORLD = function(_, isReloadingUi)
    if not inventory then
        scanBags()
    end
    -- L2: every time we enter somewhere. After /reload the instance we're in
    -- counts as said, so a later ghost run back in stays quiet too.
    if isReloadingUi then
        pcall(L.line)
    else
        pcall(L.check)
    end
    if entered then
        return
    end
    entered = true
    read(RequestRaidInfo)
    -- The login briefing, once per login (not after /reload), once the
    -- quest log has filled in.
    if not isReloadingUi then
        read("C_Timer.After", 5, function()
            pcall(briefNow)
        end)
    end
    -- Played time is asked for once per login (it prints the two "Total time
    -- played" lines). After /reload the file just written has it.
    local p = prior("played")
    if isReloadingUi and p and type(p.total) == "number" and loaded.snapshot.at then
        played = { total = p.total, level = p.level, at = loaded.snapshot.at }
    else
        read(RequestTimePlayed)
    end
    if isReloadingUi and loaded and session then
        local last = loaded.sessions[#loaded.sessions]
        if type(last) == "table" and type(last.events) == "table" then
            loaded.sessions[#loaded.sessions] = nil
            for _, e in ipairs(session.events) do
                last.events[#last.events + 1] = e
            end
            last.logout = nil
            session = last
        end
    end
    -- The coach counts from the session as joined above.
    S.guarded(S.xpBaseline)
    if S.coachOn() then
        S.guarded(S.showCoach)
    end
end

handlers.PLAYER_XP_UPDATE = function()
    S.guarded(S.countXp)
end

-- Combat: the coach stands still (and hides, if asked).
handlers.PLAYER_REGEN_DISABLED = function()
    if S.hidesInCombat() then
        S.hideCoach()
    end
end

handlers.PLAYER_REGEN_ENABLED = function()
    if S.coachOn() then
        S.guarded(S.showCoach)
    end
    -- L2: entered while fighting, so the line waited.
    if L.waiting then
        pcall(L.check)
    end
end

-- The logout countdown (and /quit's): the card, until it's cancelled.
handlers.PLAYER_CAMPING = function()
    S.guarded(S.showCard)
end

handlers.PLAYER_QUITING = function()
    S.guarded(S.showCard)
end

handlers.LOGOUT_CANCEL = function()
    S.hideCard()
end

handlers.ZONE_CHANGED_NEW_AREA = function()
    local zone = zoneNow()
    if not zone or zone == lastZone then
        return
    end
    lastZone = zone
    addEvent("zone", { zone = zone, instance = read(IsInInstance) == true or nil })
end

handlers.PLAYER_LEVEL_UP = function(level)
    addEvent("level", { level = arg(level) })
end

-- Coalesced: changes within a minute of the last point update that point.
handlers.PLAYER_MONEY = function()
    local money = read(GetMoney)
    if not money then
        return
    end
    local t = now()
    if lastMoney and t and lastMoney.t and t - lastMoney.t < MONEY_WINDOW then
        lastMoney.money = money
    else
        lastMoney = addEvent("money", { money = money })
    end
end

-- Where a quest was taken or handed in, and who to (the quest log, Q1b):
-- the zone, the map and the player's position on it (0-1, to 0.1%; none in
-- an instance), and the giver, the NPC the quest window is open on. A quest
-- shared by another player has a player there, and a player's name is never
-- kept.
local function questPlace(fields)
    fields.zone = zoneNow()
    local map = read("C_Map.GetBestMapForUnit", "player")
    fields.map = map
    local pos = map and read("C_Map.GetPlayerMapPosition", map, "player")
    if type(pos) == "table" and pos.GetXY then
        local x, y = read(pos.GetXY, pos)
        if type(x) == "number" and type(y) == "number" then
            fields.x = math.floor(x * 1000 + 0.5) / 1000
            fields.y = math.floor(y * 1000 + 0.5) / 1000
        end
    end
    -- Only on a definite "not a player": a missing API or a secret answer
    -- records no giver.
    if read(UnitIsPlayer, "npc") == false then
        fields.giver = read(UnitName, "npc")
    end
    return fields
end

-- Retail sends the quest id alone; older clients sent the log index first.
handlers.QUEST_ACCEPTED = function(a, b)
    local id = arg(b or a)
    addEvent("quest_accepted", questPlace({
        id = id,
        title = id and read("C_QuestLog.GetTitleForQuestID", id) or nil,
    }))
    if not pcall(planTick, "accept", id) then
        planErrors = planErrors + 1
    end
end

handlers.QUEST_LOG_UPDATE = function()
    if not pcall(planObjectives) then
        planErrors = planErrors + 1
    end
end

handlers.QUEST_TURNED_IN = function(questID, xp, money)
    local id = arg(questID)
    addEvent("quest", questPlace({
        id = id,
        title = id and read("C_QuestLog.GetTitleForQuestID", id) or nil,
        xp = arg(xp),
        money = arg(money),
    }))
    if not pcall(planTick, "turn_in", id) then
        planErrors = planErrors + 1
    end
end

-- No killer: that's restricted, and the recap says so instead.
handlers.PLAYER_DEAD = function()
    addEvent("death", { zone = zoneNow() })
end

handlers.ENCOUNTER_END = function(id, name, _, _, success)
    if arg(success) == 1 then
        addEvent("encounter", { id = arg(id), name = arg(name) })
    end
end

-- Repairs: while a merchant is open, a drop in the repair cost is what was
-- paid. (Read on durability changes, not at MERCHANT_CLOSED, when the cost
-- may no longer be readable.)
handlers.MERCHANT_SHOW = function()
    merchantOpen = true
    repairCost = repairCostNow()
    guarded(atMerchant)
    guarded(C.atVendor)
end

handlers.UPDATE_INVENTORY_DURABILITY = function()
    if not merchantOpen then
        return
    end
    local cost = repairCostNow()
    if repairCost and cost and cost < repairCost then
        addEvent("repair", { cost = repairCost - cost })
    end
    repairCost = cost
end

handlers.MERCHANT_CLOSED = function()
    merchantOpen, repairCost = false, nil
    here = {}
    undock()
    guarded(C.leaveVendor)
end

-- The AH (B2): the list panel docks beside it, and what's in its results
-- counts as here.
handlers.AUCTION_HOUSE_SHOW = function()
    here, under = {}, {}
    if listsSlot() then
        guarded(showLists, rawget(_G, "AuctionHouseFrame"))
    end
end

handlers.AUCTION_HOUSE_BROWSE_RESULTS_UPDATED = function()
    guarded(auctionBrowse)
end

handlers.ITEM_SEARCH_RESULTS_UPDATED = function(itemKey)
    guarded(auctionItem, itemKey)
end

handlers.AUCTION_HOUSE_CLOSED = function()
    here, under = {}, {}
    undock()
end

handlers.BAG_UPDATE_DELAYED = function()
    scanBags()
    if bankOpen then
        scanBank()
    end
    -- Live counts in the panels that are open.
    if mailOpen and errandFrame and errandFrame:IsShown() then
        guarded(atMailbox)
    end
    if listFrame and listFrame:IsShown() then
        guarded(renderLists, listDocked)
    end
    -- B3: a sold or sent item's tag goes right away.
    guarded(C.tagBags)
    if merchantOpen then
        guarded(C.atVendor)
    end
end

handlers.BANKFRAME_OPENED = function()
    bankOpen = true
    scanBank()
end

-- The main bank's slots change without a BAG_UPDATE.
handlers.PLAYERBANKSLOTS_CHANGED = function()
    if bankOpen then
        scanBank()
    end
end

handlers.BANKFRAME_CLOSED = function()
    bankOpen = false
    inventory = carried() or inventory
end

handlers.MAIL_SHOW = function()
    mailOpen = true
    guarded(atMailbox)
end

handlers.MAIL_CLOSED = function()
    mailOpen = false
    if errandFrame then
        errandFrame:Hide()
    end
end

handlers.MAIL_INBOX_UPDATE = function()
    if mailOpen then
        scanMail()
    end
end

-- The profession window (C1): its recipe list fills in after SHOW, and
-- again when a recipe is learned or the skill rises.
handlers.TRADE_SKILL_SHOW = function()
    pcall(R.scan)
end

handlers.TRADE_SKILL_LIST_UPDATE = function()
    pcall(R.scan)
end

handlers.TIME_PLAYED_MSG = function(total, level)
    total, level = arg(total), arg(level)
    if type(total) == "number" then
        played = { total = total, level = level, at = now() }
    end
end

handlers.UPDATE_INSTANCE_INFO = function()
    local n = read(GetNumSavedInstances)
    if type(n) ~= "number" then
        return
    end
    local t, out = now(), {}
    for i = 1, n do
        local name, _, reset, _, locked, _, _, isRaid, _, difficulty = read(GetSavedInstanceInfo, i)
        if name and locked then
            out[#out + 1] = {
                name = name,
                difficulty = difficulty,
                reset_at = t and type(reset) == "number" and t + reset or nil,
                raid = isRaid or nil,
            }
        end
    end
    lockouts = out
end

handlers.ITEM_DATA_LOAD_RESULT = function(itemID, success)
    local id = arg(itemID)
    if id and pendingItems[id] then
        pendingItems[id] = false
        if arg(success) then
            fillItem(id)
        end
    end
end

-- Builds the whole file from this session plus the sessions carried forward.
-- If anything here fails, ForeverBuddyDB keeps what the client loaded, so the
-- file is never left half-built.
handlers.PLAYER_LOGOUT = function()
    if not session then
        return
    end
    session.logout = now()

    local sessions = {}
    if loaded then
        for _, s in ipairs(loaded.sessions) do
            if type(s) == "table" then
                sessions[#sessions + 1] = s
            end
        end
    end
    sessions[#sessions + 1] = session
    while #sessions > MAX_SESSIONS do
        table.remove(sessions, 1)
        truncated = true
    end
    while #session.events > MAX_EVENTS do
        table.remove(session.events, 1)
        truncated = true
    end

    -- Refreshed, but a value that can't be read now keeps the login one.
    for k, v in pairs(identity()) do
        character[k] = v
    end

    local db = {
        character = character,
        snapshot = snapshot(session.logout),
        items = items,
        sessions = sessions,
        bridge = receipts and next(receipts) and receipts or nil,
        plan = progress,
        briefed = next(briefed) and briefedKept() or nil,
    }
    local version, build = read(GetBuildInfo)
    db._meta = {
        schema = SCHEMA,
        addon = VERSION,
        build = version and build and (tostring(version) .. "." .. tostring(build)) or nil,
        written = session.logout,
        loaded_prior = loaded ~= nil,
        truncated = truncated,
        secret_hits = secretHits,
        tooltip_errors = tooltipErrors > 0 and tooltipErrors or nil,
        plan_errors = planErrors > 0 and planErrors or nil,
        list_errors = listErrors > 0 and listErrors or nil,
        session_errors = S.errors > 0 and S.errors or nil,
        missing_events = missingEvents,
        errors = errors,
    }
    db._meta.counts = counts(db) -- last
    ForeverBuddyDB = db
end

local frame = CreateFrame("Frame")

frame:SetScript("OnEvent", function(_, event, ...)
    local handler = handlers[event]
    if not handler then
        return
    end
    local ok, err = pcall(handler, ...)
    if not ok then
        errors = errors or {}
        errors[event] = string.sub(tostring(err), 1, 200)
    end
end)

for event in pairs(handlers) do
    if not pcall(frame.RegisterEvent, frame, event) then
        missingEvents[#missingEvents + 1] = event
    end
end
table.sort(missingEvents)
