-- The Forever API census (SIM5a): what the real client defines, diffed
-- against what our fakes pretend it defines.
--
-- The census comes from the probe addon: /fbprobe census in game, then
-- /reload, writes the name and type of every global (one level into each
-- table, two into Enum) and each widget type's methods to
-- WTF/Account/<ACCOUNT>/SavedVariables/ForeverBuddyProbe.lua. Names and
-- types only. Copy that file to tools/probe-addon/results/census.lua and
-- commit it: run.lua then fails if the addon calls anything it doesn't list.
--
--   luajit tools/addon-test/census.lua <ForeverBuddyProbe.lua>        the report
--   luajit tools/addon-test/census.lua <ForeverBuddyProbe.lua> --all  every name, not samples
--
-- The report compares three things with the census:
--   * the APIs ForeverBuddy calls (read("...")): any missing is a real bug;
--   * our fake client (wow.lua): names and widget methods it gives the
--     addon that Forever doesn't have, so a test could pass on a lie;
--   * WOWAPI (tools/wowapi, if its data/ is fetched): both ways.

local M = {}

-- The census table from a ForeverBuddyProbe.lua SavedVariables file, or nil
-- and why.
function M.load(text, parse)
    local db = parse(text, "ForeverBuddyProbeDB")
    if type(db) ~= "table" then
        return nil, "not a ForeverBuddyProbe file"
    end
    if type(db.census) ~= "table" or type(db.census.globals) ~= "table" then
        return nil, "no census in it: run /fbprobe census in game, then /reload"
    end
    return db.census
end

-- The kind the census recorded for a name ("f" function, "t" table, "w"
-- widget, ...), or nil if the client doesn't define it. Dotted names look
-- one level into a table ("C_Item.GetItemInfo"), or two under Enum.
function M.kind(census, name)
    local head, rest = name:match("^([^%.]+)%.(.+)$")
    if not head then
        return census.globals[name]
    end
    local inner, leaf = rest:match("^([^%.]+)%.([^%.]+)$")
    if inner then
        local t = census.tables[head .. "." .. inner]
        return t and t[leaf]
    end
    local t = census.tables[head]
    return t and t[rest]
end

-- Every API ForeverBuddy reaches through read(), as H2 finds them.
function M.addonApis(src)
    local apis = {}
    for name in src:gmatch('read%("([%w_%.]+)"') do
        apis[name] = true
    end
    for name in src:gmatch("read%(([%a_][%w_%.]*)") do
        if name:match("^%u") or name == "date" then
            apis[name] = true
        end
    end
    local out = {}
    for name in pairs(apis) do
        out[#out + 1] = name
    end
    table.sort(out)
    return out
end

-- A widget, real (userdata at [0]) or faked (a metatable whose __index is a
-- function, as wow.lua's and WOWAPI's frames have): not walked, since the
-- census doesn't walk real ones either.
local function widget(v)
    if type(rawget(v, 0)) == "userdata" then
        return true
    end
    local mt = getmetatable(v)
    return type(mt) == "table" and type(rawget(mt, "__index")) == "function"
end

-- The names an environment defines: its globals, and one level into each
-- table it holds (two under Enum), as dotted names. Widgets aren't walked.
function M.names(env)
    local out = {}
    for k, v in pairs(env) do
        if type(k) == "string" then
            out[k] = true
            if type(v) == "table" and not widget(v) and k ~= "_G" then
                for k2, v2 in pairs(v) do
                    if type(k2) == "string" then
                        out[k .. "." .. k2] = true
                        if k == "Enum" and type(v2) == "table" then
                            for k3 in pairs(v2) do
                                if type(k3) == "string" then
                                    out[k .. "." .. k2 .. "." .. k3] = true
                                end
                            end
                        end
                    end
                end
            end
        end
    end
    return out
end

-- The names in `set` the census doesn't list, sorted.
function M.absent(census, set)
    local out = {}
    for name in pairs(set) do
        if M.kind(census, name) == nil then
            out[#out + 1] = name
        end
    end
    table.sort(out)
    return out
end

-- The census's names (globals, then dotted members) not in `set`, sorted.
function M.extra(census, set)
    local out = {}
    for name in pairs(census.globals) do
        if not set[name] then
            out[#out + 1] = name
        end
    end
    for t, members in pairs(census.tables) do
        for m in pairs(members) do
            local name = t .. "." .. m
            if m ~= "<more>" and not set[name] then
                out[#out + 1] = name
            end
        end
    end
    table.sort(out)
    return out
end

-- The command line --------------------------------------------------------------

local function list(names, all, limit)
    limit = limit or 40
    if #names == 0 then
        return "  none"
    end
    local shown = {}
    for i = 1, all and #names or math.min(limit, #names) do
        shown[#shown + 1] = "  " .. names[i]
    end
    if not all and #names > limit then
        shown[#shown + 1] = ("  ... and %d more (--all)"):format(#names - limit)
    end
    return table.concat(shown, "\n")
end

local function main(args)
    local dir = (args[0] or ""):match("^(.*)[/\\]") or "."
    package.path = dir .. "/?.lua;" .. package.path
    local wow = require("wow")
    local path = args[1]
    if not path then
        io.stderr:write("usage: census.lua <ForeverBuddyProbe.lua> [--all]\n")
        os.exit(2)
    end
    local all = args[2] == "--all"
    local census, err = M.load(wow.readFile(path), wow.parse)
    if not census then
        io.stderr:write(path .. ": " .. err .. "\n")
        os.exit(1)
    end
    local b = census.build or {}
    local nGlobals, nTables = 0, 0
    for _ in pairs(census.globals) do
        nGlobals = nGlobals + 1
    end
    for _ in pairs(census.tables) do
        nTables = nTables + 1
    end
    print(("Forever census: build %s (%s), interface %s; %d globals, %d tables walked"):format(
        tostring(b.version), tostring(b.build), tostring(b.toc), nGlobals, nTables))

    -- 1. The addon.
    local root = dir .. "/../.."
    local src = wow.readFile(root .. "/src-tauri/resources/addon/ForeverBuddy/ForeverBuddy.lua")
    local apis = M.addonApis(src)
    local set = {}
    for _, name in ipairs(apis) do
        set[name] = true
    end
    local missing = M.absent(census, set)
    print(("\nForeverBuddy calls %d APIs; not in Forever: %d"):format(#apis, #missing))
    print(list(missing, true))

    -- 2. Our fake client.
    local c = wow.new({ addon = root .. "/src-tauri/resources/addon/ForeverBuddy/ForeverBuddy.lua" })
    c.login(nil)
    local fake = M.names(c.env())
    -- The addon's own globals aren't the fake's: it defined them itself.
    for name in pairs(fake) do
        if name:match("^ForeverBuddy") then
            fake[name] = nil
        end
    end
    local lies = M.absent(census, fake)
    print(("\nThe fake client (wow.lua) gives %d names; not in Forever: %d"):format(
        (function() local n = 0 for _ in pairs(fake) do n = n + 1 end return n end)(), #lies))
    print(list(lies, all))
    local frameMethods = census.widgets and census.widgets.Frame
    if frameMethods then
        local wrong = {}
        for _, m in ipairs(c.widgetMethods()) do
            if not frameMethods[m] then
                wrong[#wrong + 1] = m
            end
        end
        print(("\nWidget methods the fake models that Forever's Frame lacks: %d"):format(#wrong))
        print(list(wrong, true))
    end

    -- 3. WOWAPI.
    local wowapi = root .. "/tools/wowapi"
    local f = io.open(wowapi .. "/wowapi/data/apidocs.lua", "rb")
    if not f then
        print("\nWOWAPI: skipped (run tools/wowapi/fetch-data.sh first)")
        return
    end
    f:close()
    package.path = wowapi .. "/?.lua;" .. wowapi .. "/?/init.lua;" .. package.path
    local WoW = require("wowapi")
    local sim = WoW.new({ quiet = true })
    local emulated = M.names(sim.env)
    local fakeOnly = M.absent(census, emulated)
    local realOnly = M.extra(census, emulated)
    print(("\nWOWAPI emulates names Forever lacks: %d"):format(#fakeOnly))
    print(list(fakeOnly, all))
    print(("\nForever defines names WOWAPI lacks: %d"):format(#realOnly))
    print(list(realOnly, all))
end

if arg and arg[0] and arg[0]:match("census%.lua$") then
    main(arg)
end

return M
