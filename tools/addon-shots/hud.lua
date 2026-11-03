-- The default HUD (HUD1): a full 1920x1080 screen of the game, drawn by
-- WOWAPI, with any addons on top (ForeverBuddy, or a third-party addon
-- such as EllesmereUI for SIM6).
--
--   luajit tools/addon-shots/hud.lua <out.svg> [--blizzard <interface/addons dir>] [--addon <dir>]...
--       [--background <texture>] [--ui-scale <pixels per unit>] [--class <class> [--level <n>]]
--       [--layout <name or file>] [--stealth]
--
-- With --blizzard (the CASC extract's interface/addons, local only), the
-- screen is Blizzard's own UI: every Blizzard addon the client loads at
-- login, in its load order, with Edit Mode placing the HUD from its
-- default layout. Without it, WOWAPI's stand-in FrameXML (what CI has).
-- Set WOWAPI_ART (and WOWAPI_OFFLINE=1) to draw with the extract's art.
--
-- It looks like logging in: Thrandor, a level 12 warrior in Elwynn, at
-- 85% health with Hogger targeted, the UI at the client's 1080p scale
-- (768 units tall). --class picks a character from fixtures.lua instead
-- (rogue: spells on the bar, energy, combo points, a target). There's no
-- 3D world; --background draws a texture in its place. --layout moves Edit
-- Mode systems (layouts/<name>.lua, or a file of the same shape), as a saved
-- layout would. The scene itself is login_scene.lua, which addon-live's
-- `scene` command uses too.
--
-- Prints a summary of the Lua errors, most frequent first. This script only
-- writes files; it never starts a browser or opens a window.

local out, blizzard, extra, background, uiScale = nil, nil, {}, nil, 1080 / 768
local class, level, layout, stealth
local i = 1
while arg[i] do
    if arg[i] == "--stealth" then
        stealth, i = true, i + 1
    elseif arg[i] == "--layout" then
        layout, i = arg[i + 1], i + 2
    elseif arg[i] == "--class" then
        class, i = arg[i + 1]:lower(), i + 2
    elseif arg[i] == "--level" then
        level, i = tonumber(arg[i + 1]), i + 2
    elseif arg[i] == "--blizzard" then
        blizzard, i = arg[i + 1], i + 2
    elseif arg[i] == "--addon" then
        extra[#extra + 1], i = arg[i + 1], i + 2
    elseif arg[i] == "--background" then
        background, i = arg[i + 1], i + 2
    elseif arg[i] == "--ui-scale" then
        uiScale, i = tonumber(arg[i + 1]), i + 2
    else
        out, i = arg[i], i + 1
    end
end
assert(out, "usage: hud.lua <out.svg> [--blizzard <interface/addons dir>] [--addon <dir>]...")

local here = (arg[0]:match("^(.*)[/\\]") or ".")
local wowapiDir = here .. "/../wowapi"
package.path = wowapiDir .. "/?.lua;" .. wowapiDir .. "/?/init.lua;" .. here .. "/?.lua;" .. package.path
local render = require("wowapi.render")
local scene = require("login_scene")

local art = os.getenv("WOWAPI_ART") ~= nil
local sim, loaded = scene.boot({ blizzard = blizzard, extra = extra, class = class, level = level, layout = layout,
    stealth = stealth, uiScale = uiScale, art = art })

local f = assert(io.open(out, "w"))
f:write(render.svg(sim, { art = art, embed = true, background = art and background or nil }))
f:close()

local counts, list = {}, {}
for _, e in ipairs(sim.errors) do
    local msg = tostring(e.message or e):gsub("\n.*", ""):gsub("^.-/interface/addons/", ""):sub(1, 160)
    counts[msg] = (counts[msg] or 0) + 1
end
for msg, n in pairs(counts) do
    list[#list + 1] = { msg, n }
end
table.sort(list, function(a, b)
    return a[2] > b[2]
end)
print(("wrote %s: %d Blizzard addons, %d more, %d Lua errors"):format(out, loaded, #extra, #sim.errors))
for k = 1, math.min(15, #list) do
    print(("%5d  %s"):format(list[k][2], list[k][1]))
end
