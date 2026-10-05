-- Written by tools/addon-test/run.lua (scenario secret_session). Don't edit: change the scenario and run it again.
ForeverBuddyDB = {
	["_meta"] = {
		["addon"] = "0.2.0",
		["build"] = "1.60.1.70009",
		["counts"] = {
			["bag_items"] = 0,
			["events"] = 3,
			["items"] = 0,
			["sessions"] = 1,
		},
		["loaded_prior"] = false,
		["missing_events"] = {
		},
		["schema"] = 1,
		["secret_hits"] = 10,
		["truncated"] = false,
		["written"] = 1790964120,
	},
	["character"] = {
		["guid"] = "Player-4613-0A1B2C3D",
		["name"] = "Thrandor",
		["realm"] = "Classic Beta PvP 2",
		["surname"] = "Vargur",
	},
	["items"] = {
	},
	["sessions"] = {
		{
			["events"] = {
				{
					["kind"] = "quest",
					["t"] = 1790964060,
				}, -- [1]
				{
					["kind"] = "money",
					["money"] = 26200,
					["t"] = 1790964060,
				}, -- [2]
				{
					["kind"] = "level",
					["t"] = 1790964060,
				}, -- [3]
			},
			["id"] = 1790964000,
			["login"] = 1790964000,
			["logout"] = 1790964120,
			["start"] = {
				["level"] = 12,
				["money"] = 25000,
				["xp"] = 1200,
				["zone"] = "Elwynn Forest",
			},
		}, -- [1]
	},
	["snapshot"] = {
		["at"] = 1790964120,
	},
}
