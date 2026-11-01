-- SIM6: EllesmereUI in the emulator. Loads every addon in an AddOns folder
-- (EllesmereUI's, fetched at its pinned commit, plus ForeverBuddy with the
-- slots the app writes) into WOWAPI, logs in, and writes SVGs of the whole
-- screen (Ellesmere's own frames) and of our window with Ellesmere's skin,
-- plus a report of what loaded, what errored and which frames show.
--
-- EllesmereUI is someone else's code, so this runs only in CI
-- (.github/workflows/sim6-ellesmere.yml), never on a developer's machine.
-- In CI it draws on WOWAPI's stand-in FrameXML with the downloaded retail
-- art, not Forever's own (the extract never leaves Julia's Mac).
--
--   lua5.1 tools/addon-shots/ellesmere.lua <AddOns folder> <out dir> [--no-art]

local addonsDir, out = arg[1], arg[2]
assert(addonsDir and out, "usage: ellesmere.lua <AddOns folder> <out dir> [--no-art]")
local art = arg[3] ~= "--no-art"

local here = (arg[0]:match("^(.*)[/\\]") or ".")
local wowapiDir = here .. "/../wowapi"
package.path = wowapiDir .. "/?.lua;" .. wowapiDir .. "/?/init.lua;" .. package.path
local WoW = require("wowapi")
local layout = require("wowapi.layout")
local render = require("wowapi.render")

local function quote(s)
    return "'" .. tostring(s):gsub("'", "'\\''") .. "'"
end
os.execute("mkdir -p " .. quote(out))

-- Thrandor Vargur, as in scene.lua, an hour after the app wrote the slots.
-- fakeData off: unmocked APIs answer nil rather than random data (random
-- auras send AuraUtil.ForEachAura round forever).
local sim = WoW.new({
    player = { name = "Thrandor", class = "WARRIOR", race = "Human", level = 12, money = 25000, zone = "Elwynn Forest" },
    now = 1790960000 + 3600,
    art = art,
    offline = not art,
    quiet = true,
    fakeData = false,
})
sim:Mock("UnitName", function(unit)
    if unit == "player" then
        return "Thrandor", "Vargur"
    end
end)

local report = {}
local function add(s)
    report[#report + 1] = s
end

local okLoad, loadErr = pcall(sim.LoadAllAddons, sim, addonsDir)
local okLogin, loginErr = okLoad and pcall(function()
    sim:Login()
    sim:Advance(6)
end)

-- A screen's SVG, cropped to `frames` (and a margin) when any is shown.
local written = {}
local function shot(name, frames)
    if art then
        pcall(sim._registerAddonArt, sim)
    end
    local ok, svg = pcall(render.svg, sim, { art = art, embed = true })
    if not ok then
        add("- `" .. name .. "`: the renderer failed: " .. tostring(svg):gsub("\n.*", ""))
        return
    end
    local l, b, r, t
    for _, f in ipairs(frames or {}) do
        if f and sim._isVisible(f) then
            local x, y, w, h = layout.rect(sim, f)
            if x then
                l, b = math.min(l or x, x), math.min(b or y, y)
                r, t = math.max(r or x + w, x + w), math.max(t or y + h, y + h)
            end
        end
    end
    if l then
        local M = 12
        local w, h = math.ceil(r - l + 2 * M), math.ceil(t - b + 2 * M)
        local y = layout.SCREEN_H - (math.floor(b - M) + h)
        svg = svg:gsub('viewBox="0 0 %d+ %d+" width="%d+" height="%d+"',
            string.format('viewBox="%d %d %d %d" width="%d" height="%d"', math.floor(l - M), y, w, h, w, h), 1)
    end
    local f = assert(io.open(out .. "/" .. name .. ".svg", "w"))
    f:write(svg)
    f:close()
    written[#written + 1] = name
end

if okLogin then
    -- 1. The whole screen: Ellesmere's unit frames, action bars and the rest.
    shot("ellesmere-screen")
    -- 2. Our window with Ellesmere's skin, Plan and Settings tabs.
    pcall(sim.Slash, sim, "/fb plan")
    sim:Advance(1)
    local w = sim:Get("ForeverBuddyWindow")
    local around = { w }
    for _, tab in ipairs(type(w) == "table" and w.Tabs or {}) do
        around[#around + 1] = tab
    end
    shot("ellesmere-fb-plan", around)
    if type(w) == "table" and w.Tabs and w.Tabs[5] then
        pcall(sim.Click, sim, w.Tabs[5])
        sim:Advance(1)
        shot("ellesmere-fb-settings", around)
    end
    shot("ellesmere-screen-with-window")
end

-- The report.
local r = {}
local function line(s)
    r[#r + 1] = s
end
line("# SIM6: EllesmereUI in the emulator")
line("")
line(("WOWAPI's stand-in FrameXML, %s art. Login as Thrandor Vargur, the app's slots, /fb."):format(
    art and "downloaded retail" or "placeholder"))
line("")
line("## Addons")
line("")
local names = {}
for name, a in pairs(sim.addons) do
    names[#names + 1] = ("- %s: %s"):format(name, a.loaded and "loaded" or "not loaded")
end
table.sort(names)
for _, n in ipairs(names) do
    line(n)
end
if not okLoad then
    line("- loading failed: " .. tostring(loadErr))
end
if okLoad and not okLogin then
    line("- login failed: " .. tostring(loginErr))
end
line("")
line("## Ellesmere's skin on our window")
line("")
local E = sim:Get("EllesmereUI")
line("- EllesmereUI global: " .. (type(E) == "table" and "yes" or "no"))
line("- RegisterSkin: " .. ((type(E) == "table" and type(E.RegisterSkin) == "function") and "yes" or "no"))
line("")
line("## Shown frames by addon prefix")
line("")
local counts = {}
for obj, s in pairs(sim.widgetState) do
    if s.isFrame and sim._isVisible(obj) and type(obj.GetName) == "function" then
        local name = obj:GetName()
        if type(name) == "string" then
            local prefix = name:match("^(Ellesmere%a*)") or name:match("^(ForeverBuddy)") or name:match("^(EUI)")
            if prefix then
                counts[prefix] = (counts[prefix] or 0) + 1
            end
        end
    end
end
local keys = {}
for k in pairs(counts) do
    keys[#keys + 1] = k
end
table.sort(keys)
for _, k in ipairs(keys) do
    line(("- %s: %d"):format(k, counts[k]))
end
if #keys == 0 then
    line("- none")
end
line("")
line("## Lua errors")
line("")
local seen, order = {}, {}
for _, e in ipairs(sim.errors) do
    local msg = tostring(type(e) == "table" and (e.message or e.msg) or e):gsub("\n.*", "")
    if not seen[msg] then
        order[#order + 1] = msg
    end
    seen[msg] = (seen[msg] or 0) + 1
end
for i = 1, math.min(40, #order) do
    local m = order[i]
    line("- `" .. m:sub(1, 220) .. "`" .. (seen[m] > 1 and (" (x" .. seen[m] .. ")") or ""))
end
if #order > 40 then
    line(("- ... and %d more"):format(#order - 40))
end
if #order == 0 then
    line("- none")
end
line("")
line("## Pictures")
line("")
for _, n in ipairs(written) do
    line("- " .. n)
end
for _, s in ipairs(report) do
    line(s)
end
local f = assert(io.open(out .. "/report.md", "w"))
f:write(table.concat(r, "\n") .. "\n")
f:close()
print(table.concat(r, "\n"))
