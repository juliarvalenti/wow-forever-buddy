-- The default HUD (HUD1): a full 1920x1080 screen of the game, drawn by
-- WOWAPI, with any addons on top (ForeverBuddy, or a third-party addon
-- such as EllesmereUI for SIM6).
--
--   luajit tools/addon-shots/hud.lua <out.svg> [--blizzard <interface/addons dir>] [--addon <dir>]...
--       [--background <texture>] [--ui-scale <pixels per unit>]
--
-- With --blizzard (the CASC extract's interface/addons, local only), the
-- screen is Blizzard's own UI: every Blizzard addon the client loads at
-- login, in its load order, with Edit Mode placing the HUD from its
-- default layout. Without it, WOWAPI's stand-in FrameXML (what CI has).
-- Set WOWAPI_ART (and WOWAPI_OFFLINE=1) to draw with the extract's art.
--
-- It looks like logging in: Thrandor, a level 12 warrior in Elwynn, at
-- 85% health with Hogger targeted, the UI at the client's 1080p scale
-- (768 units tall). There's no 3D world; --background draws a texture in
-- its place (a loading screen from the extract, say).
--
-- Prints a summary of the Lua errors, most frequent first. This script only
-- writes files; it never starts a browser or opens a window.

local out, blizzard, extra, background, uiScale = nil, nil, {}, nil, 1080 / 768
local i = 1
while arg[i] do
    if arg[i] == "--blizzard" then
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
package.path = wowapiDir .. "/?.lua;" .. wowapiDir .. "/?/init.lua;" .. package.path
local WoW = require("wowapi")
local toc = require("wowapi.toc")
local render = require("wowapi.render")

local function quote(s)
    return "'" .. s:gsub("'", "'\\''") .. "'"
end

local function readFile(path)
    local f = io.open(path, "rb")
    if not f then
        return nil
    end
    local text = f:read("*a")
    f:close()
    return text
end

-- The Blizzard addons the client loads at login, in its order: not load on
-- demand, not glue-only, allowed for this game type (or its family);
-- FrameXML and its dependencies first (its SecureTemplates are inherited
-- by addons that don't list it), then LoadFirst addons, then the rest by
-- name, each after its dependencies.
local function loginOrder(dir)
    local found = {}
    local p = assert(io.popen("ls " .. quote(dir)))
    for name in p:lines() do
        local path = toc.findToc(dir .. "/" .. name)
        local text = path and readFile(path)
        if text then
            local t = toc.parse(text, name)
            local md = t.metadata
            local function on(v)
                return v and v:match("1") and toc.gameTypeAllowed(v)
            end
            local glue = md.allowload and md.allowload:lower():match("^%s*glue")
            if t.gameTypeOk and not on(md.loadondemand) and not glue then
                found[name:lower()] = { dir = dir .. "/" .. name, deps = t.deps, first = on(md.loadfirst) }
            end
        end
    end
    p:close()
    local order, seen = {}, {}
    local function visit(key)
        key = key:lower()
        local a = found[key]
        if seen[key] or not a then
            return
        end
        seen[key] = true
        for _, d in ipairs(a.deps) do
            visit(d)
        end
        order[#order + 1] = a.dir
    end
    local names = {}
    for k in pairs(found) do
        names[#names + 1] = k
    end
    table.sort(names, function(a, b)
        local fa, fb = found[a].first and 0 or 1, found[b].first and 0 or 1
        if fa ~= fb then
            return fa < fb
        end
        return a < b
    end)
    visit("blizzard_framexml")
    for _, k in ipairs(names) do
        visit(k)
    end
    return order
end

local art = os.getenv("WOWAPI_ART") ~= nil
local sim = WoW.new({
    player = { name = "Thrandor", class = "WARRIOR", race = "Human", level = 12, money = 25000, zone = "Elwynn Forest",
        health = 425, healthMax = 500 },
    art = art,
    offline = os.getenv("WOWAPI_OFFLINE") == "1" or not art,
    quiet = true,
    -- Unmocked APIs return empty defaults, not random data: Blizzard's
    -- AuraUtil.ForEachAura never ends on random auras.
    fakeData = false,
    apiDocsDir = blizzard and (blizzard .. "/blizzard_apidocumentationgenerated") or nil,
})

-- The client's UI scale at 1080p: UIParent is 768 units tall, each unit
-- 1080/768 pixels. Set before anything loads, since Blizzard's code reads
-- the screen size as it loads.
local uiParent = sim:Get("UIParent")
sim.widgetState[uiParent].scale = uiScale
local screenW, screenH = 1920 / uiScale, 1080 / uiScale
sim:Mock("GetScreenWidth", function()
    return screenW
end)
sim:Mock("GetScreenHeight", function()
    return screenH
end)

-- Edit Mode, as on a fresh install: no saved layouts (so the active one is
-- Blizzard's preset 1, Modern) and its account settings (its own "show in
-- Edit Mode" toggles) off, grid spacing 100.
local function layouts()
    return { layouts = {}, activeLayout = 1 }
end
sim:Mock("C_EditMode.GetLayouts", layouts)
sim:Mock("C_EditMode.GetAccountSettings", function()
    local settings = {}
    for name, v in pairs(sim.env.Enum.EditModeAccountSetting or {}) do
        settings[#settings + 1] = { setting = v, value = name == "GridSpacing" and 100 or 0 }
    end
    return settings
end)

local loaded = 0
for _, dir in ipairs(blizzard and loginOrder(blizzard) or {}) do
    if sim:LoadAddon(dir) then
        loaded = loaded + 1
    end
end
for _, dir in ipairs(extra) do
    local ok, why = sim:LoadAddon(dir)
    if not ok then
        print("not loaded: " .. dir .. " (" .. tostring(why) .. ")")
    end
end

sim:Login()
sim:SetTarget({ name = "Hogger", level = 11, classification = "elite", creatureType = "Humanoid",
    reaction = 2, health = 520, healthMax = 666, class = "WARRIOR", guid = "Creature-0-0-0-0-448-0" })
-- The client announces the layouts at login; Blizzard's Edit Mode places
-- the HUD then. An error in it is the UI's, recorded like any other.
local em = blizzard and sim:Get("EditModeManagerFrame")
if em and type(em.InitializeAccountSettings) == "function" then
    sim:_pcall(function()
        em:InitializeAccountSettings()
    end)
    sim:FireEvent("EDIT_MODE_LAYOUTS_UPDATED", layouts(), false)
end
sim:Advance(2)
if art then
    sim:_registerAddonArt()
end

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
