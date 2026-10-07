-- Written by tools/addon-test/run.lua (scenario secret_session). Don't edit: change the scenario and run it again.
ForeverBuddyDB = {
	["_meta"] = {
		["addon"] = "0.9.0",
		["build"] = "1.60.1.70009",
		["counts"] = {
			["bag_items"] = 0,
			["events"] = 3,
			["items"] = 1,
			["quests_done"] = 3,
			["sessions"] = 1,
		},
		["loaded_prior"] = false,
		["missing_events"] = {
		},
		["schema"] = 1,
		["secret_hits"] = 11,
		["truncated"] = false,
		["written"] = 1790964120,
	},
	["character"] = {
		["class"] = "WARRIOR",
		["faction"] = "Alliance",
		["guid"] = "Player-4613-0A1B2C3D",
		["guild"] = {
			["name"] = "Hearthguard",
			["rank"] = "Officer",
		},
		["level"] = 13,
		["name"] = "Thrandor",
		["race"] = "Human",
		["realm"] = "Classic Beta PvP 2",
		["sex"] = 2,
		["surname"] = "Vargur",
	},
	["items"] = {
		[25] = {
			["class"] = 7,
			["icon"] = 132889,
			["ilvl"] = 10,
			["name"] = "Worn Shortsword",
			["quality"] = 1,
			["sell"] = 13,
			["subclass"] = 5,
		},
	},
	["sessions"] = {
		{
			["events"] = {
				{
					["kind"] = "quest",
					["map"] = 1429,
					["t"] = 1790964060,
					["x"] = 0.412,
					["y"] = 0.657,
					["zone"] = "Elwynn Forest",
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
		["bags"] = {
			[0] = {
				["free"] = 15,
				["items"] = {
				},
				["name"] = "Backpack",
				["size"] = 16,
			},
		},
		["equipped"] = {
			[16] = "|cffffffff|Hitem:25::::::::12:::::|h[Worn Shortsword]|h|r",
		},
		["ilvl"] = {
			["avg"] = 21.5,
			["equipped"] = 20.25,
		},
		["lockouts"] = {
		},
		["money"] = 26200,
		["played"] = {
			["level"] = 594,
			["total"] = 19671,
		},
		["professions"] = {
			{
				["line"] = 182,
				["max"] = 75,
				["name"] = "Herbalism",
				["skill"] = 60,
			}, -- [1]
			{
				["line"] = 197,
				["max"] = 75,
				["name"] = "Tailoring",
				["skill"] = 34,
				["spec"] = 2,
			}, -- [2]
			{
				["line"] = 185,
				["max"] = 75,
				["name"] = "Cooking",
				["skill"] = 29,
			}, -- [3]
		},
		["quests_done"] = {
			7, -- [1]
			176, -- [2]
			783, -- [3]
		},
		["rest_state"] = "Rested",
		["rested"] = 674,
		["xp"] = 0,
		["xp_max"] = 8800,
		["zone"] = {
			["map"] = 1429,
			["subzone"] = "Goldshire",
			["zone"] = "Elwynn Forest",
		},
	},
}
