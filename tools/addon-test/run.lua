-- Runs ForeverBuddy.lua against the fake client in wow.lua, scenario by
-- scenario, checks what it wrote, and saves those files as fixtures for the
-- Rust ingest tests (src-tauri/tests/fixtures/addon/<scenario>.lua).
--
--   lua5.1 tools/addon-test/run.lua           run, and rewrite the fixtures
--   lua5.1 tools/addon-test/run.lua --check   run; fail if a fixture is stale
--
-- Plain Lua 5.1 like the game (LuaJIT and later Lua versions work too).

local dir = (arg and arg[0] or ""):match("^(.*)[/\\]") or "."
package.path = dir .. "/?.lua;" .. package.path

local wow = require("wow")
local serialize = require("serialize").serialize

local ROOT = dir .. "/../.."
local ADDON_DIR = ROOT .. "/src-tauri/resources/addon/ForeverBuddy/"
local FIXTURES = ROOT .. "/src-tauri/tests/fixtures/addon/"
local DAY, HOUR, MINUTE = 86400, 3600, 60

-- Checks ---------------------------------------------------------------------

local function fail(msg)
    error(msg, 0)
end

local function show(v)
    return type(v) == "string" and string.format("%q", v) or tostring(v)
end

local function eq(got, want, what)
    if got ~= want then
        fail(what .. ": expected " .. show(want) .. ", got " .. show(got))
    end
end

local function entries(t)
    local n = 0
    for _ in pairs(t or {}) do
        n = n + 1
    end
    return n
end

-- Reads a file the addon wrote and checks what holds for every one of them:
-- the shape, and _meta.counts against a recount done here, independently.
local function file(text)
    local db = wow.parse(text, "ForeverBuddyDB")
    eq(type(db), "table", "ForeverBuddyDB")
    local meta = db._meta
    eq(type(meta), "table", "_meta")
    eq(meta.schema, 1, "_meta.schema")
    local events, bagItems = 0, 0
    for _, s in pairs(db.sessions) do
        events = events + entries(s.events)
    end
    for _, bag in pairs(db.snapshot and db.snapshot.bags or {}) do
        bagItems = bagItems + entries(bag.items)
    end
    local want = { sessions = entries(db.sessions), events = events, items = entries(db.items), bag_items = bagItems }
    eq(entries(meta.counts), entries(want), "number of _meta.counts")
    for k, v in pairs(want) do
        eq(meta.counts[k], v, "_meta.counts." .. k)
    end
    return db
end

-- Scenarios ------------------------------------------------------------------

local scenarios = {}
local clients -- every client the running scenario made

local function scenario(name, fn)
    scenarios[#scenarios + 1] = { name = name, fn = fn }
end

local function client(opts)
    opts = opts or {}
    opts.addon = ADDON_DIR .. "ForeverBuddy.lua"
    local c = wow.new(opts)
    clients[#clients + 1] = c
    return c
end

-- Logs in with `text` on disk, plays `minutes`, logs out: the new file.
local function play(c, text, minutes, o)
    c.login(text, o)
    c.advance(minutes * MINUTE)
    return c.logout()
end

local function firstFile()
    local c = client()
    return play(c, nil, 120), c
end

scenario("first_login", function()
    local text = firstFile()
    local db = file(text)
    eq(#db.sessions, 1, "sessions")
    local s = db.sessions[1]
    eq(s.id, wow.EPOCH, "session id")
    eq(s.login, wow.EPOCH, "login")
    eq(s.logout, wow.EPOCH + 2 * HOUR, "logout")
    eq(#s.events, 0, "events")
    eq(db.snapshot.at, s.logout, "snapshot.at")
    eq(db.character.name, "Thrandor", "name")
    eq(db.character.surname, "Vargur", "surname")
    eq(db.character.realm, "Classic Beta PvP 2", "realm")
    eq(db.character.guid, "Player-4613-0A1B2C3D", "guid")
    eq(db._meta.build, "1.60.1.70009", "build")
    eq(db._meta.written, s.logout, "written")
    eq(db._meta.loaded_prior, false, "loaded_prior")
    eq(db._meta.truncated, false, "truncated")
    eq(db._meta.secret_hits, 0, "secret_hits")
    eq(#db._meta.missing_events, 0, "missing_events")
    eq(db._meta.errors, nil, "errors")
    return text
end)

-- The client read the file back: the old session is carried forward.
scenario("second_login", function()
    local first, c = firstFile()
    c.advance(DAY)
    local text = play(c, first, 60)
    local db = file(text)
    eq(#db.sessions, 2, "sessions")
    eq(db.sessions[1].login, wow.EPOCH, "first session kept")
    eq(db.sessions[1].logout, wow.EPOCH + 2 * HOUR, "first session's logout kept")
    eq(db.sessions[2].login, wow.EPOCH + 2 * HOUR + DAY, "second session")
    eq(db._meta.loaded_prior, true, "loaded_prior")
    return text
end)

-- The client didn't load the file: this session is still written whole,
-- and only the session the app may not have seen is lost.
scenario("readback_missing", function()
    local first, c = firstFile()
    c.advance(DAY)
    local text = play(c, first, 60, { readback = false })
    local db = file(text)
    eq(#db.sessions, 1, "sessions")
    eq(db.sessions[1].login, wow.EPOCH + 2 * HOUR + DAY, "this session")
    eq(db._meta.loaded_prior, false, "loaded_prior")
    return text
end)

-- A loaded file that doesn't validate is ignored, never merged.
scenario("rejects_bad_prior", function()
    local first, c = firstFile()
    local bad = {
        ["newer schema"] = function(db) db._meta.schema = 2 end,
        ["wrong counts"] = function(db) db._meta.counts.sessions = 2 end,
        ["no counts"] = function(db) db._meta.counts = nil end,
        ["no _meta"] = function(db) db._meta = nil end,
        ["sessions not a table"] = function(db) db.sessions = "x" end,
        ["not a table"] = function() return "x" end,
    }
    for what, spoil in pairs(bad) do
        local db = wow.parse(first, "ForeverBuddyDB")
        db = spoil(db) or db
        c.advance(DAY)
        local after = file(play(c, serialize("ForeverBuddyDB", db), 60))
        eq(#after.sessions, 1, what .. ": sessions")
        eq(after._meta.loaded_prior, false, what .. ": loaded_prior")
    end
end)

-- /reload writes the file and loads it again; it's still one session.
scenario("reload", function()
    local c = client()
    c.login(nil)
    c.advance(30 * MINUTE)
    c.reload()
    c.advance(30 * MINUTE)
    local text = c.logout()
    local db = file(text)
    eq(#db.sessions, 1, "sessions")
    eq(db.sessions[1].login, wow.EPOCH, "login")
    eq(db.sessions[1].logout, wow.EPOCH + HOUR, "logout")
    eq(db._meta.loaded_prior, true, "loaded_prior")

    -- If the reload didn't read the file back, the second half starts over.
    c = client()
    c.login(nil)
    c.advance(30 * MINUTE)
    c.reload({ readback = false })
    c.advance(30 * MINUTE)
    db = file(c.logout())
    eq(#db.sessions, 1, "sessions without read-back")
    eq(db.sessions[1].login, wow.EPOCH + 30 * MINUTE, "login without read-back")
    return text
end)

-- An event the client refuses to register is listed, and the rest work. Here
-- that's PLAYER_ENTERING_WORLD, so a reload can't be told from a login and
-- starts a second session.
scenario("unknown_event", function()
    local c = client({ unknown_events = { "PLAYER_ENTERING_WORLD" } })
    c.login(nil)
    c.advance(30 * MINUTE)
    c.reload()
    c.advance(30 * MINUTE)
    local text = c.logout()
    local db = file(text)
    eq(#db._meta.missing_events, 1, "missing_events")
    eq(db._meta.missing_events[1], "PLAYER_ENTERING_WORLD", "missing event")
    eq(#db.sessions, 2, "sessions")
    eq(db._meta.errors, nil, "errors")
    return text
end)

-- Every guarded call returns secrets: nothing secret is saved (wow.lua
-- refuses to write one), fields are left out rather than zeroed, and the
-- hits are counted.
scenario("secret_values", function()
    local c = client({ secret = "all" })
    local text = play(c, nil, 60)
    local db = file(text)
    eq(db.character.name, nil, "name")
    eq(db.character.surname, nil, "surname")
    eq(db.character.realm, nil, "realm")
    eq(db.character.guid, nil, "guid")
    eq(db._meta.build, nil, "build")
    eq(#db.sessions, 1, "sessions")
    if db._meta.secret_hits < 7 then
        fail("secret_hits: expected at least 7, got " .. show(db._meta.secret_hits))
    end
    return text
end)

-- A character without a surname (older characters, or a client that
-- returns none) just has no surname field.
scenario("no_surname", function()
    local c = client({ character = { name = "Brannic", realm = "Classic Beta PvP 2" } })
    local db = file(play(c, nil, 60))
    eq(db.character.name, "Brannic", "name")
    eq(db.character.surname, nil, "surname")
end)

-- APIs that throw or don't exist give nil, not errors.
scenario("api_failures", function()
    for _, mode in ipairs({ "throw", "missing" }) do
        local c = client({ [mode] = "all" })
        local db = file(play(c, nil, 60))
        eq(db.character.name, nil, mode .. ": name")
        eq(db._meta.build, nil, mode .. ": build")
        eq(db._meta.secret_hits, 0, mode .. ": secret_hits")
        eq(db._meta.errors, nil, mode .. ": errors")
        eq(#db.sessions, 1, mode .. ": sessions")
    end
end)

-- Twelve sessions: the file keeps the newest ten and says it dropped some.
scenario("session_cap", function()
    local c = client()
    local text, logins = nil, {}
    for i = 1, 12 do
        logins[i] = c.now
        text = play(c, text, 60)
        local db = file(text)
        eq(db._meta.truncated, i > 10, "truncated after session " .. i)
        c.advance(DAY)
    end
    local db = file(text)
    eq(#db.sessions, 10, "sessions")
    eq(db.sessions[1].login, logins[3], "oldest kept")
    eq(db.sessions[10].login, logins[12], "newest")
    return text
end)

-- The TOC loads the file, names the SavedVariables, and has the version the
-- addon writes into _meta.
scenario("toc", function()
    local toc = wow.readFile(ADDON_DIR .. "ForeverBuddy.toc")
    local fields, files = {}, {}
    for line in toc:gmatch("[^\r\n]+") do
        local k, v = line:match("^## (%w+): (.*)$")
        if k then
            fields[k] = v
        elseif not line:match("^#") then
            files[#files + 1] = line
        end
    end
    eq(fields.Interface, "16001", "Interface")
    eq(fields.SavedVariablesPerCharacter, "ForeverBuddyDB", "SavedVariablesPerCharacter")
    eq(fields.SavedVariables, nil, "account-wide SavedVariables")
    eq(#files, 1, "files")
    eq(files[1], "ForeverBuddy.lua", "file")
    local db = file(firstFile())
    eq(fields.Version, db._meta.addon, "Version")
end)

-- Runner ---------------------------------------------------------------------

local check = arg and arg[1] == "--check"
local failures, stale = 0, 0

for _, s in ipairs(scenarios) do
    clients = {}
    local ok, text = xpcall(s.fn, debug.traceback)
    if ok then
        for _, c in ipairs(clients) do
            if #c.errors > 0 then
                ok, text = false, "errors escaped the addon:\n  " .. table.concat(c.errors, "\n  ")
                break
            end
        end
    end
    if not ok then
        failures = failures + 1
        print("FAIL " .. s.name .. ": " .. tostring(text))
    elseif text then
        local path = FIXTURES .. s.name .. ".lua"
        local content = "-- Written by tools/addon-test/run.lua (scenario " .. s.name
            .. "). Don't edit: change the scenario and run it again.\n" .. text
        local f = io.open(path, "rb")
        local current = f and f:read("*a")
        if f then
            f:close()
        end
        if current == content then
            print("ok   " .. s.name)
        elseif check then
            stale = stale + 1
            print("STALE " .. s.name .. ": " .. path .. " differs from what the addon writes now")
        else
            local out = assert(io.open(path, "wb"))
            out:write(content)
            out:close()
            print("ok   " .. s.name .. " (fixture written)")
        end
    else
        print("ok   " .. s.name)
    end
end

if stale > 0 then
    print("Run `lua5.1 tools/addon-test/run.lua` and commit the fixtures.")
end
if failures > 0 or stale > 0 then
    os.exit(1)
end
