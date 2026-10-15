-- Pictures of the in-game window (G1): runs ForeverBuddy in WOWAPI, the
-- WoW: Forever client emulator, and writes one SVG of the window per tab,
-- with the slots the app writes and with the empty ones the addon ships.
-- CI turns them into PNGs for the designer (.github/workflows/addon-shots.yml).
--
--   lua5.1 tools/addon-shots/scene.lua <out dir> [--no-art]
--
-- WOWAPI is vendored, with our patches, in tools/wowapi (see its ORIGIN.md).
-- This script only writes files; it never starts a browser or opens a window.

local out = arg[1]
assert(out, "usage: scene.lua <out dir> [--no-art]")
local art = arg[2] ~= "--no-art"

local here = (arg[0]:match("^(.*)[/\\]") or ".")
local ROOT = here .. "/../.."
local wowapiDir = ROOT .. "/tools/wowapi"
local ADDON = ROOT .. "/src-tauri/resources/addon/ForeverBuddy"
-- The slots the app's send_to_game writes, from src-tauri/src/contract.rs
-- (H5), for Thrandor Vargur.
local BRIDGE = ROOT .. "/src-tauri/tests/fixtures/bridge"
local BRIDGE_STAMP = 1790960000
local SLOTS = { "Tooltip1", "Tooltip2", "Plan", "Briefing", "Lists", "Cleanup" }

package.path = wowapiDir .. "/?.lua;" .. wowapiDir .. "/?/init.lua;" .. package.path
local WoW = require("wowapi")
local layout = require("wowapi.layout")
local render = require("wowapi.render")

local function sh(cmd)
    local ok = os.execute(cmd)
    assert(ok == 0 or ok == true, "failed: " .. cmd)
end

local function quote(s)
    return "'" .. s:gsub("'", "'\\''") .. "'"
end

-- A copy of the addon with its Data/ slots taken from `from`, or as shipped.
local function addonCopy(name, from)
    local dir = out .. "/.addon-" .. name .. "/ForeverBuddy"
    sh("rm -rf " .. quote(dir) .. " && mkdir -p " .. quote(dir) .. " && cp -R " .. quote(ADDON) .. "/. " .. quote(dir))
    if from then
        for _, slot in ipairs(SLOTS) do
            sh("cp " .. quote(from .. "/" .. slot .. ".lua") .. " " .. quote(dir .. "/Data/" .. slot .. ".lua"))
        end
    end
    return dir
end

-- Thrandor Vargur (Forever characters have a surname), level 12 in Elwynn,
-- an hour after the app wrote the slots.
local function boot(dir)
    local sim = WoW.new({
        player = { name = "Thrandor", class = "WARRIOR", race = "Human", level = 12, money = 25000, zone = "Elwynn Forest" },
        now = BRIDGE_STAMP + 3600,
        art = art,
        offline = not art,
        quiet = true,
    })
    sim:Mock("UnitName", function(unit)
        if unit == "player" then
            return "Thrandor", "Vargur"
        end
    end)
    sim:AddItem(2589, { name = "Linen Cloth", quality = 1 })
    sim:AddItem(6948, { name = "Hearthstone", quality = 1 })
    sim:AddItem(14047, { name = "Runecloth", quality = 1 })
    sim:AddItem(117, { name = "Tough Jerky", quality = 1 })
    sim.bags[0][1] = { itemID = 2589, stackCount = 4 }
    sim.bags[0][2] = { itemID = 6948, stackCount = 1 }
    assert(sim:LoadAddon(dir))
    sim:Login()
    sim:Advance(6)
    return sim
end

-- The window and its tab buttons (which hang below it), plus a margin.
local function bounds(sim)
    local l, b, r, t
    local function add(frame)
        local x, y, w, h = layout.rect(sim, frame)
        if x then
            l, b = math.min(l or x, x), math.min(b or y, y)
            r, t = math.max(r or x + w, x + w), math.max(t or y + h, y + h)
        end
    end
    add(sim:Get("ForeverBuddyWindow"))
    for i = 1, 5 do
        add(sim:Get("ForeverBuddyWindowTab" .. i))
    end
    assert(l, "the window has no position")
    local M = 12
    return math.floor(l - M), math.floor(b - M), math.ceil(r - l + 2 * M), math.ceil(t - b + 2 * M)
end

local written = {}

-- The whole screen as SVG, cropped to the window.
local function shot(sim, name)
    local svg = render.svg(sim, { art = art, embed = true })
    local l, b, w, h = bounds(sim)
    local y = layout.SCREEN_H - (b + h)
    local n
    svg, n = svg:gsub('viewBox="0 0 %d+ %d+" width="%d+" height="%d+"',
        string.format('viewBox="%d %d %d %d" width="%d" height="%d"', l, y, w, h, w, h), 1)
    assert(n == 1, "WOWAPI's SVG header changed")
    local f = assert(io.open(out .. "/" .. name .. ".svg", "w"))
    f:write(svg)
    f:close()
    written[#written + 1] = name
end

local TABS = { "plan", "list", "errands", "cleanup" }

local function tabs(sim, state)
    for _, tab in ipairs(TABS) do
        sim:Slash("/fb " .. tab)
        sim:Advance(1)
        shot(sim, state .. "-" .. tab)
    end
    sim:Click(sim:Get("ForeverBuddyWindowTab5"))
    sim:Advance(1)
    shot(sim, state .. "-settings")
end

sh("mkdir -p " .. quote(out))
local problems = {}
for _, run in ipairs({ { "filled", BRIDGE }, { "empty", nil } }) do
    local sim = boot(addonCopy(run[1], run[2]))
    tabs(sim, run[1])
    for _, e in ipairs(sim.errors) do
        problems[#problems + 1] = run[1] .. ": " .. tostring(e.message or e.msg or e)
    end
    sh("rm -rf " .. quote(out .. "/.addon-" .. run[1]))
end

print("wrote " .. #written .. " pictures to " .. out .. ": " .. table.concat(written, ", "))
if #problems > 0 then
    print("Lua errors in WOWAPI:\n  " .. table.concat(problems, "\n  "))
    os.exit(1)
end
