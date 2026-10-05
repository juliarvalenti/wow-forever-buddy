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

handlers.PLAYER_LOGIN = function()
    local t = now()
    character = identity()
    session = { id = t, login = t, events = {} }
end

-- After /reload the client has just written the file and read it back, and
-- PLAYER_LOGIN fired again: carry on with the session that was running
-- instead of starting another. If the file didn't load, the reload starts a
-- new session; the two just aren't joined.
handlers.PLAYER_ENTERING_WORLD = function(_, isReloadingUi)
    if entered then
        return
    end
    entered = true
    if not (isReloadingUi and loaded and session) or #session.events > 0 then
        return
    end
    local last = loaded.sessions[#loaded.sessions]
    if type(last) == "table" and type(last.events) == "table" then
        loaded.sessions[#loaded.sessions] = nil
        last.logout = nil
        session = last
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
