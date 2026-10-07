-- Written by tools/addon-test/run.lua (scenario plan). Don't edit: change the scenario and run it again.
ForeverBuddyDB = {
	["_meta"] = {
		["addon"] = "0.6.0",
		["build"] = "1.60.1.70009",
		["counts"] = {
			["bag_items"] = 2,
			["events"] = 3,
			["items"] = 3,
			["quests_done"] = 3,
			["sessions"] = 1,
		},
		["loaded_prior"] = false,
		["missing_events"] = {
		},
		["schema"] = 1,
		["secret_hits"] = 0,
		["truncated"] = false,
		["written"] = 1790964000,
	},
	["bridge"] = {
		["Plan"] = {
			["schema"] = 1,
			["seen"] = 1790964000,
			["stamp"] = 1790960000,
		},
	},
	["character"] = {
		["class"] = "WARRIOR",
		["faction"] = "Alliance",
		["guid"] = "Player-4613-0A1B2C3D",
		["guild"] = {
			["name"] = "Hearthguard",
			["rank"] = "Officer",
		},
		["level"] = 12,
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
		[2589] = {
			["class"] = 7,
			["icon"] = 132889,
			["ilvl"] = 10,
			["name"] = "Linen Cloth",
			["quality"] = 1,
			["sell"] = 13,
			["subclass"] = 5,
		},
		[6948] = {
			["class"] = 7,
			["icon"] = 132889,
			["ilvl"] = 10,
			["name"] = "Hearthstone",
			["quality"] = 1,
			["sell"] = 13,
			["subclass"] = 5,
		},
	},
	["plan"] = {
		["done"] = {
			true, -- [1]
			true, -- [2]
			true, -- [3]
			true, -- [4]
		},
		["finished"] = true,
		["id"] = 7,
	},
	["sessions"] = {
		{
			["events"] = {
				{
					["giver"] = "Marshal Dughan",
					["id"] = 176,
					["kind"] = "quest_accepted",
					["map"] = 1429,
					["t"] = 1790964000,
					["title"] = "Wanted: Hogger",
					["x"] = 0.412,
					["y"] = 0.657,
					["zone"] = "Elwynn Forest",
				}, -- [1]
				{
					["id"] = 176,
					["kind"] = "quest",
					["map"] = 1429,
					["money"] = 75,
					["t"] = 1790964000,
					["title"] = "Wanted: Hogger",
					["x"] = 0.412,
					["xp"] = 450,
					["y"] = 0.657,
					["zone"] = "Elwynn Forest",
				}, -- [2]
				{
					["kind"] = "money",
					["money"] = 25075,
					["t"] = 1790964000,
				}, -- [3]
			},
			["id"] = 1790964000,
			["login"] = 1790964000,
			["logout"] = 1790964000,
			["start"] = {
				["level"] = 12,
				["money"] = 25000,
				["xp"] = 1200,
				["zone"] = "Elwynn Forest",
			},
		}, -- [1]
	},
	["snapshot"] = {
		["at"] = 1790964000,
		["bags"] = {
			[0] = {
				["free"] = 14,
				["items"] = {
					{
						["count"] = 1,
						["link"] = "|cffffffff|Hitem:6948::::::::12:::::|h[Hearthstone]|h|r",
					}, -- [1]
					{
						["count"] = 4,
						["link"] = "|cffffffff|Hitem:2589::::::::12:::::|h[Linen Cloth]|h|r",
					}, -- [2]
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
		["money"] = 25075,
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
		["xp"] = 1650,
		["xp_max"] = 8800,
		["zone"] = {
			["map"] = 1429,
			["subzone"] = "Goldshire",
			["zone"] = "Elwynn Forest",
		},
	},
}
