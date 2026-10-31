-- Addon-set smoke test (SIM5b): each addon in an AddOns folder, run headless
-- through WOWAPI (tools/wowapi, the WoW: Forever client emulator), with what
-- went wrong listed per addon: load failures, Lua errors, actions the
-- client's security blocked, and calls WOWAPI only stubs or doesn't know.
--
--   luajit tools/addon-smoke/smoke.lua <AddOns folder> [--only A,B] [--md out.md]
--   luajit tools/addon-smoke/smoke.lua --install <list> <folder>    fetch, then run
--
-- `--install` fetches each `owner/repo` in <list> (addons.txt) from GitHub
-- and packages it as the CurseForge packager would (.pkgmeta externals,
-- move-folders), into <folder>/AddOns, through WOWAPI's own installer.
--
-- Each addon gets a fresh client (Interface 16001, secret values on). The
-- session: log in, 10 s idle, combat in and out, /reload, 5 s. Only
-- addons the client would load at login are run (not LoadOnDemand ones,
-- nor ones whose .toc excludes Forever). Its dependencies load first, from
-- the same folder.
--
-- Nothing here touches a real WoW install, and nothing opens a window.

local dir = (arg[0]:match("^(.*)[/\\]") or ".")
local ROOT = dir .. "/../.."
local WOWAPI = ROOT .. "/tools/wowapi"
package.path = WOWAPI .. "/?.lua;" .. WOWAPI .. "/?/init.lua;" .. package.path

local function fail(msg)
    io.stderr:write(msg .. "\n")
    os.exit(2)
end

-- `s` as one POSIX shell word: single-quoted, with each ' written as '\''.
-- (Lua's %q is a Lua literal, not shell quoting: a folder name could break
-- out of it.)
local function sq(s)
    return "'" .. tostring(s):gsub("'", "'\\''") .. "'"
end

local function sh(cmd)
    local r = os.execute(cmd)
    return r == 0 or r == true
end

local f = io.open(WOWAPI .. "/wowapi/data/apidocs.lua", "rb")
if not f then
    fail("WOWAPI's data is missing: run tools/wowapi/fetch-data.sh first")
end
f:close()

local WoW = require("wowapi")
local tocLib = require("wowapi.toc")

io.stdout:setvbuf("line")

-- A finding list as Lua source, for the child process's result file.
local function dump(v)
    if type(v) == "table" then
        local parts = {}
        for k, x in pairs(v) do
            parts[#parts + 1] = "[" .. dump(k) .. "]=" .. dump(x)
        end
        return "{" .. table.concat(parts, ",") .. "}"
    elseif type(v) == "string" then
        return ("%q"):format(v)
    end
    return tostring(v)
end

-- Arguments.
local args, opts = {}, {}
local i = 1
while arg[i] do
    local a = arg[i]
    if a == "--only" or a == "--md" then
        opts[a:sub(3)] = arg[i + 1]
        i = i + 2
    elseif a == "--one" then
        -- Internal: run one addon in this process and write its findings.
        opts.one, opts.out = arg[i + 1], arg[i + 2]
        i = i + 3
    elseif a == "--install" then
        opts.install = true
        i = i + 1
    else
        args[#args + 1] = a
        i = i + 1
    end
end

local addonsDir
if opts.install then
    local list, dest = args[1], args[2]
    if not list or not dest then
        fail("usage: smoke.lua --install <list> <folder>")
    end
    local Installer = require("wowapi.installer")
    local inst = Installer.new({ root = dest, log = function() end })
    -- Every entry is owner/repo@<full commit SHA>: the report is repeatable,
    -- and nobody can change the code we run by pushing to a branch. We
    -- fetch that one commit ourselves and hand the installer the folder.
    local pinned = dest .. "/pinned"
    sh("mkdir -p " .. sq(pinned))
    for line in io.lines(list) do
        local entry = line:match("^%s*([^#%s]+)")
        if entry then
            local owner, repo, sha = entry:match("^([%w%-_%.]+)/([%w%-_%.]+)@(%x+)$")
            if not sha or #sha ~= 40 then
                fail(list .. ": '" .. entry .. "' must be owner/repo@<40-hex commit SHA>")
            end
            io.write("fetching " .. owner .. "/" .. repo .. " at " .. sha:sub(1, 12) .. " ... ")
            local work = pinned .. "/" .. repo
            local url = "https://github.com/" .. owner .. "/" .. repo .. ".git"
            local okFetch = sh("rm -rf " .. sq(work) .. " && git init -q " .. sq(work)
                .. " && git -C " .. sq(work) .. " fetch -q --depth 1 " .. sq(url) .. " " .. sq(sha)
                .. " && git -C " .. sq(work) .. " checkout -q FETCH_HEAD"
                .. " && git -C " .. sq(work) .. " submodule update -q --init --recursive --depth 1")
            local head = okFetch and io.popen("git -C " .. sq(work) .. " rev-parse HEAD"):read("*l")
            if head ~= sha then
                print("failed: could not fetch that commit")
            else
                local ok, err = pcall(inst.install, inst, work)
                print(ok and "ok" or ("failed: " .. tostring(err)))
            end
        end
    end
    addonsDir = dest .. "/AddOns"
else
    addonsDir = args[1]
end
if not addonsDir then
    fail("usage: smoke.lua <AddOns folder> [--only A,B] [--md out.md]")
end

-- The addons to run: every folder whose .toc loads at login on Forever.
local only
if opts.only then
    only = {}
    for name in opts.only:gmatch("[^,]+") do
        only[name] = true
    end
end
local names = {}
for name in io.popen("ls " .. sq(addonsDir)):lines() do
    if not only or only[name] then
        names[#names + 1] = name
    end
end
table.sort(names)

-- One addon's session; returns its findings.
local function run(name)
    local sim = WoW.new({ quiet = true, secretValues = true })
    local result = { name = name, errors = {}, blocked = {}, stubbed = {}, undefined = {} }
    local ok, why = sim:LoadAddon(addonsDir .. "/" .. name)
    if not ok then
        result.loaded = false
        result.reason = tostring(why)
    else
        result.loaded = true
        pcall(function()
            sim:Login()
            sim:Advance(10)
            sim:EnterCombat()
            sim:Advance(2)
            sim:LeaveCombat()
            sim:Advance(2)
            sim:Reload()
            sim:Advance(5)
        end)
    end
    -- Errors, the same message counted once ("(x5)"), with the ones that are
    -- WOWAPI's limits, not the addon's, kept apart.
    local seen, order = {}, {}
    result.emulator = {}
    for _, e in ipairs(sim.errors) do
        local msg = tostring(e.message or e):gsub("\n.*", "")
        if msg:find("LibSharedMedia%-3%.0: Attempt to register an invalid") then
            -- WOWAPI loads no media files, so every font and texture an
            -- addon registers with LibSharedMedia fails its check here.
            result.emulator[#result.emulator + 1] = msg
        else
            if not seen[msg] then
                order[#order + 1] = msg
            end
            seen[msg] = (seen[msg] or 0) + 1
        end
    end
    for _, msg in ipairs(order) do
        result.errors[#result.errors + 1] = seen[msg] > 1 and ("%s (x%d)"):format(msg, seen[msg]) or msg
    end
    for _, b in ipairs(sim.blockedActions or {}) do
        result.blocked[#result.blocked + 1] = ("[%s] %s: %s"):format(b.kind, b.addon, b.action)
    end
    for k, n in pairs(sim.stubbedCalls) do
        result.stubbed[#result.stubbed + 1] = ("%s (x%d)"):format(k, n)
    end
    table.sort(result.stubbed)
    local saved = {}
    for _, a in pairs(sim.addons) do
        for _, v in ipairs(a.toc.savedVariables) do
            saved[v] = true
        end
        for _, v in ipairs(a.toc.savedVariablesPerCharacter) do
            saved[v] = true
        end
    end
    for k in pairs(sim.undefinedGlobals) do
        if not k:match("^__") and rawget(sim.env, k) == nil and not saved[k] then
            result.undefined[#result.undefined + 1] = k
        end
    end
    table.sort(result.undefined)
    return result
end

-- The child: one addon, findings written to a file.
if opts.one then
    local okRun, r = pcall(run, opts.one)
    if not okRun then
        r = { name = opts.one, loaded = false, reason = "crashed the emulator: " .. tostring(r),
            errors = {}, blocked = {}, stubbed = {}, undefined = {} }
    end
    local w = assert(io.open(opts.out, "w"))
    w:write("return " .. dump(r))
    w:close()
    os.exit(0)
end

-- The parent: each addon in its own process, so one that hangs or eats
-- memory in the emulator is a row in the report, not the end of the run.
-- 120 s and about 2 GB each, where the system has `timeout` and `ulimit -v`.
local LIMIT_S = 120
local lua = arg[-1] or "lua5.1"
local hasTimeout = os.execute("command -v timeout >/dev/null 2>&1")
hasTimeout = hasTimeout == 0 or hasTimeout == true
local tmp = os.tmpname()

local function runIsolated(name)
    os.remove(tmp)
    local cmd = ("ulimit -v 2000000 2>/dev/null; %s%s %s --one %s %s %s"):format(
        hasTimeout and ("timeout " .. LIMIT_S .. " ") or "", sq(lua), sq(arg[0]), sq(name), sq(tmp), sq(addonsDir))
    local status = os.execute(cmd)
    local chunk = loadfile(tmp)
    if chunk then
        return chunk()
    end
    local timedOut = (status == 124 or status == 124 * 256)
    return { name = name, loaded = false,
        reason = timedOut and ("timed out after " .. LIMIT_S .. " s") or "the emulator died (out of memory?)",
        errors = {}, blocked = {}, stubbed = {}, undefined = {} }
end

local results = {}
for _, name in ipairs(names) do
    local t = tocLib.load(addonsDir .. "/" .. name)
    local lod = t and (t.loadOnDemand or (t.directives and t.directives.loadondemand == "1"))
    if t and not lod then
        io.write(("%-32s "):format(name))
        io.flush()
        local r = runIsolated(name)
        results[#results + 1] = r
        print(r.loaded and ("%d errors, %d blocked, %d stubbed, %d unknown globals"):format(
            #r.errors, #r.blocked, #r.stubbed, #r.undefined) or ("not loaded: " .. r.reason))
    end
end

-- The report, in Markdown.
local out = {}
local function add(s)
    out[#out + 1] = s
end
add("# Addon smoke test")
add("")
add(("%d addons from `%s`, each run headless through WOWAPI (Interface 16001, secret values on): log in, idle, combat in and out, /reload."):format(#results, addonsDir))
add("")
add("Lua errors count each distinct message once. \"Emulator\" counts errors that are WOWAPI's limits, not the addon's (it loads no media files, so LibSharedMedia rejects every font and texture).")
add("")
add("| Addon | Loaded | Lua errors | Blocked | Stubbed calls | Unknown globals | Emulator |")
add("|---|---|---|---|---|---|---|")
for _, r in ipairs(results) do
    add(("| %s | %s | %d | %d | %d | %d | %d |"):format(r.name, r.loaded and "yes" or ("no: " .. r.reason),
        #r.errors, #r.blocked, #r.stubbed, #r.undefined, #(r.emulator or {})))
end
for _, r in ipairs(results) do
    if #r.errors + #r.blocked + #r.undefined > 0 then
        add("")
        add("## " .. r.name)
        local function section(title, items, limit)
            if #items > 0 then
                add("")
                add(("**%s** (%d)"):format(title, #items))
                for j = 1, math.min(limit, #items) do
                    add("- `" .. items[j]:gsub("\n.*", ""):sub(1, 200) .. "`")
                end
                if #items > limit then
                    add(("- ... and %d more"):format(#items - limit))
                end
            end
        end
        section("Lua errors", r.errors, 10)
        section("Blocked actions", r.blocked, 10)
        section("Globals read but not emulated (or typos)", r.undefined, 30)
        section("Called but only stubbed", r.stubbed, 20)
    end
end
local md = table.concat(out, "\n") .. "\n"
if opts.md then
    local w = assert(io.open(opts.md, "w"))
    w:write(md)
    w:close()
    print("report: " .. opts.md)
else
    print("\n" .. md)
end
