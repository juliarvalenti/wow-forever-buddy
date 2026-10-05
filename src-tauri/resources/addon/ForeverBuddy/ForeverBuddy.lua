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
--   TooltipDataProcessor (bridge spec §5): no frames shown, no chat output,
--   no Blizzard function replaced.
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
local VERSION = "0.4.0"
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

-- Bridge slots -------------------------------------------------------------------

local SLOT_SCHEMA = 1 -- the slot format this version reads
local SLOT_NAMES = { "Tooltip1", "Tooltip2" }
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
                out.items[slot] = { link = link, count = arg(info.stackCount) }
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

-- A table from the loaded file, if it's one.
local function prior(key)
    local s = loaded and loaded.snapshot
    if type(s) == "table" and type(s[key]) == "table" then
        return s[key]
    end
    return nil
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
    s.lockouts = lockouts or prior("lockouts")
    -- Only readable at the banker and the mailbox: without a visit this
    -- session, the last one carries forward (a relog mustn't wipe it).
    s.bank = bank or prior("bank")
    s.mail = mail or prior("mail")
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

-- The default view (INGAME §8): "Your alts: Coinpurse 340 bank · …" in gold
-- with names in class colour, at most three, then the price and a hint.
local function compactLines(tooltip, others, price)
    local parts = {}
    for k = 1, math.min(#others, COMPACT_NAMES) do
        local row = others[k]
        local name, c = plain(row.name), classColor(row.class)
        if c then
            -- Our own colour code around the escaped name; |r returns to gold.
            name = string.format("|cff%02x%02x%02x", math.floor(c.r * 255 + 0.5),
                math.floor(c.g * 255 + 0.5), math.floor(c.b * 255 + 0.5)) .. name .. "|r"
        end
        parts[#parts + 1] = name .. " " .. row.total .. " " .. mainPlace(row)
    end
    if #others > COMPACT_NAMES then
        parts[#parts + 1] = "+" .. (#others - COMPACT_NAMES) .. " more"
    end
    tooltip:AddLine(" ")
    tooltip:AddLine("Your alts: " .. table.concat(parts, " · "), 1, 0.82, 0)
    if price > 0 then
        -- "~", not "≈": the game's fonts may not have the glyph.
        tooltip:AddLine("~" .. coins(price) .. " each at your last scan", 1, 1, 1)
    end
    tooltip:AddLine("Shift for details", 0.5, 0.6, 0.8)
end

-- The Shift view: a head, this character first with its live count, then
-- each alt by place and date, the total and the scan.
local function fullLines(tooltip, slot, id, others, price)
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
            right = table.concat(where, ", ") .. " · " .. ago(row.alt.seen or 0)
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
        tooltip:AddDoubleLine("Last scan", "~" .. coins(price) .. " each" .. scan, 1, 0.82, 0, 1, 1, 1)
    end
    tooltip:AddLine("As of each alt's last logout", 0.5, 0.5, 0.5)
end

local function addItemLines(tooltip, id)
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
    -- Nothing when only this character has it, or nobody does: never an
    -- empty head (INGAME §8).
    if #others == 0 then
        return
    end
    local price = type(entry) == "table" and tonumber(entry[1]) or 0
    if read("IsShiftKeyDown") then
        fullLines(tooltip, slot, id, others, price)
    else
        compactLines(tooltip, others, price)
    end
end

local hooked = false
local function hookTooltips()
    if hooked or not (TooltipDataProcessor and Enum and Enum.TooltipDataType) then
        return
    end
    hooked = true
    TooltipDataProcessor.AddTooltipPostCall(Enum.TooltipDataType.Item, function(tooltip, data)
        if not pcall(addItemLines, tooltip, data and data.id) then
            tooltipErrors = tooltipErrors + 1
        end
    end)
end

handlers.PLAYER_LOGIN = function()
    local t = now()
    character = identity()
    hookTooltips()
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
    read(RequestRaidInfo)
    -- Played time is asked for once per login (it prints the two "Total time
    -- played" lines). After /reload the file just written has it.
    local p = prior("played")
    if isReloadingUi and p and type(p.total) == "number" and loaded.snapshot.at then
        played = { total = p.total, level = p.level, at = loaded.snapshot.at }
    else
        read(RequestTimePlayed)
    end
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

handlers.BAG_UPDATE_DELAYED = function()
    scanBags()
    if bankOpen then
        scanBank()
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
end

handlers.MAIL_CLOSED = function()
    mailOpen = false
end

handlers.MAIL_INBOX_UPDATE = function()
    if mailOpen then
        scanMail()
    end
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
