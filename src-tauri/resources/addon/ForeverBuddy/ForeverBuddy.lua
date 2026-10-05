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
-- * Nothing visible: no frames shown, no chat output, no Blizzard function
--   replaced or hooked.
-- * Bounded: at most 10 sessions and 2,000 events per session in the file.
--
-- tools/addon-test/run.lua runs this file against a fake client.

local ADDON_NAME = ...

local SCHEMA = 1
local VERSION = "0.2.0"
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
    return {
        name = name,
        surname = surname ~= "" and surname or nil,
        realm = read(GetRealmName),
        guid = read(UnitGUID, "player"),
    }
end

-- Events -------------------------------------------------------------------------

local handlers = {}

handlers.ADDON_LOADED = function(name)
    if name == ADDON_NAME then
        loaded = validate(ForeverBuddyDB)
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

handlers.PLAYER_LOGIN = function()
    local t = now()
    character = identity()
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
    if entered then
        return
    end
    entered = true
    if not (isReloadingUi and loaded and session) then
        return
    end
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

handlers.QUEST_TURNED_IN = function(questID, xp, money)
    local id = arg(questID)
    addEvent("quest", {
        id = id,
        title = id and read("C_QuestLog.GetTitleForQuestID", id) or nil,
        xp = arg(xp),
        money = arg(money),
    })
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
end

handlers.BAG_UPDATE_DELAYED = scanBags

handlers.BANKFRAME_OPENED = function()
    bankOpen = true
end

handlers.BANKFRAME_CLOSED = function()
    bankOpen = false
    inventory = carried() or inventory
end

handlers.MAIL_SHOW = function()
    mailOpen = true
end

handlers.MAIL_CLOSED = function()
    mailOpen = false
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
        snapshot = { at = session.logout },
        items = items,
        sessions = sessions,
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
