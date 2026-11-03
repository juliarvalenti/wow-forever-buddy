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

-- opts: addon (the ForeverBuddy folder), class (the player's class; one
-- without a fixtures.lua character gets the generic one), blizzard (the
-- extract's interface/addons, art on only), art (draw with the game's art),
-- layout (an Edit Mode layout). Returns the SVG, the number of Lua errors
-- the scene logged, and the class drawn (nil for the generic character).
function M.draw(opts)
    local class = opts.class and opts.class:lower()
    if not fixtures.CLASSES[class or ""] then
        class = nil
    end
    local sim = scene.boot({
        blizzard = opts.blizzard,
        extra = { opts.addon },
        class = class,
        layout = opts.layout,
        art = opts.art == true,
        offline = true,
    })
    sim:Advance(1)
    local svg = render.svg(sim, { art = opts.art == true, embed = true, addonFonts = true, labels = false })
    return svg, #sim.errors, class
end

return M
