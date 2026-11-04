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
-- logs in.
M.MOMENTS = { standing = true, combat = true, stealth = true, low_health = true }

local function enter(sim, moment)
    if moment == "combat" then
        sim:EnterCombat()
    elseif moment == "low_health" then
        local p = sim.units.player
        sim:SetHealth("player", math.floor((p.healthMax or p.health or 100) * 0.25))
    end
end

-- opts: addon (the ForeverBuddy folder), class (the player's class; one
-- without a fixtures.lua character gets the generic one), blizzard (the
-- extract's interface/addons, art on only), art (draw with the game's art),
-- layout (an Edit Mode layout), moment (one of M.MOMENTS, standing by
-- default). Returns the SVG, the number of Lua errors the scene logged, and
-- the class drawn (nil for the generic character).
function M.draw(opts)
    local class = opts.class and opts.class:lower()
    if not fixtures.CLASSES[class or ""] then
        class = nil
    end
    local moment = M.MOMENTS[opts.moment or ""] and opts.moment or "standing"
    local sim = scene.boot({
        blizzard = opts.blizzard,
        extra = { opts.addon },
        class = class,
        layout = opts.layout,
        stealth = moment == "stealth",
        art = opts.art == true,
        offline = true,
    })
    enter(sim, moment)
    sim:Advance(1)
    local svg = render.svg(sim, { art = opts.art == true, embed = true, addonFonts = true, labels = false })
    return svg, #sim.errors, class
end

return M
