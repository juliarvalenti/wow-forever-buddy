-- Written by tools/addon-test/run.lua (scenario record). Don't edit: change the scenario and run it again.
ForeverBuddyDB = {
	["_meta"] = {
		["addon"] = "0.9.2",
		["build"] = "1.60.1.70009",
		["counts"] = {
			["bag_items"] = 3,
			["events"] = 2,
			["items"] = 4,
			["quests_done"] = 2,
			["sessions"] = 1,
		},
		["loaded_prior"] = false,
		["missing_events"] = {
		},
		["schema"] = 1,
		["secret_hits"] = 0,
		["truncated"] = false,
		["written"] = 1790964661,
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
	["record"] = {
		["addon"] = "0.9.2",
		["events"] = {
			{
				["e"] = "PLAYER_LOGIN",
				["s"] = {
					["b"] = 0,
					["bk"] = false,
					["c"] = false,
					["l"] = 0,
					["m"] = 0,
					["ml"] = false,
					["q"] = 0,
					["sv"] = 0,
					["w"] = 0,
					["x"] = 0,
					["xm"] = 8800,
				},
				["t"] = 0,
			}, -- [1]
			{
				["a"] = {
					true, -- [1]
					false, -- [2]
				},
				["e"] = "PLAYER_ENTERING_WORLD",
				["t"] = 0,
			}, -- [2]
			{
				["e"] = "UPDATE_INSTANCE_INFO",
				["t"] = 1,
			}, -- [3]
			{
				["a"] = {
					19552, -- [1]
					475, -- [2]
				},
				["e"] = "TIME_PLAYED_MSG",
				["t"] = 1,
			}, -- [4]
			{
				["e"] = "BAG_UPDATE_DELAYED",
				["s"] = {
					["b"] = 16,
					["bk"] = false,
					["c"] = false,
					["l"] = 12,
					["m"] = 25000,
					["ml"] = false,
					["q"] = 0,
					["sv"] = 2,
					["w"] = 1,
					["x"] = 1200,
					["xm"] = 8800,
				},
				["t"] = 1,
			}, -- [5]
			{
				["e"] = "UPDATE_INSTANCE_INFO",
				["t"] = 2,
			}, -- [6]
			{
				["e"] = "BAG_UPDATE_DELAYED",
				["t"] = 601,
			}, -- [7]
			{
				["a"] = {
					"player", -- [1]
				},
				["e"] = "PLAYER_XP_UPDATE",
				["s"] = {
					["b"] = 16,
					["bk"] = false,
					["c"] = false,
					["l"] = 12,
					["m"] = 25000,
					["ml"] = false,
					["q"] = 0,
					["sv"] = 2,
					["w"] = 1,
					["x"] = 1700,
					["xm"] = 8800,
				},
				["t"] = 601,
			}, -- [8]
			{
				["e"] = "PLAYER_MONEY",
				["s"] = {
					["b"] = 16,
					["bk"] = false,
					["c"] = false,
					["l"] = 12,
					["m"] = 26000,
					["ml"] = false,
					["q"] = 0,
					["sv"] = 2,
					["w"] = 1,
					["x"] = 1700,
					["xm"] = 8800,
				},
				["t"] = 601,
			}, -- [9]
			{
				["e"] = "MAIL_SHOW",
				["t"] = 601,
			}, -- [10]
			{
				["e"] = "MAIL_INBOX_UPDATE",
				["s"] = {
					["b"] = 16,
					["bk"] = false,
					["c"] = false,
					["l"] = 12,
					["m"] = 26000,
					["ml"] = true,
					["q"] = 0,
					["sv"] = 2,
					["w"] = 1,
					["x"] = 1700,
					["xm"] = 8800,
				},
				["t"] = 601,
			}, -- [11]
			{
				["e"] = "MAIL_CLOSED",
				["t"] = 601,
			}, -- [12]
			{
				["e"] = "PLAYER_LEAVING_WORLD",
				["s"] = {
					["b"] = 0,
					["bk"] = false,
					["c"] = false,
					["l"] = 0,
					["m"] = 0,
					["ml"] = false,
					["q"] = 0,
					["sv"] = 0,
					["w"] = 0,
					["x"] = 0,
					["xm"] = 8800,
				},
				["t"] = 661,
			}, -- [13]
			{
				["e"] = "PLAYER_LOGOUT",
				["t"] = 661,
			}, -- [14]
		},
	},
	["sessions"] = {
		{
			["events"] = {
				{
					["count"] = 2,
					["item"] = 117,
					["kind"] = "gain",
					["t"] = 1790964601,
				}, -- [1]
				{
					["kind"] = "money",
					["money"] = 26000,
					["t"] = 1790964601,
				}, -- [2]
			},
			["id"] = 1790964000,
			["login"] = 1790964000,
			["logout"] = 1790964661,
			["start"] = {
				["level"] = 12,
				["money"] = 25000,
				["xp"] = 1200,
				["zone"] = "Elwynn Forest",
			},
		}, -- [1]
	},
	["snapshot"] = {
		["at"] = 1790964611,
		["bags"] = {
			[0] = {
				["free"] = 13,
				["items"] = {
					{
						["bound"] = false,
						["count"] = 1,
						["link"] = "|cffffffff|Hitem:6948::::::::12:::::|h[Hearthstone]|h|r",
					}, -- [1]
					{
						["bound"] = false,
						["count"] = 4,
						["link"] = "|cffffffff|Hitem:2589::::::::12:::::|h[Linen Cloth]|h|r",
					}, -- [2]
					{
						["bound"] = false,
						["count"] = 2,
						["link"] = "|cffffffff|Hitem:117::::::::12:::::|h[Tough Jerky]|h|r",
					}, -- [3]
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
			{
				["difficulty"] = "Normal",
				["name"] = "The Deadmines",
				["reset_at"] = 1791136802,
			}, -- [1]
			{
				["difficulty"] = "40 Player",
				["name"] = "Molten Core",
				["raid"] = true,
				["reset_at"] = 1791482402,
			}, -- [2]
		},
		["mail"] = {
			["at"] = 1790964601,
			["items"] = {
				{
					["cod"] = 0,
					["days_left"] = 3,
					["items"] = {
					},
					["money"] = 5,
					["sender"] = "Gankalot",
					["subject"] = "Secret plans",
				}, -- [1]
			},
		},
		["money"] = 26000,
		["played"] = {
			["level"] = 1135,
			["total"] = 20212,
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
			783, -- [2]
		},
		["rest_state"] = "Rested",
		["rested"] = 674,
		["xp"] = 1700,
		["xp_max"] = 8800,
		["zone"] = {
			["map"] = 1429,
			["subzone"] = "Goldshire",
			["zone"] = "Elwynn Forest",
		},
	},
}
