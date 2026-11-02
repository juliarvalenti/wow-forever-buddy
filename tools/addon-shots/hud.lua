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
-- layout would.
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
package.path = wowapiDir .. "/?.lua;" .. wowapiDir .. "/?/init.lua;" .. package.path
local WoW = require("wowapi")
local toc = require("wowapi.toc")
local render = require("wowapi.render")
package.path = here .. "/?.lua;" .. package.path
local fixtures = require("fixtures")
local fixturePlayer = class and fixtures.player(class, level)
if class and not fixturePlayer then
    local known = {}
    for k in pairs(fixtures.CLASSES) do
        known[#known + 1] = k
    end
    error("no fixture for class " .. class .. " (have: " .. table.concat(known, ", ") .. ")", 0)
end

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

-- Blizzard's UI needs Forever's own data, which isn't in git: without it the
-- unit frames draw hollow and Forever-only art as grey boxes. Say so.
if blizzard then
    for file, script in pairs({ ["globalcolor.csv"] = "fetch-globalcolors.sh",
        ["atlases-forever.txt"] = "fetch-forever-atlases.sh", ["globalstrings-forever.csv"] = "fetch-forever-strings.sh" }) do
        local f = io.open(wowapiDir .. "/wowapi/data/" .. file, "r")
        if f then
            f:close()
        else
            io.stderr:write(("warning: no wowapi/data/%s; run tools/wowapi/%s once\n"):format(file, script))
        end
    end
end
local sim = WoW.new({
    player = fixturePlayer or { name = "Thrandor", class = "WARRIOR", race = "Human", level = 12, money = 25000,
        zone = "Elwynn Forest", health = 425, healthMax = 500 },
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
--
-- With --layout, a saved layout instead: Forever's preset with the named
-- systems moved, made active (saved layouts come after the presets).
-- The file is either Lua returning a list of moves, or the export string
-- the game's Edit Mode gives (Share > Copy to Clipboard).
local layoutMoves, layoutString
if layout then
    local file = layout:find("[/\\]") and layout or (here .. "/layouts/" .. layout .. ".lua")
    local f = assert(io.open(file, "r"))
    local text = f:read("*a")
    f:close()
    if text:match("^%s*%d") then
        layoutString = require("editmode_string").decode(text)
    else
        layoutMoves = assert(assert(loadstring(text, "@" .. file))(), "a layout file returns a list of moves")
    end
end
local function layouts()
    if not (layoutMoves or layoutString) then
        return { layouts = {}, activeLayout = 1 }
    end
    local env = sim.env
    local presets = env.EditModePresetLayoutManager:GetCopyOfPresetLayouts()
    local saved = presets[1]
    saved.layoutName = "Scene"
    saved.layoutType = env.Enum.EditModeLayoutType.Account
    for _, s in ipairs(layoutString and layoutString.systems or {}) do
        local found = false
        for _, info in ipairs(saved.systems) do
            if info.system == s.system and info.systemIndex == s.systemIndex then
                found = true
                info.isInDefaultPosition = s.isInDefaultPosition
                if not s.isInDefaultPosition then
                    info.anchorInfo, info.anchorInfo2 = s.anchorInfo, s.anchorInfo2
                end
                local have = {}
                for _, setting in ipairs(info.settings) do
                    have[setting.setting] = setting
                end
                for id, value in pairs(s.settings) do
                    if have[id] then
                        have[id].value = value
                    else
                        table.insert(info.settings, { setting = id, value = value })
                    end
                end
            end
        end
        if not found then
            print(("layout string: no system %d/%s here, skipped"):format(s.system, tostring(s.systemIndex)))
        end
    end
    for _, move in ipairs(layoutMoves or {}) do
        local system = env.Enum.EditModeSystem[move.system]
        local indices = env.Enum["EditMode" .. move.system .. "SystemIndices"]
        local index = move.index and indices and indices[move.index] or nil
        local found = false
        for _, info in ipairs(saved.systems) do
            if info.system == system and info.systemIndex == index then
                info.anchorInfo = { point = move.point, relativeTo = move.relativeTo or "UIParent",
                    relativePoint = move.relativePoint or move.point, offsetX = move.x or 0, offsetY = move.y or 0 }
                info.isInDefaultPosition = false
                found = true
            end
        end
        assert(found, "no Edit Mode system " .. move.system .. (move.index and ("." .. move.index) or ""))
    end
    return { layouts = { saved }, activeLayout = #presets + 1 }
end
sim:Mock("C_EditMode.GetLayouts", layouts)
sim:Mock("C_EditMode.GetAccountSettings", function()
    local settings = {}
    for name, v in pairs(sim.env.Enum.EditModeAccountSetting or {}) do
        settings[#settings + 1] = { setting = v, value = name == "GridSpacing" and 100 or 0 }
    end
    return settings
end)

-- Nothing timed at login (breath, fatigue): the client answers "UNKNOWN"
-- for each mirror timer. And the personal resource display is off, as on a
-- fresh install.
sim:Mock("GetMirrorTimerInfo", function()
    return "UNKNOWN", 0, 0, 0, false, ""
end)
sim:Mock("GetMirrorTimerProgress", function()
    return 0
end)
sim:Mock("C_GameRules.IsPersonalResourceDisplayEnabled", function()
    return false
end)
-- No new pets waiting to be shown off (the collections micro button asks).
sim:Mock("C_PetJournal.GetNumPetsNeedingFanfare", function()
    return 0
end)
-- Not in a pet battle (the action bar controller asks before paging).
sim:Mock("C_PetBattles.IsInBattle", function()
    return false
end)

if class then
    fixtures.mock(sim, class, { stealth = stealth })
end

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
-- The client's settings load at login too; the extra action bars show or
-- hide then (MultiActionBar_Update).
sim:FireEvent("SETTINGS_LOADED")
if class then
    fixtures.apply(sim, class)
else
    sim:SetTarget({ name = "Hogger", level = 11, classification = "elite", creatureType = "Humanoid",
        reaction = 2, health = 520, healthMax = 666, class = "WARRIOR", guid = "Creature-0-0-0-0-448-0" })
end
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
