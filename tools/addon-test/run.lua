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
-- Layout snapshots (SIM4): text, not SavedVariables, so they stay out of the
-- Rust fixtures folder, which the ingest tests read whole.
local SNAPSHOTS = dir .. "/snapshots/"
local function flag(name)
    for _, a in ipairs(arg or {}) do
        if a == name then
            return true
        end
    end
    return false
end
local CHECK = flag("--check")
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

-- W1b: a colour the window set ({ r, g, b[, a] }) as "#rrggbb", as STYLE
-- and the design write it.
local function hex(c)
    return c and string.format("#%02x%02x%02x", math.floor(c[1] * 255 + 0.5), math.floor(c[2] * 255 + 0.5),
        math.floor(c[3] * 255 + 0.5))
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
    -- Counted only when the file has the list (0.4.0 on).
    if db.snapshot and db.snapshot.quests_done then
        want.quests_done = entries(db.snapshot.quests_done)
    end
    eq(entries(meta.counts), entries(want), "number of _meta.counts")
    for k, v in pairs(want) do
        eq(meta.counts[k], v, "_meta.counts." .. k)
    end
    return db
end

-- A frame tree's layout as stable text: per widget its kind and name, its
-- anchors (relative to a named frame, its parent, or another widget by its
-- path), size, text and whether it's shown. Children follow in creation
-- order, so the same build gives the same text.
local function layoutDump(root)
    local paths = {}
    local function index(w, path)
        paths[w] = path
        for i, c in ipairs(w.children) do
            index(c, path .. "." .. i)
        end
    end
    index(root, rawget(root, "name") or "root")
    local function rel(r)
        if r == nil then
            return "parent"
        end
        return rawget(r, "name") or paths[r] or "outside"
    end
    local lines = {}
    local function walk(w, depth)
        local parts = { string.rep("  ", depth) .. (rawget(w, "kind") or "Frame") }
        if rawget(w, "name") then
            parts[#parts + 1] = rawget(w, "name")
        end
        for _, a in ipairs(w.points) do
            parts[#parts + 1] = string.format("%s>%s:%s(%s,%s)", a[1], rel(a[2]), a[3] or a[1], a[4], a[5])
        end
        if rawget(w, "width") or rawget(w, "height") then
            parts[#parts + 1] = string.format("%sx%s", rawget(w, "width") or "?", rawget(w, "height") or "?")
        end
        if rawget(w, "text") and rawget(w, "text") ~= "" then
            parts[#parts + 1] = string.format("%q", rawget(w, "text"))
        end
        if not w.shown then
            parts[#parts + 1] = "hidden"
        end
        lines[#lines + 1] = table.concat(parts, " ")
        for _, c in ipairs(w.children) do
            walk(c, depth + 1)
        end
    end
    walk(root, 0)
    return table.concat(lines, "\n") .. "\n"
end

-- Compares `text` with snapshots/<name>.txt; rewrites it unless --check, so
-- a layout change shows up as a diff to review and commit.
local function snapshot(name, text)
    local path = SNAPSHOTS .. name .. ".txt"
    local f = io.open(path, "rb")
    local current = f and f:read("*a")
    if f then
        f:close()
    end
    if current == text then
        return
    end
    if CHECK then
        fail("snapshot " .. path .. " differs from the layout now: run without --check and review the diff")
    end
    local out = assert(io.open(path, "wb"))
    out:write(text)
    out:close()
end

-- A file generated from the addon for the app (P4a's settings-schema.json):
-- rewritten when it changed, and under --check, a difference fails.
local function generated(path, text)
    local f = io.open(path, "rb")
    local current = f and f:read("*a")
    if f then
        f:close()
    end
    if current == text then
        return
    end
    if CHECK then
        fail(path .. " differs from what the addon declares now: run without --check and commit it")
    end
    local out = assert(io.open(path, "wb"))
    out:write(text)
    out:close()
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
    eq(s.start.money, 25000, "start.money")
    eq(s.start.xp, 1200, "start.xp")
    eq(s.start.level, 12, "start.level")
    eq(s.start.zone, "Elwynn Forest", "start.zone")
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

-- /reload writes the file and loads it again; it's still one session, with
-- the events from both sides of it.
scenario("reload", function()
    local c = client()
    c.login(nil)
    c.advance(30 * MINUTE)
    c.loot(2589, 2)
    c.reload()
    c.advance(30 * MINUTE)
    c.loot(14047, 1)
    local text = c.logout()
    local db = file(text)
    eq(#db.sessions, 1, "sessions")
    local s = db.sessions[1]
    eq(s.login, wow.EPOCH, "login")
    eq(s.logout, wow.EPOCH + HOUR, "logout")
    eq(#s.events, 2, "events")
    eq(s.events[1].item, 2589, "before the reload")
    eq(s.events[2].item, 14047, "after the reload")
    eq(db._meta.loaded_prior, true, "loaded_prior")
    -- Played time is asked for once per login (it prints to chat), and
    -- carries on across the reload from the file.
    eq(c.world.requests.played, 1, "RequestTimePlayed calls")
    eq(db.snapshot.played.total, c.world.played + HOUR, "played across the reload")

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

-- In `events` below: this field must be absent.
local NONE = {}

-- Checks a session's events: `want` lists, in order, a table of the fields
-- each event must have (other fields aren't checked; NONE means absent).
local function events(s, want)
    local kinds = {}
    for i, e in ipairs(s.events) do
        kinds[i] = e.kind
    end
    eq(#s.events, #want, "events (" .. table.concat(kinds, ", ") .. ")")
    for i, fields in ipairs(want) do
        for k, v in pairs(fields) do
            eq(s.events[i][k], v ~= NONE and v or nil,"event " .. i .. " (" .. tostring(s.events[i].kind) .. ")." .. k)
        end
    end
end

-- An evening in Westfall and the Deadmines: every kind of event once.
-- V2: everything the snapshot holds, from a session with a bank and a
-- mailbox visit, a lockout, and an item the client had to load.
local function stocked(o)
    local c = client(o)
    c.world.lockouts = {
        { name = "The Deadmines", reset = 2 * DAY, raid = false, difficulty = "Normal" },
        { name = "Molten Core", reset = 6 * DAY, raid = true, difficulty = "40 Player" },
    }
    c.world.inbox = {
        { sender = "Coinpurse", subject = "Linen for you", money = 500, cod = 0, days = 29.5,
          items = { { id = 2589, count = 10 } } },
    }
    c.world.uncached[14047] = true -- the bank's Runecloth
    return c
end

scenario("snapshot", function()
    local c = stocked()
    local t0 = wow.EPOCH
    c.login(nil)
    c.advance(10 * MINUTE)
    c.bank()
    c.advance(5 * MINUTE)
    c.mail()
    c.advance(HOUR)
    local text = c.logout()
    local db = file(text)
    local out = t0 + HOUR + 15 * MINUTE

    local ch = db.character
    eq(ch.class, "WARRIOR", "class")
    eq(ch.race, "Human", "race")
    eq(ch.sex, 2, "sex")
    eq(ch.faction, "Alliance", "faction")
    eq(ch.level, 12, "level")
    eq(ch.guild.name, "Hearthguard", "guild")
    eq(ch.guild.rank, "Officer", "guild rank")

    local s = db.snapshot
    eq(s.at, out, "at")
    eq(s.money, 25000, "money")
    eq(s.xp, 1200, "xp")
    eq(s.xp_max, 8800, "xp_max")
    eq(s.rested, 674, "rested")
    eq(s.rest_state, "Rested", "rest_state")
    eq(s.ilvl.avg, 21.5, "ilvl.avg")
    eq(s.ilvl.equipped, 20.25, "ilvl.equipped")
    eq(s.played.total, c.world.played + (out - t0), "played.total")
    eq(s.played.level, c.world.played_level + (out - t0), "played.level")
    eq(s.zone.zone, "Elwynn Forest", "zone")
    eq(s.zone.subzone, "Goldshire", "subzone")
    eq(s.zone.map, 1429, "map")
    eq(s.equipped[16], wow.link(25), "main hand")
    eq(s.bags[0].size, 16, "backpack size")
    eq(s.bags[0].free, 14, "backpack free")
    eq(s.bags[0].name, "Backpack", "backpack name")
    eq(s.bags[0].items[1].link, wow.link(6948), "backpack slot 1")
    eq(s.bags[0].items[2].count, 4, "backpack slot 2 count")
    eq(s.bags[1], nil, "an empty bag slot")

    eq(s.bank.at, t0 + 10 * MINUTE, "bank.at")
    eq(s.bank.bags[-1].items[1].count, 20, "bank slot 1")
    eq(s.bank.bags[6].size, 98, "bank tab")
    eq(s.mail.at, t0 + 15 * MINUTE, "mail.at")
    local letter = s.mail.items[1]
    eq(letter.sender, "Coinpurse", "sender")
    eq(letter.subject, "Linen for you", "subject")
    eq(letter.money, 500, "letter money")
    eq(letter.days_left, 29.5, "days_left")
    eq(letter.items[1].link, wow.link(2589), "attachment")
    eq(letter.items[1].count, 10, "attachment count")

    eq(#s.professions, 3, "professions")
    eq(s.professions[3].name, "Cooking", "the fifth index, after two nils")
    eq(s.professions[1].skill, 60, "skill")
    eq(s.professions[1].spec, nil, "no specialization (-1)")
    eq(s.professions[2].spec, 2, "a specialization index")
    eq(#s.lockouts, 2, "lockouts")
    eq(s.lockouts[1].reset_at, t0 + 1 + 2 * DAY, "reset_at")
    eq(s.lockouts[1].raid, nil, "a dungeon isn't a raid")
    eq(s.lockouts[2].raid, true, "a raid")
    eq(s.lockouts[2].difficulty, "40 Player", "raid difficulty")

    -- Every item seen has its static info, including the one that had to load.
    for _, id in ipairs({ 25, 2589, 6948, 14047 }) do
        eq(db.items[id] and db.items[id].name, wow.ITEMS[id], "items[" .. id .. "]")
    end
    eq(c.world.requests.items, 1, "item data requests")
    eq(c.world.requests.played, 1, "RequestTimePlayed calls")
    return text
end)

-- BUG-SATCHELS and BUG-ADV (Julia's first logouts): on Forever the bags load
-- a moment after login, and by PLAYER_LOGOUT the client has torn the
-- character down, so bags, gear and money read empty then. Logging in isn't
-- a gain, and the file holds what was seen during play.
scenario("forever", function()
    local c = stocked({ forever = true })
    local t0 = wow.EPOCH
    c.login(nil)
    c.advance(10 * MINUTE)
    c.give(117, 2) -- the last bag change
    c.advance(MINUTE)
    -- AUDIT-F: changes with no bag update still reach the kept snapshot.
    c.gainXp(500)
    c.setMoney(26000)
    c.advance(MINUTE)
    -- The logout card, read after the teardown has begun: the gold is this
    -- session's +10s from play, not the whole purse lost to a 0.
    c.world.phase = "torn"
    c.startLogout()
    local card = c.global("ForeverBuddyCardFrame")
    local gold
    for _, r in ipairs(card.rows) do
        if r.shown and r.label.text == "Gold" then
            gold = r.value.text
        end
    end
    eq(gold, "|cff1eff00+10s|r", "the card's gold, from play")
    -- A saves update during the teardown reads none; this session's stay.
    c.fire("UPDATE_INSTANCE_INFO")
    local db = file(c.logout())

    local s = db.sessions[1]
    eq(s.events[1].kind, "gain", "the jerky is the first gain")
    eq(s.events[1].item, 117, "not the whole inventory at login")
    local gains = 0
    for _, e in ipairs(s.events) do
        gains = gains + (e.kind == "gain" and 1 or 0)
    end
    eq(gains, 1, "and the only gain")

    local snap = db.snapshot
    eq(snap.at >= t0 + 1 + 11 * MINUTE and snap.at < db._meta.written, true, "as of the last good look")
    eq(snap.money, 26000, "money from play, not the teardown's 0")
    eq(snap.xp, 1700, "XP gained after the last bag change")
    eq(table.concat(snap.quests_done, ","), "7,783", "quests done, from play")
    eq(#snap.professions, 3, "professions, from play")
    eq(db.character.level, 12, "the level isn't the teardown's 0")
    eq(snap.bags[0].size, 16, "the backpack")
    eq(snap.bags[0].items[1].link, wow.link(6948), "its items")
    eq(snap.equipped[16], wow.link(25), "worn gear")
    eq(db._meta.counts.bag_items > 0, true, "counted")
    eq(snap.lockouts and #snap.lockouts, 2, "kept apart from it, still there")
    eq(snap.played.total ~= nil, true, "played, as of the logout")

    -- With nothing seen during play (the bags never loaded), no snapshot:
    -- the app keeps what it had rather than empty bags.
    local early = client({ forever = true })
    early.world.bags = {}
    early.login(nil)
    local text = early.logout()
    eq(file(text).snapshot, nil, "no hollow snapshot")
end)

-- R2a: no snapshot is taken in combat, and the throttled retry that lands
-- mid-fight is dropped. Leaving combat takes one, so loot from the fight is
-- in the file even when nothing else changes before an instant logout.
scenario("combat_loot", function()
    local c = stocked({ forever = true })
    c.login(nil)
    c.advance(10 * MINUTE)
    c.give(117, 2) -- a good snapshot, out of combat
    local function linen(items)
        local n = 0
        for _, item in pairs(items) do
            if item and (item.id == 2589 or item.link == wow.link(2589)) then
                n = n + item.count
            end
        end
        return n
    end
    local before = linen(c.world.bags[0].slots)
    c.combat(true)
    c.advance(5)
    c.loot(2589, 3) -- inside the throttle window: a retry is scheduled
    c.advance(30) -- it fires while still fighting, and does nothing
    c.combat(false)
    c.advance(5)
    local snap = file(c.logout()).snapshot
    eq(linen(snap.bags[0].items), before + 3, "the fight's loot is in the snapshot")
end)

-- The bank and mailbox can only be read there, so a session without a visit
-- carries the last ones forward instead of wiping them (probe runs 2-3).
scenario("carry_forward", function()
    local c = stocked()
    c.login(nil)
    c.advance(10 * MINUTE)
    c.bank()
    c.mail()
    c.advance(HOUR)
    local first = c.logout()
    local visited = c.now - HOUR

    c.advance(DAY)
    local text = play(c, first, 60)
    local db = file(text)
    eq(db.snapshot.bank.at, visited, "bank as of the visit")
    eq(db.snapshot.mail.at, visited, "mail as of the visit")
    eq(db.snapshot.bank.bags[-1].items[1].count, 20, "bank contents")
    eq(db.snapshot.lockouts[1].name, "The Deadmines", "lockouts")

    -- Without read-back there's nothing to carry: left out, never emptied.
    c.advance(DAY)
    db = file(play(c, text, 60, { readback = false }))
    eq(db.snapshot.bank, nil, "bank without read-back")
    eq(db.snapshot.mail, nil, "mail without read-back")
    return text
end)

scenario("adventure", function()
    local c = client()
    local t0 = wow.EPOCH
    c.login(nil)
    c.advance(5 * MINUTE)
    c.enterZone("Westfall")
    c.enterZone("Westfall") -- the same zone again isn't a change
    c.advance(MINUTE)
    c.loot(2589, 3)
    c.setMoney(25150)
    c.advance(10)
    c.setMoney(25200) -- within a minute: updates the last point
    c.advance(10 * MINUTE)
    c.die(800)
    c.advance(MINUTE)
    c.openMerchant()
    c.sell(2589, 7, 70)
    c.buy(117, 5, 125)
    c.repairAll()
    c.closeMerchant()
    c.advance(MINUTE)
    c.use(117)
    c.advance(5 * MINUTE)
    c.turnIn(176, 1350, 1200)
    c.levelUp()
    c.advance(MINUTE)
    c.enterZone("The Deadmines", true)
    c.advance(20 * MINUTE)
    c.encounter(639, "Edwin VanCleef", false) -- a wipe isn't logged
    c.encounter(639, "Edwin VanCleef", true)
    c.advance(10 * MINUTE)
    local text = c.logout()
    local db = file(text)
    local merchant = t0 + 17 * MINUTE + 10
    events(db.sessions[1], {
        { kind = "zone", t = t0 + 5 * MINUTE, zone = "Westfall" },
        { kind = "gain", t = t0 + 6 * MINUTE, item = 2589, count = 3 },
        { kind = "money", t = t0 + 6 * MINUTE, money = 25200 },
        { kind = "death", t = t0 + 16 * MINUTE + 10, zone = "Westfall" },
        { kind = "lose", t = merchant, item = 2589, count = 7, how = "sold" },
        { kind = "money", t = merchant, money = 25200 + 70 - 125 - 800 },
        { kind = "gain", item = 117, count = 5, how = "bought" },
        { kind = "repair", cost = 800 },
        { kind = "lose", item = 117, count = 1, how = "used" },
        { kind = "quest", id = 176, title = "Wanted: Hogger", xp = 1350, money = 1200 },
        { kind = "money", money = 25200 + 70 - 125 - 800 + 1200 },
        { kind = "level", level = 13 },
        { kind = "zone", zone = "The Deadmines", instance = true },
        { kind = "encounter", id = 639, name = "Edwin VanCleef" },
    })
    eq(db.sessions[1].events[1].instance, nil, "Westfall isn't an instance")
    eq(db.sessions[1].events[2].how, nil, "looting has no how")
    eq(db.sessions[1].start.zone, "Elwynn Forest", "start.zone")
    return text
end)

-- Moving things around isn't gaining or losing them: the bank, and swapping
-- gear. The mailbox is gaining and losing, and says so.
scenario("moves_are_not_loot", function()
    local c = client()
    c.login(nil)
    c.bank({ [2589] = 4 }, { [14047] = 2 })
    c.loot(2488, 1)
    c.equip(16, 2488) -- the Worn Shortsword goes back to the bags
    c.mail({ [25] = 1 }, { [14047] = 5 })
    local db = file(c.logout())
    events(db.sessions[1], {
        { kind = "gain", item = 2488, count = 1 },
        { kind = "lose", item = 25, count = 1, how = "mailed" },
        { kind = "gain", item = 14047, count = 5, how = "mail" },
    })
end)

-- Secret event arguments and unreadable bags: fields are left out, an
-- encounter that can't be told a success isn't logged, and items aren't
-- diffed at all rather than seeming to vanish.
scenario("secret_session", function()
    local c = client({
        secret_args = { PLAYER_LEVEL_UP = true, QUEST_TURNED_IN = true, ENCOUNTER_END = true },
        secret = { ["C_Container.GetContainerItemInfo"] = true },
    })
    c.login(nil)
    c.advance(MINUTE)
    c.loot(2589, 3)
    c.turnIn(176, 1350, 1200)
    c.levelUp()
    c.encounter(639, "Edwin VanCleef", true)
    c.advance(MINUTE)
    c.use(6948)
    local text = c.logout()
    local db = file(text)
    events(db.sessions[1], {
        { kind = "quest", id = NONE, title = NONE, xp = NONE, money = NONE },
        { kind = "money", money = 26200 },
        { kind = "level", level = NONE },
    })
    if db._meta.secret_hits < 8 then
        fail("secret_hits: expected at least 8, got " .. show(db._meta.secret_hits))
    end
    return text
end)

-- Bags that turn secret mid-session aren't read as everything being used up:
-- that scan is skipped, and the next readable one catches up.
scenario("bags_go_secret", function()
    local c
    c = client({
        api = {
            ["C_Container.GetContainerItemInfo"] = function(bag, slot)
                local item = c.world.bags[bag] and c.world.bags[bag].slots[slot]
                if item and c.hidden then
                    return c.secret()
                elseif item then
                    return { itemID = item.id, stackCount = item.count }
                end
            end,
        },
    })
    c.login(nil)
    c.hidden = true
    c.loot(2589, 1)
    c.hidden = false
    c.use(6948)
    local db = file(c.logout())
    events(db.sessions[1], {
        { kind = "gain", item = 2589, count = 1 },
        { kind = "lose", item = 6948, count = 1, how = "used" },
    })
end)

-- 2,100 deaths: the newest 2,000 are kept, and the file says some went.
scenario("event_cap", function()
    local c = client()
    c.login(nil)
    for _ = 1, 2100 do
        c.advance(1)
        c.die()
    end
    local db = file(c.logout())
    local s = db.sessions[1]
    eq(#s.events, 2000, "events")
    eq(s.events[1].t, wow.EPOCH + 101, "oldest kept")
    eq(db._meta.truncated, true, "truncated")
    eq(db._meta.counts.events, 2000, "counts.events")
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
    -- Account-wide: only the briefing toggle (INGAME §9, "saved per
    -- account"). The character data stays per character (v0.2 spec §2): a
    -- toggle lost to "Exit Now" is harmless, a character's file isn't.
    eq(fields.SavedVariables, "ForeverBuddySettings", "account-wide SavedVariables")
    eq(fields.AddonCompartmentFunc, "ForeverBuddy_OnAddonCompartmentClick", "compartment")
    -- The bridge slots load first, so their globals exist when the addon runs.
    eq(table.concat(files, ", "),
        "Data/Tooltip1.lua, Data/Tooltip2.lua, Data/Plan.lua, Data/Briefing.lua, Data/Lists.lua, Data/Cleanup.lua, "
            .. "Data/Settings.lua, Settings.lua, ForeverBuddy.lua", "files")
    local db = file(firstFile())
    eq(fields.Version, db._meta.addon, "Version")
end)

-- Bridge receipts (bridge spec §4): the addon notes each slot's stamp and
-- schema when it loads, and saves them. The bundled stubs (nil) leave none.
scenario("bridge", function()
    eq(file(firstFile()).bridge, nil, "no receipts from the stubs")

    local c = client({
        slots = {
            ["Data/Tooltip1.lua"] = 'ForeverBuddyData_Tooltip1 = {\n\t["schema"] = 1,\n\t["stamp"] = 1790960000,\n\t["app"] = "0.4.0",\n}\n',
            -- From a newer app: noted, so the app can tell, but not used.
            ["Data/Tooltip2.lua"] = 'ForeverBuddyData_Tooltip2 = {\n\t["schema"] = 2,\n\t["stamp"] = 1790960000,\n}\n',
        },
    })
    local text = play(c, nil, 30)
    local db = file(text)
    eq(db.bridge.Tooltip1.stamp, 1790960000, "Tooltip1 stamp")
    eq(db.bridge.Tooltip1.schema, 1, "Tooltip1 schema")
    eq(db.bridge.Tooltip1.seen, wow.EPOCH, "seen at load")
    eq(db.bridge.Tooltip2.schema, 2, "Tooltip2 schema")
    eq(entries(db.bridge), 2, "receipts")
    return text
end)

-- Quest recording (quest data spike, #94): accepts as session events, and
-- every completed quest id at logout, sorted.
scenario("quests", function()
    local c = client()
    c.login(nil)
    c.advance(10 * MINUTE)
    c.accept(176, { name = "Marshal Dughan" })
    c.advance(20 * MINUTE)
    c.turnIn(176, 450, 75, { name = "Marshal Dughan" })
    c.advance(5 * MINUTE)
    local text = c.logout()
    local db = file(text)
    local kinds = {}
    for _, e in ipairs(db.sessions[1].events) do
        kinds[#kinds + 1] = e.kind
    end
    eq(table.concat(kinds, ","), "quest_accepted,quest,money", "events")
    local accepted = db.sessions[1].events[1]
    eq(accepted.id, 176, "accepted id")
    eq(accepted.title, "Wanted: Hogger", "accepted title")
    -- Where and from whom (Q1b): zone, map, position to 0.1%, the giver.
    eq(accepted.zone, "Elwynn Forest", "zone")
    eq(accepted.map, 1429, "map")
    eq(accepted.x, 0.412, "x")
    eq(accepted.y, 0.657, "y")
    eq(accepted.giver, "Marshal Dughan", "giver")
    eq(db.sessions[1].events[2].giver, "Marshal Dughan", "turned in to")

    -- A quest shared by another player keeps no name; an instance, no position.
    local shared = client()
    shared.login(nil)
    shared.world.instance = true
    shared.accept(176, { name = "Someoneelse", player = true })
    local e = file(shared.logout()).sessions[1].events[1]
    eq(e.giver, nil, "no player's name")
    eq(e.x, nil, "no position in an instance")
    eq(e.zone, "Elwynn Forest", "zone still")
    eq(table.concat(db.snapshot.quests_done, ","), "7,176,783", "quests_done, sorted")
    eq(db._meta.counts.quests_done, 3, "counted")

    -- A client without the API leaves the list out, never empty.
    local old = client({ api = { ["C_QuestLog.GetAllCompletedQuestIDs"] = false } })
    eq(file(play(old, nil, 10)).snapshot.quests_done, nil, "no list without the API")
    return text
end)

-- Tonight's plan (P1, INGAME §7): this character's plan from the Plan slot,
-- /fb plan to show it, live tick-off, the one manual tick, waypoints from
-- recorded positions, and the progress kept across logins.
local PLAN = 'ForeverBuddyData_Plan = {\n\t["schema"] = 1,\n\t["stamp"] = 1790960000,\n\t["plans"] = {\n'
    .. '\t\t{ ["id"] = 7, ["name"] = "Thrandor", ["surname"] = "Vargur", ["title"] = "Hogger", ["steps"] = {\n'
    .. '\t\t\t{ ["text"] = "Take Wanted: Hogger", ["kind"] = "accept", ["quest"] = 176, ["zone"] = "Elwynn Forest",'
    .. ' ["giver"] = "Marshal Dughan", ["map"] = 1429, ["x"] = 0.412, ["y"] = 0.657 },\n'
    .. '\t\t\t{ ["text"] = "Find |Hitem:1|h[Hogger]|h by the river", ["kind"] = "objective", ["zone"] = "Elwynn Forest" },\n'
    .. '\t\t\t{ ["text"] = "Defeat Hogger", ["kind"] = "objective", ["quest"] = 176, ["zone"] = "Elwynn Forest" },\n'
    .. '\t\t\t{ ["text"] = "Hand in Wanted: Hogger", ["kind"] = "turn_in", ["quest"] = 176, ["zone"] = "Elwynn Forest",'
    .. ' ["map"] = 1436, ["x"] = 0.5, ["y"] = 0.5 },\n'
    .. '\t\t} },\n'
    .. '\t\t{ ["id"] = 8, ["name"] = "Coinpurse", ["surname"] = "", ["title"] = "Not yours", ["steps"] = {\n'
    .. '\t\t\t{ ["text"] = "Someone else\'s step", ["kind"] = "objective" },\n'
    .. '\t\t} },\n'
    .. '\t},\n}\n'

scenario("plan", function()
    local c = client({ slots = { ["Data/Plan.lua"] = PLAN } })
    c.login(nil)
    -- The arrival line belongs to the B1 briefing (INGAME §7, §9).
    eq(#c.chat, 0, "no chat line of its own at login")
    c.advance(5)
    eq(table.concat(c.chat, "\n"), "|cffffd100Forever Buddy:|r tonight's plan is ready", "the briefing says it arrived")
    c.chat = {}
    eq(c.global("ForeverBuddyPlanFrame"), nil, "never opens by itself")
    eq(c.global("ForeverBuddyWindow"), nil, "nor does the window (U1)")

    -- U1: /fb plan opens the window's Plan tab, the same steps.
    c.slash("/fb plan")
    local w = c.global("ForeverBuddyWindow")
    eq(w.shown, true, "/fb plan opens the window")
    local tab = w.panes[1]
    eq(tab.scroll.shown, true, "on the Plan tab")
    eq(tab.rows[1].num.text .. ". " .. tab.rows[1].label.text, "1. Take Wanted: Hogger", "the steps")
    eq(hex(tab.rows[1].label.color), "#8e1a16", "the current step in wax (W1b)")
    eq(tab.rows[1].wax.shown, true, "its number on wax")
    eq(tab.rows[2].wax.shown, false, "the next one's isn't")
    eq(tab.rows[2].label.text, "Find ||Hitem:1||h[Hogger]||h by the river", "escaped here too")
    eq(tab.rows[3].go.enabled, false, "no recorded position")
    eq(tab.rows[3].go.shown, false, "so no Show on map")
    eq(tab.head.text, "Elwynn Forest", "the plan's place on the page")
    eq(tab.count.text, "0 of 4 done", "progress")
    eq(w.fresh.text:match("^From Forever Buddy · %d%d:%d%d") ~= nil, true, "the freshness line")
    eq(w.syncButton.shown ~= false and w.sync.text, "", "a Sync button, no typed line (B4)")
    eq(tab.tracker.checked, false, "the tracker is off")
    -- Its checkbox shows the §7 tracker.
    c.click(tab.tracker)
    local f = c.global("ForeverBuddyPlanFrame")
    eq(f.shown, true, "shown")
    eq(f.title.text, "Tonight's plan · Elwynn Forest", "title")
    local row = function(i)
        return f.rows[i]
    end
    eq(row(1).label.text, "1. Take Wanted: Hogger", "step 1")
    eq(row(1).detail.text, "Elwynn Forest · Marshal Dughan", "where")
    eq(table.concat(row(1).label.color, ","), "1,0.82,0", "the current step is gold")
    eq(row(2).label.text, "2. Find ||Hitem:1||h[Hogger]||h by the river", "plan text is escaped")
    eq(row(1).go.enabled, true, "a waypoint in this zone")
    eq(row(4).go.enabled, false, "not from another map")
    eq(row(4).go.tip, "Go to Elwynn Forest first.", "why not")
    eq(row(3).go.enabled, false, "no recorded position")
    eq(row(3).go.tip, "No position recorded for this quest yet.", "says so")
    eq(f.footer.text:match("(%d+ of %d+ done)$"), "0 of 4 done", "footer")

    c.click(row(1).go)
    eq(c.waypoints[1].map, 1429, "the game's own waypoint")
    eq(c.waypoints[1].x, 0.412, "at the recorded spot")

    -- The game says the quest was taken: step 1 ticks and its waypoint goes.
    c.accept(176, { name = "Marshal Dughan" })
    eq(table.concat(row(1).label.color, ","), "0.5,0.5,0.5", "done steps are grey")
    eq(hex(tab.rows[1].label.color), "#9a8162", "in the window too, faded (W1b)")
    eq(tab.rows[1].tick[1].shown and tab.rows[1].num.shown, false, "ticked, its number gone")
    eq(tab.count.text, "1 of 4 done", "the window follows")
    eq(table.concat(row(2).label.color, ","), "1,0.82,0", "next step")
    eq(c.waypoints[2], "cleared", "the ticked step's waypoint is cleared")
    -- Only a step with no quest id is ticked by hand, and can be unticked.
    c.click(row(2))
    c.click(row(2))
    eq(f.footer.text:match("(%d+ of %d+ done)$"), "1 of 4 done", "ticked and unticked")
    c.click(row(2))
    c.click(row(3))
    c.click(row(4))
    eq(f.footer.text:match("(%d+ of %d+ done)$"), "2 of 4 done", "steps the game ticks can't be clicked")
    -- An objective step ticks when its quest is complete in the log.
    c.questComplete(176)
    eq(f.footer.text:match("(%d+ of %d+ done)$"), "3 of 4 done", "objective done")
    c.turnIn(176, 450, 75)
    eq(f.footer.text, "All 4 done · from Forever Buddy", "all done")
    eq(table.concat(c.chat, "\n"), "|cffffd100Forever Buddy:|r tonight's plan is done.", "one chat line when done")

    -- Progress is kept: the next login shows the same ticks and says nothing.
    local text = c.logout()
    eq(file(text).plan.id, 7, "progress saved")
    local again = client({ slots = { ["Data/Plan.lua"] = PLAN } })
    again.login(text)
    again.advance(5)
    eq(#again.chat, 0, "nothing at login for a plan already seen")
    again.slash("/fb plan")
    local tracker = again.global("ForeverBuddyWindow").panes[1].tracker
    again.click(tracker)
    local g = again.global("ForeverBuddyPlanFrame")
    eq(g.footer.text, "All 4 done · from Forever Buddy", "progress after login")
    again.click(tracker)
    eq(g.shown, false, "unticked, it hides")

    -- Another character's plan isn't this one's.
    local other = client({
        slots = { ["Data/Plan.lua"] = PLAN },
        character = { name = "Velyra", surname = "Duskmane", realm = "Classic Beta PvP 2", guid = "Player-1" },
    })
    other.login(nil)
    eq(#other.chat, 0, "no plan, no chat")
    other.slash("/fb plan")
    eq(other.global("ForeverBuddyWindow").panes[1].footer.text,
        "No plan for Velyra. Make one in Forever Buddy, or ask Claude for one.", "empty")
    return text
end)

-- U1: a switch in the window, by its label: on the Settings page, or (W1)
-- on a module's page, which is opened to find it.
local function setting(c, label)
    c.slash("/fb")
    local w = c.global("ForeverBuddyWindow")
    local pages = { w.Tabs[5] }
    for _, b in pairs(w.mods) do
        pages[#pages + 1] = b
    end
    for _, b in ipairs(pages) do
        c.click(b)
        for _, cb in ipairs(w.pages[b.key].checks) do
            if cb.label.text == label then
                return cb
            end
        end
    end
    error("no setting " .. label)
end

-- Alt-aware tooltips (bridge spec §5): lines from the tooltip index, this
-- character's own count live, slot text shown as plain text.
local function tooltipSlot(name, body)
    return "ForeverBuddyData_" .. name .. " = {\n\t[\"schema\"] = 1,\n\t[\"stamp\"] = 1790960000,\n" .. body .. "}\n"
end
local ALTS = '\t["alts"] = {\n'
    .. '\t\t{ ["name"] = "Coinpurse", ["surname"] = "", ["class"] = "WARRIOR", ["seen"] = ' .. (wow.EPOCH - DAY) .. ' },\n'
    -- This character: its row comes live from the game instead.
    .. '\t\t{ ["name"] = "Thrandor", ["surname"] = "Vargur", ["class"] = "PALADIN", ["seen"] = ' .. (wow.EPOCH - DAY) .. ' },\n'
    .. '\t\t{ ["name"] = "Evil|Hitem:19019|h[Thunderfury]|h", ["surname"] = "", ["class"] = "PALADIN", ["seen"] = ' .. (wow.EPOCH - 3 * DAY) .. ' },\n'
    .. '\t},\n'

scenario("tooltip", function()
    local c = client({
        slots = {
            ["Data/Tooltip1.lua"] = tooltipSlot("Tooltip1", ALTS .. '\t["items"] = {},\n'),
            ["Data/Tooltip2.lua"] = tooltipSlot("Tooltip2", ALTS
                .. '\t["scanAt"] = ' .. (wow.EPOCH - 3 * DAY) .. ',\n'
                -- Runecloth: 1g 12s; Coinpurse 340 in the bank, Thrandor 5
                -- in bags (stale), the third alt 3 in the mail.
                .. '\t["items"] = {\n\t\t[14047] = { 11200, 1, 0, 340, 0, 0, 2, 5, 0, 0, 0, 3, 0, 0, 3, 0 },\n\t},\n'),
        },
    })
    c.login(nil)
    local lines = table.concat(c.hover(14047), "\n")
    -- Compact by default (INGAME §8): the other alts on one gold line, names
    -- in class colour (our own codes around the escaped name), then the price
    -- and a hint. No head, no total, no footer, and never this character.
    eq(lines, table.concat({
        "Runecloth",
        " ",
        "Your alts: |cffc79c6eCoinpurse|r 340 bank · |cfff58cbaEvil||Hitem:19019||h[Thunderfury]||h|r 3 mail",
        "~1g 12s each at your last scan",
        "Shift for details",
    }, "\n"), "tooltip")

    -- Shift: this character first, live, then the alts by place and date.
    c.world.shift = true
    eq(table.concat(c.hover(14047), "\n"), table.concat({
        "Runecloth",
        " ",
        "Forever Buddy",
        "Thrandor | 20 · on you",
        "Coinpurse | 340 bank · 1 day ago",
        "Evil||Hitem:19019||h[Thunderfury]||h | 3 mail · 3 days ago",
        "All characters | 363",
        "Last scan | ~1g 12s each · 3 days ago",
        "As of each alt's last logout",
    }, "\n"), "with Shift")
    c.world.shift = false

    eq(#c.hover(2488), 1, "nothing for an item no alt holds")
    -- The Hearthstone is in this character's bags only: nothing either way.
    eq(#c.hover(6948), 1, "nothing for an item only this character holds")
    c.world.shift = true
    eq(#c.hover(6948), 1, "nothing for it with Shift either")
    c.world.shift = false
    c.world.combat = true
    eq(#c.hover(14047), 1, "nothing in combat")
    c.world.combat = false
    -- U1: "Your alts on item tooltips" off leaves the game's own lines.
    local box = setting(c, "Your alts on item tooltips")
    eq(box.checked, true, "on by default")
    c.click(box)
    eq(#c.hover(14047), 1, "off: nothing added")
    c.click(box)
    eq(#c.hover(14047), 5, "on again")

    local big = client({
        slots = { ["Data/Tooltip1.lua"] = tooltipSlot("Tooltip1", '\t["tooLarge"] = true,\n') },
    })
    big.login(nil)
    eq(big.hover(2488)[4], "Alt data too large to send", "too large")

    eq(file(c.logout())._meta.tooltip_errors, nil, "no tooltip errors")
end)

-- TIP2 (INGAME §8): stale places and scans in grey, and the upgrade hint
-- from each alt's level and worn item levels.
local function worn(set)
    local t = {}
    for s = 1, 19 do
        t[s] = set[s] or 0
    end
    return "{ " .. table.concat(t, ", ") .. " }"
end
local function alt(name, class, level, gear, extra, surname)
    return '\t\t{ ["name"] = "' .. name .. '", ["surname"] = "' .. (surname or "") .. '", ["class"] = "' .. class
        .. '", ["seen"] = ' .. (wow.EPOCH - DAY) .. ', ["level"] = ' .. level
        .. ', ["worn"] = ' .. worn(gear) .. (extra or "") .. ' },\n'
end
local G = "|cff808080"
local function day(t)
    return (os.date("!%d %b", t):gsub("^0", ""))
end

-- TIP3 (a), INGAME §15: weapons in the upgrade hint. `hands` are the item
-- ids in main hand, off hand and ranged; the addon asks the game whether the
-- main hand is a two-hander.
scenario("weapons", function()
    local W = "|cffffffff"
    local function hands(mh, oh, r)
        return ', ["hands"] = { ' .. mh .. ', ' .. oh .. ', ' .. r .. ' }'
    end
    local alts = '\t["alts"] = {\n'
        .. alt("Thrandor", "PALADIN", 60, {}, nil, "Vargur") -- this character
        -- Two one-handers, 40 and 30; a dual wielder.
        .. alt("Kaelor", "ROGUE", 60, { [16] = 40, [17] = 30 }, hands(10021, 10021, 0))
        -- A two-hander at 50.
        .. alt("Coinpurse", "WARRIOR", 60, { [16] = 50 }, hands(10020, 0, 0))
        -- A 30 mace, a 20 off hand, a 10 wand.
        .. alt("Sela", "PRIEST", 60, { [16] = 30, [17] = 20, [18] = 10 }, hands(10021, 0, 0))
        -- Two one-handers, 40 and 35; a dual wielder.
        .. alt("Brannic", "HUNTER", 60, { [16] = 40, [17] = 35 }, hands(10021, 10021, 0))
        -- A 48 one-hander and nothing in the off hand (no dual wield).
        .. alt("Grom", "PALADIN", 60, { [16] = 48 }, hands(10021, 0, 0))
        .. '\t},\n'
    local c = client({
        slots = {
            ["Data/Tooltip1.lua"] = tooltipSlot("Tooltip1", alts .. '\t["items"] = {},\n'),
            ["Data/Tooltip2.lua"] = tooltipSlot("Tooltip2", alts .. '\t["items"] = {},\n'),
        },
    })
    -- The same index with Sela holding a Reaper and a sword in her bags, so
    -- the Shift view (which needs a holder) can be checked.
    local held = client({
        slots = {
            ["Data/Tooltip1.lua"] = tooltipSlot("Tooltip1", alts
                .. '\t["items"] = { [10010] = { 0, 4, 1, 0, 0, 0 } },\n'),
            ["Data/Tooltip2.lua"] = tooltipSlot("Tooltip2", alts
                .. '\t["items"] = { [10011] = { 0, 4, 1, 0, 0, 0 } },\n'),
        },
    })
    held.login(nil)
    held.world.equipped[16] = 10010
    c.login(nil)
    -- This character holds the Reaper itself, so it's never the best fit.
    c.world.equipped[16] = 10010
    local KAELOR, SELA, GROM = "|cfffff569Kaelor|r", "|cffffffffSela|r", "|cfff58cbaGrom|r"

    -- A two-handed axe: Brannic against the average of 40 and 35 (+25), Grom
    -- against its main hand alone, the off hand being empty (+15, not +39),
    -- Coinpurse against its two-hander (+13, third: dropped). Rogues and
    -- priests can't.
    eq(c.hover(10010)[3], "Upgrade for Brannic" .. W .. " (+25 item level|r" .. W .. ")|r · " .. GROM .. W
        .. " (+15|r" .. W .. ")|r", "two-hander")
    -- A one-hand sword: dual wielders take the better hand (Kaelor's off
    -- hand, +33; Brannic's off hand, +28). Coinpurse wears a two-hander:
    -- skipped. Priests can't use swords.
    eq(c.hover(10011)[3], "Upgrade for " .. KAELOR .. W .. " (+33 item level|r" .. W .. ")|r · Brannic" .. W
        .. " (+28|r" .. W .. ")|r", "one-hander")
    -- A shield: only warriors, paladins and shamans. Coinpurse wears a
    -- two-hander (skipped); Grom, a paladin, has an empty off hand, and an
    -- empty off hand isn't compared with (§15). Nobody.
    eq(#c.hover(10012), 1, "shield: not over an empty off hand")
    -- A wand: the ranged slot, casters only. Sela +50.
    eq(c.hover(10013)[3], "Upgrade for " .. SELA .. W .. " (+50 item level|r" .. W .. ")|r", "wand")

    -- Shift says what a weapon was set against.
    held.world.shift = true
    local function upgradeRow(id)
        for _, l in ipairs(held.hover(id)) do
            if l:find("^Upgrade for | ") then
                return l
            end
        end
    end
    eq(upgradeRow(10010), "Upgrade for | Brannic +25 " .. G .. "(over main and off hand)|r · " .. GROM .. " +15",
        "Shift: averaged, and the main hand alone (no suffix)")
    eq(upgradeRow(10011), "Upgrade for | " .. KAELOR .. " +33 " .. G .. "(over the off hand)|r · Brannic +28 "
        .. G .. "(over the off hand)|r", "Shift: off hand")

    -- A main hand the client hasn't cached (Brannic's and Grom's sword): no
    -- guess for those alts, so only Coinpurse is left.
    c.world.uncached[10021] = true
    eq(c.hover(10010)[3], "Upgrade for |cffc79c6eCoinpurse|r" .. W .. " (+13 item level|r" .. W .. ")|r",
        "unknown hand: skipped")
end)

scenario("tooltip_v2", function()
    local alts = '\t["alts"] = {\n'
        -- Bank seen 15 days ago: stale.
        .. alt("Coinpurse", "WARRIOR", 30, { [1] = 60, [11] = 50, [12] = 50 }, ', ["bank"] = ' .. (wow.EPOCH - 15 * DAY))
        .. alt("Thrandor", "PALADIN", 60, {}, nil, "Vargur") -- this character: never in the hint
        .. alt("Kaelor", "ROGUE", 52, { [1] = 50, [11] = 44, [12] = 52 })
        .. alt("Sela", "PRIEST", 60, { [1] = 30 }, ', ["mail"] = ' .. (wow.EPOCH - 2 * DAY))
        .. '\t},\n'
    local c = client({
        slots = {
            ["Data/Tooltip1.lua"] = tooltipSlot("Tooltip1", alts .. '\t["items"] = {},\n'),
            ["Data/Tooltip2.lua"] = tooltipSlot("Tooltip2", alts
                .. '\t["scanAt"] = ' .. (wow.EPOCH - 12 * DAY) .. ',\n'
                -- Runecloth: Coinpurse 340 in the (stale) bank, Sela 3 in the (fresh) mail.
                .. '\t["items"] = {\n\t\t[14047] = { 11200, 1, 0, 340, 0, 0, 4, 0, 0, 3, 0 },\n\t},\n'),
        },
    })
    c.login(nil)
    -- This character wears a 63 helm and two 55 rings: neither is an upgrade for it.
    c.world.equipped[1], c.world.equipped[11], c.world.equipped[12] = 10004, 10003, 10003

    -- (a) The stale bank alt is grey whole, dated; the fresh one keeps its
    -- colour. The 12-day-old scan gets a grey age.
    eq(table.concat(c.hover(14047), "\n"), table.concat({
        "Runecloth",
        " ",
        "Your alts: " .. G .. "Coinpurse 340 bank (as of " .. day(wow.EPOCH - 15 * DAY) .. ")|r · |cffffffffSela|r 3 mail",
        "~1g 12s each at your last scan" .. G .. " · 12 days ago|r",
        "Shift for details",
    }, "\n"), "stale compact")
    c.world.shift = true
    local shift = c.hover(14047)
    eq(shift[5], "Coinpurse | " .. G .. "340 bank · 15 days ago|r", "stale Shift row")
    eq(shift[6], "Sela | 3 mail · 1 day ago", "fresh row unchanged")
    eq(shift[8], "Last scan | " .. G .. "~1g 12s each · 12 days ago|r", "stale scan row")
    c.world.shift = false

    -- (b) Leather head 63, needs 58, held by nobody: the hint alone. Kaelor
    -- (rogue, 52, head 50) gains 13 once level 58. Sela is a priest: no
    -- leather. Coinpurse's 60 helm makes it a +3 sidegrade: below +5.
    eq(table.concat(c.hover(10001), "\n"), table.concat({
        "Shadowcraft Cap",
        " ",
        "Upgrade for |cfffff569Kaelor|r|cffffffff (+13 item level|r" .. G .. ", once level 58|r|cffffffff)|r",
    }, "\n"), "hint alone, under level")

    -- A ring (55, needs 50): against each alt's lower ring. Sela +55, Kaelor
    -- +11 (44 < 52), Coinpurse +5 is third: top two only.
    eq(c.hover(10003)[3],
        "Upgrade for |cffffffffSela|r|cffffffff (+55 item level|r|cffffffff)|r · |cfffff569Kaelor|r|cffffffff (+11|r|cffffffff)|r",
        "two names, best first")

    -- Mail head 61: rogues and priests can't, the warrior's 60 helm is a sidegrade.
    eq(#c.hover(10002), 1, "nobody it upgrades")
    -- Cloth hood 40: anyone can wear it. A downgrade for this character's 63
    -- helm, +10 for Sela (head 30).
    eq(c.hover(10005)[3], "Upgrade for |cffffffffSela|r|cffffffff (+10 item level|r|cffffffff)|r", "cloth: anyone")

    -- This character is the best fit: no hint (the game compares for it).
    c.world.equipped[11] = nil
    eq(#c.hover(10003), 1, "best fit is you")
    c.world.equipped[11] = 10003

    -- Soulbound or bind on pickup can't reach an alt.
    c.world.bound[10003] = "Soulbound"
    eq(#c.hover(10003), 1, "soulbound")
    c.world.bound[10003] = "Binds when picked up"
    eq(#c.hover(10003), 1, "bind on pickup")
    c.world.bound[10003] = nil

    eq(file(c.logout())._meta.tooltip_errors, nil, "no tooltip errors")
end)

-- B1 (INGAME §9): the login briefing, one chat line from live facts and the
-- Briefing slot, the note on its own line, /fb brief, and the toggle.
local PREFIX = "|cffffd100Forever Buddy:|r "
-- An alt's name is in its class colour, like everywhere we name one.
local SELA = "|cffffffffSela|r"
local function briefingSlot(notes)
    return 'ForeverBuddyData_Briefing = {\n\t["schema"] = 1,\n\t["stamp"] = 1790960000,\n'
        .. '\t["mail"] = {\n'
        -- This character's own mail is never a fact; the first other alt is.
        .. '\t\t{ ["name"] = "Thrandor", ["surname"] = "Vargur", ["class"] = "PALADIN", ["letters"] = 5 },\n'
        .. '\t\t{ ["name"] = "Sela", ["surname"] = "", ["class"] = "PRIEST", ["letters"] = 2, ["expires"] = '
        .. (wow.EPOCH + DAY) .. ' },\n'
        .. '\t\t{ ["name"] = "Kaelor", ["surname"] = "", ["class"] = "ROGUE", ["letters"] = 1 },\n'
        .. '\t},\n'
        .. '\t["notes"] = {\n' .. (notes or "") .. '\t},\n}\n'
end
local NOTES = '\t\t{ ["name"] = "Sela", ["surname"] = "", ["class"] = "PRIEST", ["id"] = 8, ["text"] = "Not mine", ["once"] = true },\n'
    .. '\t\t{ ["name"] = "Thrandor", ["surname"] = "Vargur", ["class"] = "PALADIN", ["id"] = 7, '
    .. '["text"] = "Hand in the Onyxia attunement |cffff0000before|r raid", ["once"] = true },\n'

scenario("briefing", function()
    local c = client({ slots = { ["Data/Briefing.lua"] = briefingSlot(NOTES) } })
    c.world.questlog = {
        { questID = 1, header = true, done = true }, -- a zone header isn't a quest
        { questID = 176, done = true },
        { questID = 783, done = true },
        { questID = 7, done = true },
        { questID = 33 },
    }
    c.world.durability = { [1] = { 80, 100 }, [5] = { 24, 100 } }
    c.login(nil)
    eq(#c.chat, 0, "nothing before the quest log has loaded")
    c.advance(5)
    eq(table.concat(c.chat, "\n"), table.concat({
        PREFIX .. "3 quests ready to hand in · repair due (24%) · " .. SELA .. " has 2 letters waiting",
        -- The note's own colour codes are shown as text, not run.
        '|cffffd100Note:|r "Hand in the Onyxia attunement ||cffff0000before||r raid"',
    }, "\n"), "the briefing")

    -- /fb brief repeats it.
    c.chat = {}
    c.slash("/fb brief")
    eq(#c.chat, 2, "/fb brief repeats both lines")
    local text = c.logout()
    local db = file(text)
    eq(db.briefed[7], wow.EPOCH + 5, "the shown note's receipt")

    -- A relog before the app rewrote the slot: the once note isn't shown again.
    c.chat = {}
    c.login(text)
    c.advance(5)
    eq(table.concat(c.chat, "\n"), PREFIX .. "3 quests ready to hand in · repair due (24%) · " .. SELA .. " has 2 letters waiting",
        "a once note shows once")
    eq(file(c.logout()).briefed[7], wow.EPOCH + 5, "the receipt is kept until the app reads it")

    -- /reload doesn't brief again.
    c.chat = {}
    c.reload()
    c.advance(5)
    eq(#c.chat, 0, "not after /reload")

    -- The toggle is account-wide and survives logins.
    c.slash("/fb brief off")
    eq(c.chat[1], PREFIX .. "login briefing off.", "off")
    c.chat = {}
    c.login(c.logout())
    c.advance(5)
    eq(#c.chat, 0, "off: silent at login")
    eq(c.settings.briefing, false, "saved account-wide")
    c.slash("/fb brief on")
    eq(c.settings == nil or c.settings.briefing == nil, true, "on: nothing saved")

    -- Nothing to say: no line at login; /fb brief says so.
    local quiet = client({})
    quiet.login(nil)
    quiet.advance(5)
    eq(#quiet.chat, 0, "silent when there's nothing to say")
    quiet.slash("/fb brief")
    eq(quiet.chat[1], PREFIX .. "nothing to report.", "/fb brief with nothing")
    quiet.slash("/fb help")
    eq(quiet.chat[2], PREFIX .. "/fb opens the window; /fb plan, list, errands or cleanup opens that tab; "
        .. "/fb coach this session's strip; /fb card off hides the logout card; "
        .. "/fb brief repeats the login briefing; /fb brief off turns it off; /fb minimap shows or hides the button.",
        "/fb help")

    -- Not in combat.
    local fighting = client({ slots = { ["Data/Briefing.lua"] = briefingSlot(NOTES) } })
    fighting.login(nil)
    fighting.world.combat = true
    fighting.advance(5)
    eq(#fighting.chat, 0, "nothing in combat")
    return text
end)

-- G1 (INGAME §16): goals in the briefing and the coach. Thrandor Vargur is
-- level 12 with 1,200 of 8,800 XP and 2g 50s. The account wants 500g for the
-- mount (Sela holds 200g); Sela's own level goal isn't Thrandor's.
local function goalsSlot(goals)
    return 'ForeverBuddyData_Briefing = {\n\t["schema"] = 1,\n\t["stamp"] = 1790960000,\n'
        .. '\t["mail"] = {},\n\t["notes"] = {},\n\t["goals"] = {\n' .. goals .. '\t},\n}\n'
end
local LEVEL_GOAL = '\t\t{ ["id"] = 1, ["kind"] = "level", ["target"] = 14, ["name"] = "Thrandor", ["surname"] = "Vargur", ["by"] = '
    .. (wow.EPOCH + 3 * DAY) .. ' },\n'
local MOUNT_GOAL = '\t\t{ ["id"] = 2, ["kind"] = "gold", ["target"] = 5000000, ["label"] = "for the mount", ["alts"] = {\n'
    -- Thrandor's own gold at logout is replaced by what he holds now.
    .. '\t\t\t{ ["name"] = "Thrandor", ["surname"] = "Vargur", ["money"] = 99990000 },\n'
    .. '\t\t\t{ ["name"] = "Sela", ["surname"] = "", ["money"] = 2000000 },\n\t\t} },\n'
local SELA_GOAL = '\t\t{ ["id"] = 3, ["kind"] = "level", ["target"] = 40, ["name"] = "Sela", ["surname"] = "" },\n'

scenario("goals", function()
    local PREFIX = "|cffffd100Forever Buddy:|r "
    local fri = os.date("!%a", wow.EPOCH + 3 * DAY)
    local c = client({ slots = { ["Data/Briefing.lua"] = goalsSlot(MOUNT_GOAL .. SELA_GOAL .. LEVEL_GOAL) } })
    c.login(nil)
    c.advance(5)
    eq(c.chat[1], PREFIX .. "level 14 by " .. fri .. ": 1.9 to go +1 goal", "the soonest goal, and how many more")

    -- The coach: levels to go before 10 minutes, then the time at this pace.
    c.slash("/fb coach")
    local f = c.global("ForeverBuddyCoachFrame")
    local function row(label)
        for _, r in ipairs(f.rows) do
            if r.shown and r.label.text == label then
                return r.value.text
            end
        end
    end
    eq(row("Level 14 by " .. fri), "1.9 levels to go", "no pace yet")
    c.gainXp(1000)
    c.advance(60 * MINUTE)
    -- 1.75 levels of 8,800 at 1,000 XP an hour is over 10 hours: levels.
    eq(row("Level 14 by " .. fri), "1.8 levels to go", "no time past 10 hours")
    c.gainXp(4000)
    c.advance(5)
    -- 11,400 XP left at 5,000 XP in the session's 60m 10s.
    eq(row("Level 14 by " .. fri), "~2h 17m of play", "the time at this pace")

    -- Account gold: Sela's 200g at her logout plus Thrandor's live 2g 50s.
    local gold = client({ slots = { ["Data/Briefing.lua"] = goalsSlot(MOUNT_GOAL .. SELA_GOAL) } })
    gold.login(nil)
    gold.advance(5)
    eq(gold.chat[1], PREFIX .. "500g for the mount: 298g to go", "the label is the goal's own name")
    gold.slash("/fb coach")
    local g = gold.global("ForeverBuddyCoachFrame")
    local found
    for _, r in ipairs(g.rows) do
        if r.shown and r.label.text == "500g goal" then
            found = r.value.text
        end
    end
    eq(found, "298g to go", "the gold row")

    -- Reached since the last login: said once (the app drops it after the
    -- next logout); labels are shown as text.
    local done = client({ slots = { ["Data/Briefing.lua"] = goalsSlot(
        '\t\t{ ["id"] = 4, ["kind"] = "level", ["target"] = 12, ["name"] = "Thrandor", ["surname"] = "Vargur", ["done"] = true },\n') } })
    done.login(nil)
    done.advance(5)
    eq(done.chat[1], PREFIX .. "level 12 done", "done")
    local piped = client({ slots = { ["Data/Briefing.lua"] = goalsSlot(
        '\t\t{ ["id"] = 5, ["kind"] = "gold", ["target"] = 5000000, ["label"] = "|cffff0000x|r" },\n') } })
    piped.login(nil)
    piped.advance(5)
    eq(piped.chat[1], PREFIX .. "500g ||cffff0000x||r: 498g to go", "escaped")

    -- Another character's goal alone says nothing.
    local other = client({ slots = { ["Data/Briefing.lua"] = goalsSlot(SELA_GOAL) } })
    other.login(nil)
    other.advance(5)
    eq(#other.chat, 0, "not Thrandor's")
end)

-- Shopping lists and alt errands (B2, INGAME §10). Thrandor (alt 1) has 4
-- Linen Cloth in bags and 20 Runecloth in the bank; Sela (2) gathers for
-- Tailoring; Kaelor (3) holds the raid's jerky.
local LISTS = 'ForeverBuddyData_Lists = {\n\t["schema"] = 1,\n\t["stamp"] = 1790960000,\n'
    .. '\t["alts"] = {\n'
    .. '\t\t{ ["name"] = "Thrandor", ["surname"] = "Vargur", ["class"] = "WARRIOR", ["seen"] = ' .. wow.EPOCH .. ' },\n'
    .. '\t\t{ ["name"] = "Sela", ["surname"] = "", ["class"] = "PRIEST", ["seen"] = ' .. (wow.EPOCH - 10 * DAY) .. ' },\n'
    .. '\t\t{ ["name"] = "Kaelor", ["surname"] = "", ["class"] = "ROGUE", ["seen"] = ' .. wow.EPOCH .. ' },\n'
    .. '\t},\n'
    .. '\t["lists"] = {\n'
    .. '\t\t{ ["id"] = 1, ["name"] = "Tailoring", ["for"] = 2, ["items"] = {\n'
    .. '\t\t\t{ ["id"] = 14047, ["name"] = "Runecloth", ["need"] = 30, ["price"] = 11200,'
    .. ' ["held"] = { 1, 0, 20, 0, ' .. (wow.EPOCH - DAY) .. ', 2, 0, 4, 0, ' .. (wow.EPOCH - 10 * DAY) .. ' },'
    .. ' ["errands"] = { 1, 20, 0 } },\n'
    .. '\t\t\t{ ["id"] = 2589, ["name"] = "Linen Cloth", ["need"] = 6,'
    .. ' ["held"] = { 1, 4, 0, 0, ' .. wow.EPOCH .. ' }, ["errands"] = { 1, 4, 4 } },\n'
    .. '\t\t\t{ ["name"] = "Mooncloth", ["need"] = 2, ["held"] = {} },\n'
    .. '\t\t} },\n'
    .. '\t\t{ ["id"] = 2, ["name"] = "Raid night ]] |cffff0000x", ["items"] = {\n'
    .. '\t\t\t{ ["id"] = 117, ["name"] = "Tough Jerky", ["need"] = 20, ["price"] = 50,'
    .. ' ["held"] = { 3, 25, 0, 0, ' .. wow.EPOCH .. ' } },\n'
    .. '\t\t} },\n'
    .. '\t},\n}\n'

scenario("lists", function()
    local PREFIX = "|cffffd100Forever Buddy:|r "
    local SELA = "|cffffffffSela|r"
    local c = client({ slots = { ["Data/Lists.lua"] = LISTS } })
    c.login(nil)
    c.advance(5)
    eq(c.chat[1], PREFIX .. "2 errands at the mailbox", "the briefing counts this character's errands")
    c.chat = {}

    -- A vendor that sells the jerky: that list docks beside it, the other
    -- folds away, and the jerky's button is marked.
    c.openMerchant({ 117, 6948 })
    local f = c.global("ForeverBuddyListFrame")
    eq(f.shown, true, "docks at a vendor")
    eq(f.point[2], c.global("MerchantFrame"), "beside the merchant")
    eq(f.heading.text, "Your list", "heading")
    eq(f.meta.text, "Raid night ]] ||cffff0000x", "list names are escaped")
    eq(f.rows[1].label.text, "Tough Jerky |cffffd100· here|r", "here")
    eq(f.rows[1].right.text, "done", "Kaelor's 25 cover it")
    eq(f.rows[2] and f.rows[2].shown, nil, "one row")
    eq(f.footer.text:match("^%+1 list · From Forever Buddy"), "+1 list · From Forever Buddy", "the rest fold")
    -- The mark is the glow and the tag the addon adds to the button.
    local button = c.global("MerchantItem1ItemButton")
    eq(button.children[1].shown, true, "the vendor's button is marked")
    eq(button.children[2].text, "list", "with a tag")
    eq(#c.global("MerchantItem2ItemButton").children, 0, "not the hearthstone")
    c.closeMerchant()
    eq(f.shown, false, "closes with the vendor")

    -- One that sells linen: Sela's list, with where everyone stands.
    c.openMerchant({ 2589 })
    eq(f.meta.text, "Tailoring", "Sela's list")
    eq(f.rows[1].label.text, "Runecloth", "not here")
    eq(f.rows[1].right.text, "need 26", "Sela's own 4 count")
    eq(f.rows[1].detail.text, SELA .. " has 4 in bank, still 26 short · ~1g 12s at last scan", "where Sela stands")
    eq(f.rows[2].label.text, "Linen Cloth |cffffd100· here|r", "here")
    eq(f.rows[2].detail.text, "you have 4 in bags", "live counts for this character")
    eq(f.rows[3].detail.text, "your alts have 0", "a free-text item")
    c.closeMerchant()

    -- The AH: browse results count as here; listings under the last scan
    -- are counted, never called a buy.
    c.openAuctionHouse()
    eq(f.shown, true, "docks at the AH")
    eq(f.point[2], c.global("AuctionHouseFrame"), "beside the AH")
    c.browseAuctions({ 14047 })
    eq(f.meta.text, "Tailoring", "the list with something in the results")
    eq(f.rows[1].label.text, "Runecloth |cffffd100· here|r", "in the results")
    c.searchAuctions(14047, { 9000, 10000, 12000 })
    eq(f.rows[1].detail.text, SELA .. " has 4 in bank, still 26 short · first two rows are under it", "under the scan")
    c.closeAuctionHouse()
    eq(f.shown, false, "closes with the AH")

    -- U1: /fb list opens the window's Lists tab, every list. The panel
    -- above only docks.
    c.slash("/fb list")
    local w = c.global("ForeverBuddyWindow")
    local lists = w.panes[2]
    eq(w.shown, true, "/fb list opens the window")
    eq(lists.scroll.shown, true, "on the Lists tab")
    eq(f.shown, false, "not the docking panel")
    eq(lists.meta.text, "Tailoring · Raid night ]] ||cffff0000x", "every list")
    eq(lists.rows[1].label.text, "Runecloth", "the same rows")
    eq(lists.footer.text, "", "the window's footer says where it's from")
    -- At a vendor, "· here" shows in the tab too.
    c.openMerchant({ 2589 })
    eq(lists.rows[2].label.text, "Linen Cloth |cffffd100· here|r", "here")
    c.closeMerchant()

    -- The Errands tab: the same rows, and Fill recipient waits for a mailbox.
    c.slash("/fb errands")
    local tab = w.panes[3]
    eq(tab.scroll.shown, true, "on the Errands tab")
    eq(lists.scroll.shown, false, "one tab at a time")
    eq(tab.rows[2].label.text, "Linen Cloth ×4 to " .. SELA, "errand")
    eq(tab.rows[2].fill.enabled, false, "not at a mailbox")
    eq(tab.footer.text, "Fill recipient works at a mailbox. Nothing is attached or sent for you.", "says why")
    c.click(tab.rows[2].fill)
    eq(c.global("SendMailNameEditBox"):GetText(), nil, "types nothing away from the mailbox")

    -- The mailbox: Sela's errands. Linen is in the bags; the Runecloth is in
    -- the bank, so its button waits.
    c.openMailbox()
    local e = c.global("ForeverBuddyErrandFrame")
    eq(e.shown, true, "errands at the mailbox")
    eq(e.point[2], c.global("MailFrame"), "beside the mailbox")
    eq(e.meta.text, "from |cffc79c6eThrandor|r", "from this character")
    eq(e.rows[1].label.text, "Runecloth ×20 to " .. SELA, "errand")
    eq(e.rows[1].detail.text, "20 in your bank · visit the bank first", "in the bank")
    eq(e.rows[1].fill.enabled, false, "waits for the bank")
    eq(e.rows[2].label.text, "Linen Cloth ×4 to " .. SELA, "errand")
    eq(e.rows[2].detail.text, "you have 4 in bags · Sela's Tailoring list", "by name, never a pronoun")
    eq(e.rows[2].fill.enabled, true, "ready")
    eq(e.rows[2].fill.tip, "Types \"Sela\" in the To field. Attach the Linen Cloth yourself, then press Send.", "tip")
    eq(e.footer.text, "Nothing is attached or sent for you.", "footer")
    local box = c.global("SendMailNameEditBox")
    c.click(e.rows[1].fill)
    eq(box:GetText(), nil, "a waiting errand types nothing")
    c.click(e.rows[2].fill)
    eq(box:GetText(), "Sela", "only the To field")
    -- Fetched from the bank: the bags update and the button is ready.
    c.bank(nil, { [14047] = 20 })
    eq(e.rows[1].detail.text, "you have 20 in bags · Sela's Tailoring list", "live")
    eq(e.rows[1].fill.enabled, true, "ready now")
    eq(tab.rows[1].fill.enabled, true, "the open Errands tab too, at the mailbox")
    eq(tab.footer.text, "Nothing is attached or sent for you.", "and its footer")
    c.closeMailbox()
    eq(e.shown, false, "closes with the mailbox")
    eq(tab.rows[1].fill.enabled, false, "the tab waits again")
    local text = c.logout()
    eq(file(text).bridge.Lists.stamp, 1790960000, "receipt")

    -- On Sela, the errand reads as what's coming, and the mailbox has none.
    local sela = client({
        slots = { ["Data/Lists.lua"] = LISTS },
        character = { name = "Sela", surname = "", realm = "Classic Beta PvP 2", guid = "Player-2" },
    })
    sela.login(nil)
    sela.advance(5)
    eq(#sela.chat, 0, "no errands, no briefing")
    sela.slash("/fb list")
    local g = sela.global("ForeverBuddyWindow").panes[2]
    eq(g.rows[1].right.text, "Thrandor can send 20", "coming from Thrandor")
    sela.openMailbox()
    eq(sela.global("ForeverBuddyErrandFrame"), nil, "no errands panel")
    sela.slash("/fb errands")
    eq(sela.global("ForeverBuddyWindow").panes[3].footer.text, "No errands for Sela.", "says so")

    -- Without the slot: a vendor shows nothing, /fb list says there are none.
    local none = client()
    none.login(nil)
    none.openMerchant({ 117 })
    eq(none.global("ForeverBuddyListFrame"), nil, "no panel without lists")
    none.slash("/fb list")
    eq(none.global("ForeverBuddyWindow").panes[2].footer.text, "No lists yet. Make one in Forever Buddy.", "empty")
    return text
end)

-- Can make (C1, INGAME §12): other characters whose recipes make the
-- hovered item, from the index's `makes` and each alt's `prof`. Kaelor's
-- recipes were read ten days ago, so they're grey and dated.
local function prof(skill, at)
    return ', ["prof"] = { ["Tailoring"] = { ["skill"] = ' .. skill .. ', ["at"] = ' .. at .. ' } }'
end
local CRAFTERS = tooltipSlot("Tooltip1", '\t["alts"] = {\n'
    .. alt("Thrandor", "PALADIN", 60, {}, prof(300, wow.EPOCH), "Vargur")
    .. alt("Sela", "PRIEST", 40, {}, prof(285, wow.EPOCH - DAY))
    .. alt("Kaelor", "ROGUE", 30, {}, prof(150, wow.EPOCH - 10 * DAY))
    .. alt("Velyra", "DRUID", 60, {}, prof(300, wow.EPOCH))
    .. alt("Brannic", "HUNTER", 52, {}, prof(300, wow.EPOCH))
    .. '\t},\n\t["items"] = {},\n\t["makes"] = {\n'
    .. '\t\t[2568] = { 1, "Tailoring", 2, "Tailoring", 3, "Tailoring", 4, "Tailoring", 5, "Tailoring" },\n'
    .. '\t\t[2572] = { 2, "Tailoring" },\n'
    .. '\t\t[2574] = { 2, "Tailoring", 3, "Tailoring" },\n'
    .. '\t\t[2576] = { 2, "Tailoring", 3, "Tailoring", 4, "Tailoring" },\n'
    .. '\t\t[2578] = { 1, "Tailoring" },\n'
    .. '\t},\n')

-- C2 (INGAME §12 (c)): what one craft of 2572 takes, 1 Coarse Thread (2320,
-- Sela's bank), 2 Linen Cloth (2589, live in this character's bags) and 1
-- Silk (4306, nobody), split over both halves by item parity.
local MATS_ALTS = '\t["alts"] = {\n'
    .. alt("Thrandor", "PALADIN", 60, {}, prof(300, wow.EPOCH), "Vargur")
    .. alt("Sela", "PRIEST", 40, {}, prof(285, wow.EPOCH))
    .. '\t},\n'
local MATS_EVEN = tooltipSlot("Tooltip1", MATS_ALTS
    .. '\t["items"] = { [2320] = { 0, 2, 0, 1, 0, 0 } },\n'
    .. '\t["makes"] = { [2572] = { 2, "Tailoring" }, [2574] = { 2, "Tailoring" } },\n'
    .. '\t["mats"] = { [2572] = { 2320, 1, 2589, 2, 4306, 1 }, [2574] = { 2320, 1 } },\n')
local MATS_ODD = tooltipSlot("Tooltip2", MATS_ALTS
    .. '\t["items"] = { [2589] = { 0, 2, 1, 0, 0, 0 } },\n')

scenario("materials", function()
    local c = client({ slots = { ["Data/Tooltip1.lua"] = MATS_EVEN, ["Data/Tooltip2.lua"] = MATS_ODD } })
    c.login(nil)
    local SELA = "|cffffffffSela|r"
    eq(c.hover(2572)[3], SELA .. " can make this · materials 2 of 3", "compact: 2 of 3")
    eq(c.hover(2574)[3], SELA .. " can make this · |cff40ff40all materials on hand|r", "compact: all")
    c.world.shift = true
    local lines = c.hover(2572)
    local from = 0
    for i, l in ipairs(lines) do
        if l == "Materials for one" then
            from = i
        end
    end
    eq(table.concat(lines, "\n", from, from + 3), table.concat({
        "Materials for one",
        "Item 2320 | 1 of 1 · Sela bank",
        "Linen Cloth | 2 of 2 · on you",
        "Item 4306 | 0 of 1",
    }, "\n"), "Shift rows")
    c.world.shift = false
end)

scenario("crafting", function()
    local c = client({ slots = { ["Data/Tooltip1.lua"] = CRAFTERS } })
    c.login(nil)
    -- Kaelor's recipes are ten days old: grey in the compact line.
    local SELA, KAELOR = "|cffffffffSela|r", G .. "Kaelor|r"
    local function compact(id)
        return c.hover(id)[3]
    end
    eq(compact(2572), SELA .. " can make this", "one")
    eq(compact(2574), SELA .. " and " .. KAELOR .. " can make this", "two")
    eq(compact(2576), SELA .. ", " .. KAELOR .. " and Velyra can make this", "three")
    -- Never this character; past three, "+N".
    eq(compact(2568), SELA .. ", " .. KAELOR .. ", Velyra +1 can make this", "past three")
    eq(c.hover(2568)[4], "Shift for details", "Shift has more")
    eq(#c.hover(2578), 1, "nothing when only this character can make it")

    c.world.shift = true
    eq(table.concat(c.hover(2574), "\n"), table.concat({
        "Item 2574",
        " ",
        "Forever Buddy",
        "Sela | Tailoring 285",
        "Kaelor | " .. G .. "Tailoring 150 · as of " .. day(wow.EPOCH - 10 * DAY) .. "|r",
        "As of each alt's last logout",
    }, "\n"), "Shift")
    c.world.shift = false

    -- An index from before C1 has no `makes`: no line.
    local old = client({ slots = { ["Data/Tooltip1.lua"] = tooltipSlot("Tooltip1", '\t["alts"] = {},\n\t["items"] = {},\n') } })
    old.login(nil)
    eq(#old.hover(2572), 1, "no makes, no line")
end)

-- Lockouts at the entrance (L2, INGAME §13): the Briefing slot's lockouts.
-- Thrandor (this character) is saved to Molten Core too and is never
-- named; Sela's save has already reset.
local function saves(name, class, list, surname)
    return '\t\t{ ["name"] = "' .. name .. '", ["surname"] = "' .. (surname or "") .. '", ["class"] = "' .. class
        .. '", ["saves"] = { ' .. list .. ' } },\n'
end
local WEEK = wow.EPOCH + 3 * DAY
local ENTRANCE = 'ForeverBuddyData_Briefing = {\n\t["schema"] = 1,\n\t["stamp"] = 1790960000,\n'
    .. '\t["mail"] = {},\n\t["notes"] = {},\n\t["lockouts"] = {\n'
    .. saves("Thrandor", "PALADIN", '"Molten Core", "40 Player", ' .. WEEK, "Vargur")
    .. saves("Velyra", "DRUID", '"Molten Core", "40 Player", ' .. WEEK
        .. ', "Scarlet Monastery", "Heroic", ' .. (wow.EPOCH + 5 * HOUR))
    .. saves("Kaelor", "ROGUE", '"Molten Core", "40 Player", ' .. WEEK)
    .. saves("Sela", "PRIEST", '"Molten Core", "40 Player", ' .. (wow.EPOCH - 1))
    .. saves("Brannic", "HUNTER", '"Onyxia\'s Lair", "40 Player", ' .. WEEK)
    .. '\t},\n}\n'

scenario("entrance", function()
    local PREFIX = "|cffffd100Forever Buddy:|r "
    local KAELOR = "|cfffff569Kaelor|r"
    local tue = os.date("!%a", WEEK)
    local c = client({ slots = { ["Data/Briefing.lua"] = ENTRANCE } })
    c.login(nil)
    c.chat = {}
    c.enterInstance("Molten Core", "raid", 9, "40 Player")
    eq(table.concat(c.chat, "\n"), PREFIX .. "Velyra and " .. KAELOR .. " are saved to Molten Core (resets " .. tue .. ").",
        "two alts, never this character, not an expired save")
    -- Once per instance per session: out and back in (a ghost run) is quiet.
    c.leaveInstance("Searing Gorge")
    c.enterInstance("Molten Core", "raid", 9, "40 Player")
    eq(#c.chat, 1, "said once")

    -- A difficulty that isn't normal is named; under a day, the hours.
    c.enterInstance("Scarlet Monastery", "party", 2, "Heroic")
    eq(c.chat[2], PREFIX .. "Velyra is saved to Scarlet Monastery (Heroic) (resets in 5 h).", "heroic, hours")
    -- Nobody saved here: nothing.
    c.enterInstance("Deadmines", "party", 1, "Normal")
    eq(#c.chat, 2, "nothing to say")

    -- In combat it waits for combat to end.
    c.world.combat = true
    c.enterInstance("Onyxia's Lair", "raid", 9, "40 Player")
    eq(#c.chat, 2, "not in combat")
    c.combat(false)
    eq(c.chat[3], PREFIX .. "Brannic is saved to Onyxia's Lair (resets " .. tue .. ").", "after combat")

    -- After /reload inside, silent, and still silent on the way back in.
    local fresh = client({ slots = { ["Data/Briefing.lua"] = ENTRANCE } })
    fresh.login(nil)
    fresh.world.zone, fresh.world.instance = "Molten Core", true
    fresh.world.dungeon = { name = "Molten Core", kind = "raid", difficulty = 9, difficultyName = "40 Player" }
    fresh.chat = {}
    fresh.reload()
    fresh.leaveInstance("Searing Gorge")
    fresh.enterInstance("Molten Core", "raid", 9, "40 Player")
    eq(#fresh.chat, 0, "/reload inside is silent, and so is the way back in")

    -- The off switch, saved per account.
    local off = client({ slots = { ["Data/Briefing.lua"] = ENTRANCE } })
    off.settings = { lockouts = false }
    off.login(nil)
    off.chat = {}
    off.enterInstance("Molten Core", "raid", 9, "40 Player")
    eq(#off.chat, 0, "off")
end)

-- Bag cleanup (B3, INGAME §14): Thrandor's Linen Cloth is marked to sell;
-- his Hearthstone (soulbound) and a Felcloth Hood are marked to send to
-- Sela. Tags in the bags, tooltip lines, the vendor line, the errands.
local CLEANUP = 'ForeverBuddyData_Cleanup = {\n\t["schema"] = 1,\n\t["stamp"] = 1790960000,\n'
    .. '\t["alts"] = {\n'
    .. '\t\t{ ["name"] = "Thrandor", ["surname"] = "Vargur", ["class"] = "WARRIOR" },\n'
    .. '\t\t{ ["name"] = "Sela", ["surname"] = "", ["class"] = "PRIEST" },\n'
    .. '\t},\n\t["marks"] = {\n'
    -- Five per mark: item, action, recipient, reason code (B3b), gain.
    .. '\t\t[1] = { 2589, "sell", 0, "grey", 0, 6948, "send", 2, "", 0, 10005, "send", 2, "upgrade", 9 },\n'
    .. '\t},\n}\n'

-- P4d: the Bags module, display only. Off until turned on (designer: no
-- surprise on an update). Then junk gets a coin, and gear its item level in
-- its quality's colour at the top left (beside a bag mark if there is one),
-- in the bags and in the bank. Each option off takes its part away, and an
-- item the client keeps secret gets nothing.
local function extrasOf(button)
    local out = {}
    for _, ch in ipairs(button.children) do
        if ch.shown then
            if ch.texture == "Interface\\Buttons\\UI-GroupLoot-Coin-Up" then
                out.junk = true
            elseif ch.text then
                out.level, out.color, out.x = tostring(ch.text), ch.color, ch.points[#ch.points][4]
            end
        end
    end
    return out
end

scenario("bags", function()
    local c = client({ slots = { ["Data/Cleanup.lua"] = CLEANUP } })
    c.world.bags[0].slots[3] = { id = 4865, count = 2 }
    c.world.bags[0].slots[4] = { id = 10001, count = 1 }
    c.world.bags[0].slots[5] = { id = 10002, count = 1, unreadable = true }
    c.world.bags[0].slots[6] = { id = 10005, count = 1 } -- marked to send
    c.world.bags[6].slots[1] = { id = 10010, count = 1 }
    c.world.bags[6].slots[2] = { id = 4865, count = 1 }
    c.login(nil)
    c.advance(5)
    c.openBags()
    c.advance(1)
    local bag = c.global("ContainerFrame1")
    local function at(slot)
        return extrasOf(bag.buttons[slot])
    end
    eq(at(3).junk or at(4).level, nil, "off by default: nothing drawn")
    c.click(setting(c, "Junk in your bags"))
    c.click(setting(c, "Item level on gear"))
    eq(at(3).junk, true, "a coin on the grey pelt")
    eq(at(3).level, nil, "no item level on junk")
    eq(at(4).level, "63", "the cap's item level")
    eq(table.concat(at(4).color, ","), "0,0.44,0.87", "in its quality's colour")
    eq(at(4).x, 2, "at the top left")
    eq(at(6).level, "40", "the marked hood's")
    eq(at(6).x, 14, "moved along beside its mark")
    eq(at(4).junk, nil, "no coin on gear")
    eq(next(at(2)), nil, "linen: nothing")
    eq(next(at(5)), nil, "a secret item: nothing")

    c.openBank()
    c.advance(1)
    local bank = c.global("BankPanel")
    eq(extrasOf(bank.buttons[1]).level, "63", "gear in the bank")
    eq(extrasOf(bank.buttons[2]).junk, true, "junk in the bank")
    -- Another tab: the game reuses the buttons for its items, with no bag
    -- update, and they're done again.
    c.world.bags[6].slots[1] = { id = 10020, count = 1 }
    bank:RefreshAllItemsForSelectedTab()
    c.advance(1)
    eq(extrasOf(bank.buttons[1]).level, "50", "after a tab switch")

    eq(c.global("ForeverBuddySettings").bagJunk, true, "saved")
    c.click(setting(c, "Junk in your bags"))
    eq(at(3).junk, nil, "junk off: no coin")
    eq(extrasOf(bank.buttons[2]).junk, nil, "nor in the bank")
    c.click(setting(c, "Item level on gear"))
    eq(at(4).level, nil, "item level off: none")
    eq(c.global("ForeverBuddySettings").bagJunk, nil, "off is the default again")
    c.closeBank()
    file(c.logout())
end)

-- M1: the Merchant module, the first that acts. Off by default; at a vendor
-- the player opened it repairs (guild bank first when allowed, else own gold)
-- and sells up to 12 grey items, and marked ones when that's on. It leaves
-- quest items, unreadable items and ones whose binding it can't read; does
-- nothing in combat; and never uses an item with no vendor open.
scenario("merchant", function()
    local PREFIX = "|cffffd100Forever Buddy:|r "
    local c = client({ slots = { ["Data/Cleanup.lua"] = CLEANUP } })
    local bags = c.world.bags
    bags[1] = { size = 16, slots = {} }
    -- Fifteen grey stacks (one a quest's, one of unknown binding, one
    -- unreadable), the marked linen, and gear.
    for slot = 1, 15 do
        bags[1].slots[slot] = { id = 4865, count = 1 }
    end
    bags[1].slots[1].bindUnknown = true
    bags[1].slots[2].unreadable = true
    c.world.quest_items[4865] = nil
    bags[0].slots[3] = { id = 10001, count = 1 }
    c.world.repair = 2500
    c.login(nil)
    c.advance(5)
    local function lastSay()
        return c.chat[#c.chat]
    end

    -- Off: a vendor, junk and damage, and nothing happens.
    local chatBefore = #c.chat
    c.openMerchant({ 117 })
    c.advance(2)
    eq(#c.world.sold, 0, "off: nothing sold")
    eq(c.world.repaired, nil, "off: nothing repaired")
    eq(#c.chat, chatBefore, "off: nothing said")
    c.closeMerchant()

    -- Repair, own gold.
    c.click(setting(c, "Repair at vendors"))
    local money = c.world.money
    c.openMerchant({ 117 })
    c.advance(2)
    eq(c.world.repaired and c.world.repaired.guild, false, "repaired from own gold")
    eq(c.world.money, money - 2500, "paid")
    eq(lastSay():find("repaired for", 1, true) ~= nil, true, "and says so: " .. tostring(lastSay()))
    c.closeMerchant()

    -- Guild first, when the rank allows and the funds cover it.
    c.click(setting(c, "Guild bank pays first"))
    c.world.repair, c.world.repaired = 900, nil
    c.world.guild_repair, c.world.guild_funds = true, 5000
    money = c.world.money
    c.openMerchant({ 117 })
    c.advance(2)
    eq(c.world.repaired and c.world.repaired.guild, true, "the guild bank paid")
    eq(c.world.money, money, "not our gold")
    eq(lastSay():find("from the guild bank", 1, true) ~= nil, true, "and says so")
    c.closeMerchant()
    -- Not enough in the guild's allowance: own gold instead.
    c.world.repair, c.world.repaired, c.world.guild_funds = 900, nil, 100
    c.openMerchant({ 117 })
    c.advance(2)
    eq(c.world.repaired and c.world.repaired.guild, false, "own gold when the guild can't cover it")
    c.closeMerchant()
    -- Not enough gold at all: nothing, and a line saying why.
    c.world.repair, c.world.repaired, c.world.guild_repair = 9e9, nil, false
    c.openMerchant({ 117 })
    c.advance(2)
    eq(c.world.repaired, nil, "can't afford it: no repair")
    eq(lastSay():find("not enough gold to repair", 1, true) ~= nil, true, "says why")
    c.closeMerchant()
    c.world.repair = 0

    -- Sell grey: at most 12, never the unreadable or unknown-binding ones,
    -- nor the quest's.
    c.click(setting(c, "Sell grey items"))
    c.openMerchant({ 117 })
    c.advance(2)
    eq(#c.world.sold, 12, "twelve sold, the cap")
    local soldSlots = {}
    for _, s in ipairs(c.world.sold) do
        eq(s.id, 4865, "only grey items")
        soldSlots[s.bag .. ":" .. s.slot] = true
    end
    eq(soldSlots["1:1"], nil, "not the one whose binding is unknown")
    eq(soldSlots["1:2"], nil, "not the unreadable one")
    eq(bags[0].slots[2] ~= nil and bags[0].slots[3] ~= nil, true, "the linen and the cap stay")
    eq(lastSay():find("sold 12 items for", 1, true) ~= nil, true, "says what it sold: " .. tostring(lastSay()))
    c.closeMerchant()
    -- Closed: bag changes sell nothing.
    local sold = #c.world.sold
    c.loot(4865, 2)
    c.advance(2)
    eq(#c.world.sold, sold, "nothing sells with the vendor closed")

    -- A quest's grey item stays.
    c.world.quest_items[4865] = 176
    c.openMerchant({ 117 })
    c.advance(2)
    eq(#c.world.sold, sold, "a quest's items stay")
    c.closeMerchant()
    c.world.quest_items[4865] = nil

    -- Marked items, only with their own option on.
    c.click(setting(c, "Sell grey items"))
    c.openMerchant({ 117 })
    c.advance(2)
    eq(#c.world.sold, sold, "marked linen stays while that option is off")
    c.closeMerchant()
    c.click(setting(c, "Sell items you marked"))
    c.openMerchant({ 117 })
    c.advance(2)
    eq(c.world.sold[#c.world.sold].id, 2589, "the marked linen sold")
    c.closeMerchant()

    -- In combat, nothing.
    c.click(setting(c, "Sell grey items"))
    sold = #c.world.sold
    c.world.combat = true
    c.openMerchant({ 117 })
    c.advance(2)
    eq(#c.world.sold, sold, "nothing in combat")
    c.world.combat = false
    c.closeMerchant()

    eq(#c.world.misused, 0, "nothing used with no vendor open")
    file(c.logout())
end)

scenario("cleanup", function()
    local PREFIX = "|cffffd100Forever Buddy:|r "
    local SELA = "|cffffffffSela|r"
    local c = client({ slots = { ["Data/Cleanup.lua"] = CLEANUP } })
    c.world.bound[6948] = "Soulbound"
    c.login(nil)
    c.give(10005, 1)
    -- U1: /fb cleanup opens the window's Cleanup tab.
    c.slash("/fb cleanup")
    local tab = c.global("ForeverBuddyWindow").panes[4]
    eq(tab.scroll.shown, true, "on the Cleanup tab")
    eq(tab.top.text, "4 to sell · ~52c at a vendor · 2 to send", "the totals line")
    eq(tab.rows[1].label.text, "Felcloth Hood", "by name")
    eq(tab.rows[1].right.text, "→ " .. SELA, "a send")
    eq(tab.rows[1].detail.text, "+9 item level", "its reason")
    eq(tab.rows[2].label.text, "Hearthstone", "no reason")
    eq(tab.rows[2].detail.text, "", "none shown")
    eq(tab.rows[3].label.text, "Linen Cloth ×4", "a sell")
    eq(tab.rows[3].right.text, "sell", "sell")
    eq(tab.rows[3].detail.text, "grey", "grey")
    eq(tab.footer.text, "Mark items in the Forever Buddy app.", "where marks are made")

    -- Tags in the backpack: Hearthstone (slot 1) a letter, Linen (slot 2) a
    -- coin; our own child texture, nothing else on the button.
    local bag = c.global("ContainerFrame1")
    c.give(117, 1) -- a bag update tags them
    local tagOf = function(slot)
        local t = bag.buttons[slot].children[1]
        return t and t.shown and t.texture or nil
    end
    eq(tagOf(1), "Interface\\Minimap\\Tracking\\Mailbox", "send: a letter")
    eq(tagOf(2), "Interface\\MoneyFrame\\UI-GoldIcon", "sell: a coin")
    eq(tagOf(4), nil, "the jerky isn't marked")
    -- U1: "Marks in your bags" off takes the tags away; on brings them back.
    local marks = setting(c, "Marks in your bags")
    c.click(marks)
    eq(tagOf(1), nil, "off: no tags")
    c.click(marks)
    eq(tagOf(1), "Interface\\Minimap\\Tracking\\Mailbox", "on again")

    -- Tooltips.
    eq(c.hover(2589)[2], "Marked to sell in Forever Buddy" .. G .. " · grey|r|cffffffff · 13c each at a vendor|r",
        "sell line, with its reason")
    eq(c.hover(10005)[2], "Marked to send to " .. SELA .. G .. " (+9 item level)|r", "send line, with the gain")
    eq(c.hover(6948)[2], "Marked to send to " .. SELA .. G .. ", but it's soulbound|r", "soulbound")
    eq(#c.hover(14047), 1, "not marked, nothing")

    -- At a vendor: one grey line, no button.
    c.openMerchant({})
    local v = c.global("ForeverBuddyCleanupFrame")
    eq(v.shown, true, "at the vendor")
    eq(v.point[2], c.global("MerchantFrame"), "on its own, no list")
    eq(v.text.text, "4 marked to sell · ~52c at the vendor", "the line")
    c.sell(2589, 4, 52)
    eq(v.shown, false, "sold: nothing left to say")
    eq(tagOf(2), nil, "and the tag goes right away")
    c.closeMerchant()

    -- At the mailbox, marked sends are errands.
    c.openMailbox()
    local e = c.global("ForeverBuddyErrandFrame")
    eq(e.shown, true, "errands")
    eq(e.meta.text, "from |cffc79c6eThrandor|r", "from this character")
    eq(e.rows[2].label.text, "Felcloth Hood to " .. SELA, "a marked send")
    eq(e.rows[2].detail.text, "in your bags · marked in Forever Buddy", "why")
    eq(e.rows[2].fill.enabled, true, "Fill recipient")
    c.click(e.rows[2].fill)
    eq(c.global("SendMailNameEditBox"):GetText(), "Sela", "only the name")
    c.closeMailbox()

    -- Another character has nothing marked.
    local sela = client({
        slots = { ["Data/Cleanup.lua"] = CLEANUP },
        character = { name = "Sela", surname = "", realm = "Classic Beta PvP 2", guid = "Player-2" },
    })
    sela.login(nil)
    sela.slash("/fb cleanup")
    eq(sela.global("ForeverBuddyWindow").panes[4].footer.text, "Nothing marked on Sela.", "nothing")
    eq(#sela.hover(2589), 1, "no lines for someone else's marks")
end)

-- Known recipes (C1): read from this character's own profession window,
-- learned recipes only, item ids and skill; carried forward until the next
-- look, and dropped with the profession.
local TAILORING = {
    name = "Tailoring",
    skill = 34,
    max = 75,
    recipes = {
        -- C2: required reagents (2 Linen Cloth, 1 Coarse Thread); an optional
        -- reagent (type 0) isn't recorded.
        { id = 2387, learned = true, out = 2568, mats = { { 2589, 2 }, { 2320, 1 }, { 6260, 1, 0 } } },
        { id = 2389, learned = true, out = 2572 },
        { id = 2390, learned = false, out = 2575 }, -- not learned: not recorded
        { id = 2391, learned = true, out = 2568 }, -- a second recipe for the same item
    },
}

scenario("recipes", function()
    local c = client()
    c.login(nil)
    c.openProfession(TAILORING)
    c.closeProfession()
    -- Someone else's window, and one opened in combat, record nothing.
    c.openProfession({ name = "Blacksmithing", skill = 300, max = 300, linked = true,
        recipes = { { id = 9999, learned = true, out = 12345 } } })
    c.closeProfession()
    c.world.combat = true
    c.openProfession({ name = "Cooking", skill = 29, max = 75, recipes = { { id = 2538, learned = true, out = 2679 } } })
    c.closeProfession()
    c.world.combat = false
    local text = c.logout()
    local r = file(text).snapshot.recipes
    eq(entries(r), 1, "only Tailoring")
    eq(table.concat(r.Tailoring.made, ","), "2568,2572", "learned, sorted, once each")
    eq(r.Tailoring.skill, 34, "skill")
    eq(r.Tailoring.max, 75, "max")
    eq(r.Tailoring.at, c.now, "when")
    eq(table.concat(r.Tailoring.mats[2568], ","), "2589,2,2320,1", "required reagents only")
    eq(r.Tailoring.mats[2572], nil, "no reagents listed, none recorded")

    -- Not opened this session: the last scan carries forward.
    local again = client()
    again.login(text)
    eq(table.concat(file(again.logout()).snapshot.recipes.Tailoring.made, ","), "2568,2572", "carried forward")

    eq(file(text).snapshot.recipe_probe, nil, "no recipe item seen, no probe")

    -- The C1b probe: a pattern looted this session gets its spell and line
    -- types noted, numbers only.
    local probe = client()
    probe.login(nil)
    probe.loot(4355, 1)
    probe.advance(5)
    local p = file(probe.logout()).snapshot.recipe_probe[4355]
    eq(p.spell, 483, "GetItemSpell's spell")
    eq(table.concat(p.lines, ","), "0,20,0", "tooltip line types")

    -- A dropped profession takes its recipes with it.
    local dropped = client()
    dropped.world.professions[2] = nil
    dropped.login(text)
    eq(file(dropped.logout()).snapshot.recipes, nil, "gone with the profession")
    return text
end)

-- This session (S2, INGAME §8): the coach strip, off until /fb coach, and
-- the card during the logout countdown. Runecloth has a last-scan price of
-- 1g 12s in the tooltip index.
local PRICES = tooltipSlot("Tooltip2", '\t["alts"] = {},\n\t["items"] = {\n\t\t[14047] = { 11200 },\n\t},\n')

scenario("session", function()
    local PREFIX = "|cffffd100Forever Buddy:|r "
    local c = client({ slots = { ["Data/Tooltip2.lua"] = PRICES } })
    c.login(nil)
    eq(c.global("ForeverBuddyCoachFrame"), nil, "the coach is off by default")
    c.slash("/fb coach")
    eq(c.chat[#c.chat], PREFIX .. "session coach on.", "/fb coach")
    local f = c.global("ForeverBuddyCoachFrame")
    eq(f.shown, true, "shown")
    eq(f.heading.text, "This session", "heading")
    eq(c.settings, nil, "settings are saved at logout, not before")

    -- An hour: 312g, 41 items looted (40 Runecloth priced, a hood not),
    -- one quest, 1,000 XP.
    c.advance(30 * MINUTE)
    c.setMoney(c.world.money + 3120000)
    c.loot(14047, 40)
    c.loot(10005, 1)
    c.gainXp(1000)
    c.turnIn(176, 0, 75)
    c.advance(30 * MINUTE)
    local rows = function(frame)
        local out = {}
        for _, r in ipairs(frame.rows) do
            if r.shown then
                out[#out + 1] = r.label.text .. " | " .. r.value.text
            end
        end
        return table.concat(out, "\n")
    end
    eq(f.meta.text, "1h 0m", "session length")
    eq(rows(f), "Gold | +312g · 312g/hr\nExperience | 1,000/hr\nLevel 13 in | ~6h 36m\nLoot | 41 items · ~44g",
        "the rows")

    -- In combat it stands still; with the option, it hides.
    c.combat(true)
    c.setMoney(c.world.money + 10000000)
    c.advance(10)
    eq(rows(f):match("^Gold | ([^\n]*)"), "+312g · 312g/hr", "no updates in combat")
    c.combat(false)
    c.slash("/fb coach combat")
    eq(c.chat[#c.chat], PREFIX .. "session coach hides in combat.", "the option")
    c.combat(true)
    eq(f.shown, false, "hidden in combat")
    c.combat(false)
    eq(f.shown, true, "back after")

    -- XP across a level-up counts the rest of the old bar too.
    c.gainXp(7000)
    c.advance(10)
    -- 8,000 XP in 1h 0m 20s.
    eq(rows(f):match("Experience | ([^\n]*)"), "7,956/hr", "across the level-up")

    -- The logout card: during the countdown, gone when it's cancelled.
    c.startLogout()
    local card = c.global("ForeverBuddyCardFrame")
    eq(card.shown, true, "the card")
    local h = tonumber(os.date("!%H", c.now))
    local part = (h >= 5 and h < 12 and "morning") or (h >= 12 and h < 17 and "afternoon")
        or (h >= 17 and h < 22 and "evening") or "night"
    eq(card.heading.text, "Thrandor's " .. part, "title")
    eq(card.ding.text, "Ding! Level 13", "levelled")
    eq(rows(card), "Played | 1h 0m\nGold | |cff1eff00+1,312g|r\nBest find | |cff0070ddFelcloth Hood|r\nQuests | 1", "rows")
    eq(card.footer.text, "In Adventures after you close WoW", "footer")
    c.cancelLogout()
    eq(card.shown, false, "cancelled")

    c.slash("/fb card off")
    eq(c.chat[#c.chat], PREFIX .. "session card at logout off.", "/fb card off")
    c.startLogout()
    eq(card.shown, false, "off")
    c.slash("/fb card on")

    -- A /reload keeps the session, so the coach keeps counting from login,
    -- and it comes back by itself.
    local text = c.reload()
    eq(c.settings.coach, true, "the coach stays on")
    eq(c.settings.coachCombat, true, "and the option")
    eq(c.settings.card, nil, "the card's default isn't written")
    local g = c.global("ForeverBuddyCoachFrame")
    eq(g.shown, true, "back after /reload")
    eq(rows(g):match("^Gold | ([^\n]*)"), "+1,312g · 1,304g/hr", "still from login (1h 0m 20s)")
    eq(file(text)._meta.session_errors, nil, "no errors")

    c.slash("/fb coach")
    eq(g.shown, false, "/fb coach again")
    eq(c.settings.coach, nil, "off isn't written")
    local out = c.logout()

    -- A short session with nothing to say gets no card; a loss reads white.
    local brief = client()
    brief.login(nil)
    brief.advance(4 * MINUTE)
    brief.startLogout()
    eq(brief.global("ForeverBuddyCardFrame"), nil, "no card for 4 minutes of nothing")
    brief.setMoney(brief.world.money - 20000)
    brief.startLogout()
    local lost = brief.global("ForeverBuddyCardFrame")
    eq(rows(lost), "Played | 4m\nGold | -2g", "a loss in white")
    return out
end)

-- The window and minimap button (U1, INGAME §17): opened by the button, the
-- compartment or /fb, never by itself; every toggle on its Settings tab and
-- in Esc › Options › AddOns; nothing changes in combat.
scenario("window", function()
    local PREFIX = "|cffffd100Forever Buddy:|r "
    local c = client({ slots = { ["Data/Plan.lua"] = PLAN } })
    c.login(nil)
    c.advance(5)
    c.chat = {}
    eq(c.global("ForeverBuddyWindow"), nil, "never opens by itself")

    -- The minimap button: our own shield, on the minimap.
    local b = c.global("ForeverBuddyMinimapButton")
    eq(b.parent, c.global("Minimap"), "on the minimap")
    eq(b.shown, true, "shown")
    eq(b.children[2].texture, "Interface\\AddOns\\ForeverBuddy\\Media\\Icon", "our shield")
    b.scripts.OnEnter(b)
    local tip = c.global("GameTooltip").lines
    eq(tip[1], "Forever Buddy", "tooltip")
    eq(tip[2], "|cff4fb8ffClick|r to open · |cff4fb8ffDrag|r to move", "what a click does")
    eq(tip[3], "|cff4fb8ffRight-click|r to hide this button", "and a right-click")
    eq(tip[4]:match("^From Forever Buddy · %d%d:%d%d") ~= nil, true, "freshness")

    -- A click opens it on its last tab (Plan at first); a click closes it.
    c.click(b)
    local w = c.global("ForeverBuddyWindow")
    eq(w.shown, true, "opened")
    eq(w.title, "Forever Buddy", "titled")
    eq(w.panes[1].scroll.shown, true, "on Plan")
    local esc = false
    for _, name in ipairs(c.global("UISpecialFrames")) do
        esc = esc or name == "ForeverBuddyWindow"
    end
    eq(esc, true, "Esc closes it")
    c.click(w.Tabs[4])
    eq(w.panes[4].scroll.shown, true, "another tab")
    eq(w.panes[1].scroll.shown, false, "one at a time")
    c.click(b)
    eq(w.shown, false, "closed")
    c.slash("/fb")
    eq(w.panes[4].scroll.shown, true, "/fb opens on the last tab")

    -- The compartment opens and closes it; its tooltip is the name and the
    -- freshness line.
    c.global("ForeverBuddy_OnAddonCompartmentClick")()
    eq(w.shown, false, "the compartment toggles it")
    c.global("ForeverBuddy_OnAddonCompartmentEnter")(nil, b)
    eq(#c.global("GameTooltip").lines, 2, "name and freshness only")

    -- W1: the rail. Tonight's pages, the modules in Settings.lua's order with
    -- a dot each, and Settings at the foot; the Settings page holds what
    -- isn't a module. And the same options in Esc › Options › AddOns.
    c.slash("/fb")
    c.click(w.Tabs[5])
    local rail = {}
    for i = 1, 4 do
        rail[#rail + 1] = w.Tabs[i].label.text
    end
    for _, id in ipairs({ "bags", "map", "quest", "tooltips", "coach" }) do
        rail[#rail + 1] = w.mods[id].label.text
    end
    eq(table.concat(rail, ", "), "Plan, Lists, Errands, Cleanup, Bags, Map, Quests, Tooltips, Coach", "the rail")
    eq(w.heading.text, "Settings", "the Settings page")
    local labels = {}
    for _, cb in ipairs(w.panes[5].checks) do
        labels[#labels + 1] = cb.label.text .. (cb.checked and " ✓" or "")
    end
    eq(table.concat(labels, ", "), "Login briefing ✓, Lockouts at the entrance ✓, Session card at logout ✓, "
        .. "Marks in your bags ✓, Minimap button ✓, Record my next session", "what isn't a module")
    -- A module's page: its name and line, its switch at the top right, and
    -- its options. U1d: a dependent option isn't shown until its parent is
    -- on, indented under it, and the rows below move up rather than leave a
    -- gap.
    c.click(w.mods.coach)
    eq(w.heading.text .. " / " .. w.fresh.text, "Coach / This session's gold, experience and loot, on screen.",
        "the Coach page")
    eq(w.modSwitch.shown ~= false and w.modSwitch.checked, true, "its switch, on")
    local checks = w.pages.coach.checks
    eq(checks[1].label.text .. ", " .. checks[2].label.text, "Session coach, Hide the coach in combat", "its options")
    eq(checks[2].shown, false, "combat hiding shows only with the coach")
    c.click(checks[1])
    eq(checks[2].shown, true, "coach on: combat hiding appears")
    eq(checks[2].points[1][4] - checks[1].points[1][4], 36, "indented under the coach")
    eq(c.global("ForeverBuddyCoachFrame").shown, true, "and the coach shows")
    -- The module off: the coach goes at once (no /reload), its options
    -- hide, and the rail's dot dims; back on, it's as it was.
    c.click(w.modSwitch)
    eq(c.global("ForeverBuddyCoachFrame").shown, false, "module off: the coach goes")
    eq(checks[1].shown, false, "its options hide")
    eq(hex(w.mods.coach.dot.layout.SetVertexColor), "#7d6c58", "the diamond dims")
    eq(c.global("ForeverBuddySettings").modules.coach, false, "saved per account")
    c.click(w.modSwitch)
    eq(c.global("ForeverBuddyCoachFrame").shown, true, "back on: the coach is back")
    eq(checks[1].checked, true, "with its setting kept")
    c.click(checks[1])
    eq(checks[2].shown, false, "coach off: gone again")
    -- Every switch has its one line on hover.
    for _, cb in ipairs(w.panes[5].checks) do
        cb.scripts.OnEnter(cb)
        local tip = c.global("GameTooltip").lines
        eq(tip[1], cb.label.text, "the hover names " .. cb.label.text)
        eq(type(tip[2]) == "string" and #tip[2] > 10, true, cb.label.text .. " has a line: " .. tostring(tip[2]))
        cb.scripts.OnLeave(cb)
    end

    -- Esc › Options › AddOns: the client's vertical layout, the same
    -- toggles as rows, each with its line as the tooltip.
    local options = c.global("Settings").categories[1]
    eq(options.name, "Forever Buddy", "in Esc › Options › AddOns")
    eq(options.vertical, true, "the vertical layout, not a canvas")
    local rows = {}
    for _, init in ipairs(options.layout.inits) do
        rows[#rows + 1] = init.kind == "header" and ("[" .. init.text .. "]") or init.setting.name
    end
    eq(table.concat(rows, ", "), "[In game], Login briefing, Lockouts at the entrance, "
        .. "Session card at logout, Marks in your bags, "
        .. "[Bags], Bags, Junk in your bags, Item level on gear, "
        .. "[Map], Map, Coordinates on the world map, Coordinates under the minimap, "
        .. "Fade the world map while you move, How much of the map shows while moving, "
        .. "[Quests], Quests, Quest levels in the quest log, Mark quests that are new in Forever, "
        .. "Quest progress on item tooltips, See-through quest tracker, How much of the tracker shows, "
        .. "[Tooltips], Tooltips, Your alts on item tooltips, "
        .. "[Coach], Coach, Session coach, Hide the coach in combat, "
        .. "[Merchant], Merchant, Repair at vendors, Guild bank pays first, Sell grey items, Sell items you marked, "
        .. "[This window], Minimap button, [Help fix bugs], Record my next session",
        "every row in order")
    local function row(name)
        for _, init in ipairs(options.layout.inits) do
            if init.kind == "checkbox" and init.setting.name == name then
                return init
            end
        end
    end
    local combatRow, coachRow = row("Hide the coach in combat"), row("Session coach")
    eq(combatRow.parent, coachRow, "combat hiding hangs off the coach")
    eq(combatRow:IsShown(), false, "and isn't shown while it's off")
    coachRow.setting:SetValue(true)
    eq(combatRow:IsShown(), true, "shown once the coach is on")
    eq(checks[1].checked, true, "and the window's Coach page agrees")
    coachRow.setting:SetValue(false)
    eq(row("Login briefing").tooltip,
        "One quiet chat line when you log in: quests to hand in, repairs, mail on your alts, your note.",
        "the hover line is the row's tooltip")
    eq(row("Login briefing").setting.default, true, "Defaults puts it back on")
    c.click(w.Tabs[5])
    local brief = w.panes[5].checks[1]
    c.click(brief)
    eq(c.global("ForeverBuddySettings").briefing, false, "saved per account")
    eq(row("Login briefing").setting:GetValue(), false, "the Options row follows")
    row("Login briefing").setting:SetValue(true)
    eq(c.global("ForeverBuddySettings").briefing, nil, "on again, from Options")
    eq(brief.checked, true, "and the tab follows")
    -- In combat, a click in Options changes nothing, and the page re-reads
    -- the setting so its box goes back.
    c.world.combat = true
    row("Login briefing").setting:SetValue(false)
    eq(c.global("ForeverBuddySettings").briefing, nil, "nothing saved in combat")
    c.advance(1)
    local notified = c.global("Settings").notified
    eq(notified[#notified], "FOREVERBUDDY_WINDOW_BRIEFING", "the Options box is told to go back")
    c.world.combat = false
    c.click(w.panes[5].checks[2])
    eq(c.global("ForeverBuddySettings").lockouts, false, "lockouts off is saved")
    c.click(w.panes[5].checks[2])
    -- In combat, nothing in it changes.
    c.world.combat = true
    c.click(brief)
    eq(brief.checked, true, "the box goes back")
    eq(c.global("ForeverBuddySettings").briefing, nil, "nothing saved")
    c.world.combat = false

    -- Right-click hides the button, with one line saying how to get it back.
    c.click(b, "RightButton")
    eq(b.shown, false, "hidden")
    eq(c.chat[#c.chat], PREFIX .. "minimap button hidden. /fb minimap or the Settings tab brings it back.", "says how")
    local minimapBox
    for _, cb in ipairs(w.panes[5].checks) do
        if cb.option.key == "window.minimap" then
            minimapBox = cb
        end
    end
    eq(minimapBox.checked, false, "the Settings tab agrees")
    c.slash("/fb minimap")
    eq(b.shown, true, "back")
    eq(c.chat[#c.chat], PREFIX .. "minimap button shown.", "says so")
    c.click(minimapBox)
    eq(b.shown, false, "the checkbox hides it too")
    c.click(minimapBox)

    -- Dragged, it follows the cursor round the edge, and the angle is kept.
    c.world.cursor = { 1000, 800 } -- straight above the minimap's centre
    b.scripts.OnDragStart(b)
    b.scripts.OnUpdate(b)
    b.scripts.OnDragStop(b)
    eq(c.global("ForeverBuddySettings").minimapAngle, 90, "the angle")
    eq(b.scripts.OnUpdate, nil, "stops following")

    c.slash("/fb sync")
    eq(c.world.requests.reload, 1, "/fb sync reloads (B4; the sync scenario has the rest)")
end)

-- Record mode (SIM1 b): "Record my next session" records one session, then
-- turns itself off. Numbers only: events by name, number and boolean
-- arguments, a probe of the client after each; money as the change since
-- the first probe. Its file is the fixture the replay below plays back.
scenario("record", function()
    local PREFIX = "|cffffd100Forever Buddy:|r "
    local c = stocked({ forever = true })
    c.settings = { recordNext = true }
    c.world.inbox = { { sender = "Gankalot", subject = "Secret plans", money = 5, cod = 0, days = 3, items = {} } }
    c.login(nil)
    eq(c.chat[1], PREFIX .. "recording this session to help fix bugs (numbers only).", "says so at login")
    c.advance(10 * MINUTE)
    c.give(117, 2)
    c.gainXp(500)
    c.setMoney(26000)
    c.openMailbox()
    c.closeMailbox()
    -- R2a: item loads (hundreds at the AH) are counted, not logged.
    for _ = 1, 50 do
        c.fire("ITEM_DATA_LOAD_RESULT", 117, true)
    end
    c.advance(MINUTE)
    local text = c.logout()
    local db = file(text)
    local r = db.record
    eq(r.addon, db._meta.addon, "which addon")
    eq(r.events[1].e, "PLAYER_LOGIN", "from the login")
    eq(r.events[#r.events].e, "PLAYER_LOGOUT", "to the logout")
    eq(r.events[1].s.m, 0, "money is the change since the first probe")
    eq(c.settings.recordNext, nil, "turned itself off")
    local loads = 0
    for _, e in ipairs(r.events) do
        loads = loads + (e.e == "ITEM_DATA_LOAD_RESULT" and 1 or 0)
    end
    eq(loads, 0, "item loads aren't logged one by one")
    eq(r.counts.ITEM_DATA_LOAD_RESULT >= 50, true, "they're counted")

    -- Numbers only: no string from the game but event names and the
    -- allowlist (and the markers for what was dropped).
    local allowed = { ForeverBuddy = true, player = true, ["~"] = true, ["nil"] = true, secret = true }
    local seen = {}
    for _, e in ipairs(r.events) do
        seen[e.e] = true
        for _, a in ipairs(e.a or {}) do
            if type(a) == "string" then
                eq(allowed[a], true, "argument " .. a .. " of " .. e.e)
            end
        end
        for k, v in pairs(e.s or {}) do
            eq(type(v) ~= "string" or v == "secret", true, "probe " .. k .. " of " .. e.e)
        end
    end
    eq(seen.BAG_UPDATE_DELAYED and seen.PLAYER_MONEY and seen.MAIL_SHOW and true, true, "the events in between")
    eq(text:find("Gankalot", 1, true) == nil or db.snapshot.mail ~= nil, true, "mail text only where it always was")

    -- The next login says it once; nothing is recorded then.
    c.chat = {}
    c.login(text)
    eq(c.chat[1], PREFIX .. "last session's recording is saved. Recording is off again.", "once")
    local again = file(c.logout())
    eq(again.record, nil, "off again")
    c.login(text)
    eq(#c.chat, 1, "said once only")

    -- /fb record ticks the same box.
    c.slash("/fb record")
    eq(c.chat[#c.chat], PREFIX .. "recording your next session.", "/fb record")
    eq(c.global("ForeverBuddySettings").recordNext, true, "the box")
    c.slash("/fb record")
    eq(c.chat[#c.chat], PREFIX .. "recording off.", "and off")

    -- Capped: past 3000 entries it stops and says it was cut short.
    local long = client()
    long.settings = { recordNext = true }
    long.login(nil)
    for _ = 1, 3100 do
        long.fire("PLAYER_MONEY")
    end
    local cut = file(long.logout()).record
    eq(#cut.events, 3000, "capped")
    eq(cut.truncated, true, "cut short")
    return text
end)

-- Replays a recording (SIM1 b): the fake client answers as the probes say,
-- the recorded events fire in order, and what the addon writes has to hold
-- up. A probe with no backpack is the client before it's up (`loading`) or
-- after the teardown (`torn`). `onProbe(phase, probe)`, if given, sees each.
local REPLAY_MONEY = 100000

local function replay(rec, onProbe)
    local c = client({ forever = true })
    c.login(nil, { events = false })
    local w, t, last = c.world, 0, nil
    local lockouts = {}
    for i = 1, 10 do
        lockouts[i] = { name = "Instance " .. i, reset = 3 * DAY, raid = false, difficulty = "Normal" }
    end
    local wasLive = false
    for _, e in ipairs(rec.events) do
        c.advance(math.max(0, e.t - t))
        t = e.t
        local s = e.s
        if s then
            last = s
            -- H2: no backpack means the character isn't up: before the
            -- first live look it's still loading, after one it's torn.
            if type(s.b) == "number" and s.b > 0 then
                w.phase, wasLive = "live", true
            else
                w.phase = wasLive and "torn" or "loading"
            end
            if onProbe then
                onProbe(w.phase, s)
            end
            if type(s.m) == "number" then
                w.money = REPLAY_MONEY + s.m
            end
            w.level = type(s.l) == "number" and s.l > 0 and s.l or w.level
            w.xp = type(s.x) == "number" and s.x or w.xp
            w.xp_max = type(s.xm) == "number" and s.xm > 0 and s.xm or w.xp_max
            if type(s.sv) == "number" then
                w.lockouts = { unpack(lockouts, 1, math.min(s.sv, #lockouts)) }
            end
            w.combat = s.c == true
        end
        local args = {}
        for i, a in ipairs(e.a or {}) do
            args[i] = (a ~= "nil" and a ~= "~" and a ~= "secret") and a or nil
        end
        c.fire(e.e, unpack(args, 1, #(e.a or {})))
    end
    return file(c.save()), last
end

-- Real sessions from Julia's client (record mode), each a fixture holding
-- `ForeverBuddyDB = { record = ... }` (README › Recordings). Unlike the
-- synthetic one, which the fake itself made, these say what Forever does.
local REAL = {}

-- H2: what forever.lua's rows predict a probe shows outside the live phase,
-- checked against real recordings. A probe key -> how to read the API's
-- answer from it (all counts or levels, so "not up" is 0).
local PROBED = { b = true, l = true, x = true, w = true, q = true, sv = true }
local confirmed = {} -- API name -> the recording that confirmed its row

local function crossCheck(recName)
    local rows = {}
    for api, r in pairs(wow.FOREVER) do
        if r.probe and PROBED[r.probe] then
            rows[r.probe] = { api = api, row = r }
        end
    end
    local liveSeen = {} -- probe key -> a live probe had it above 0
    return function(phase, s)
        for key, v in pairs(rows) do
            local got = s[key]
            if type(got) == "number" then
                if phase == "live" then
                    liveSeen[key] = liveSeen[key] or got > 0
                else
                    local emptied = v.row[phase] ~= nil
                    if emptied and got ~= 0 then
                        fail(("forever.lua: %s reads empty while %s, but recording %s shows %s=%s"):format(
                            v.api, phase, recName, key, tostring(got)))
                    elseif not emptied and got == 0 and liveSeen[key] then
                        fail(("forever.lua: %s answers as live while %s, but recording %s shows %s=0"):format(
                            v.api, phase, recName, key))
                    elseif liveSeen[key] then
                        confirmed[v.api] = confirmed[v.api] or recName
                    end
                end
            end
        end
    end
end

scenario("replay", function()
    local recordings = { synthetic = file(wow.readFile(FIXTURES .. "record.lua")).record }
    for _, name in ipairs(REAL) do
        recordings[name] = wow.parse(wow.readFile(FIXTURES .. name .. ".lua"), "ForeverBuddyDB").record
    end
    for name, rec in pairs(recordings) do
        local real = name ~= "synthetic"
        local db = replay(rec, real and crossCheck(name) or nil)
        eq(db._meta.errors, nil, name .. ": no addon errors")
        local gains = 0
        for _, s in ipairs(db.sessions) do
            for _, e in ipairs(s.events) do
                gains = gains + (e.kind == "gain" and 1 or 0)
            end
        end
        eq(gains, 0, name .. ": logging in isn't a gain (the bags don't change in a replay)")
        -- The last probe the character could be read at: the file says the same.
        local live
        for _, e in ipairs(rec.events) do
            if e.s and type(e.s.b) == "number" and e.s.b > 0 then
                live = e.s
            end
        end
        if live then
            eq(db.snapshot ~= nil, true, name .. ": a snapshot")
            eq(db.snapshot.money, REPLAY_MONEY + live.m, name .. ": its money is the last live one")
            eq(db.snapshot.lockouts and #db.snapshot.lockouts or 0, math.min(live.sv or 0, 10),
                name .. ": its saves are the last live ones")
        else
            eq(db.snapshot, nil, name .. ": never live, no snapshot")
        end
    end
end)

-- H4: chaos. Each seed plays a random session (Forever's loading and
-- teardown or not, a few APIs secret, then ~30 steps of looting, selling,
-- combat, /reloads, deaths, zones and money) and checks what must always
-- hold: the file parses and its counts add up, the caps hold, every gain is
-- something the session gave, and a snapshot is never hollow. The runner's
-- after-each checks (no escaped, unmodelled or caught errors) apply to each
-- seed too. A seed is its own scenario, chaos_<seed>, so a failure names
-- it; CHAOS_SEEDS=n changes how many run (default 200), CHAOS_ONLY=n runs
-- one and prints its steps.
local MAX_SESSIONS_ADDON, MAX_EVENTS_ADDON = 10, 2000
-- APIs a seed may make secret: they return values the addon must guard.
local CHAOS_SECRET = { "GetMoney", "UnitXP", "UnitLevel", "GetInventoryItemLink", "C_Container.GetContainerItemInfo" }

local function rng(seed)
    local s = (seed * 7919) % 2147483647
    if s <= 0 then
        s = s + 2147483646
    end
    return function(n)
        s = (s * 16807) % 2147483647
        return (s % n) + 1
    end
end

local function chaos(seed, verbose)
    local r = rng(seed)
    local steps = {}
    local function note(s)
        steps[#steps + 1] = s
        if verbose then
            print("  " .. s)
        end
    end
    local secret = {}
    for _, name in ipairs(CHAOS_SECRET) do
        if r(8) == 1 then
            secret[name] = true
            note("secret " .. name)
        end
    end
    local forever = r(2) == 1
    note("forever " .. tostring(forever))
    local c = client({ forever = forever, secret = next(secret) and secret or nil })
    -- What the bags hold, so only real items are used or sold; what the
    -- session gave, which is all a gain may be.
    local held = { [6948] = 1, [2589] = 4 }
    local given = {}
    local ITEMS = { 2589, 14047, 117, 6948 }
    -- H4b: the client's login in different orders. Plain, or by hand:
    -- money before PLAYER_LOGIN, the bags before PLAYER_ENTERING_WORLD, or
    -- the bags late with a BAG_UPDATE_DELAYED before they've loaded and two
    -- after.
    local function login(text)
        local how = r(4)
        if how == 1 then
            note("login")
            c.login(text)
            return
        end
        c.login(text, { events = false })
        c.fire("ADDON_LOADED", "ForeverBuddy")
        if how == 2 then
            note("login: money first")
            c.fire("PLAYER_MONEY")
        end
        c.fire("PLAYER_LOGIN")
        if how == 3 then
            note("login: bags before entering the world")
            c.world.phase = "live"
            c.fire("BAG_UPDATE_DELAYED")
            c.fire("PLAYER_ENTERING_WORLD", true, false)
        else
            note("login: bags late, and twice")
            c.fire("PLAYER_ENTERING_WORLD", true, false)
            c.advance(r(30))
            c.fire("BAG_UPDATE_DELAYED")
            c.world.phase = "live"
            c.fire("BAG_UPDATE_DELAYED")
            c.fire("BAG_UPDATE_DELAYED")
        end
    end

    local function step()
        local k = r(13)
        if k == 1 then
            local t = r(900)
            note("advance " .. t)
            c.advance(t)
        elseif k == 2 then
            local id, n = ITEMS[r(#ITEMS)], r(5)
            note("loot " .. id .. " x" .. n)
            c.loot(id, n)
            held[id], given[id] = (held[id] or 0) + n, true
        elseif k == 3 then
            for id, n in pairs(held) do
                if n > 0 then
                    note("use " .. id)
                    c.use(id, 1)
                    held[id] = n - 1
                    break
                end
            end
        elseif k == 4 then
            local on = r(2) == 1
            note("combat " .. tostring(on))
            c.combat(on)
        elseif k == 5 then
            note("reload")
            c.combat(false)
            c.reload()
        elseif k == 6 then
            note("levelUp")
            c.levelUp()
        elseif k == 7 then
            local xp = r(3000)
            note("gainXp " .. xp)
            c.gainXp(xp)
        elseif k == 8 then
            note("die")
            c.die(r(500))
        elseif k == 9 then
            local zone = ({ "Westfall", "Duskwood", "Stormwind City" })[r(3)]
            note("zone " .. zone)
            c.enterZone(zone)
        elseif k == 10 then
            note("vendor")
            c.openMerchant({ 117 })
            if (held[2589] or 0) > 0 then
                c.sell(2589, 1, 10)
                held[2589] = held[2589] - 1
            end
            c.closeMerchant()
        elseif k == 11 then
            local m = r(50000)
            note("money +" .. m)
            c.setMoney(c.world.money + m)
        elseif k == 12 and r(12) == 1 then
            -- H4b: a burst of loot, past the event cap on its own.
            local n = MAX_EVENTS_ADDON + r(500)
            note("burst of " .. n .. " loots")
            for i = 1, n do
                local id = ITEMS[(i % #ITEMS) + 1]
                c.loot(id, 1)
                held[id], given[id] = (held[id] or 0) + 1, true
            end
        else
            note("logout countdown")
            c.startLogout()
            c.cancelLogout()
        end
    end

    local function check(ok, what)
        if not ok then
            fail("seed " .. seed .. ": " .. what .. "\n  steps: " .. table.concat(steps, "; "))
        end
    end
    local function checkFile(text)
        local okParse, db = pcall(file, text)
        check(okParse, "the file doesn't parse or its counts don't add up: " .. tostring(db))
        check(entries(db.sessions) <= MAX_SESSIONS_ADDON, "more than " .. MAX_SESSIONS_ADDON .. " sessions")
        for _, s in pairs(db.sessions) do
            check(entries(s.events) <= MAX_EVENTS_ADDON, "a session past " .. MAX_EVENTS_ADDON .. " events")
            for _, e in pairs(s.events or {}) do
                if e.kind == "gain" and e.how == nil then
                    check(given[e.item] == true, "a gain of " .. tostring(e.item) .. " the session never gave")
                end
            end
        end
        if db.snapshot then
            check(type(db.snapshot.bags) == "table" and db.snapshot.bags[0] ~= nil,
                "a hollow snapshot (no backpack)")
        end
    end

    -- One login, or now and then (H4b) a chain of 11 to 13, past the
    -- session cap. Each logs out one of three ways, and every file written
    -- is checked.
    local text
    local logins = r(10) == 1 and 10 + r(3) or 1
    for n = 1, logins do
        login(text)
        for _ = 1, n == 1 and 30 or r(5) do
            step()
        end
        c.combat(false)
        local out = r(3)
        if out == 1 then
            note("logout: countdown")
            c.startLogout()
            c.advance(r(20))
        elseif out == 2 and forever then
            note("logout: teardown before the countdown")
            c.world.phase = "torn"
            c.startLogout()
        else
            note("logout: instant")
        end
        text = c.logout()
        checkFile(text)
        c.advance(r(3600))
    end
end

local CHAOS_ONLY = tonumber(os.getenv("CHAOS_ONLY") or "")
for seed = 1, CHAOS_ONLY and 0 or tonumber(os.getenv("CHAOS_SEEDS") or "") or 200 do
    scenario("chaos_" .. seed, function()
        chaos(seed)
    end)
end
if CHAOS_ONLY then
    scenario("chaos_" .. CHAOS_ONLY, function()
        print("chaos seed " .. CHAOS_ONLY .. ":")
        chaos(CHAOS_ONLY, true)
    end)
end

-- W1: the window has its own look now, so with EllesmereUI installed it no
-- longer asks to be skinned (R3a's RegisterSkin), and it opens the same.
scenario("eui_skin", function()
    local registered = {}
    local eui = {
        RegisterSkin = function(name, fn)
            registered[name] = fn
            return true
        end,
    }
    local c = client({ globals = { EllesmereUI = eui } })
    c.login(nil)
    eq(registered.ForeverBuddy, nil, "no skin registered")
    c.slash("/fb list")
    local win = c.global("ForeverBuddyWindow")
    eq(win.shown and win.heading.text, "Lists", "the window opens in its own look")
    file(c.logout())
end)

-- R3a: a bag slot Forever hands back as a table holding secrets
-- (canaccesstable is false, and indexing it raises). The bag read is
-- unknown, counted in secret_hits, and nothing is logged as gained when it
-- reads again.
scenario("unreadable_table", function()
    local c = client()
    c.login(nil)
    c.advance(5)
    local before = c.world.bags[0].slots[1]
    c.world.bags[0].slots[1] = { id = before.id, count = before.count, unreadable = true }
    c.fire("BAG_UPDATE_DELAYED")
    c.advance(5)
    c.world.bags[0].slots[1] = before
    local db = file(c.logout())
    if db._meta.secret_hits < 1 then
        fail("secret_hits: expected at least 1, got " .. show(db._meta.secret_hits))
    end
    for _, s in ipairs(db.sessions) do
        for _, e in ipairs(s.events or {}) do
            if e.kind == "gain" then
                fail("an unreadable bag slot logged a gain of " .. show(e.item))
            end
        end
    end
end)

-- R3a (addon bible 2.19): the keyring is bag -2 on Forever. A key moved onto
-- it and back is carried all along, so coming back isn't a gain. (Before,
-- only bags 0-5 counted: the key "vanished" and its return was logged.)
scenario("keyring", function()
    local KEY = 5396
    local c = client()
    c.world.bags[-2] = { size = 4, slots = {} }
    c.world.bags[0].slots[5] = { id = KEY, count = 1 }
    c.login(nil)
    c.advance(5)
    c.world.bags[0].slots[5] = nil
    c.world.bags[-2].slots[1] = { id = KEY, count = 1 }
    c.fire("BAG_UPDATE_DELAYED")
    c.advance(5)
    c.world.bags[-2].slots[1] = nil
    c.world.bags[0].slots[5] = { id = KEY, count = 1 }
    c.fire("BAG_UPDATE_DELAYED")
    c.advance(5)
    local db = file(c.logout())
    for _, s in ipairs(db.sessions) do
        for _, e in ipairs(s.events or {}) do
            if e.kind == "gain" and e.item == KEY then
                fail("a key back off the keyring was logged as a gain")
            end
        end
    end
end)

-- R3a (addon bible 3.19): a forbidden tooltip, or one with a forbidden owner
-- (a nameplate aura button), allows nothing but IsForbidden. Our item lines
-- stay off it; the fake's tooltip raises if it's written to.
scenario("forbidden_tooltip", function()
    -- The `tooltip` scenario's slots: Runecloth on three alts.
    local c = client({
        slots = {
            ["Data/Tooltip1.lua"] = tooltipSlot("Tooltip1", ALTS .. '\t["items"] = {},\n'),
            ["Data/Tooltip2.lua"] = tooltipSlot("Tooltip2", ALTS
                .. '\t["scanAt"] = ' .. (wow.EPOCH - 3 * DAY) .. ',\n'
                .. '\t["items"] = {\n\t\t[14047] = { 11200, 1, 0, 340, 0, 0, 2, 5, 0, 0, 0, 3, 0, 0, 3, 0 },\n\t},\n'),
        },
    })
    c.login(nil)
    eq(#c.hover(14047) > 1, true, "an ordinary tooltip gets our lines")
    eq(#c.hover(14047, "tooltip"), 1, "a forbidden tooltip gets none")
    eq(#c.hover(14047, "owner"), 1, "nor one whose owner is forbidden")
end)

-- R3a, from EllesmereUI's guardrails: globals Forever doesn't have. Retail
-- keeps these only as deprecation fallbacks, and Forever loads none of them,
-- so a call returns nothing there (through read()) while it works on retail.
-- Each has a C_ form; GetSpecializationInfoByID has none and must be guarded.
local FOREVER_LACKS = [[
GetSpecialization GetSpecializationInfo GetNumSpecializationsForClassID GetActiveSpecGroup
GetSpecializationMasterySpells GetTalentInfo GetSpecializationInfoByID
ActionBindsItem BindEnchant ConfirmBindOnUse ConfirmNoRefundOnUse ConfirmOnUse DoesItemContainSpec
DropItemOnUnit EndBoundTradeable EndRefund EquipItemByName GetDetailedItemLevelInfo GetItemChildInfo
GetItemClassInfo GetItemCooldown GetItemCount GetItemCreationContext GetItemFamily GetItemGem
GetItemInfo GetItemInfoInstant GetItemInventorySlotInfo GetItemQualityColor GetItemSetInfo
GetItemSpecInfo GetItemSpell GetItemSubClassInfo GetItemUniqueness GetItemIcon IsArtifactPowerItem
IsConsumableItem IsCorruptedItem IsCosmeticItem IsCurrentItem IsDressableItem IsEquippableItem
IsEquippedItem IsEquippedItemType IsHarmfulItem IsHelpfulItem IsItemInRange IsUsableItem ItemHasRange
PickupItem ReplaceEnchant ReplaceTradeEnchant ReplaceTradeskillEnchant UseItemByName
AcceptSockets ClickSocketButton CloseSocketInfo GetExistingSocketInfo GetExistingSocketLink
GetNewSocketInfo GetNewSocketLink GetNumSockets GetSocketItemBoundTradeable GetSocketItemInfo
GetSocketItemRefundable GetSocketTypes HasBoundGemProposed
GetCoinIcon GetCoinText GetCoinTextureString GetZonePVPInfo IsSubZonePVPPOI SetPVP TogglePVP
GetPetTalentTree PetAssistMode PickupTradeMoney PlayVocalErrorSoundID
]]

scenario("forever_globals", function()
    local src = wow.readFile(ADDON_DIR .. "ForeverBuddy.lua")
    src = src:gsub("%-%-%[%[.-%]%]", ""):gsub("%-%-[^\n]*", "")
    local found = {}
    for name in FOREVER_LACKS:gmatch("%S+") do
        -- The bare name: "C_Item.GetItemInfo" or obj:GetItemCount() is fine,
        -- GetItemInfo(...) and read("GetItemInfo", ...) are not.
        for pos in src:gmatch("()%f[%w_]" .. name .. "%f[^%w_]") do
            local before = src:sub(pos - 1, pos - 1)
            if before ~= "." and before ~= ":" then
                found[#found + 1] = name
                break
            end
        end
    end
    table.sort(found)
    eq(table.concat(found, ", "), "", "globals Forever doesn't have (use the C_ form)")
end)

-- SIM4: the fake is as strict as the client where a mistake would otherwise
-- pass. An unknown widget method raises (and is noted even if the addon's
-- pcall swallows it), a misspelled anchor raises, and a secret value can't
-- be indexed, measured or joined.
scenario("strict_fake", function()
    local c = client()
    c.login(nil)
    local f = c.global("CreateFrame")("Frame", nil, c.global("UIParent"))
    local ok, err = pcall(function()
        f:SetPointt("TOPLEFT")
    end)
    eq(ok, false, "an unknown method raises")
    eq(tostring(err):match("not modelled in wow.lua: SetPointt") ~= nil, true, "and names it")
    eq(c.unmodelled[1], "SetPointt", "noted for the run's check")
    c.unmodelled = {} -- this one was on purpose
    eq(f.tracker, nil, "an unset field is nil, not a function")
    eq(pcall(f.SetPoint, f, "TOPLEF"), false, "a misspelled point raises")
    eq(pcall(f.SetPoint, f, "TOP", nil, "BOTTOMLEF", 0, 0), false, "so does a misspelled relative point")
    f:SetPoint("TOPLEFT", 8, -4)
    f:SetPoint("TOPLEFT", c.global("UIParent"), "TOPLEFT", 10, 0)
    eq(#f.points, 1, "the same point again replaces it")
    eq(f.points[1][4], 10, "with its offset")
    local s = c.secret()
    eq(pcall(function() return s.x end), false, "a secret can't be indexed")
    -- (# can't be made to error: Lua 5.1 ignores __len on tables.)
    eq(pcall(function() return "x" .. s end), false, "or joined")
    eq(pcall(function() return s + 1 end), false, "or added")
end)

-- H3: what the coverage report showed no scenario reached, where it
-- matters. (Window drags and a button tooltip stay unreached on purpose.)
scenario("coverage_gaps", function()
    local PREFIX = "|cffffd100Forever Buddy:|r "

    -- Opening the bags re-tags marked items through the game's OpenAllBags
    -- (hooksecurefunc), with no bag update.
    local c = client({ slots = { ["Data/Cleanup.lua"] = CLEANUP } })
    c.login(nil)
    c.advance(5)
    local bag = c.global("ContainerFrame1")
    local function tagOf(slot)
        local t = bag.buttons[slot].children[1]
        return t and t.shown and t.texture or nil
    end
    c.openBags()
    c.advance(1)
    eq(tagOf(1), "Interface\\Minimap\\Tracking\\Mailbox", "opening the bags tags them")

    -- A vendor's next page: the game redraws it with MerchantFrame_Update,
    -- no event, and the docked list follows.
    local v = client({ slots = { ["Data/Lists.lua"] = LISTS } })
    v.login(nil)
    v.openMerchant({ 117 })
    local f = v.global("ForeverBuddyListFrame")
    local function here(name)
        for _, row in ipairs(f.rows) do
            if row.shown and row.label.text and row.label.text:find(name, 1, true) then
                return row.label.text:find("here", 1, true) ~= nil
            end
        end
        return false
    end
    eq(here("Linen Cloth"), false, "page 1 has no linen")
    v.merchantPage(2, { 2589 })
    eq(here("Linen Cloth"), true, "page 2 has: the hook redrew the list")

    -- Without GetInventoryItemID, a worn item's id comes from its link.
    local l = client({ api = { GetInventoryItemID = false } })
    local db = file(play(l, nil, 1))
    eq(db.items[25] ~= nil, true, "the worn sword is still noted (from its link)")

    -- Quitting (not logging out) shows the session card too.
    local q = client()
    q.login(nil)
    q.advance(30 * MINUTE)
    q.gainXp(1000) -- the card needs a session with something in it
    q.advance(30 * MINUTE)
    q.fire("PLAYER_QUITING")
    eq(q.global("ForeverBuddyCardFrame").shown, true, "the card on /quit")

    -- Five facts: the briefing shows four and points at /fb brief.
    local b = client({ slots = {
        ["Data/Briefing.lua"] = briefingSlot(), ["Data/Lists.lua"] = LISTS, ["Data/Plan.lua"] = PLAN,
    } })
    b.world.questlog = { { questID = 176, done = true } }
    b.world.durability = { [5] = { 20, 100 } }
    b.login(nil)
    b.advance(5)
    eq(b.chat[1]:match("and more: /fb brief$") ~= nil, true, "capped at four: " .. tostring(b.chat[1]))
    eq(b.chat[1]:sub(1, #PREFIX), PREFIX, "one line")

    -- Plate: a paladin at 60 can wear it, a priest can't.
    local alts = '\t["alts"] = {\n'
        .. alt("Thrandor", "PALADIN", 60, {}, nil, "Vargur")
        .. alt("Grom", "PALADIN", 60, { [1] = 40 })
        .. alt("Sela", "PRIEST", 60, { [1] = 10 })
        .. '\t},\n'
    local p = client({ slots = {
        ["Data/Tooltip1.lua"] = tooltipSlot("Tooltip1", alts .. '\t["items"] = {},\n'),
        ["Data/Tooltip2.lua"] = tooltipSlot("Tooltip2", alts .. '\t["items"] = {},\n'),
    } })
    p.login(nil)
    p.world.equipped[1] = 10004 -- this character wears it, so it isn't the best fit
    local line = table.concat(p.hover(10004), "\n")
    eq(line:find("Grom", 1, true) ~= nil, true, "a paladin can wear plate: " .. line)
    eq(line:find("Sela", 1, true), nil, "a priest can't")

    -- Two Settings boxes no scenario ticked: the coach, and record mode.
    local s = client()
    s.login(nil)
    s.click(setting(s, "Session coach"))
    eq(s.global("ForeverBuddySettings").coach, true, "the coach box turns it on")
    eq(s.global("ForeverBuddyCoachFrame").shown, true, "and shows it")
    local rec = setting(s, "Record my next session")
    s.click(rec)
    eq(s.global("ForeverBuddySettings").recordNext, true, "record mode armed")
    s.click(rec)
    eq(s.global("ForeverBuddySettings").recordNext, nil, "and disarmed, nothing left saved")
end)

-- SIM4: the window's layout on each tab, from the harness's slots, as a
-- snapshot in snapshots/window.txt. A layout change is a diff to review.
scenario("window_layout", function()
    local c = client({ slots = {
        ["Data/Plan.lua"] = PLAN, ["Data/Lists.lua"] = LISTS, ["Data/Cleanup.lua"] = CLEANUP,
    } })
    c.login(nil)
    c.advance(5)
    local parts = {}
    for _, tab in ipairs({ "plan", "list", "errands", "cleanup" }) do
        c.slash("/fb " .. tab)
        parts[#parts + 1] = "== /fb " .. tab .. "\n" .. layoutDump(c.global("ForeverBuddyWindow"))
    end
    snapshot("window", table.concat(parts, "\n"))
end)

-- SIM5a: the census lookups, on a made-up census in the probe's format. The
-- real one, once committed, is checked against the addon's APIs in H2's
-- fidelity block below.
scenario("census", function()
    local censusLib = require("census")
    local text = serialize("ForeverBuddyProbeDB", { census = {
        build = { build = "test" },
        globals = { GetMoney = "f", C_Item = "t", Enum = "t", UIParent = "w" },
        tables = {
            C_Item = { GetItemInfo = "f" },
            Enum = { BagIndex = "t" },
            ["Enum.BagIndex"] = { Keyring = "n" },
        },
    } })
    local cen = censusLib.load(text, wow.parse)
    eq(censusLib.kind(cen, "GetMoney"), "f", "a global")
    eq(censusLib.kind(cen, "C_Item.GetItemInfo"), "f", "one level into a table")
    eq(censusLib.kind(cen, "Enum.BagIndex.Keyring"), "n", "two under Enum")
    eq(censusLib.kind(cen, "C_Item.GetItemCount"), nil, "a member it lacks")
    eq(censusLib.kind(cen, "GetItemInfo"), nil, "a global it lacks")
    local absent = censusLib.absent(cen, { GetMoney = true, GetItemInfo = true, ["C_Item.GetItemInfo"] = true })
    eq(table.concat(absent, ","), "GetItemInfo", "absent() lists only what it lacks")
    local _, why = censusLib.load(serialize("ForeverBuddyProbeDB", { characters = {} }), wow.parse)
    eq(why ~= nil and why:match("no census") ~= nil, true, "a probe file without a census says so")

    -- Our fake's frames are widgets, not namespaces: their fields aren't names.
    local c = client()
    c.login(nil)
    local names = censusLib.names(c.env())
    eq(names.UIParent, true, "the fake's UIParent is a name")
    eq(names["UIParent.children"], nil, "its fields aren't")
    eq(names["C_Timer.After"], true, "a namespace's members are")
end)

-- SIM5d: Blizzard's API docs, read from a made-up extract in their format:
-- the docs folder, and one UI addon beside it. The reader lists folders with
-- ls and find, so it's for a Mac or Linux machine with the extract; Windows
-- skips this.
scenario("apidocs", function()
    if package.config:sub(1, 1) == "\\" then
        return
    end
    local apidocs = require("apidocs")
    local root = os.tmpname()
    os.remove(root)
    local docs = root .. "/blizzard_apidocumentationgenerated"
    assert(os.execute("mkdir -p '" .. docs .. "' '" .. root .. "/blizzard_mailframe'"))
    local function write(path, text)
        local w = assert(io.open(path, "w"))
        w:write(text)
        w:close()
    end
    write(docs .. "/itemdocumentation.lua", [[
local Item = {
    Name = "Item", Type = "System", Namespace = "C_Item",
    Functions = {
        { Name = "GetItemCount", Type = "Function", SecretArguments = "AllowedWhenUntainted",
          Arguments = { { Name = "itemInfo", Type = "ItemInfo", Nilable = false } },
          Returns = { { Name = "count", Type = "number", Nilable = false },
                      { Name = "icon", Type = "fileID", Nilable = true } } },
    },
    Events = {
        { Name = "BagUpdate", Type = "Event", LiteralName = "BAG_UPDATE",
          Payload = { { Name = "bagID", Type = "BagIndex", Nilable = false } } },
    },
}
APIDocumentation:AddDocumentationTable(Item)
]])
    write(docs .. "/simpleframeapidocumentation.lua", [[
local SimpleFrameAPI = {
    Name = "SimpleFrameAPI", Type = "ScriptObject",
    Functions = { { Name = "SetScript", Type = "Function", SecretReturnsForAspect = { Enum.SecretAspect.ObjectSecurity } } },
}
APIDocumentation:AddDocumentationTable(SimpleFrameAPI)
]])
    write(docs .. "/constantsdocumentation.lua", [[
local C = { Name = "Constants", Type = "System", Tables = { { Value = Constants.X.MAX + 1 } } }
APIDocumentation:AddDocumentationTable(C)
]])
    write(root .. "/blizzard_mailframe/mailframe.xml", [[
<OnClick>HandleModifiedItemClick(GetInboxItemLink(InboxFrame.openMailID, self:GetID()));</OnClick>
]])
    local index = apidocs.load(docs)
    os.execute("rm -rf '" .. root .. "'")

    eq(#index.failed, 0, "docs naming enums and constants load")
    eq(index.systems, 3, "every system")
    eq(apidocs.where(index, "C_Item.GetItemCount"), "documented", "a namespaced function")
    eq(table.concat(index.functions["C_Item.GetItemCount"].secrets, ","), "SecretArguments=AllowedWhenUntainted",
        "with its secret flags")
    eq(index.events.BAG_UPDATE ~= nil, true, "an event, by its literal name")
    eq(apidocs.where(index, "UIParent.SetScript"), "documented", "a frame's method, documented per widget type")
    eq(apidocs.where(index, "GetInboxItemLink"), "code", "undocumented, but Blizzard's XML calls it")
    eq(apidocs.where(index, "InboxFrame.GetID"), "code", "a method Blizzard's code calls")
    eq(apidocs.where(index, "C_TradeSkillUI.GetAllRecipeIDs"), nil, "in neither")
    eq(apidocs.stub(index, "C_Item.GetItemCount"), table.concat({
        "        -- C_Item.GetItemCount(itemInfo) -> count: number, icon: fileID?",
        "        -- Secret: SecretArguments=AllowedWhenUntainted",
        '        ["C_Item.GetItemCount"] = function(itemInfo)',
        "            return 0, nil",
        "        end,",
    }, "\n"), "a stub shaped as documented")
end)

-- H2: the cross-check itself, on made-up probes: rows that agree with a
-- recording are confirmed, and one it contradicts fails the run.
scenario("forever_rows", function()
    local check = crossCheck("made-up")
    check("live", { b = 16, l = 12, x = 500, w = 3, q = 2, sv = 1 })
    check("torn", { b = 0, l = 0, x = 0, w = 0, q = 0, sv = 0 })
    eq(confirmed.UnitLevel, "made-up", "a torn level of 0 confirms UnitLevel's row")
    eq(confirmed["C_Container.GetContainerNumSlots"], "made-up", "and the backpack's")
    local ok, err = pcall(check, "torn", { b = 0, l = 12 })
    eq(ok, false, "a level that survives the teardown contradicts the row")
    eq(tostring(err):find("UnitLevel reads empty while torn", 1, true) ~= nil, true, "and says which: " .. tostring(err))
    for api, by in pairs(confirmed) do
        if by == "made-up" then
            confirmed[api] = nil
        end
    end
end)

-- H5: the slots the app actually writes, not hand-written ones. The Rust
-- test src-tauri/src/contract.rs ingests lists.lua (from this harness),
-- adds a list, a plan, a cleanup mark and a login note for Thrandor, and
-- writes every slot to src-tauri/tests/fixtures/bridge/ the way
-- send_to_game does. The addon has to accept each one and use what's in it.
local BRIDGE = ROOT .. "/src-tauri/tests/fixtures/bridge/"
local BRIDGE_STAMP = 1790960000 -- contract.rs's STAMP

local function texts(rows)
    local out = {}
    for _, r in ipairs(rows or {}) do
        if r.shown ~= false and r.label and r.label.text then
            out[#out + 1] = r.label.text
        end
    end
    return table.concat(out, " | ")
end

scenario("contract", function()
    local names = { "Tooltip1", "Tooltip2", "Plan", "Briefing", "Lists", "Cleanup", "Settings" }
    local slots = {}
    for _, name in ipairs(names) do
        slots["Data/" .. name .. ".lua"] = wow.readFile(BRIDGE .. name .. ".lua")
    end
    local c = client({ slots = slots })
    c.login(nil)
    c.advance(10)

    -- Briefing: this character's login note, on its own line.
    eq(table.concat(c.chat, "\n"):find("Train Tailoring", 1, true) ~= nil, true,
        "the login note from the Briefing slot: " .. table.concat(c.chat, " / "))

    -- Plan: the window's Plan tab has the steps.
    c.slash("/fb plan")
    local w = c.global("ForeverBuddyWindow")
    eq(w.panes[1].rows[1].label.text, "Take Wanted: Hogger", "the Plan slot's first step")

    -- Lists: the list and its item.
    c.slash("/fb list")
    local lists = texts(w.panes[2].rows)
    eq(lists:find("Linen Cloth", 1, true) ~= nil, true, "the Lists slot's item: " .. lists)

    -- Cleanup: the marked item.
    c.slash("/fb cleanup")
    local cleanup = texts(w.panes[4].rows)
    eq(cleanup:find("Linen Cloth", 1, true) ~= nil, true, "the Cleanup slot's mark: " .. cleanup)

    -- Every slot was read, at the schema this addon knows.
    local db = file(c.logout())
    -- Settings: the approved profile was applied (the coach is on).
    eq(db.settings.values["window.coach"], true, "the Settings slot's profile")
    eq(type(db.settings.profile), "number", "and its number is kept")
    for _, name in ipairs(names) do
        local r = db.bridge and db.bridge[name]
        eq(r and r.stamp, BRIDGE_STAMP, name .. " was loaded")
        eq(r and r.schema, 1, name .. "'s schema")
    end
end)

-- H6: the bursts the client delivers in a single frame, each run as one
-- frame against FRAME_BUDGET: a full page of AH results with a list docked,
-- hundreds of item loads, the quest log updating over and over with a plan
-- loaded, a full mailbox, and full bags changing. The runner fails the
-- scenario if any handler or frame is over budget.
scenario("bursts", function()
    local c = client({ slots = { ["Data/Plan.lua"] = PLAN, ["Data/Lists.lua"] = LISTS, ["Data/Cleanup.lua"] = CLEANUP } })
    local ids = {}
    for id in pairs(wow.ITEMS) do
        ids[#ids + 1] = id
    end
    table.sort(ids)
    -- Full bags: four 16-slot bags beside the backpack, every slot used.
    for bag = 1, 4 do
        local slots = {}
        for slot = 1, 16 do
            slots[slot] = { id = ids[(bag * 16 + slot) % #ids + 1], count = slot }
        end
        c.world.bags[bag] = { size = 16, slots = slots }
    end
    c.world.questlog = {}
    for i = 1, 25 do
        c.world.questlog[i] = { questID = 1000 + i, done = i % 3 == 0 }
    end
    for i = 1, 50 do
        c.world.inbox[i] = { sender = "Alt" .. i, subject = "Stuff", money = i, cod = 0, days = 20,
            items = { { id = ids[i % #ids + 1], count = 2 } } }
    end
    c.login(nil)
    c.advance(10)

    local frames = {}
    c.openAuctionHouse()
    frames.ah = c.frame(function()
        local page = {}
        for i = 1, 500 do
            page[i] = ids[i % #ids + 1]
        end
        c.browseAuctions(page)
    end)
    c.closeAuctionHouse()
    frames.items = c.frame(function()
        for i = 1, 300 do
            c.fire("ITEM_DATA_LOAD_RESULT", ids[i % #ids + 1], true)
        end
    end)
    frames.quests = c.frame(function()
        for _ = 1, 20 do
            c.fire("QUEST_LOG_UPDATE")
        end
    end)
    frames.mail = c.frame(function()
        c.openMailbox()
    end)
    c.closeMailbox()
    frames.bags = c.frame(function()
        c.fire("BAG_UPDATE_DELAYED")
    end)
    -- A session at the event cap, then the logout countdown: the card adds
    -- up the whole session (H4b found it over budget). Fixed here, so every
    -- run meets it, not only the chaos seeds that happen to.
    for _ = 1, 2100 do
        c.loot(2589, 1) -- onto the backpack's Linen Cloth stack
    end
    c.advance(10 * 60)
    frames.logout = c.frame(function()
        c.startLogout()
    end)
    -- (Not counted under --coverage, whose hook is already set.)
    local counting = wow.cost.on and debug.gethook() == nil
    for name, n in pairs(frames) do
        eq(n > 0, counting, name .. " was counted")
    end
    local db = file(c.logout())
    eq(#db.sessions[1].events, 2000, "the session reached the cap")
end)

-- P4a: the settings, as Settings.lua declares them. Each declaration is
-- checked, and src-tauri/resources/settings-schema.json is generated from
-- them for the app (stale fails --check, like the bridge fixtures).
local SCHEMA_JSON = ROOT .. "/src-tauri/resources/settings-schema.json"
local SCHEMA_FIELDS = {
    "id", "module", "section", "type", "default", "label", "help", "parent", "acts", "showHelp",
    -- For the types after bool (P4b-follow): a number's range, a choice's
    -- values, text's length.
    "min", "max", "step", "values", "maxLength",
    -- W1: a module's own on/off.
    "moduleSwitch",
}

local function declared()
    local c = client()
    c.login(nil)
    local s = c.ns().settings
    file(c.logout())
    return s
end

-- The addon's own check of a value against a declaration (U.prefs.fits).
local function declaredFits()
    local c = client()
    c.login(nil)
    local fits = c.ns().fits
    file(c.logout())
    return fits
end

local function json(v, indent)
    indent = indent or ""
    if type(v) == "string" then
        return '"' .. v:gsub('[%c"\\]', function(ch)
            return ({ ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\t"] = "\\t" })[ch]
                or string.format("\\u%04x", ch:byte())
        end) .. '"'
    elseif type(v) ~= "table" then
        return tostring(v)
    end
    local inner = indent .. "  "
    local parts = {}
    if v[1] ~= nil then
        for _, x in ipairs(v) do
            parts[#parts + 1] = inner .. json(x, inner)
        end
        return "[\n" .. table.concat(parts, ",\n") .. "\n" .. indent .. "]"
    end
    for _, k in ipairs(v.__order) do
        if v[k] ~= nil then
            parts[#parts + 1] = inner .. json(k) .. ": " .. json(v[k], inner)
        end
    end
    return "{\n" .. table.concat(parts, ",\n") .. "\n" .. indent .. "}"
end

scenario("settings_schema", function()
    local s = declared()
    eq(type(s.version), "number", "a schema version")
    local ids, options = {}, {}
    for i, o in ipairs(s.options) do
        local what = "option " .. i .. " (" .. tostring(o.id) .. ")"
        eq(type(o.id) == "string" and o.id:match("^%l%w*%.%a%w*$") ~= nil, true, what .. ": id is module.name")
        eq(ids[o.id], nil, what .. ": unique")
        eq(o.id:match("^(%l%w*)%."), o.module, what .. ": its id starts with its module")
        eq(o.type == "bool" or o.type == "number", true, what .. ": a bool or a number (the types the UI draws)")
        if o.type == "number" then
            eq(type(o.min) == "number" and type(o.max) == "number" and o.min < o.max, true, what .. ": a range")
            eq(type(o.step) == "number" and o.step > 0, true, what .. ": a step")
            eq(type(o.default) == "number" and o.default >= o.min and o.default <= o.max, true,
                what .. ": a default in range")
        else
            eq(type(o.default), "boolean", what .. ": a default")
        end
        eq(type(o.label) == "string" and #o.label > 0, true, what .. ": a label")
        eq(type(o.help) == "string" and #o.help > 10, true, what .. ": a help line")
        eq(type(o.section) == "string", true, what .. ": a section")
        eq(type(o.acts), "boolean", what .. ": says whether it acts")
        -- M1: an option that acts is off for a new player (P4 §8.1).
        if o.acts then
            eq(o.default, false, what .. ": acts, so it's off by default")
        end
        if o.parent then
            eq(ids[o.parent] ~= nil, true, what .. ": its parent is declared before it")
        end
        for k in pairs(o) do
            local known = false
            for _, f in ipairs(SCHEMA_FIELDS) do
                known = known or f == k
            end
            eq(known, true, what .. ": field " .. tostring(k) .. " is one the schema knows")
        end
        ids[o.id] = true
        local row = { __order = SCHEMA_FIELDS }
        for _, f in ipairs(SCHEMA_FIELDS) do
            row[f] = o[f]
        end
        options[#options + 1] = row
    end
    -- P4c: lints and presets name only declared options, with values that
    -- option takes (a lint's number condition is a { min, max } range).
    local byId = {}
    for _, o in ipairs(s.options) do
        byId[o.id] = o
    end
    local fits = declaredFits()
    -- A map's keys in order, for the JSON (an array has none).
    local function ordered(t)
        if type(t) ~= "table" or t[1] ~= nil then
            return t
        end
        local keys, out = {}, {}
        for k, v in pairs(t) do
            keys[#keys + 1] = k
            out[k] = ordered(v)
        end
        table.sort(keys)
        out.__order = keys
        return out
    end
    local lints, presets = {}, {}
    for i, l in ipairs(s.lints or {}) do
        local what = "lint " .. i .. " (" .. tostring(l.id) .. ")"
        eq(type(l.id) == "string" and type(l.say) == "string" and #l.say > 10, true, what .. ": an id and a line")
        eq(type(l.when) == "table" and next(l.when) ~= nil, true, what .. ": when it fires")
        for id, v in pairs(l.when) do
            eq(byId[id] ~= nil, true, what .. ": " .. id .. " is declared")
            if type(v) == "table" then
                eq(byId[id].type, "number", what .. ": a range is for a number")
            else
                eq(fits(byId[id], v), true, what .. ": " .. id .. " takes " .. tostring(v))
            end
        end
        lints[#lints + 1] = { __order = { "id", "when", "say" }, id = l.id, when = ordered(l.when), say = l.say }
    end
    for i, p in ipairs(s.presets or {}) do
        local what = "preset " .. i .. " (" .. tostring(p.id) .. ")"
        eq(type(p.id) == "string" and type(p.label) == "string", true, what .. ": an id and a label")
        for id, v in pairs(p.values) do
            eq(byId[id] ~= nil and fits(byId[id], v), true, what .. ": " .. id .. " = " .. tostring(v))
        end
        -- Every option has its row, except recording, which no preset
        -- touches (designer, P4c), and options that act, which are only
        -- turned on one at a time, never by a bundle (M1, security on #201);
        -- a number only goes with its parent on.
        for _, o in ipairs(s.options) do
            local has = p.values[o.id] ~= nil
            if o.id == "window.record" then
                eq(has, false, what .. ": leaves recording alone")
            elseif o.acts then
                eq(has, false, what .. ": leaves " .. o.id .. " alone (it acts)")
            elseif o.type == "bool" then
                eq(has, true, what .. ": has " .. o.id)
            end
        end
        presets[#presets + 1] = { __order = { "id", "label", "values" }, id = p.id, label = p.label, values = ordered(p.values) }
    end
    generated(SCHEMA_JSON, json({
        __order = { "version", "options", "lints", "presets" },
        version = s.version, options = options, lints = lints, presets = presets,
    }) .. "\n")
end)

-- P4a: every declared value is run. For each option, on and then off (the
-- rest at their defaults): set from the Options page, a short session
-- through it, the file it writes, and the value kept at the next login.
-- Then every option off at once, and every option on.
local function optionRows(c)
    local rows = {}
    for _, init in ipairs(c.global("Settings").categories[1].layout.inits) do
        if init.kind == "checkbox" or init.kind == "slider" then
            rows[#rows + 1] = init
        end
    end
    return rows
end

local function playAWhile(c)
    c.advance(10 * MINUTE)
    c.loot(2589, 2)
    c.combat(true)
    c.loot(117, 1)
    c.advance(30)
    c.combat(false)
    c.openMailbox()
    c.closeMailbox()
    -- P4d: the bags and the bank, where the Bags module draws.
    c.openBags()
    c.openBank()
    c.advance(1)
    c.closeBank()
    -- M1: a vendor, with junk to sell and gear to repair, where the Merchant
    -- module acts; nothing is ever used with no vendor open.
    c.loot(4865, 3)
    c.world.repair = 120
    c.openMerchant({ 117 })
    c.advance(2)
    c.closeMerchant()
    c.loot(4865, 1)
    c.advance(2)
    eq(#c.world.misused, 0, "nothing used outside a vendor")
    c.gainXp(800)
    c.startLogout()
    c.advance(5)
end

scenario("settings_matrix", function()
    local s = declared()
    local profiles = {}
    -- A bool on and off; a number (P4c) at its min, its max and its default.
    local function values(o)
        if o.type == "number" then
            return { o.min, o.max, o.default }
        end
        return { true, false }
    end
    for _, o in ipairs(s.options) do
        for _, value in ipairs(values(o)) do
            profiles[#profiles + 1] = { name = o.id .. "=" .. tostring(value), set = { [o.id] = value } }
        end
    end
    local allOn, allOff = { name = "all on", set = {} }, { name = "all off", set = {} }
    for _, o in ipairs(s.options) do
        local v = values(o)
        allOn.set[o.id], allOff.set[o.id] = v[1], v[2]
        if o.type == "number" then
            allOn.set[o.id], allOff.set[o.id] = o.max, o.min
        end
    end
    profiles[#profiles + 1] = allOn
    profiles[#profiles + 1] = allOff

    for _, p in ipairs(profiles) do
        local c = client()
        c.login(nil)
        c.advance(10)
        local rows = optionRows(c)
        eq(#rows, #s.options, p.name .. ": every option is on the Options page")
        for i, o in ipairs(s.options) do
            local want = p.set[o.id]
            if want ~= nil then
                rows[i].setting:SetValue(want)
                eq(rows[i].setting:GetValue(), want, p.name .. ": " .. o.id .. " takes the value")
            end
        end
        playAWhile(c)
        local text = c.logout()
        file(text)
        -- Next login: the same values, except recording, which turns itself
        -- off after the one session it recorded.
        local again = client()
        again.settings = c.settings
        again.login(text)
        local after = optionRows(again)
        for i, o in ipairs(s.options) do
            local want = p.set[o.id]
            if want == nil then
                want = o.default
            end
            if o.id == "window.record" then
                want = false
            end
            eq(after[i].setting:GetValue(), want, p.name .. ": " .. o.id .. " is kept")
        end
        file(again.logout())
    end
end)

-- P4b: the Settings slot as the app writes it: a numbered profile.
local function settingsSlot(profile, values, schema)
    local parts = {}
    for k, v in pairs(values) do
        parts[#parts + 1] = string.format("[%q] = %s", k, type(v) == "string" and string.format("%q", v) or tostring(v))
    end
    table.sort(parts)
    return string.format(
        "ForeverBuddyData_Settings = { schema = %d, stamp = 1790960000, profile = %d, values = { %s } }\n",
        schema or 1, profile, table.concat(parts, ", "))
end

-- P4c: the Map module. Display only, and nothing at all while it's off.
scenario("map", function()
    local c = client()
    c.login(nil)
    c.advance(5)
    local function row(id)
        for i, o in ipairs(declared().options) do
            if o.id == id then
                return optionRows(c)[i]
            end
        end
    end
    local map = c.global("WorldMapFrame")

    -- Off (the default): no frame is made, and moving changes nothing.
    eq(c.global("ForeverBuddyMinimapCoords"), nil, "no minimap frame while off")
    eq(c.global("ForeverBuddyMapCoords"), nil, "no world map frame while off")
    c.worldMap(true)
    c.move(true)
    eq(map:GetAlpha(), 1, "the map doesn't fade while off")
    c.move(false)
    c.worldMap(false)

    -- Under the minimap: where you are, five times a second; nothing in an
    -- instance.
    row("map.minimap").setting:SetValue(true)
    local mini = c.global("ForeverBuddyMinimapCoords")
    eq(mini.parent, c.global("Minimap"), "a child of the minimap")
    eq(mini.points[1][1] .. ">" .. mini.points[1][3], "TOP>BOTTOM", "under it")
    c.update(mini, 0.3)
    eq(mini.children[1].text, "41.2, 65.7", "where you are")
    c.enterInstance("The Deadmines", "party", 1, "Normal")
    c.update(mini, 0.3)
    eq(mini.children[1].text, "", "nothing in an instance")
    c.leaveInstance("Westfall")
    row("map.minimap").setting:SetValue(false)
    eq(mini.shown, false, "off: hidden")
    eq(c.update(mini, 0.3), false, "so its OnUpdate doesn't run")

    -- On the world map: you, and the cursor while it's over the map.
    row("map.coords").setting:SetValue(true)
    local coords = c.global("ForeverBuddyMapCoords")
    eq(coords.parent, map.ScrollContainer, "on the world map")
    c.worldMap(true)
    c.update(coords, 0.3)
    eq(coords.children[1].text, "You 41.2, 65.7", "you")
    c.world.mapCursor = { 0.3, 0.405 }
    c.update(coords, 0.1)
    eq(coords.children[1].text, "You 41.2, 65.7", "not again within 0.2 s")
    c.update(coords, 0.1)
    eq(coords.children[1].text, "You 41.2, 65.7     Cursor 30.0, 40.5", "and the cursor")
    c.world.mapCursor = nil
    c.worldMap(false)

    -- Fading: the map at the chosen alpha while you move, whole when you stop.
    row("map.fade").setting:SetValue(true)
    c.worldMap(true)
    c.move(true)
    eq(map:GetAlpha(), 0.5, "faded while moving (the default, 50%)")
    c.move(false)
    eq(map:GetAlpha(), 1, "whole when you stop")
    row("map.fadeAlpha").setting:SetValue(0.3)
    c.move(true)
    eq(map:GetAlpha(), 0.3, "at the chosen alpha")
    c.move(false)
    c.worldMap(false)
    -- Opening the map while already moving.
    c.world.moving = true
    c.worldMap(true)
    eq(map:GetAlpha(), 0.3, "faded as it opens")
    c.world.moving = false
    c.move(false)

    -- The window's slider, on the Map module's page (W1): dragging sets it,
    -- but not in combat.
    local slider = setting(c, "How much of the map shows while moving")
    eq(slider.slider, true, "a slider")
    eq(c.global("ForeverBuddyWindow").current, "map", "on the Map page")
    eq(slider.value.text, "30%", "its value, in percent")
    slider:SetValue(0.65, true)
    eq(row("map.fadeAlpha").setting:GetValue(), 0.65, "dragged to 65%")
    c.world.combat = true
    slider:SetValue(0.2, true)
    eq(row("map.fadeAlpha").setting:GetValue(), 0.65, "not in combat")
    eq(slider:GetValue(), 0.65, "the slider goes back")
    c.world.combat = false

    -- Off again: events unregistered and the map whole.
    row("map.fade").setting:SetValue(false)
    c.move(true)
    eq(map:GetAlpha(), 1, "off: no fading")
    c.move(false)
    local db = file(c.logout())
    eq(db.settings.values["map.fadeAlpha"], 0.65, "the file has the number")
    eq(db.settings.values["map.coords"], true, "and the rest")
    eq(c.settings.map.fadeAlpha, 0.65, "kept account-wide")

    -- From the app: a profile's number is applied when it's in range, and
    -- ignored when it isn't.
    local slot = client({ slots = { ["Data/Settings.lua"] = settingsSlot(3, { ["map.fadeAlpha"] = 0.25 }) } })
    slot.settings = c.settings
    slot.login(nil)
    eq(slot.global("ForeverBuddySettings").map.fadeAlpha, 0.25, "an approved 25% is applied")
    file(slot.logout())
    local bad = client({ slots = { ["Data/Settings.lua"] = settingsSlot(4, { ["map.fadeAlpha"] = 0.95 }) } })
    bad.settings = slot.settings
    bad.login(nil)
    eq(bad.global("ForeverBuddySettings").map.fadeAlpha, 0.25, "95% is out of range: left alone")
    file(bad.logout())

    -- A tick that fails is noted in the file once and stops, rather than
    -- failing five times a second.
    local broken = client()
    broken.settings = bad.settings
    broken.login(nil)
    row = function(id)
        for i, o in ipairs(declared().options) do
            if o.id == id then
                return optionRows(broken)[i]
            end
        end
    end
    row("map.minimap").setting:SetValue(true)
    local m = broken.global("ForeverBuddyMinimapCoords")
    m.children[1].SetText = function()
        error("the text broke")
    end
    broken.update(m, 0.3)
    eq(m.scripts.OnUpdate, nil, "the tick stops")
    db = wow.parse(broken.logout(), "ForeverBuddyDB")
    eq(tostring(db._meta.errors.map):find("the text broke", 1, true) ~= nil, true, "noted under map")
    eq(db._meta.window_errors, 1, "counted once")
    -- (A clean session after, so the runner's no-caught-errors check holds.)
    broken.login(nil)
    file(broken.logout())
end)

-- P4e: the Quest module's display part. Nothing here acts on a quest.
scenario("quest", function()
    local c = client()
    c.world.questlog = {
        { header = true },
        { questID = 176, title = "Wanted: Hogger", level = 11,
          objectives = { { text = "Linen Cloth: 3/8", type = "item", numFulfilled = 3, numRequired = 8 } } },
        { questID = 55012, title = "Skyborne Supplies", level = 14,
          objectives = { { text = "Gnoll slain: 2/6", type = "monster", numFulfilled = 2, numRequired = 6 } } },
    }
    c.login(nil)
    c.advance(5)
    local function row(id)
        for i, o in ipairs(declared().options) do
            if o.id == id then
                return optionRows(c)[i]
            end
        end
    end
    local log = c.global("QuestLogQuests_Update")
    local function titles()
        c.global("QuestLogQuests_Update")()
        local out = {}
        for _, b in ipairs(c.questTitles) do
            out[#out + 1] = b.Text.text
        end
        return table.concat(out, " / ")
    end
    local function questLines(id)
        local out = {}
        for _, line in ipairs(c.hover(id)) do
            if line:find("^Quest: ") then
                out[#out + 1] = line
            end
        end
        return table.concat(out, " / ")
    end
    local tracker = c.global("ObjectiveTrackerFrame")

    -- Off (the default): the log as the client draws it, no tooltip lines,
    -- no hook and no tracker frame.
    eq(titles(), "Wanted: Hogger / Skyborne Supplies", "the log as the client has it")
    eq(c.global("QuestLogQuests_Update"), log, "not hooked while off")
    eq(questLines(2589), "", "no quest line on the tooltip")
    eq(#tracker.children, 0, "no frame on the tracker")

    -- Levels, and the tag on a quest that's new in Forever (55000 and up).
    row("quest.levels").setting:SetValue(true)
    eq(titles(), "[11] Wanted: Hogger / [14] Skyborne Supplies", "levels before the titles")
    row("quest.newTag").setting:SetValue(true)
    eq(titles(), "[11] Wanted: Hogger / [14] Skyborne Supplies |cff40c0ff(New)|r", "the new one tagged")
    row("quest.levels").setting:SetValue(false)
    eq(titles(), "Wanted: Hogger / Skyborne Supplies |cff40c0ff(New)|r", "the tag alone")
    row("quest.newTag").setting:SetValue(false)
    eq(titles(), "Wanted: Hogger / Skyborne Supplies", "both off: the client's titles again")

    -- Tooltips: the quest that needs the item, and how far along.
    row("quest.tooltips").setting:SetValue(true)
    eq(questLines(2589), "Quest: Wanted: Hogger 3/8", "Linen Cloth is for Hogger")
    eq(questLines(6948), "", "the Hearthstone is for no quest")
    row("quest.tooltips").setting:SetValue(false)
    eq(questLines(2589), "", "off: gone")

    -- The tracker: see-through until the mouse is over it.
    row("quest.trackerFade").setting:SetValue(true)
    local fader = tracker.children[1]
    c.update(fader, 0.3)
    eq(tracker:GetAlpha(), 0.4, "see-through (the default, 40%)")
    c.world.trackerMouse = true
    c.update(fader, 0.3)
    eq(tracker:GetAlpha(), 1, "whole under the mouse")
    c.world.trackerMouse = false
    row("quest.trackerAlpha").setting:SetValue(0.25)
    c.update(fader, 0.3)
    eq(tracker:GetAlpha(), 0.25, "at the chosen alpha")
    row("quest.trackerFade").setting:SetValue(false)
    eq(tracker:GetAlpha(), 1, "off: whole")
    eq(c.update(fader, 0.3), false, "and its frame doesn't tick")

    local db = file(c.logout())
    eq(db.settings.values["quest.trackerAlpha"], 0.25, "the file has the number")
    eq(c.settings.quest.trackerAlpha, 0.25, "kept account-wide")
end)

-- W1: the module contract. Each module's own switch turns all of it off at
-- once, with no /reload, and on again as it was; its options are kept. Off,
-- a module draws nothing, shows no frame and adds no tooltip line.
scenario("modules", function()
    local c = client()
    c.world.questlog = {
        { questID = 176, title = "Wanted: Hogger", level = 11,
          objectives = { { text = "Linen Cloth: 3/8", type = "item", numFulfilled = 3, numRequired = 8 } } },
    }
    c.world.bags[0].slots[3] = { id = 4865, count = 2 }
    c.login(nil)
    c.advance(5)
    local w
    local function module(id, on)
        c.slash("/fb")
        w = c.global("ForeverBuddyWindow")
        c.click(w.mods[id])
        if (w.modSwitch.checked and true or false) ~= on then
            c.click(w.modSwitch)
        end
        eq(w.modSwitch.checked and true or false, on, id .. (on and " on" or " off"))
    end
    local function altLines()
        local n = 0
        for _, line in ipairs(c.hover(2589)) do
            if line:find("Quest: ", 1, true) or line:find("Velyra", 1, true) then
                n = n + 1
            end
        end
        return n
    end

    -- Every module on by default, so nobody's choices went dark on update.
    c.slash("/fb")
    w = c.global("ForeverBuddyWindow")
    for id, b in pairs(w.mods) do
        eq(hex(b.dot.layout.SetVertexColor), "#ff7a2e", id .. " is on")
    end

    -- Map: the minimap's coordinates go and come back.
    c.click(setting(c, "Coordinates under the minimap"))
    local mini = c.global("ForeverBuddyMinimapCoords")
    eq(mini.shown, true, "map: coordinates shown")
    module("map", false)
    eq(mini.shown, false, "map off: hidden")
    eq(setting(c, "Coordinates under the minimap").checked, true, "its option kept")
    module("map", true)
    eq(mini.shown, true, "map on: back")

    -- Quests: the log and the tooltip.
    c.click(setting(c, "Quest levels in the quest log"))
    c.click(setting(c, "Quest progress on item tooltips"))
    c.global("QuestLogQuests_Update")()
    eq(c.questTitles[1].Text.text, "[11] Wanted: Hogger", "quests: the level")
    eq(altLines(), 1, "and the tooltip line")
    module("quest", false)
    c.global("QuestLogQuests_Update")()
    eq(c.questTitles[1].Text.text, "Wanted: Hogger", "quests off: the client's title")
    eq(altLines(), 0, "and no tooltip line")
    module("quest", true)

    -- Bags: the coin on junk.
    c.openBags()
    c.advance(1)
    c.click(setting(c, "Junk in your bags"))
    local bag = c.global("ContainerFrame1")
    eq(extrasOf(bag.buttons[3]).junk, true, "bags: a coin on junk")
    module("bags", false)
    eq(extrasOf(bag.buttons[3]).junk, nil, "bags off: gone from the open bags")
    module("bags", true)
    eq(extrasOf(bag.buttons[3]).junk, true, "bags on: back")

    -- Merchant (it acts): off, a vendor sells nothing even with selling
    -- turned on; back on, it sells.
    c.click(setting(c, "Sell grey items"))
    module("merchant", false)
    c.openMerchant({ 117 })
    c.advance(2)
    eq(#c.world.sold, 0, "merchant off: nothing sold")
    c.closeMerchant()
    module("merchant", true)
    c.openMerchant({ 117 })
    c.advance(2)
    eq(#c.world.sold > 0, true, "merchant on: the grey pelts sell")
    c.closeMerchant()

    -- From the app: a profile can turn a module off like any setting.
    local db = file(c.logout())
    eq(db.settings.values["map.enabled"], true, "the file has each module's switch")
    local slot = client({ slots = { ["Data/Settings.lua"] = settingsSlot(9, { ["map.enabled"] = false }) } })
    slot.settings = c.settings
    slot.login(nil)
    eq(slot.global("ForeverBuddyMinimapCoords") == nil or not slot.global("ForeverBuddyMinimapCoords").shown,
        true, "an approved 'Map off' leaves the coordinates hidden")
    eq(slot.global("ForeverBuddySettings").modules.map, false, "and turned it off")
    file(slot.logout())
end)

-- P4b-follow (security): what the addon accepts from a profile, for every
-- type a declaration can have, not only today's bools.
scenario("settings_fits", function()
    local c = client()
    c.login(nil)
    local fits = c.ns().fits
    local num = { type = "number", min = 0, max = 1 }
    local choice = { type = "enum", values = { "left", "right" } }
    local text = { type = "string", maxLength = 5 }
    for _, case in ipairs({
        { { type = "bool" }, true, true }, { { type = "bool" }, "true", false }, { { type = "bool" }, 1, false },
        { num, 0, true }, { num, 1, true }, { num, 0.5, true },
        { num, -0.1, false }, { num, 1.5, false }, { num, "0.5", false },
        { num, 0 / 0, false }, { { type = "number" }, math.huge, false },
        { choice, "left", true }, { choice, "up", false }, { choice, 1, false },
        { text, "hello", true }, { text, "hello!", false }, { text, "a\nb", false }, { text, true, false },
        { { type = "string" }, string.rep("x", 200), true }, { { type = "string" }, string.rep("x", 201), false },
        { { type = "color" }, "ff0000", false }, -- no other type is accepted
    }) do
        eq(fits(case[1], case[2]), case[3], case[1].type .. " " .. tostring(case[2]))
    end
    file(c.logout())
end)

-- P4b: a profile approved in the app is applied once, at login, through
-- each option's own set. After that the player's own toggles win until a
-- newer profile comes. The file says every option's value and the last
-- profile applied, so the app knows what the game has.
scenario("settings_slot", function()
    local coach
    for i, o in ipairs(declared().options) do
        if o.id == "window.coach" then
            coach = i
        end
    end
    local function values(c)
        local db = file(c.logout())
        return db.settings.values, db.settings.profile, db
    end

    local c = client({ slots = { ["Data/Settings.lua"] = settingsSlot(5, {
        ["window.coach"] = true,
        ["window.minimap"] = false,
        ["window.card"] = "yes", -- not this option's type
        ["window.nope"] = true, -- not an option this addon has
    }) } })
    c.login(nil)
    c.advance(10)
    eq(optionRows(c)[coach].setting:GetValue(), true, "the coach is on")
    eq(c.chat[#c.chat], PREFIX .. "new settings from the app are on. Keep them or go back in Forever Buddy.",
        "it says so")
    local v, profile, db = values(c)
    eq(profile, 5, "the file says which profile it applied")
    eq(v["window.coach"], true, "and the values: the coach")
    eq(v["window.minimap"], false, "the minimap button")
    eq(v["window.card"], true, "a value of the wrong type is left alone")
    eq(v["window.nope"], nil, "an option this addon doesn't have isn't")
    eq(db.bridge.Settings.stamp, 1790960000, "the slot's receipt")

    -- Your own toggle wins: the same profile isn't applied again.
    c.login(nil)
    optionRows(c)[coach].setting:SetValue(false)
    c.logout()
    c.chat = {}
    c.login(nil)
    eq(optionRows(c)[coach].setting:GetValue(), false, "the coach stays off")
    eq(#c.chat == 0 or not c.chat[#c.chat]:find("new settings", 1, true), true, "nothing said")
    v, profile = values(c)
    eq(profile, 5, "still profile 5")

    -- A newer profile is applied; one from a newer app (schema) isn't.
    local newer = client({ slots = { ["Data/Settings.lua"] = settingsSlot(6, { ["window.coach"] = true }) } })
    newer.settings = c.settings
    newer.login(nil)
    eq(optionRows(newer)[coach].setting:GetValue(), true, "profile 6 turns it back on")
    v, profile = values(newer)
    eq(profile, 6, "profile 6")
    local future = client({ slots = { ["Data/Settings.lua"] = settingsSlot(7, { ["window.coach"] = false }, 2) } })
    future.settings = newer.settings
    future.login(nil)
    eq(optionRows(future)[coach].setting:GetValue(), true, "a newer slot schema isn't read")
    v, profile = values(future)
    eq(profile, 6, "so nothing new was applied")
end)

-- U1d: the coach strip, the plan tracker and the logout card are placed in
-- Edit Mode. Opening it shows each one (built if it never has been) with
-- its name, drags them, and saves where they're dropped; closing it hides
-- the previews again. Outside Edit Mode they don't move.
scenario("edit_mode", function()
    local c = client({ slots = { ["Data/Plan.lua"] = PLAN } })
    c.login(nil)
    c.advance(10)
    eq(c.global("ForeverBuddyCoachFrame"), nil, "the coach isn't built (it's off)")

    c.editMode(true)
    local frames = {
        coach = c.global("ForeverBuddyCoachFrame"),
        plan = c.global("ForeverBuddyPlanFrame"),
        card = c.global("ForeverBuddyCardFrame"),
    }
    local names = {
        coach = "Forever Buddy: session coach",
        plan = "Forever Buddy: tonight's plan",
        card = "Forever Buddy: logout card",
    }
    for key, f in pairs(frames) do
        eq(f ~= nil, true, key .. " is built for Edit Mode")
        eq(f.shown, true, key .. " shows while placing")
        eq(f.editOverlay.shown, true, key .. " is highlighted")
        eq(f.editOverlay.label.text, names[key], key .. " is named")
        eq(f:IsMouseEnabled(), true, key .. " takes the mouse to be dragged")
        -- Dragged and dropped somewhere else: kept.
        f.scripts.OnDragStart(f)
        eq(f.layout.StartMoving ~= nil, true, key .. " moves")
        f:ClearAllPoints()
        f:SetPoint("CENTER", c.global("UIParent"), "CENTER", 100, -50)
        f.scripts.OnDragStop(f)
        local at = c.global("ForeverBuddySettings")[key .. "At"]
        eq(at and table.concat(at, ","), "CENTER,100,-50", key .. "'s place is saved")
    end

    c.editMode(false)
    for key, f in pairs(frames) do
        eq(f.shown, false, key .. "'s preview goes away")
        eq(f.editOverlay.shown, false, key .. "'s highlight goes")
        f.layout.StartMoving = nil
        f.scripts.OnDragStart(f)
        eq(f.layout.StartMoving, nil, key .. " doesn't move outside Edit Mode")
    end
    eq(frames.coach:IsMouseEnabled(), false, "the coach takes no clicks outside it")
    eq(frames.card:IsMouseEnabled(), false, "nor does the card (only its ×)")

    -- Shown for real later, they're where they were left.
    c.click(setting(c, "Session coach"))
    local f = c.global("ForeverBuddyCoachFrame")
    eq(f.shown, true, "the coach on")
    eq(f.points[1][1] .. "," .. f.points[1][4] .. "," .. f.points[1][5], "CENTER,100,-50", "where it was placed")

    -- A frame shown before Edit Mode stays shown after it.
    c.editMode(true)
    c.editMode(false)
    eq(f.shown, true, "the coach stays on")

    -- Next login: built where they were placed.
    local text = c.logout()
    local again = client({ slots = { ["Data/Plan.lua"] = PLAN } })
    again.settings = c.settings
    again.login(text)
    again.slash("/fb plan")
    again.click(again.global("ForeverBuddyWindow").panes[1].tracker)
    local plan = again.global("ForeverBuddyPlanFrame")
    eq(plan.points[1][1] .. "," .. plan.points[1][4], "CENTER,100", "the plan tracker too, next time")
    file(again.logout())
end)

-- B4: Sync reloads the UI, ReloadUI() first in the click or keypress, and
-- _meta.sync says what came of it. Refused once, it's "Type /reload" for good.
scenario("sync", function()
    local c = client()
    c.login(nil)
    c.advance(5)
    c.slash("/fb")
    local win = c.global("ForeverBuddyWindow")
    eq(win.syncButton.shown ~= false, true, "the footer has a Sync button")
    eq(win.sync.text, "", "and no typed fallback beside it")

    -- The click: the client reloads, and the next file says it did.
    c.click(win.syncButton)
    eq(c.world.requests.reload, 1, "the click calls ReloadUI")
    local db = wow.parse(c.reload(), "ForeverBuddyDB")
    eq(db._meta.sync.result, "pending", "asked for, written by the reload itself")
    db = wow.parse(c.logout(), "ForeverBuddyDB")
    eq(db._meta.sync.result, "reloaded", "the load after it was a reload")
    eq(c.settings.syncAsked, nil, "and the ask is cleared")

    -- /fb sync does the same from the keyboard. A plain login after it
    -- (no reload came) is "unknown".
    c.login(nil)
    c.slash("/fb sync")
    eq(c.world.requests.reload, 2, "/fb sync calls ReloadUI")
    c.logout()
    c.login(nil)
    db = wow.parse(c.logout(), "ForeverBuddyDB")
    eq(db._meta.sync.result, "unknown", "a login, not a reload, after the ask")

    -- Refused: the chat says so, the footer goes back to typing, for good.
    c.login(nil)
    c.world.reloadBlocked = true
    c.chat = {}
    c.slash("/fb")
    win = c.global("ForeverBuddyWindow")
    c.click(win.syncButton)
    eq(c.chat[#c.chat], "|cffffd100Forever Buddy:|r the game didn't let the addon reload. Type /reload to sync.", "the chat says why")
    eq(win.syncButton.shown, false, "the button goes")
    eq(win.sync.text, "Type /reload to sync", "the typed way is back")
    db = wow.parse(c.logout(), "ForeverBuddyDB")
    eq(db._meta.sync.result, "blocked", "the file says it was refused")
    c.login(nil)
    c.slash("/fb sync")
    eq(c.world.requests.reload, 3, "refused, /fb sync no longer calls it")
    eq(c.chat[#c.chat], "|cffffd100Forever Buddy:|r type /reload to sync.", "it says to type it")
    c.slash("/fb")
    eq(c.global("ForeverBuddyWindow").syncButton.shown, false, "still gone after a login")
    file(c.logout())
end)

-- U1d: a client without Edit Mode keeps the old way (drag any time), and
-- one without the vertical Settings layout gets the canvas page.
scenario("settings_fallbacks", function()
    local c = client({ noEditMode = true, canvasOnly = true })
    c.login(nil)
    c.advance(10)
    local coach = setting(c, "Session coach")
    c.click(coach)
    local f = c.global("ForeverBuddyCoachFrame")
    eq(f:IsMouseEnabled(), true, "no Edit Mode: the coach takes the mouse")
    f.scripts.OnDragStart(f)
    eq(f.layout.StartMoving ~= nil, true, "and drags any time")
    f:ClearAllPoints()
    f:SetPoint("TOPLEFT", c.global("UIParent"), "TOPLEFT", 80, -120)
    f.scripts.OnDragStop(f)
    eq(table.concat(c.global("ForeverBuddySettings").coachAt, ","), "TOPLEFT,80,-120", "saved as before")

    local options = c.global("Settings").categories[1]
    eq(options.name, "Forever Buddy", "still in Esc › Options › AddOns")
    eq(options.vertical, nil, "as the canvas")
    eq(#options.frame.checks, 30, "with every toggle")
    eq(options.frame.checks[4].shown, true, "combat hiding shows: the coach is on")
    file(c.logout())
end)

-- Runner ---------------------------------------------------------------------

local check = CHECK
local failures, stale = 0, 0
-- H3: --coverage notes what the scenarios ran of ForeverBuddy.lua.
local coverage = flag("--coverage") and require("coverage")
if coverage then
    coverage.start()
end

-- R2a: every error the addon catches and counts lands in _meta under one of
-- these. No scenario expects one, so after each scenario every client's
-- file must have none of them.
local CAUGHT = { "errors", "error_counts", "tooltip_errors", "plan_errors", "list_errors", "session_errors", "window_errors" }

local function caught(c)
    local okSave, saved = pcall(c.save)
    local db = okSave and saved and wow.parse(saved, "ForeverBuddyDB")
    local meta = type(db) == "table" and db._meta
    if type(meta) ~= "table" then
        return nil
    end
    for _, k in ipairs(CAUGHT) do
        if meta[k] ~= nil then
            return "_meta." .. k .. " = " .. serialize("x", meta[k])
        end
    end
end

-- H6: budgets, in Lua VM instructions (wow.lua's `metered`). A plain Lua
-- 5.1 interpreter, like the game's, runs very roughly 100M of them a
-- second, so a handler gets about 1.5 ms and a frame about 4 ms of the
-- 16 ms a 60 fps frame has. Lua 5.1 counts run ~1.6 times LuaJIT's; these
-- fit both.
HANDLER_BUDGET = 150000
FRAME_BUDGET = 400000
-- Logging out runs these once, while the player is leaving and nothing
-- moves on screen, and they do the session's whole bookkeeping (the card's
-- tally, the last snapshot, the file). A capped session with full bags
-- takes them near 140k in Lua 5.1, so they get their own, larger budget,
-- still under the ~300k the card cost before H4b's fix.
LOGOUT_BUDGET = 220000
local LOGOUT = { PLAYER_CAMPING = true, PLAYER_LEAVING_WORLD = true, PLAYER_LOGOUT = true }

local function overBudget(c)
    for _, call in ipairs(c.costs) do
        local budget = LOGOUT[call.name] and LOGOUT_BUDGET or HANDLER_BUDGET
        if call.n > budget then
            return ("%s took %d instructions (budget %d)"):format(call.name, call.n, budget)
        end
    end
    if c.worstFrame > FRAME_BUDGET then
        return ("a frame took %d instructions (budget %d)"):format(c.worstFrame, FRAME_BUDGET)
    end
end

for _, s in ipairs(scenarios) do
    clients = {}
    wow.cost.scenario = s.name
    local ok, text = xpcall(s.fn, debug.traceback)
    if ok then
        for _, c in ipairs(clients) do
            if #c.errors > 0 then
                ok, text = false, "errors escaped the addon:\n  " .. table.concat(c.errors, "\n  ")
                break
            end
            -- Raised inside the addon's own pcalls too, so it counts even
            -- when the addon swallowed it.
            if #c.unmodelled > 0 then
                ok, text = false, "widget methods wow.lua doesn't model (a typo, or model it): "
                    .. table.concat(c.unmodelled, ", ")
                break
            end
            local found = caught(c)
            if found then
                ok, text = false, "the addon caught an error:\n  " .. found
                break
            end
            local slow = overBudget(c)
            if slow then
                ok, text = false, "over the time budget: " .. slow
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

-- Forever fidelity (H2) ------------------------------------------------------
--
-- Every API the addon calls (through read(), which is how it reaches the
-- client) needs a row in forever.lua and a mock here; fails otherwise. Then
-- a printout: how much of that is modelled, confirmed and exercised.

do
    local src = wow.readFile(ADDON_DIR .. "ForeverBuddy.lua")
    local apis, events = {}, {}
    for name in src:gmatch('read%("([%w_%.]+)"') do
        apis[name] = true
    end
    -- read(GetMoney), read(Minimap.GetWidth, Minimap); not a local's
    -- method (read(pos.GetXY, pos)).
    for name in src:gmatch("read%(([%a_][%w_%.]*)") do
        if name:match("^%u") or name == "date" then
            apis[name] = true
        end
    end
    for name in src:gmatch("\nhandlers%.([%u_]+) = ") do
        events[name] = true
    end

    local probe = client()
    probe.login(nil)
    local function mocked(name)
        local v = probe.global(name:match("^[^%.]+"))
        for part in name:gmatch("%.([^%.]+)") do
            v = type(v) == "table" and v[part] or nil
        end
        return v ~= nil
    end

    local names, problems = {}, {}
    for name in pairs(apis) do
        names[#names + 1] = name
    end
    table.sort(names)
    local rowed, phased, reported, assumed, called, unmocked, uncalled = 0, 0, 0, 0, 0, {}, {}
    for _, name in ipairs(names) do
        local row = wow.FOREVER[name]
        if not row then
            problems[#problems + 1] = name .. " has no row in forever.lua (what does Forever return while loading and torn?)"
        else
            rowed = rowed + 1
            if row.loading ~= nil or row.torn ~= nil then
                phased = phased + 1
            end
            if row.from == "report" or row.from == "recording" then
                reported = reported + 1
            else
                assumed = assumed + 1
            end
        end
        if not mocked(name) then
            unmocked[#unmocked + 1] = name
        end
        if wow.calls[name] then
            called = called + 1
        else
            uncalled[#uncalled + 1] = name
        end
    end
    for name in pairs(wow.FOREVER) do
        if not apis[name] then
            problems[#problems + 1] = "forever.lua has a row for " .. name .. ", which the addon no longer calls"
        end
    end
    for _, name in ipairs(unmocked) do
        problems[#problems + 1] = name .. " isn't mocked in wow.lua"
    end

    -- SIM5a: once a real client's census is committed, every API the addon
    -- calls must be in it. (tools/addon-test/census.lua has the full report.)
    local censusText = (function()
        local f = io.open(dir .. "/../probe-addon/results/census.lua", "rb")
        if not f then
            return nil
        end
        local s = f:read("*a")
        f:close()
        return s
    end)()
    local censusNote = "no census yet (/fbprobe census, see tools/addon-test/census.lua)"
    if censusText then
        local censusLib = require("census")
        local cen, why = censusLib.load(censusText, wow.parse)
        if not cen then
            problems[#problems + 1] = "tools/probe-addon/results/census.lua: " .. why
        else
            local b = cen.build or {}
            censusNote = ("checked against the census of build %s"):format(tostring(b.build))
            for _, name in ipairs(names) do
                if censusLib.kind(cen, name) == nil then
                    problems[#problems + 1] = name .. " isn't in the real Forever client (census, build "
                        .. tostring(b.build) .. ")"
                end
            end
        end
    end

    -- SIM5d: Blizzard's own API docs, where the CASC extract has put them in
    -- the app's cache (never on CI). Every handled event must be documented.
    -- An API in neither the docs nor Blizzard's UI code is only a warning:
    -- old C functions are often undocumented, so the census decides.
    local docsNote = "no Blizzard API docs here (the CASC extract, see tools/addon-test/apidocs.lua)"
    local apidocs = require("apidocs")
    local docsDir = apidocs.find()
    if docsDir then
        local index = apidocs.load(docsDir)
        local nowhere = {}
        for _, name in ipairs(names) do
            if not apidocs.where(index, name) then
                nowhere[#nowhere + 1] = name
            end
        end
        local documented = 0
        for name in pairs(events) do
            if index.events[name] then
                documented = documented + 1
            else
                problems[#problems + 1] = name .. " isn't an event in Blizzard's API docs"
            end
        end
        docsNote = ("Blizzard's API docs: %d handled events documented; APIs in neither the docs nor its UI code: %s"):format(
            documented, #nowhere > 0 and table.concat(nowhere, ", ") or "none")
    end

    local handled, fired, unfired = 0, 0, {}
    for name in pairs(events) do
        handled = handled + 1
        if wow.fired[name] then
            fired = fired + 1
        else
            unfired[#unfired + 1] = name
        end
    end
    table.sort(unfired)
    local nConfirmed = 0
    for _ in pairs(confirmed) do
        nConfirmed = nConfirmed + 1
    end

    print(("Forever fidelity: %d APIs the addon calls, %d mocked, %d with a row in forever.lua"):format(
        #names, #names - #unmocked, rowed))
    print(("  %d answer differently while loading or torn; %d rows from a report or recording, %d assumed"):format(
        phased, reported, assumed))
    print(("  %d confirmed by a real recording%s"):format(nConfirmed,
        #REAL == 0 and " (none yet: README › Recordings)" or ""))
    print("  API names: " .. censusNote)
    print("  " .. docsNote)
    print(("  %d called by the scenarios; never called: %s"):format(called,
        #uncalled > 0 and table.concat(uncalled, ", ") or "none"))
    print(("Events: %d handled, %d fired by the scenarios; never fired: %s"):format(handled, fired,
        #unfired > 0 and table.concat(unfired, ", ") or "none"))
    table.sort(problems)
    for _, p in ipairs(problems) do
        failures = failures + 1
        print("FAIL fidelity: " .. p)
    end
end

if coverage then
    coverage.stop()
    local report = coverage.report(wow.readFile(ADDON_DIR .. "ForeverBuddy.lua"))
    print(report)
    -- On a PR, the job's summary page shows it too.
    local summary = os.getenv("GITHUB_STEP_SUMMARY")
    if summary and summary ~= "" then
        local f = io.open(summary, "a")
        if f then
            f:write("### Addon coverage\n\n```\n" .. report .. "```\n")
            f:close()
        end
    end
end

-- H6: the costliest handlers and frame of the run, in Lua VM instructions.
do
    local rows = {}
    for name, w in pairs(wow.cost.worst) do
        rows[#rows + 1] = { name = name, n = w.n, scenario = w.scenario }
    end
    table.sort(rows, function(a, b)
        return a.n > b.n or (a.n == b.n and a.name < b.name)
    end)
    local lines = {}
    if #rows > 0 then
        lines[1] = ("Handler cost (%s instructions; budget %d per handler, %d at logout, %d per frame):"):format(
            jit and "LuaJIT" or "Lua 5.1", HANDLER_BUDGET, LOGOUT_BUDGET, FRAME_BUDGET)
        for i = 1, math.min(8, #rows) do
            local r = rows[i]
            lines[#lines + 1] = ("  %8d  %s  (%s)"):format(r.n, r.name, r.scenario or "?")
        end
        local f = wow.cost.frame
        lines[#lines + 1] = ("  costliest frame: %d (%s)"):format(f.n, f.scenario or "?")
        -- On a PR, the job's summary page shows it too.
        local summary = os.getenv("GITHUB_STEP_SUMMARY")
        local out = summary and summary ~= "" and io.open(summary, "a")
        if out then
            out:write("### Handler cost\n\n```\n" .. table.concat(lines, "\n") .. "\n```\n")
            out:close()
        end
    else
        lines[1] = "Handler cost: not counted (another debug hook is set, e.g. --coverage)"
    end
    print(table.concat(lines, "\n"))
end

if stale > 0 then
    print("Run `lua5.1 tools/addon-test/run.lua` and commit the fixtures.")
end
if failures > 0 or stale > 0 then
    os.exit(1)
end
