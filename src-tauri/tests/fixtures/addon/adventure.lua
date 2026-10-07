-- Written by tools/addon-test/run.lua (scenario adventure). Don't edit: change the scenario and run it again.
ForeverBuddyDB = {
	["_meta"] = {
		["addon"] = "0.4.1",
		["build"] = "1.60.1.70009",
		["counts"] = {
			["bag_items"] = 2,
			["events"] = 14,
			["items"] = 4,
			["quests_done"] = 3,
			["sessions"] = 1,
		},
		["loaded_prior"] = false,
		["missing_events"] = {
		},
		["schema"] = 1,
		["secret_hits"] = 0,
		["truncated"] = false,
		["written"] = 1790967250,
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
		[117] = {
			["class"] = 7,
			["icon"] = 132889,
			["ilvl"] = 10,
			["name"] = "Tough Jerky",
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
	["sessions"] = {
		{
			["events"] = {
				{
					["kind"] = "zone",
					["t"] = 1790964300,
					["zone"] = "Westfall",
				}, -- [1]
				{
					["count"] = 3,
					["item"] = 2589,
					["kind"] = "gain",
					["t"] = 1790964360,
				}, -- [2]
				{
					["kind"] = "money",
					["money"] = 25200,
					["t"] = 1790964360,
				}, -- [3]
				{
					["kind"] = "death",
					["t"] = 1790964970,
					["zone"] = "Westfall",
				}, -- [4]
				{
					["count"] = 7,
					["how"] = "sold",
					["item"] = 2589,
					["kind"] = "lose",
					["t"] = 1790965030,
				}, -- [5]
				{
					["kind"] = "money",
					["money"] = 24345,
					["t"] = 1790965030,
				}, -- [6]
				{
					["count"] = 5,
					["how"] = "bought",
					["item"] = 117,
					["kind"] = "gain",
					["t"] = 1790965030,
				}, -- [7]
				{
					["cost"] = 800,
					["kind"] = "repair",
					["t"] = 1790965030,
				}, -- [8]
				{
					["count"] = 1,
					["how"] = "used",
					["item"] = 117,
					["kind"] = "lose",
					["t"] = 1790965090,
				}, -- [9]
				{
					["id"] = 176,
					["kind"] = "quest",
					["map"] = 1429,
					["money"] = 1200,
					["t"] = 1790965390,
					["title"] = "Wanted: Hogger",
					["x"] = 0.412,
					["xp"] = 1350,
					["y"] = 0.657,
					["zone"] = "Westfall",
				}, -- [10]
				{
					["kind"] = "money",
					["money"] = 25545,
					["t"] = 1790965390,
				}, -- [11]
				{
					["kind"] = "level",
					["level"] = 13,
					["t"] = 1790965390,
				}, -- [12]
				{
					["instance"] = true,
					["kind"] = "zone",
					["t"] = 1790965450,
					["zone"] = "The Deadmines",
				}, -- [13]
				{
					["id"] = 639,
					["kind"] = "encounter",
					["name"] = "Edwin VanCleef",
					["t"] = 1790966650,
				}, -- [14]
			},
			["id"] = 1790964000,
			["login"] = 1790964000,
			["logout"] = 1790967250,
			["start"] = {
				["level"] = 12,
				["money"] = 25000,
				["xp"] = 1200,
				["zone"] = "Elwynn Forest",
			},
		}, -- [1]
	},
	["snapshot"] = {
		["at"] = 1790967250,
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
						["link"] = "|cffffffff|Hitem:117::::::::12:::::|h[Tough Jerky]|h|r",
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
		["lockouts"] = {
		},
		["money"] = 25545,
		["played"] = {
			["level"] = 3724,
			["total"] = 22801,
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
			["zone"] = "The Deadmines",
		},
	},
}
