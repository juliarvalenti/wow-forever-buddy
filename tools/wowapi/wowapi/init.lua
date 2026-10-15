-- wowapi: a WoW: Forever client emulator for testing addons outside the game.
--
--   local WoW = require("wowapi")
--   local sim = WoW.new({ player = { name = "Thrall", class = "SHAMAN" } })
--   sim:LoadAddon("addons/MyAddon")
--   sim:Login()
--   sim:Slash("/myaddon hello")
--   sim:Advance(5)  -- run OnUpdate + C_Timer for 5 game seconds
local Sim = require("wowapi.sim")

return {
  VERSION = "0.1.0",
  INTERFACE = 16001,
  new = Sim.new,
  Sim = Sim,
  onNew = Sim.onNew,
  toc = require("wowapi.toc"),
  serialize = require("wowapi.serialize"),
  StripCodes = Sim.StripCodes,
}
