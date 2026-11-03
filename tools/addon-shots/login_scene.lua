-- The logged-in scene (HUD2), shared by hud.lua (one picture) and
-- addon-live's `scene` command (a session you drive): WOWAPI with
-- Blizzard's own UI loaded from the CASC extract, a fixture character
-- logged in, its target picked, and Edit Mode placing the HUD.
--
--   local scene = require("login_scene")
--   local sim, loaded = scene.boot({ blizzard = dir, class = "rogue", layout = "centered" })
--
-- The caller puts tools/wowapi and tools/addon-shots on package.path.

local WoW = require("wowapi")
local toc = require("wowapi.toc")
local fixtures = require("fixtures")

local M = {}

local here = debug.getinfo(1, "S").source:match("^@(.*)[/\\]") or "."
local wowapiDir = here .. "/../wowapi"

-- Files through wowapi.host: the app's live preview (PV1) boots this scene
-- with no io or shell.
local host = require("wowapi.host")

local function readFile(path)
    return (host.read(path))
end

-- The Blizzard addons the client loads at login, in its order: not load on
-- demand, not glue-only, allowed for this game type (or its family);
-- FrameXML and its dependencies first (its SecureTemplates are inherited
-- by addons that don't list it), then LoadFirst addons, then the rest by
-- name, each after its dependencies.
function M.loginOrder(dir)
    local found = {}
    local names = host.list(dir) or {}
    table.sort(names)
    for _, name in ipairs(names) do
        local path = toc.findToc(dir .. "/" .. name)
        local text = path and readFile(path)
        if text then
            local t = toc.parse(text, name)
            local md = t.metadata
            local function on(v)
                return v and v:match("1") and toc.gameTypeAllowed(v)
            end
            local glue = md.allowload and md.allowload:lower():match("^%s*glue")
            -- The scene is the live client: no beta/PTR-only addons (the
            -- PTR Issue Reporter box in the middle of the screen).
            local ptrOnly = on(md.onlybetaandptr)
            if t.gameTypeOk and not on(md.loadondemand) and not glue and not ptrOnly then
                found[name:lower()] = { dir = dir .. "/" .. name, deps = t.deps, first = on(md.loadfirst) }
            end
        end
    end
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

-- Blizzard's UI needs Forever's own data, which isn't in git: without it the
-- unit frames draw hollow and Forever-only art as grey boxes. Say so.
local function warnMissingData()
    for file, script in pairs({ ["globalcolor.csv"] = "fetch-globalcolors.sh",
        ["atlases-forever.txt"] = "fetch-forever-atlases.sh", ["globalstrings-forever.csv"] = "fetch-forever-strings.sh" }) do
        if not host.exists(wowapiDir .. "/wowapi/data/" .. file) and not host.embedded then
            io.stderr:write(("warning: no wowapi/data/%s; run tools/wowapi/%s once\n"):format(file, script))
        end
    end
end

-- A layout: Lua returning a list of moves, or the export string the game's
-- Edit Mode gives (Share > Copy to Clipboard). A name without a slash is
-- layouts/<name>.lua.
local function readLayout(layout)
    local file = layout:find("[/\\]") and layout or (here .. "/layouts/" .. layout .. ".lua")
    local text = assert(host.read(file))
    if text:match("^%s*%d") then
        return nil, require("editmode_string").decode(text)
    end
    return assert(assert(loadstring(text, "@" .. file))(), "a layout file returns a list of moves")
end

-- opts: blizzard (the extract's interface/addons, or nil for WOWAPI's
-- stand-ins), extra (more addon dirs), class, level, stealth, layout,
-- uiScale (default 1080/768), art (default: WOWAPI_ART is set).
-- Returns the sim, logged in and settled, and how many Blizzard addons
-- loaded.
function M.boot(opts)
    opts = opts or {}
    local blizzard, extra, class = opts.blizzard, opts.extra or {}, opts.class
    local uiScale = opts.uiScale or 1080 / 768
    local art = opts.art
    if art == nil and os then
        art = os.getenv("WOWAPI_ART") ~= nil
    end
    local fixturePlayer = class and fixtures.player(class, opts.level)
    if class and not fixturePlayer then
        local known = {}
        for k in pairs(fixtures.CLASSES) do
            known[#known + 1] = k
        end
        error("no fixture for class " .. class .. " (have: " .. table.concat(known, ", ") .. ")", 0)
    end
    if blizzard then
        warnMissingData()
    end

    local sim = WoW.new({
        player = fixturePlayer or { name = "Thrandor", class = "WARRIOR", race = "Human", level = 12, money = 25000,
            zone = "Elwynn Forest", health = 425, healthMax = 500 },
        art = art,
        offline = opts.offline or not art or (os and os.getenv and os.getenv("WOWAPI_OFFLINE") == "1"),
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

    -- Edit Mode, as on a fresh install: no saved layouts (so the active one
    -- is Blizzard's preset 1) and its account settings (its own "show in
    -- Edit Mode" toggles) off, grid spacing 100. With a layout, a saved one
    -- instead: the preset with the layout's systems moved, made active
    -- (saved layouts come after the presets).
    local layoutMoves, layoutString
    if opts.layout then
        layoutMoves, layoutString = readLayout(opts.layout)
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

    -- Nothing timed at login (breath, fatigue): the client answers
    -- "UNKNOWN" for each mirror timer. And the personal resource display is
    -- off, as on a fresh install.
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
    -- No temporary weapon enchants (the buff frame lists them with the
    -- auras, and stops on a nil list).
    sim:Mock("C_Item.GetWeaponEnchantInfo", function()
        return {}
    end)
    -- Not in a scenario (C_Scenario isn't in Forever's API docs, and the
    -- objective tracker's scenario module goes first, so it stopping kept
    -- the quests from drawing).
    sim:Mock("C_Scenario.IsInScenario", function()
        return false
    end)
    sim:Mock("C_Scenario.ShouldShowCriteria", function()
        return false
    end)
    sim:Mock("C_Scenario.GetInfo", function()
        return "", 0, 0, 0, false, false, false, 0, 0, 0, false, nil, 0
    end)
    sim:Mock("C_Scenario.GetStepInfo", function()
        return "", "", 0, false, false, false, false, 0, {}, 0, 0, 0
    end)
    sim:Mock("C_Scenario.GetBonusSteps", function()
        return {}
    end)
    sim:Mock("C_Scenario.GetProvingGroundsInfo", function()
        return 0, 0, 0, 0
    end)
    sim:Mock("C_Scenario.GetSupersededObjectives", function()
        return {}
    end)
    -- No bonus objectives (world tasks) in progress, no quest pop-ups.
    sim:Mock("GetTasksTable", function()
        return {}
    end)
    sim:Mock("GetNumAutoQuestPopUps", function()
        return 0
    end)
    -- Not in a pet battle (the action bar controller asks before paging).
    sim:Mock("C_PetBattles.IsInBattle", function()
        return false
    end)

    if class then
        fixtures.mock(sim, class, { stealth = opts.stealth })
    end

    local loaded = 0
    for _, dir in ipairs(blizzard and M.loginOrder(blizzard) or {}) do
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
    -- The client's settings load at login too; the extra action bars show
    -- or hide then (MultiActionBar_Update).
    sim:FireEvent("SETTINGS_LOADED")
    if class then
        fixtures.apply(sim, class)
    else
        sim:SetTarget({ name = "Hogger", level = 11, classification = "elite", creatureType = "Humanoid",
            reaction = 2, hostile = true, health = 520, healthMax = 666, class = "WARRIOR",
            guid = "Creature-0-0-0-0-448-0" })
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
    return sim, loaded
end

return M
