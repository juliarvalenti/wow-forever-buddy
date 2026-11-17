-- Core: helpers every module uses, one event hub, and the command registry.
-- Modules share state only through ns; the only globals are ForeverBuddyLiteDB,
-- the /fbl slash command and the named window frame (so Esc closes it).

local _, ns = ...

-- Calling APIs that may not exist on this client ---------------------------

local function resolve(fn)
    if type(fn) == "function" then
        return fn
    end
    -- A global that doesn't exist on this client arrives here as nil.
    if type(fn) ~= "string" then
        return nil
    end
    local v = _G
    for part in string.gmatch(fn, "[^%.]+") do
        if type(v) ~= "table" then
            return nil
        end
        v = v[part]
    end
    return type(v) == "function" and v or nil
end

local function pack(...)
    return { n = select("#", ...), ... }
end

local function isSecret(v)
    return (issecretvalue and issecretvalue(v)) or false
end

-- Calls an API that may be missing, may error, or may return secret values.
-- Missing or erroring returns nothing; each secret value becomes nil.
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
        end
    end
    return unpack(r, 2, r.n)
end

ns.resolve, ns.pack, ns.read = resolve, pack, read

-- Which code this is: a hash of every addon file, written by addon/stamp.mjs
-- (run by addon/install.mjs). "dev" means it was copied without stamping.
ns.build = "2169f33"

-- "2 · a1b2c3d": the .toc version (set once the addon has loaded) and the build hash.
function ns.versionString()
    return (ns.version or "?") .. " · " .. ns.build
end

function ns.num(v)
    return type(v) == "number" and v or nil
end
local num = ns.num

function ns.round(v, places)
    if type(v) ~= "number" then
        return nil
    end
    local m = 10 ^ (places or 0)
    return math.floor(v * m + 0.5) / m
end

function ns.now()
    return read(GetServerTime) or time()
end

function ns.count(t)
    local n = 0
    for _ in pairs(type(t) == "table" and t or {}) do
        n = n + 1
    end
    return n
end

function ns.say(s)
    print("|cff7fbfffForeverBuddy Lite|r: " .. s)
end

-- "12m ago" for a capture time.
function ns.ago(t)
    if not num(t) then
        return "never"
    end
    local s = math.max(0, ns.now() - t)
    if s < 60 then
        return "just now"
    elseif s < 3600 then
        return math.floor(s / 60) .. "m ago"
    elseif s < 86400 then
        return math.floor(s / 3600) .. "h ago"
    end
    return math.floor(s / 86400) .. "d ago"
end

-- Runs fn once, delay seconds after the last of a burst of calls has started.
function ns.debounce(delay, fn)
    local waiting = false
    return function()
        if waiting then
            return
        end
        waiting = true
        C_Timer.After(delay, function()
            waiting = false
            pcall(fn)
        end)
    end
end

-- Items --------------------------------------------------------------------

function ns.spellName(id)
    if not num(id) then
        return nil
    end
    return read("C_Spell.GetSpellName", id) or (read(GetSpellInfo, id))
end

-- Item info moved to C_Item on newer clients; Forever may only have that one.
function ns.itemInfo(item)
    local f = resolve("C_Item.GetItemInfo") or resolve("GetItemInfo")
    return read(f, item)
end

-- An item's name. Links can hold "[]" when the client hasn't loaded the item
-- yet; asking loads it, so the next snapshot has it.
function ns.itemName(id)
    if not num(id) then
        return nil
    end
    local name = read("C_Item.GetItemNameByID", id) or (ns.itemInfo(id))
    if not name then
        read("C_Item.RequestLoadItemDataByID", id)
    end
    return name
end

function ns.itemID(link)
    return type(link) == "string" and tonumber(link:match("item:(%d+)")) or nil
end

-- Something you wear: a weapon or armor with a slot, not a shirt, tabard, bag or ammo.
local NOT_GEAR = { INVTYPE_BODY = true, INVTYPE_TABARD = true, INVTYPE_BAG = true, INVTYPE_AMMO = true }
function ns.isGear(equipLoc, classID)
    return (classID == 2 or classID == 4) and type(equipLoc) == "string" and equipLoc ~= "" and not NOT_GEAR[equipLoc]
end

-- The tooltip lines that carry stats, as the client shows them: "+5 Stamina",
-- "Equip: ...", "(11.88 damage per second)", "Classes: Mage", "Requires Level 20",
-- armor. The reader parses these, so gear, listings and rewards score alike.
function ns.isStatLine(t)
    return type(t) == "string" and t ~= "" and (t:find("^%+") or t:find("^Equip:") or t:find("per second")
        or t:find("Armor$") or t:find("^Classes:") or t:find("^Requires ")) and true or false
end

function ns.tooltipStatLines(data)
    if type(data) ~= "table" or type(data.lines) ~= "table" then
        return nil
    end
    local out = {}
    for _, line in ipairs(data.lines) do
        local t = type(line) == "table" and line.leftText
        if ns.isStatLine(t) then
            out[#out + 1] = t
        end
    end
    return out
end

-- Stat lines for an exact link (random suffixes included), or by item id.
function ns.statLines(link, itemID)
    local data = itemID and read("C_TooltipInfo.GetItemByID", itemID)
    if type(data) ~= "table" then
        data = read("C_TooltipInfo.GetHyperlink", link or (itemID and "item:" .. itemID))
    end
    return ns.tooltipStatLines(data)
end

-- Events: any number of listeners per event --------------------------------

local frame = CreateFrame("Frame")
local listeners = {}

function ns.on(event, fn)
    if not listeners[event] then
        listeners[event] = {}
        pcall(frame.RegisterEvent, frame, event)
    end
    table.insert(listeners[event], fn)
end

frame:SetScript("OnEvent", function(_, event, ...)
    for _, fn in ipairs(listeners[event] or {}) do
        pcall(fn, ...)
    end
end)

-- The addon's own messages ("changed": something was captured; the window refreshes).
local messages = {}
function ns.listen(msg, fn)
    messages[msg] = messages[msg] or {}
    table.insert(messages[msg], fn)
end
function ns.fire(msg, ...)
    for _, fn in ipairs(messages[msg] or {}) do
        pcall(fn, ...)
    end
end

-- Commands: each module registers its own; Commands.lua dispatches /fbl ----

ns.commands = {}
-- { name, aliases = {...}, args = "[level]", help = "...", fn = function(rest) }
function ns.command(def)
    table.insert(ns.commands, def)
end

function ns.findCommand(word)
    for _, c in ipairs(ns.commands) do
        if c.name == word then
            return c
        end
        for _, a in ipairs(c.aliases or {}) do
            if a == word then
                return c
            end
        end
    end
end

-- Buttons call this, so a button and its command can't drift apart.
function ns.run(name, rest)
    local c = ns.findCommand(name)
    if c then
        c.fn(rest or "")
    end
end
