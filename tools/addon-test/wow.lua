-- A fake WoW client, just enough to run ForeverBuddy.lua outside the game:
-- the APIs it calls, frames with an event bus, a clock, C_Timer, secret
-- values, and SavedVariables kept as text the way the client keeps them.
--
--   local client = wow.new({ addon = path })
--   client.login(text)        -- run the addon; `text` is the file on disk
--   client.advance(3600)      -- an hour passes (due timers run)
--   local text = client.logout()
--
-- Each login runs the addon in a fresh environment, like a new Lua state in
-- the game. Every API can be made secret, throwing or missing to test the
-- addon's guard.

local serialize = require("serialize").serialize
local unpack = unpack or table.unpack

local M = {}

-- Fri 2 Oct 2026, 18:00 UTC: a fixed clock keeps the fixtures stable.
M.EPOCH = 1790964000

-- The clock stays real in "all" modes: a client never hides the time.
local CLOCK = { GetServerTime = true, time = true }

local LUA = {
    "assert", "error", "getmetatable", "ipairs", "next", "pairs", "pcall", "rawequal",
    "rawget", "rawlen", "rawset", "select", "setmetatable", "tonumber", "tostring", "type",
    "xpcall", "string", "table", "math",
}

-- Loads Lua source with `env` as its globals, on Lua 5.1 or later.
local function loadIn(src, name, env)
    if setfenv then
        local f = assert(loadstring(src, name))
        setfenv(f, env)
        return f
    end
    return assert(load(src, name, "t", env))
end
M.loadIn = loadIn

local function readFile(path)
    local f = assert(io.open(path, "rb"))
    local s = f:read("*a")
    f:close()
    return s
end
M.readFile = readFile

-- The global `name` set by a SavedVariables file's text.
function M.parse(text, name)
    local env = {}
    loadIn(text, "=savedvariables", env)()
    return env[name]
end

local function pack(...)
    return { n = select("#", ...), ... }
end

local function applies(mode, name)
    if mode == "all" then
        return not CLOCK[name]
    end
    return type(mode) == "table" and mode[name] == true
end

-- opts:
--   addon      path to ForeverBuddy.lua
--   character  { name, surname, realm, guid }; the default has a surname,
--              as Forever characters do (probe run 1)
--   api        { Name = function or false } overrides; false removes it
--   secret, throw, missing   "all" (all but the clock) or a set of API names
--   unknown_events           events RegisterEvent refuses, as on Forever
function M.new(opts)
    local client = {
        now = M.EPOCH,
        errors = {}, -- errors that escaped the addon's own OnEvent
        unknown = {},
    }
    for _, e in ipairs(opts.unknown_events or {}) do
        client.unknown[e] = true
    end
    local char = opts.character
        or { name = "Thrandor", surname = "Vargur", realm = "Classic Beta PvP 2", guid = "Player-4613-0A1B2C3D" }
    local secrets = setmetatable({}, { __mode = "k" })
    local state -- the running addon's environment, frames and timers

    -- An opaque value the addon may hold but must never save.
    function client.secret()
        local s = {}
        secrets[s] = true
        return s
    end

    local api = {
        GetServerTime = function()
            return client.now
        end,
        time = function()
            return client.now
        end,
        UnitName = function(unit)
            -- Forever returns the surname second, where retail puts the realm.
            if unit == "player" then
                return char.name, char.surname
            end
        end,
        UnitGUID = function(unit)
            if unit == "player" then
                return char.guid
            end
        end,
        GetRealmName = function()
            return char.realm
        end,
        GetBuildInfo = function()
            return "1.60.1", "70009", "Sep 30 2026", 16001
        end,
    }
    for name, f in pairs(opts.api or {}) do
        api[name] = f or nil
    end

    local function wrap(name, f)
        if applies(opts.throw, name) then
            return function()
                error(name .. ": fake failure")
            end
        end
        if applies(opts.secret, name) then
            return function(...)
                local r = pack(f(...))
                for i = 1, r.n do
                    if r[i] ~= nil then
                        r[i] = client.secret()
                    end
                end
                return unpack(r, 1, r.n)
            end
        end
        return f
    end

    local Frame = {}
    Frame.__index = Frame
    function Frame:RegisterEvent(event)
        if client.unknown[event] then
            error('Frame:RegisterEvent(): Attempt to register unknown event "' .. event .. '"', 2)
        end
        self.events[event] = true
    end
    function Frame:UnregisterEvent(event)
        self.events[event] = nil
    end
    function Frame:SetScript(kind, fn)
        self.scripts[kind] = fn
    end

    local function environment()
        local env = {}
        for _, k in ipairs(LUA) do
            env[k] = _G[k]
        end
        env.unpack = unpack
        env._G = env
        env.issecretvalue = function(v)
            return secrets[v] == true
        end
        env.issecrettable = function()
            return false
        end
        -- Rule 5: the addon never talks in chat.
        env.print = function()
            error("ForeverBuddy must not print")
        end
        env.CreateFrame = function()
            local f = setmetatable({ events = {}, scripts = {} }, Frame)
            table.insert(state.frames, f)
            return f
        end
        env.C_Timer = {
            After = function(seconds, fn)
                table.insert(state.timers, { at = client.now + seconds, fn = fn })
            end,
        }
        for name, f in pairs(api) do
            if not applies(opts.missing, name) then
                env[name] = wrap(name, f)
            end
        end
        return env
    end

    function client.fire(event, ...)
        for _, f in ipairs(state.frames) do
            if f.events[event] and f.scripts.OnEvent then
                local ok, err = pcall(f.scripts.OnEvent, f, event, ...)
                if not ok then
                    table.insert(client.errors, event .. ": " .. tostring(err))
                end
            end
        end
    end

    function client.advance(seconds)
        local target = client.now + seconds
        while true do
            table.sort(state.timers, function(a, b)
                return a.at < b.at
            end)
            local due = state.timers[1]
            if not due or due.at > target then
                break
            end
            table.remove(state.timers, 1)
            client.now = due.at
            local ok, err = pcall(due.fn)
            if not ok then
                table.insert(client.errors, "timer: " .. tostring(err))
            end
        end
        client.now = target
    end

    -- Logs in: runs the addon in a fresh state, then hands it the file the
    -- way the client does (after the addon's code ran, before ADDON_LOADED).
    -- `text` is the file on disk, or nil; with `readback = false` the client
    -- fails to load it (the beta bug fixed in build 70009).
    function client.login(text, o)
        o = o or {}
        state = { frames = {}, timers = {} }
        state.env = environment()
        loadIn(readFile(opts.addon), "@ForeverBuddy.lua", state.env)("ForeverBuddy", {})
        if text and o.readback ~= false then
            state.env.ForeverBuddyDB = M.parse(text, "ForeverBuddyDB")
        end
        client.fire("ADDON_LOADED", "ForeverBuddy")
        client.fire("PLAYER_LOGIN")
        client.fire("PLAYER_ENTERING_WORLD", not o.reload, o.reload == true)
    end

    -- Logs out and returns the file the client would write.
    function client.logout()
        client.fire("PLAYER_LOGOUT")
        local db = state.env.ForeverBuddyDB
        if db == nil then
            return nil
        end
        local function noSecrets(v, path)
            if secrets[v] then
                error("a secret value reached the file at " .. path, 0)
            end
            if type(v) == "table" then
                for k, x in pairs(v) do
                    noSecrets(x, path .. "." .. tostring(k))
                end
            end
        end
        noSecrets(db, "ForeverBuddyDB")
        return serialize("ForeverBuddyDB", db)
    end

    -- /reload: the file is written, then everything loads again from it.
    function client.reload(o)
        local text = client.logout()
        o = o or {}
        o.reload = true
        client.login(text, o)
        return text
    end

    return client
end

return M
