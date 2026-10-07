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

M.ITEMS = {
    [25] = "Worn Shortsword",
    [117] = "Tough Jerky",
    [2488] = "Gladius",
    [2589] = "Linen Cloth",
    [6948] = "Hearthstone",
    [14047] = "Runecloth",
    [10001] = "Shadowcraft Cap",
    [10002] = "Coif of Elements",
    [10003] = "Band of the Unicorn",
    [10004] = "Lionheart Helm",
    [10005] = "Felcloth Hood",
}

-- Gear for the upgrade hint: GetItemInfo's ilvl, required level, equip
-- location, item class and subclass (4 = armour; 1 cloth, 2 leather,
-- 3 mail, 4 plate, 0 misc).
M.GEAR = {
    [10001] = { 63, 58, "INVTYPE_HEAD", 4, 2 },
    [10002] = { 61, 56, "INVTYPE_HEAD", 4, 3 },
    [10003] = { 55, 50, "INVTYPE_FINGER", 4, 0 },
    [10004] = { 63, 50, "INVTYPE_HEAD", 4, 4 },
    [10005] = { 40, 35, "INVTYPE_HEAD", 4, 1 },
}

M.QUESTS = { [176] = "Wanted: Hogger" }

function M.link(id)
    return "|cffffffff|Hitem:" .. id .. "::::::::12:::::|h[" .. (M.ITEMS[id] or "?") .. "]|h|r"
end

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
--   addon      path to ForeverBuddy.lua (its TOC is read from the same folder)
--   slots      { ["Data/Tooltip1.lua"] = source } in place of the bundled stubs
--   character  { name, surname, realm, guid }; the default has a surname,
--              as Forever characters do (probe run 1)
--   api        { Name = function or false } overrides; false removes it
--   secret, throw, missing   "all" (all but the clock) or a set of API names
--   unknown_events           events RegisterEvent refuses, as on Forever
--   secret_args              a set of events whose arguments arrive secret
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

    -- The character's game state, which the APIs report. Scenarios change it
    -- through the helpers below, which fire the events the client would.
    local world = {
        money = 25000,
        xp = 1200,
        level = 12,
        zone = "Elwynn Forest",
        subzone = "Goldshire",
        instance = false,
        repair = 0,
        xp_max = 8800,
        rested = 674,
        played = 19551, -- seconds played at EPOCH, in total
        played_level = 474, -- and at this level
        -- bag -> { size, slots[slot] = { id, count } }; 0 is the backpack,
        -- -1 the bank and 6 the first bank tab (readable only at the bank).
        bags = {
            [0] = { size = 16, slots = { [1] = { id = 6948, count = 1 }, [2] = { id = 2589, count = 4 } } },
            [-1] = { size = 28, slots = { [1] = { id = 14047, count = 20 } } },
            [6] = { size = 98, slots = {} },
        },
        bank_tabs = { 6 },
        equipped = { [16] = 25 },
        guild = { name = "Hearthguard", rank = "Officer" },
        professions = {
            { name = "Herbalism", skill = 60, max = 75, line = 182 },
            -- A specialization index (GetProfessionInfo's 9th return); -1 is none.
            { name = "Tailoring", skill = 34, max = 75, line = 197, spec = 2 },
            nil,
            nil,
            { name = "Cooking", skill = 29, max = 75, line = 185 },
        },
        -- { sender, subject, money, cod, days, items = { { id, count } } }
        inbox = {},
        lockouts = {},
        -- Items whose info the client hasn't cached: GetItemInfo returns nil
        -- until RequestLoadItemDataByID loads it.
        uncached = {},
        requests = { played = 0, raid = 0, items = 0 },
        combat = false, -- InCombatLockdown
        shift = false, -- IsShiftKeyDown
        bound = {}, -- id -> the bind line its tooltip shows ("Soulbound")
        quests_done = { 783, 7 }, -- GetAllCompletedQuestIDs, in the client's order
        pos = { 0.41234, 0.65678 }, -- on map 1429 (Elwynn), outside instances
        npc = nil, -- { name, player } the quest window is open on
    }
    client.world = world

    -- Something the client does a moment later (a server reply).
    local function later(fn)
        table.insert(state.timers, { at = client.now + 1, fn = fn })
    end

    local function readable(bag)
        return bag >= 0 and bag <= 5 or world.bank_open
    end

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
            elseif unit == "npc" and world.npc then
                return world.npc.name
            end
        end,
        UnitIsPlayer = function(unit)
            if unit == "player" then
                return true
            end
            return unit == "npc" and world.npc ~= nil and world.npc.player == true
        end,
        -- A position object, as the client returns; nil in an instance.
        ["C_Map.GetPlayerMapPosition"] = function(map, unit)
            if unit == "player" and map == 1429 and not world.instance then
                return {
                    GetXY = function()
                        return world.pos[1], world.pos[2]
                    end,
                }
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
        GetMoney = function()
            return world.money
        end,
        UnitXP = function()
            return world.xp
        end,
        UnitLevel = function(unit)
            if unit == "player" then
                return world.level
            end
        end,
        GetRealZoneText = function()
            return world.zone
        end,
        IsInInstance = function()
            return world.instance, world.instance and "party" or "none"
        end,
        GetInventoryItemID = function(_, slot)
            return world.equipped[slot]
        end,
        GetRepairAllCost = function()
            return world.repair, world.repair > 0
        end,
        ["C_Container.GetContainerNumSlots"] = function(bag)
            local b = readable(bag) and world.bags[bag]
            return b and b.size or 0
        end,
        ["C_Container.GetContainerNumFreeSlots"] = function(bag)
            local b = readable(bag) and world.bags[bag]
            if not b then
                return 0, 0
            end
            local used = 0
            for _ in pairs(b.slots) do
                used = used + 1
            end
            return b.size - used, 0
        end,
        ["C_Container.GetBagName"] = function(bag)
            if bag == 0 then
                return "Backpack"
            elseif world.bags[bag] and bag > 0 and bag <= 5 then
                return "Linen Bag"
            end
        end,
        ["C_Container.GetContainerItemInfo"] = function(bag, slot)
            local item = readable(bag) and world.bags[bag] and world.bags[bag].slots[slot]
            if item then
                return { itemID = item.id, stackCount = item.count, hyperlink = M.link(item.id) }
            end
        end,
        ["C_Bank.FetchPurchasedBankTabIDs"] = function()
            return world.bank_tabs
        end,
        InCombatLockdown = function()
            return world.combat
        end,
        IsShiftKeyDown = function()
            return world.shift
        end,
        -- Bags, and with `bank` the bank too, wherever the player is.
        ["C_Item.GetItemCount"] = function(id, bank)
            local n = 0
            for b, bag in pairs(world.bags) do
                if bank or (b >= 0 and b <= 5) then
                    for _, item in pairs(bag.slots) do
                        if item.id == id then
                            n = n + item.count
                        end
                    end
                end
            end
            return n
        end,
        ["C_QuestLog.GetTitleForQuestID"] = function(id)
            return M.QUESTS[id]
        end,
        ["C_QuestLog.GetAllCompletedQuestIDs"] = function()
            local copy = {}
            for i, id in ipairs(world.quests_done) do
                copy[i] = id
            end
            return copy
        end,
        ["C_Item.GetItemInfo"] = function(id)
            local name = M.ITEMS[id]
            if not name or world.uncached[id] then
                return nil
            end
            -- name, link, quality, ilvl, min level, type, subtype, stack,
            -- equip slot, icon, sell price, class id, subclass id
            local g = M.GEAR[id]
            if g then
                return name, M.link(id), 3, g[1], g[2], "Armor", "", 1, g[3], 133070, 5000, g[4], g[5]
            end
            return name, M.link(id), 1, 10, 0, "Trade Goods", "Cloth", 20, "", 132889, 13, 7, 5
        end,
        ["C_Item.RequestLoadItemDataByID"] = function(id)
            world.requests.items = world.requests.items + 1
            later(function()
                world.uncached[id] = nil
                client.fire("ITEM_DATA_LOAD_RESULT", id, M.ITEMS[id] ~= nil)
            end)
        end,
        ["C_Map.GetBestMapForUnit"] = function(unit)
            if unit == "player" then
                return 1429
            end
        end,
        UnitClass = function(unit)
            if unit == "player" then
                return "Warrior", "WARRIOR", 1
            end
        end,
        UnitRace = function(unit)
            if unit == "player" then
                return "Human", "Human", 1
            end
        end,
        UnitSex = function()
            return 2
        end,
        UnitFactionGroup = function()
            return "Alliance", "Alliance"
        end,
        GetGuildInfo = function()
            if world.guild then
                return world.guild.name, world.guild.rank, 1
            end
        end,
        UnitXPMax = function()
            return world.xp_max
        end,
        GetXPExhaustion = function()
            return world.rested
        end,
        GetRestState = function()
            return 1, "Rested", 2
        end,
        GetAverageItemLevel = function()
            return 21.5, 20.25, 21.5
        end,
        GetSubZoneText = function()
            return world.subzone or ""
        end,
        GetInventoryItemLink = function(_, slot)
            local id = world.equipped[slot]
            return id and M.link(id)
        end,
        GetProfessions = function()
            local p = world.professions
            return p[1] and 1, p[2] and 2, p[3] and 3, p[4] and 4, p[5] and 5
        end,
        GetProfessionInfo = function(i)
            local p = world.professions[i]
            if p then
                return p.name, 136246, p.skill, p.max, 1, 21, p.line, 0, p.spec or -1, 0, p.name
            end
        end,
        RequestTimePlayed = function()
            world.requests.played = world.requests.played + 1
            later(function()
                local since = client.now - M.EPOCH
                client.fire("TIME_PLAYED_MSG", world.played + since, world.played_level + since)
            end)
        end,
        RequestRaidInfo = function()
            world.requests.raid = world.requests.raid + 1
            later(function()
                client.fire("UPDATE_INSTANCE_INFO")
            end)
        end,
        GetNumSavedInstances = function()
            return #world.lockouts
        end,
        GetSavedInstanceInfo = function(i)
            local l = world.lockouts[i]
            if l then
                -- name, lockout id, reset (seconds), difficulty id, locked,
                -- extended, instance id, is raid, max players, difficulty name
                return l.name, 7, l.reset, 1, true, false, 0, l.raid, 10, l.difficulty
            end
        end,
        GetInboxNumItems = function()
            local n = world.mail_open and #world.inbox or 0
            return n, n
        end,
        GetInboxHeaderInfo = function(i)
            local m = world.mail_open and world.inbox[i]
            if m then
                return nil, nil, m.sender, m.subject, m.money, m.cod, m.days, #m.items, false, false, false, true, false
            end
        end,
        GetInboxItemLink = function(i, a)
            local m = world.mail_open and world.inbox[i]
            local item = m and m.items[a]
            return item and M.link(item.id)
        end,
        GetInboxItem = function(i, a)
            local m = world.mail_open and world.inbox[i]
            local item = m and m.items[a]
            if item then
                return M.ITEMS[item.id], item.id, 132889, item.count, 1, true
            end
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
        env.Enum = { BagIndex = { Bank = -1 }, BankType = { Character = 0 }, TooltipDataType = { Item = 0 } }
        -- The game's tooltip hook point: callbacks run after an item tooltip
        -- is filled (client.hover).
        env.TooltipDataProcessor = {
            AddTooltipPostCall = function(kind, fn)
                state.tooltip[kind] = state.tooltip[kind] or {}
                table.insert(state.tooltip[kind], fn)
            end,
        }
        env.RAID_CLASS_COLORS = {
            WARRIOR = { r = 0.78, g = 0.61, b = 0.43 },
            PALADIN = { r = 0.96, g = 0.55, b = 0.73 },
            ROGUE = { r = 1, g = 0.96, b = 0.41 },
            PRIEST = { r = 1, g = 1, b = 1 },
        }
        -- The client's own strings for bound items (GlobalStrings).
        env.ITEM_SOULBOUND = "Soulbound"
        env.ITEM_BIND_ON_PICKUP = "Binds when picked up"
        -- The game's date(), in UTC so fixtures don't depend on the machine.
        env.date = function(fmt, t)
            return os.date("!" .. fmt, t)
        end
        for name, f in pairs(api) do
            if not applies(opts.missing, name) then
                -- "C_Container.GetContainerNumSlots" goes in env.C_Container.
                local t, key = env, name
                for part, rest in name:gmatch("([^%.]+)%.(.*)") do
                    t[part] = t[part] or {}
                    t, key = t[part], rest
                end
                t[key] = wrap(name, f)
            end
        end
        return env
    end

    function client.fire(event, ...)
        local args = pack(...)
        if opts.secret_args and opts.secret_args[event] then
            for i = 1, args.n do
                if args[i] ~= nil then
                    args[i] = client.secret()
                end
            end
        end
        for _, f in ipairs(state.frames) do
            if f.events[event] and f.scripts.OnEvent then
                local ok, err = pcall(f.scripts.OnEvent, f, event, unpack(args, 1, args.n))
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
        state = { frames = {}, timers = {}, tooltip = {} }
        state.env = environment()
        -- Every file in the TOC, in its order, as the client does: the bridge
        -- slots (opts.slots["Data/Tooltip1.lua"] = source, else the bundled
        -- stub), then the addon.
        local folder = opts.addon:match("^(.*[/\\])") or ""
        for line in readFile(folder .. "ForeverBuddy.toc"):gmatch("[^\r\n]+") do
            if not line:match("^#") then
                local src = (opts.slots or {})[line] or readFile(folder .. line)
                loadIn(src, "@" .. line, state.env)("ForeverBuddy", {})
            end
        end
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

    -- Things that happen in the game ------------------------------------------

    local function bagsChanged()
        client.fire("BAG_UPDATE_DELAYED")
    end

    -- Adds items to a bag (the backpack unless given), stacking onto a slot
    -- with the same item.
    local function put(id, count, into)
        local bag = world.bags[into or 0]
        local slots = bag.slots
        for slot = 1, bag.size do
            if slots[slot] and slots[slot].id == id then
                slots[slot].count = slots[slot].count + count
                return
            end
        end
        for slot = 1, bag.size do
            if not slots[slot] then
                slots[slot] = { id = id, count = count }
                return
            end
        end
        error("bag full")
    end

    -- Takes items out of what's carried (bags 0-5), or out of the bank.
    local function remove(id, count, fromBank)
        for b, bag in pairs(world.bags) do
            local carried = b >= 0 and b <= 5
            for slot, item in pairs(carried ~= (fromBank == true) and bag.slots or {}) do
                if item.id == id then
                    local n = math.min(count, item.count)
                    item.count, count = item.count - n, count - n
                    if item.count == 0 then
                        bag.slots[slot] = nil
                    end
                    if count == 0 then
                        return
                    end
                end
            end
        end
        error("not carrying " .. id)
    end

    function client.setMoney(money)
        world.money = money
        client.fire("PLAYER_MONEY")
    end

    function client.loot(id, count)
        put(id, count)
        bagsChanged()
    end

    function client.use(id, count)
        remove(id, count or 1)
        bagsChanged()
    end

    -- Equips `id` from the bags into `slot`; what was there goes to the bags.
    function client.equip(slot, id)
        remove(id, 1)
        if world.equipped[slot] then
            put(world.equipped[slot], 1)
        end
        world.equipped[slot] = id
        client.fire("PLAYER_EQUIPMENT_CHANGED", slot, false)
        bagsChanged()
    end

    function client.enterZone(zone, instance)
        world.zone, world.instance = zone, instance == true
        client.fire("ZONE_CHANGED_NEW_AREA")
    end

    function client.levelUp()
        world.level, world.xp = world.level + 1, 0
        client.fire("PLAYER_LEVEL_UP", world.level, 10, 0, 0, 0, 0, 0, 0, 0)
    end

    function client.die(durabilityCost)
        world.repair = world.repair + (durabilityCost or 0)
        client.fire("PLAYER_DEAD")
    end

    -- `from`: { name, player } the quest window is open on (the "npc"
    -- unit), or nil for none.
    function client.accept(id, from)
        world.npc = from
        client.fire("QUEST_ACCEPTED", id)
        world.npc = nil
    end

    function client.turnIn(id, xp, money, to)
        world.xp = world.xp + xp
        table.insert(world.quests_done, id)
        world.npc = to
        client.fire("QUEST_TURNED_IN", id, xp, money)
        world.npc = nil
        client.setMoney(world.money + money)
    end

    function client.encounter(id, name, success)
        client.fire("ENCOUNTER_END", id, name, 1, 5, success and 1 or 0)
    end

    function client.openMerchant()
        client.fire("MERCHANT_SHOW")
    end

    function client.sell(id, count, price)
        remove(id, count)
        bagsChanged()
        client.setMoney(world.money + price)
    end

    function client.buy(id, count, price)
        put(id, count)
        bagsChanged()
        client.setMoney(world.money - price)
    end

    function client.repairAll()
        local cost = world.repair
        world.repair = 0
        client.fire("UPDATE_INVENTORY_DURABILITY")
        client.setMoney(world.money - cost)
    end

    function client.closeMerchant()
        -- The client fires it twice.
        client.fire("MERCHANT_CLOSED")
        client.fire("MERCHANT_CLOSED")
    end

    -- Moves items between the bags and the main bank while it's open.
    function client.bank(deposit, withdraw)
        world.bank_open = true
        client.fire("BANKFRAME_OPENED")
        for id, count in pairs(deposit or {}) do
            remove(id, count)
            put(id, count, -1)
        end
        bagsChanged()
        client.fire("PLAYERBANKSLOTS_CHANGED", 1)
        for id, count in pairs(withdraw or {}) do
            remove(id, count, true)
            put(id, count)
        end
        bagsChanged()
        client.fire("PLAYERBANKSLOTS_CHANGED", 1)
        client.fire("BANKFRAME_CLOSED")
        world.bank_open = false
    end

    -- Opens the mailbox (world.inbox), sends and takes items, closes it.
    function client.mail(send, take)
        world.mail_open = true
        client.fire("MAIL_SHOW")
        client.fire("MAIL_INBOX_UPDATE")
        for id, count in pairs(send or {}) do
            remove(id, count)
        end
        bagsChanged()
        for id, count in pairs(take or {}) do
            put(id, count)
        end
        bagsChanged()
        client.fire("MAIL_CLOSED")
        world.mail_open = false
    end

    -- Shows the item tooltip for `id`: the game's own line, then whatever
    -- the item post-calls add. Returns the lines as "left" or "left | right".
    function client.hover(id)
        local lines = { M.ITEMS[id] or ("Item " .. id) }
        local tooltip = {
            AddLine = function(_, text)
                table.insert(lines, text)
            end,
            AddDoubleLine = function(_, left, right)
                table.insert(lines, left .. " | " .. right)
            end,
        }
        -- The tooltip's data, as TooltipDataProcessor passes it: the id and
        -- the game's own lines (a bind line for world.bound[id]).
        local data = { id = id, lines = { { leftText = lines[1] } } }
        if world.bound[id] then
            table.insert(data.lines, { leftText = world.bound[id] })
        end
        for _, fn in ipairs(state.tooltip[0] or {}) do
            local ok, err = pcall(fn, tooltip, data)
            if not ok then
                table.insert(client.errors, "tooltip: " .. tostring(err))
            end
        end
        return lines
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
