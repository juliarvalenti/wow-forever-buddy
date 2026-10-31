-- Fidelity from Blizzard's own API documentation (SIM5d).
--
-- The Forever client ships its API reference as Lua: the
-- Blizzard_APIDocumentationGenerated addon, one file per system, each
-- handing a table to APIDocumentation:AddDocumentationTable. CASC2's extract
-- puts it, with the rest of Blizzard's UI code, in the app's local cache
-- (never in this repo). This reads it into an index of every documented
-- function (namespace, arguments, returns, secret flags), event and widget
-- method, then holds ForeverBuddy and our fakes up to it.
--
-- The docs don't cover everything the client has: old globals such as
-- GetInboxItem are C functions nobody documented, and some names (Settings,
-- UiMapPoint) are defined in Blizzard's Lua. So a name is also counted as
-- present when Blizzard's own UI code (the same extract) defines or calls
-- it. A name in neither may still exist (retail's docs leave out real
-- functions too); it's the first thing to check in the census.
--
--   luajit tools/addon-test/apidocs.lua                 the report
--   luajit tools/addon-test/apidocs.lua --all           every name, not samples
--   luajit tools/addon-test/apidocs.lua stub C_Item.GetItemInfoInstant ...
--                                                       wow.lua stubs, shaped as documented
--
-- The docs folder is $WOW_APIDOCS, or the newest build under the app's cache
-- (~/Library/Application Support/com.juliarvalenti.wowforeverbuddy/cache/
-- casc-extract/wowf/<build>/interface/addons/blizzard_apidocumentationgenerated).
-- Without it everything here skips; CI has no cache, so H2 checks this only
-- on a machine that has run the extract.

local M = {}

local CACHE = "/Library/Application Support/com.juliarvalenti.wowforeverbuddy/cache/casc-extract/wowf"
local SUBDIR = "/interface/addons/blizzard_apidocumentationgenerated"

local function sq(s)
    return "'" .. tostring(s):gsub("'", "'\\''") .. "'"
end

local function readAll(path)
    local f = io.open(path, "rb")
    if not f then
        return nil
    end
    local s = f:read("*a")
    f:close()
    return s
end

-- The docs folder, or nil.
function M.find()
    local env = os.getenv("WOW_APIDOCS")
    if env and env ~= "" then
        return readAll(env .. "/itemdocumentation.lua") and env or nil
    end
    local home = os.getenv("HOME")
    if not home then
        return nil
    end
    -- The newest build folder that has the generated docs.
    local p = io.popen("ls -t " .. sq(home .. CACHE) .. " 2>/dev/null")
    if not p then
        return nil
    end
    for build in p:lines() do
        local dir = home .. CACHE .. "/" .. build .. SUBDIR
        if readAll(dir .. "/itemdocumentation.lua") then
            p:close()
            return dir
        end
    end
    p:close()
    return nil
end

-- Every Secret* key on a doc entry, as a sorted "Key=Value" list.
local function secrets(entry)
    local out = {}
    for k, v in pairs(entry) do
        if type(k) == "string" and k:match("^Secret") then
            out[#out + 1] = k .. "=" .. tostring(v)
        end
    end
    table.sort(out)
    return out
end

-- The index: functions["C_Item.GetItemInfo"] (or a global's bare name),
-- events["BAG_UPDATE"], methods["SetPoint"] (any widget type), and
-- code[name], every name Blizzard's UI code defines or calls.
function M.load(dir)
    local docs = {}
    -- Some docs name enum values and constants (Enum.SecretAspect.
    -- ObjectSecurity, Constants.X.Y + 1); any path through this resolves,
    -- to itself, and sums to 0.
    local function zero()
        return 0
    end
    local anything = setmetatable({}, { __index = function(t)
        return t
    end, __add = zero, __sub = zero, __mul = zero, __div = zero, __unm = zero })
    local env = setmetatable({
        APIDocumentation = {
            AddDocumentationTable = function(_, t)
                docs[#docs + 1] = t
            end,
        },
    }, { __index = function()
        return anything
    end })
    env._G = env
    local files, failed = 0, {}
    local p = assert(io.popen("ls " .. sq(dir)))
    for name in p:lines() do
        if name:match("%.lua$") then
            local chunk = loadstring(readAll(dir .. "/" .. name) or "", "=" .. name)
            if chunk then
                setfenv(chunk, env)
            end
            if chunk and pcall(chunk) then
                files = files + 1
            else
                failed[#failed + 1] = name
            end
        end
    end
    p:close()
    local index = { functions = {}, events = {}, methods = {}, code = {}, codeMethods = {},
        files = files, failed = failed, systems = #docs }
    for _, sys in ipairs(docs) do
        local scriptObject = sys.Type == "ScriptObject"
        for _, fn in ipairs(sys.Functions or {}) do
            local entry = {
                system = sys.Name,
                namespace = sys.Namespace,
                arguments = fn.Arguments or {},
                returns = fn.Returns or {},
                secrets = secrets(fn),
            }
            if scriptObject then
                index.methods[fn.Name] = index.methods[fn.Name] or {}
                table.insert(index.methods[fn.Name], sys.Name)
            elseif sys.Namespace then
                index.functions[sys.Namespace .. "." .. fn.Name] = entry
            else
                index.functions[fn.Name] = entry
            end
        end
        for _, ev in ipairs(sys.Events or {}) do
            if ev.LiteralName then
                index.events[ev.LiteralName] = { payload = ev.Payload or {}, secrets = secrets(ev) }
            end
        end
    end
    -- Blizzard's UI code beside the docs (Lua files, and the scripts inside
    -- its XML): every "Name(" and "A.B(" / "A:B(" it calls, and every
    -- "A.B = function" it defines.
    local addons = dir:match("^(.*)/[^/]+$")
    local q = io.popen("find " .. sq(addons) .. " \\( -name '*.lua' -o -name '*.xml' \\) -not -path " .. sq(dir .. "/*"))
    if q then
        for path in q:lines() do
            local src = readAll(path) or ""
            for a, b in src:gmatch("([%a_][%w_]*)[%.:]([%a_][%w_]*)%s*%(") do
                index.code[a .. "." .. b] = true
                index.codeMethods[b] = true
            end
            for a in src:gmatch("([%a_][%w_]*)%s*%(") do
                index.code[a] = true
            end
            for a, b in src:gmatch("([%a_][%w_]*)%.([%a_][%w_]*)%s*=%s*function") do
                index.code[a .. "." .. b] = true
            end
        end
        q:close()
    end
    return index
end

-- "documented", "code" (Blizzard's UI defines or calls it), or nil (neither).
function M.where(index, name)
    if index.functions[name] then
        return "documented"
    end
    if index.code[name] then
        return "code"
    end
    -- A method on a frame or mixin (Minimap.GetWidth): documented per
    -- widget type, not per frame, and Blizzard calls it on self or a local.
    local method = not name:match("^C_") and name:match("^[%a_][%w_]*%.([%a_][%w_]*)$")
    if method and index.methods[method] then
        return "documented"
    end
    if method and index.codeMethods[method] then
        return "code"
    end
    return nil
end

-- A wow.lua api stub for a documented function: returns shaped as the docs
-- say (a placeholder per type; nilable ones nil), with the doc as a comment.
local DEFAULTS = {
    number = "0", luaIndex = "1", bool = "false", string = '""', cstring = '""',
    table = "{}", WOWGUID = '"Player-0000-00000000"', fileID = "0",
}

function M.stub(index, name)
    local d = index.functions[name]
    if not d then
        return nil, name .. " isn't documented"
    end
    local args, rets, vals = {}, {}, {}
    for _, a in ipairs(d.arguments) do
        args[#args + 1] = a.Name
    end
    for _, r in ipairs(d.returns) do
        rets[#rets + 1] = ("%s: %s%s"):format(r.Name, r.Type, r.Nilable and "?" or "")
        vals[#vals + 1] = r.Nilable and "nil" or (DEFAULTS[r.Type] or "nil")
    end
    local lines = {}
    lines[#lines + 1] = ("        -- %s(%s) -> %s"):format(name, table.concat(args, ", "),
        #rets > 0 and table.concat(rets, ", ") or "nothing")
    if #d.secrets > 0 then
        lines[#lines + 1] = "        -- Secret: " .. table.concat(d.secrets, ", ")
    end
    lines[#lines + 1] = ('        ["%s"] = function(%s)'):format(name, table.concat(args, ", "))
    if #vals > 0 then
        lines[#lines + 1] = "            return " .. table.concat(vals, ", ")
    end
    lines[#lines + 1] = "        end,"
    return table.concat(lines, "\n")
end

-- The command line --------------------------------------------------------------

local function list(names, all, limit)
    limit = limit or 40
    if #names == 0 then
        return "  none"
    end
    local out = {}
    for i = 1, all and #names or math.min(limit, #names) do
        out[#out + 1] = "  " .. names[i]
    end
    if not all and #names > limit then
        out[#out + 1] = ("  ... and %d more (--all)"):format(#names - limit)
    end
    return table.concat(out, "\n")
end

local function count(t)
    local n = 0
    for _ in pairs(t) do
        n = n + 1
    end
    return n
end

local function main(args)
    local here = (args[0] or ""):match("^(.*)[/\\]") or "."
    package.path = here .. "/?.lua;" .. package.path
    local wow = require("wow")
    local census = require("census")
    local dir = M.find()
    if not dir then
        io.stderr:write("no API docs: run the CASC extract, or set WOW_APIDOCS\n")
        os.exit(1)
    end
    local index = M.load(dir)

    if args[1] == "stub" then
        for i = 2, #args do
            local s, err = M.stub(index, args[i])
            print(s or ("-- " .. err))
        end
        return
    end
    local all = args[1] == "--all"
    print(("Blizzard's API docs: %d systems; %d functions, %d events, %d widget methods; %d names in its UI code"):format(
        index.systems, count(index.functions), count(index.events), count(index.methods), count(index.code)))
    if #index.failed > 0 then
        print("  doc files that didn't load: " .. table.concat(index.failed, ", "))
    end

    -- 1. The addon's calls.
    local root = here .. "/../.."
    local src = wow.readFile(root .. "/src-tauri/resources/addon/ForeverBuddy/ForeverBuddy.lua")
    local apis = census.addonApis(src)
    local inCode, nowhere, secret = {}, {}, {}
    for _, name in ipairs(apis) do
        local w = M.where(index, name)
        local d = index.functions[name]
        if w == "code" then
            inCode[#inCode + 1] = name
        elseif not w then
            nowhere[#nowhere + 1] = name
        elseif d and #d.secrets > 0 then
            secret[#secret + 1] = name .. "  (" .. table.concat(d.secrets, ", ") .. ")"
        end
    end
    print(("\nForeverBuddy calls %d APIs:"):format(#apis))
    print(("  in neither the docs nor Blizzard's UI code (may be absent; the census decides): %d"):format(#nowhere))
    print(list(nowhere, true))
    print(("  undocumented, but Blizzard's UI code defines or calls them: %d"):format(#inCode))
    print(list(inCode, all))
    print(("  documented with secret flags: %d"):format(#secret))
    print(list(secret, all))

    local eventsMissing = {}
    local nEvents = 0
    for name in src:gmatch("\nhandlers%.([%u_]+) = ") do
        nEvents = nEvents + 1
        if not index.events[name] then
            eventsMissing[#eventsMissing + 1] = name
        end
    end
    table.sort(eventsMissing)
    print(("\nIt handles %d events; not in the docs: %d"):format(nEvents, #eventsMissing))
    print(list(eventsMissing, true))

    -- 2. Our fake client: functions it mocks that the client lacks, and the
    -- widget methods it models that no widget type documents.
    local c = wow.new({ addon = root .. "/src-tauri/resources/addon/ForeverBuddy/ForeverBuddy.lua" })
    c.login(nil)
    local lies = {}
    for name in pairs(census.names(c.env())) do
        local v = c.env()[name]
        if (name:match("^C_[%w_]+%.") or (type(v) == "function" and name:match("^%u")))
            and not name:match("^ForeverBuddy") and not M.where(index, name) then
            lies[#lies + 1] = name
        end
    end
    table.sort(lies)
    print(("\nThe fake client mocks functions in neither the docs nor Blizzard's UI code: %d"):format(#lies))
    print(list(lies, all))
    local wrongMethods = {}
    for _, m in ipairs(c.widgetMethods()) do
        if m:match("^%u") and not index.methods[m] and not index.codeMethods[m] then
            wrongMethods[#wrongMethods + 1] = m
        end
    end
    print(("\nWidget methods the fake models that no widget type documents and Blizzard's UI never calls: %d"):format(#wrongMethods))
    print(list(wrongMethods, true))

    -- 3. WOWAPI's own docs (retail's, from tools/wowapi/fetch-data.sh)
    -- against Forever's, both ways.
    local ok, wowapi = pcall(dofile, root .. "/tools/wowapi/wowapi/data/apidocs.lua")
    if not ok then
        print("\nWOWAPI: no data (tools/wowapi/fetch-data.sh)")
        return
    end
    local onlyForever, onlyWowapi = {}, {}
    for name in pairs(index.functions) do
        if not wowapi.functions[name] then
            onlyForever[#onlyForever + 1] = name
        end
    end
    for name in pairs(wowapi.functions) do
        if not index.functions[name] then
            onlyWowapi[#onlyWowapi + 1] = name
        end
    end
    table.sort(onlyForever)
    table.sort(onlyWowapi)
    print(("\nWOWAPI's docs lack %d functions Forever documents:"):format(#onlyForever))
    print(list(onlyForever, all))
    print(("WOWAPI documents %d functions Forever's docs don't:"):format(#onlyWowapi))
    print(list(onlyWowapi, all))
end

if arg and arg[0] and arg[0]:match("apidocs%.lua$") then
    main(arg)
end

return M
