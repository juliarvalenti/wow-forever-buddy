-- UI1: check and draw a UI document from the command line, the way
-- render_ui does in the app (preview.lua, and the addon's UICheck.lua and
-- UI.lua).
--
--   luajit tools/addon-shots/ui.lua <doc.json> <out-dir> [--class rogue] [--moment combat]
--   luajit tools/addon-shots/ui.lua --examples <out-dir>
--
-- Writes <out-dir>/<name>-<moment>.svg for each moment (all four unless
-- --moment picks one), with a target for the parts that need one. A
-- document that doesn't pass the check prints its problems and draws
-- nothing. --examples draws every ui-examples/*.json and fails on a Lua
-- error, then checks that a broken document gets the problems it should.

local here = (arg[0]:match("^(.*)[/\\]") or ".")
local wowapiDir = here .. "/../wowapi"
package.path = wowapiDir .. "/?.lua;" .. wowapiDir .. "/?/init.lua;" .. here .. "/?.lua;" .. package.path
local preview = require("preview")
local ADDON = here .. "/../../src-tauri/resources/addon/ForeverBuddy"
local uicheck = preview.uicheck(ADDON)
local MOMENTS = { "standing", "combat", "stealth", "low_health" }

local decode = require("json").decode

local function read(path)
    local f = assert(io.open(path, "rb"))
    local s = f:read("*a")
    f:close()
    return s
end

-- Draws doc in each moment with a hostile target. Returns the Lua errors
-- in all, or nil and the problems.
local function draw(doc, name, out, class, moments)
    local errors = 0
    for _, m in ipairs(moments) do
        local svg, n, _, problems = preview.draw({ addon = ADDON, class = class, moment = m, ui = doc })
        if not svg then
            return nil, problems
        end
        errors = errors + n
        local f = assert(io.open(("%s/%s-%s.svg"):format(out, name, m), "wb"))
        f:write(svg)
        f:close()
        print(("%s-%s.svg: %d KB, %d Lua errors"):format(name, m, math.floor(#svg / 1024), n))
    end
    return errors
end

local args, opts = {}, { class = "rogue" }
local i = 1
while arg[i] do
    if arg[i] == "--class" or arg[i] == "--moment" then
        opts[arg[i]:sub(3)], i = arg[i + 1], i + 2
    elseif arg[i] == "--examples" then
        opts.examples, i = true, i + 1
    else
        args[#args + 1], i = arg[i], i + 1
    end
end

if opts.examples then
    local out = assert(args[1], "usage: ui.lua --examples <out-dir>")
    local failed = 0
    for _, name in ipairs({ "round4", "minimal", "columns", "halo", "terminal", "shards", "corners", "rogue-auras" }) do
        local doc = decode(read(here .. "/ui-examples/" .. name .. ".json"))
        local c = uicheck.cost(doc)
        print(("%s costs %d frames, %d textures, %d font strings"):format(name, c.frames, c.textures, c.fontStrings))
        local errors, problems = draw(doc, name, out, opts.class, MOMENTS)
        if not errors then
            print("FAIL " .. name .. ": doesn't pass the check\n" .. uicheck.lines(problems))
            failed = failed + 1
        elseif errors > 0 then
            print("FAIL " .. name .. ": " .. errors .. " Lua errors")
            failed = failed + 1
        end
    end
    -- A broken document: each mistake found, at its path, and nothing drawn.
    local bad = decode([[{ "ui": 1, "parts": [
        { "name": "hp", "kind": "bar", "value": "health", "anchor": ["BOTTOM", "hpBar", "TOP"], "colour": "#fff" },
        { "kind": "text", "format": "{hp} {mana}", "fontSize": 200 },
        { "kind": "actionbar", "slots": [1, 999] } ] }]])
    local _, problems = draw(bad, "bad", out, opts.class, { "standing" })
    local want = { "/parts/0/anchor/1 unknown-anchor", "/parts/0/colour unknown-key", "/parts/1/format unknown-token",
        "/parts/1/fontSize out-of-range", "/parts/2/slots/1 bad-slot" }
    local got = {}
    for _, p in ipairs(problems or {}) do
        got[p.path .. " " .. p.code] = true
    end
    for _, w in ipairs(want) do
        if not got[w] then
            print("FAIL bad document: no " .. w .. "\n" .. uicheck.lines(problems or {}))
            failed = failed + 1
        end
    end
    -- Aura bars with maxDuration: the rogue's hour-long poisons are left
    -- out, Adrenaline Rush (15 seconds) stays.
    local short = preview.draw({ addon = ADDON, class = opts.class, ui = { ui = 1, parts = { { kind = "auras",
        unit = "player", maxDuration = 60, anchor = { "CENTER", "screen", "CENTER", 0, 0 } } } } })
    if not (short and short:find("Adrenaline Rush", 1, true) and not short:find("Poison", 1, true)) then
        print("FAIL aura bars: maxDuration didn't leave out the long poisons")
        failed = failed + 1
    end
    -- Over the client's limits: 40 texts with a stroke of 3 make about 1480
    -- font strings, more than a UI gets.
    local heavy = { ui = 1, parts = {} }
    for i = 1, 40 do
        heavy.parts[i] = { kind = "text", format = "{hp}", stroke = 3, anchor = { "CENTER", "screen", "CENTER", 0, i * 10 } }
    end
    local _, over = draw(heavy, "heavy", out, opts.class, { "standing" })
    if not (over and over[1] and over[1].code == "over-budget") then
        print("FAIL heavy document: not refused for its cost\n" .. uicheck.lines(over or {}))
        failed = failed + 1
    end
    print(failed == 0 and "ok: every example draws, and the broken and the heavy one are refused" or (failed .. " failed"))
    os.exit(failed == 0 and 0 or 1)
end

local path, out = args[1], args[2]
assert(path and out, "usage: ui.lua <doc.json> <out-dir> [--class rogue] [--moment combat]")
local doc = decode(read(path))
local name = path:match("([^/\\]+)%.json$") or "ui"
local errors, problems = draw(doc, name, out, opts.class, opts.moment and { opts.moment } or MOMENTS)
if not errors then
    print(uicheck.lines(problems))
    os.exit(1)
end
