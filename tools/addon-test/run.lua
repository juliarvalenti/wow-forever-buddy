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
        "Data/Tooltip1.lua, Data/Tooltip2.lua, Data/Plan.lua, Data/Briefing.lua, Data/Lists.lua, ForeverBuddy.lua", "files")
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

    c.slash("/fb plan")
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
    local g = again.global("ForeverBuddyPlanFrame")
    eq(g.footer.text, "All 4 done · from Forever Buddy", "progress after login")
    again.slash("/fb plan")
    eq(g.shown, false, "/fb plan again hides it")

    -- Another character's plan isn't this one's.
    local other = client({
        slots = { ["Data/Plan.lua"] = PLAN },
        character = { name = "Velyra", surname = "Duskmane", realm = "Classic Beta PvP 2", guid = "Player-1" },
    })
    other.login(nil)
    eq(#other.chat, 0, "no plan, no chat")
    other.slash("/fb plan")
    eq(other.global("ForeverBuddyPlanFrame").footer.text, "No plan for this character yet.", "empty")
    return text
end)

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
    quiet.slash("/fb")
    eq(quiet.chat[2], PREFIX .. "/fb plan shows tonight's plan; /fb list your lists; /fb errands what to send; "
        .. "/fb coach this session's strip; /fb card off hides the logout card; "
        .. "/fb brief repeats the login briefing; /fb brief off turns it off.", "/fb help")

    -- Not in combat.
    local fighting = client({ slots = { ["Data/Briefing.lua"] = briefingSlot(NOTES) } })
    fighting.login(nil)
    fighting.world.combat = true
    fighting.advance(5)
    eq(#fighting.chat, 0, "nothing in combat")
    return text
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

    -- /fb list: anywhere, every list, until /fb list again.
    c.slash("/fb list")
    eq(f.shown, true, "/fb list")
    eq(f.point[2], c.global("UIParent"), "undocked")
    eq(f.meta.text, "Tailoring · Raid night ]] ||cffff0000x", "every list")
    -- A vendor docks it for a while; closing it puts it back.
    c.openMerchant({})
    eq(f.point[2], c.global("MerchantFrame"), "docked at the vendor")
    c.closeMerchant()
    eq(f.shown, true, "still open")
    eq(f.point[2], c.global("UIParent"), "back where it was")
    c.slash("/fb list")
    eq(f.shown, false, "/fb list again")

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
    c.closeMailbox()
    eq(e.shown, false, "closes with the mailbox")

    c.slash("/fb errands")
    eq(c.chat[1], PREFIX .. "Runecloth ×20 to " .. SELA .. " (20 in bags) · Sela's Tailoring list", "/fb errands")
    eq(#c.chat, 2, "one line each")
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
    local g = sela.global("ForeverBuddyListFrame")
    eq(g.rows[1].right.text, "Thrandor can send 20", "coming from Thrandor")
    sela.openMailbox()
    eq(sela.global("ForeverBuddyErrandFrame"), nil, "no errands panel")
    sela.slash("/fb errands")
    eq(sela.chat[1], PREFIX .. "no errands for this character.", "says so")

    -- Without the slot: a vendor shows nothing, /fb list says there are none.
    local none = client()
    none.login(nil)
    none.openMerchant({ 117 })
    eq(none.global("ForeverBuddyListFrame"), nil, "no panel without lists")
    none.slash("/fb list")
    eq(none.global("ForeverBuddyListFrame").footer.text, "No lists yet. Make one in Forever Buddy.", "empty")
    return text
end)

-- Known recipes (C1): read from this character's own profession window,
-- learned recipes only, item ids and skill; carried forward until the next
-- look, and dropped with the profession.
local TAILORING = {
    name = "Tailoring",
    skill = 34,
    max = 75,
    recipes = {
        { id = 2387, learned = true, out = 2568 },
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

    -- Not opened this session: the last scan carries forward.
    local again = client()
    again.login(text)
    eq(table.concat(file(again.logout()).snapshot.recipes.Tailoring.made, ","), "2568,2572", "carried forward")

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
