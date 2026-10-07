-- Written by tools/addon-test/run.lua (scenario session). Don't edit: change the scenario and run it again.
ForeverBuddyDB = {
	["_meta"] = {
		["addon"] = "0.8.0",
		["build"] = "1.60.1.70009",
		["counts"] = {
			["bag_items"] = 4,
			["events"] = 6,
			["items"] = 5,
			["quests_done"] = 3,
			["sessions"] = 1,
		},
		["loaded_prior"] = true,
		["missing_events"] = {
		},
		["schema"] = 1,
		["secret_hits"] = 0,
		["truncated"] = false,
		["written"] = 1790967620,
	},
	["bridge"] = {
		["Tooltip2"] = {
			["schema"] = 1,
			["seen"] = 1790967620,
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
		[10005] = {
			["class"] = 4,
			["icon"] = 133070,
			["ilvl"] = 40,
			["name"] = "Felcloth Hood",
			["quality"] = 3,
			["sell"] = 5000,
			["subclass"] = 1,
		},
		[14047] = {
			["class"] = 7,
			["icon"] = 132889,
			["ilvl"] = 10,
			["name"] = "Runecloth",
			["quality"] = 1,
			["sell"] = 13,
			["subclass"] = 5,
		},
	},
	["sessions"] = {
		{
			["events"] = {
				{
					["kind"] = "money",
					["money"] = 3145075,
					["t"] = 1790965800,
				}, -- [1]
				{
					["count"] = 40,
					["item"] = 14047,
					["kind"] = "gain",
					["t"] = 1790965800,
				}, -- [2]
				{
					["count"] = 1,
					["item"] = 10005,
					["kind"] = "gain",
					["t"] = 1790965800,
				}, -- [3]
				{
					["id"] = 176,
					["kind"] = "quest",
					["map"] = 1429,
					["money"] = 75,
					["t"] = 1790965800,
					["title"] = "Wanted: Hogger",
					["x"] = 0.412,
					["xp"] = 0,
					["y"] = 0.657,
					["zone"] = "Elwynn Forest",
				}, -- [4]
				{
					["kind"] = "money",
					["money"] = 13145075,
					["t"] = 1790967600,
				}, -- [5]
				{
					["kind"] = "level",
					["level"] = 13,
					["t"] = 1790967610,
				}, -- [6]
			},
			["id"] = 1790964000,
			["login"] = 1790964000,
			["logout"] = 1790967620,
			["start"] = {
				["level"] = 12,
				["money"] = 25000,
				["xp"] = 1200,
				["zone"] = "Elwynn Forest",
			},
		}, -- [1]
	},
	["snapshot"] = {
		["at"] = 1790967620,
		["bags"] = {
			[0] = {
				["free"] = 12,
				["items"] = {
					{
						["count"] = 1,
						["link"] = "|cffffffff|Hitem:6948::::::::12:::::|h[Hearthstone]|h|r",
					}, -- [1]
					{
						["count"] = 4,
						["link"] = "|cffffffff|Hitem:2589::::::::12:::::|h[Linen Cloth]|h|r",
					}, -- [2]
					{
						["count"] = 40,
						["link"] = "|cffffffff|Hitem:14047::::::::12:::::|h[Runecloth]|h|r",
					}, -- [3]
					{
						["count"] = 1,
						["link"] = "|cffffffff|Hitem:10005::::::::12:::::|h[Felcloth Hood]|h|r",
					}, -- [4]
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
		["money"] = 13145075,
		["played"] = {
			["level"] = 4094,
			["total"] = 23171,
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
		["xp"] = 400,
		["xp_max"] = 8800,
		["zone"] = {
			["map"] = 1429,
			["subzone"] = "Goldshire",
			["zone"] = "Elwynn Forest",
		},
	},
}
