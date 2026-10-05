-- Written by tools/addon-test/run.lua (scenario carry_forward). Don't edit: change the scenario and run it again.
ForeverBuddyDB = {
	["_meta"] = {
		["addon"] = "0.2.0",
		["build"] = "1.60.1.70009",
		["counts"] = {
			["bag_items"] = 2,
			["events"] = 0,
			["items"] = 3,
			["sessions"] = 2,
		},
		["loaded_prior"] = true,
		["missing_events"] = {
		},
		["schema"] = 1,
		["secret_hits"] = 0,
		["truncated"] = false,
		["written"] = 1791058200,
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
	["sessions"] = {
		{
			["events"] = {
			},
			["id"] = 1790964000,
			["login"] = 1790964000,
			["logout"] = 1790968200,
			["start"] = {
				["level"] = 12,
				["money"] = 25000,
				["xp"] = 1200,
				["zone"] = "Elwynn Forest",
			},
		}, -- [1]
		{
			["events"] = {
			},
			["id"] = 1791054600,
			["login"] = 1791054600,
			["logout"] = 1791058200,
			["start"] = {
				["level"] = 12,
				["money"] = 25000,
				["xp"] = 1200,
				["zone"] = "Elwynn Forest",
			},
		}, -- [2]
	},
	["snapshot"] = {
		["at"] = 1791058200,
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
		["bank"] = {
			["at"] = 1790964600,
			["bags"] = {
				[-1] = {
					["free"] = 27,
					["items"] = {
						{
							["count"] = 20,
							["link"] = "|cffffffff|Hitem:14047::::::::12:::::|h[Runecloth]|h|r",
						}, -- [1]
					},
					["size"] = 28,
				},
				[6] = {
					["free"] = 98,
					["items"] = {
					},
					["size"] = 98,
				},
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
				["reset_at"] = 1791227401,
			}, -- [1]
		},
		["mail"] = {
			["at"] = 1790964600,
			["items"] = {
				{
					["cod"] = 0,
					["days_left"] = 29.5,
					["items"] = {
						{
							["count"] = 10,
							["link"] = "|cffffffff|Hitem:2589::::::::12:::::|h[Linen Cloth]|h|r",
						}, -- [1]
					},
					["money"] = 500,
					["sender"] = "Coinpurse",
					["subject"] = "Linen for you",
				}, -- [1]
			},
		},
		["money"] = 25000,
		["played"] = {
			["level"] = 94674,
			["total"] = 113751,
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
			}, -- [2]
			{
				["line"] = 185,
				["max"] = 75,
				["name"] = "Cooking",
				["skill"] = 29,
			}, -- [3]
		},
		["rest_state"] = "Rested",
		["rested"] = 674,
		["xp"] = 1200,
		["xp_max"] = 8800,
		["zone"] = {
			["map"] = 1429,
			["subzone"] = "Goldshire",
			["zone"] = "Elwynn Forest",
		},
	},
}
