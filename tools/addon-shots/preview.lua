-- The app's live preview (PV1, src-tauri/src/preview): the logged-in scene
-- (login_scene.lua) drawn to SVG, for the Preview tab.
--
-- The app runs this in its own Lua state, which has no io, os or shell:
-- every file comes through WOWAPI_HOST (wowapi.host), confined by the app to
-- the bundled code, the downloaded WOWAPI data, the addon with the slots the
-- app would send the game, and (only with the art switch on) the player's
-- own extracted game files.
--
--   local preview = require("preview")
--   local svg, errors = preview.draw({ addon = dir, class = "rogue", art = false })
--
-- Each draw boots a fresh scene, so the addon applies the settings profile
-- in its Data/Settings.lua at login, as in the game. With art off that takes
-- a fraction of a second.

local scene = require("login_scene")
local render = require("wowapi.render")
local fixtures = require("fixtures")

local M = {}

-- The moments the Your UI tab can show (ED1). Each is the logged-in scene
-- plus what the game would do: combat enters combat (lockdown and its
-- events, as in game), stealth shows the stealth bar, low health drops the
-- player to a quarter of their health. Standing around is the scene as it
-- logs in. UI3: no target is out of combat with nothing targeted (a UI's
-- quiet look); hidden is combat as Forever's API docs have it, with every
-- value the game marks secret hidden from the addon (the player's health
-- included), so a picture shows what a design really does then.
M.MOMENTS = { standing = true, combat = true, stealth = true, low_health = true, no_target = true, hidden = true }
M.MOMENT_ORDER = { "standing", "combat", "stealth", "low_health", "no_target", "hidden" }

-- UI3: every class there's a test character for, in the game's order.
M.CLASSES = fixtures.ORDER

local function enter(sim, moment)
    if moment == "combat" or moment == "hidden" then
        sim:EnterCombat()
        -- A fight goes on: health, power and auras change once combat has
        -- started (and, in hidden, once their values are hidden), as they
        -- would a moment into it.
        for _, unit in ipairs({ "player", "target" }) do
            if sim.units[unit] then
                sim:FireEvent("UNIT_HEALTH", unit)
                sim:FireEvent("UNIT_POWER_UPDATE", unit, "ENERGY")
                sim:FireEvent("UNIT_AURA", unit)
            end
        end
    elseif moment == "low_health" then
        local p = sim.units.player
        sim:SetHealth("player", math.floor((p.healthMax or p.health or 100) * 0.25))
    elseif moment == "no_target" then
        sim.units.target = nil
        sim:FireEvent("PLAYER_TARGET_CHANGED")
    end
end

-- A request draw can't serve, as a document's problems are given.
local function refused(code, message)
    return { { path = "", code = code, message = message } }
end

-- UI1: the document checker, the addon's own UICheck.lua (the one the game
-- runs), loaded from the addon folder `addon`.
local checkers = {}
function M.uicheck(addon)
    if not checkers[addon] then
        local src = assert(require("wowapi.host").read(addon .. "/UICheck.lua"), "no UICheck.lua in " .. addon)
        checkers[addon] = assert(loadstring(src, "@UICheck.lua"))()
    end
    return checkers[addon]
end

-- opts: addon (the ForeverBuddy folder), class (the player's class; one
-- without a fixtures.lua character gets the generic one), blizzard (the
-- extract's interface/addons, art on only), art (draw with the game's art),
-- layout (an Edit Mode layout), moment (one of M.MOMENTS, standing by
-- default), ui (a UI document, UI1: checked first, then drawn over the
-- scene by the addon's own UI.lua, the one the game has). Returns the SVG,
-- the number of Lua errors the scene logged, and
-- the class drawn (nil for the generic character). A document that doesn't
-- pass the check draws nothing: the SVG is nil and the fourth value is its
-- problems (the addon's UICheck.lua).
-- UI1: a document's problems, without drawing it, from the addon's own
-- UICheck.lua (`addon`, by default the app's bundled one). The app's
-- propose_ui checks through here, so the app, the pictures and the game
-- use the same checker.
function M.check(doc, addon)
    return M.uicheck(addon or "/addon/ForeverBuddy").check(doc)
end

function M.draw(opts)
    -- (UI3: an unknown class or moment is refused by name, not drawn as
    -- something else)
    local class = opts.class and opts.class:lower()
    if class and not fixtures.CLASSES[class] then
        return nil, 0, nil, refused("unknown-class", ("There's no test character for %q. There are: %s.")
            :format(opts.class, table.concat(M.CLASSES, ", ")))
    end
    if opts.moment and not M.MOMENTS[opts.moment] then
        return nil, 0, class, refused("unknown-moment", ("%q isn't a moment. They are: %s.")
            :format(opts.moment, table.concat(M.MOMENT_ORDER, ", ")))
    end
    local moment = opts.moment or "standing"
    local globals = {}
    if opts.ui ~= nil then
        local problems = M.check(opts.ui, opts.addon)
        if #problems > 0 then
            return nil, 0, class, problems
        end
        globals.FBUISpec = opts.ui
        globals.FBUIStealth = moment == "stealth"
    end
    local sim = scene.boot({
        blizzard = opts.blizzard,
        extra = { opts.addon },
        globals = globals,
        class = class,
        layout = opts.layout,
        stealth = moment == "stealth",
        art = opts.art == true,
        offline = true,
        secretValues = moment == "hidden",
    })
    enter(sim, moment)
    sim:Advance(1)
    local svg = render.svg(sim, { art = opts.art == true, embed = true, addonFonts = true, labels = false })
    return svg, #sim.errors, class
end

return M
