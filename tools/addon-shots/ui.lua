-- UI1: check and draw a UI document from the command line, the way
-- render_ui does in the app (preview.lua, uicheck.lua, FBUI/UI.lua).
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
local uicheck = require("uicheck")
local ADDON = here .. "/../../src-tauri/resources/addon/ForeverBuddy"
local MOMENTS = { "standing", "combat", "stealth", "low_health" }

-- A small JSON reader: objects, arrays, strings, numbers, true, false, null.
local function decode(s)
    local i = 1
    local function ws()
        i = s:find("[^ \t\r\n]", i) or #s + 1
    end
    local value
    local function str()
        local out, j = {}, i + 1
        while true do
            local c = s:sub(j, j)
            if c == "" then
                error("unterminated string")
            elseif c == '"' then
                i = j + 1
                return table.concat(out)
            elseif c == "\\" then
                local e = s:sub(j + 1, j + 1)
                local map = { n = "\n", t = "\t", r = "\r", b = "\b", f = "\f", ['"'] = '"', ["\\"] = "\\", ["/"] = "/" }
                if e == "u" then
                    local code = tonumber(s:sub(j + 2, j + 5), 16)
                    out[#out + 1] = code < 128 and string.char(code) or "?"
                    j = j + 6
                else
                    out[#out + 1] = map[e] or e
                    j = j + 2
                end
            else
                out[#out + 1] = c
                j = j + 1
            end
        end
    end
    function value()
        ws()
        local c = s:sub(i, i)
        if c == "{" then
            local t = {}
            i = i + 1
            ws()
            if s:sub(i, i) == "}" then
                i = i + 1
                return t
            end
            while true do
                ws()
                local k = str()
                ws()
                assert(s:sub(i, i) == ":", "expected : at " .. i)
                i = i + 1
                t[k] = value()
                ws()
                local d = s:sub(i, i)
                i = i + 1
                if d == "}" then
                    return t
                end
                assert(d == ",", "expected , or } at " .. i)
            end
        elseif c == "[" then
            local t = {}
            i = i + 1
            ws()
            if s:sub(i, i) == "]" then
                i = i + 1
                return t
            end
            while true do
                t[#t + 1] = value()
                ws()
                local d = s:sub(i, i)
                i = i + 1
                if d == "]" then
                    return t
                end
                assert(d == ",", "expected , or ] at " .. i)
            end
        elseif c == '"' then
            return str()
        elseif s:sub(i, i + 3) == "true" then
            i = i + 4
            return true
        elseif s:sub(i, i + 4) == "false" then
            i = i + 5
            return false
        elseif s:sub(i, i + 3) == "null" then
            i = i + 4
            return nil
        end
        local num = s:match("^-?%d+%.?%d*[eE]?[-+]?%d*", i)
        assert(num and #num > 0, "unexpected " .. c .. " at " .. i)
        i = i + #num
        return tonumber(num)
    end
    return value()
end

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
    for _, name in ipairs({ "round4", "minimal", "columns", "halo", "terminal", "shards", "corners" }) do
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
