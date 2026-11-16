
ForeverBuddyLiteDB = {
	["version"] = 1,
	["ah"] = {
		["at"] = 1794600000,
		["listings"] = 6000,
		["items"] = {
			["7357:1205"] = {
				["id"] = 7357, ["name"] = "Elder's Hat of the Eagle", ["slot"] = "INVTYPE_HEAD", ["class"] = 4, ["subtype"] = "Cloth",
				["buyout"] = 4500, ["listings"] = 2,
				["link"] = "|cnIQ2:|Hitem:7357::::::1205::20:::::::|h[Elder's Hat of the Eagle]|h|r",
				["tip"] = { "+6 Stamina", "+7 Intellect", "Requires Level 20" },
			},
			["4368:0"] = {
				["id"] = 4368, ["name"] = "Shadow Goggles", ["slot"] = "INVTYPE_HEAD", ["class"] = 4, ["subtype"] = "Cloth",
				["buyout"] = 1500, ["listings"] = 1, ["link"] = "item:4368::::::",
				["tip"] = { "+50 Stamina", "Requires Level 19", "Requires Engineering (120)" },
			},
			["9999:0"] = {
				["id"] = 9999, ["name"] = "Leather Cap", ["slot"] = "INVTYPE_HEAD", ["class"] = 4, ["subtype"] = "Leather",
				["buyout"] = 100, ["listings"] = 1, ["link"] = "|Hitem:9999|h[Leather Cap]|h",
				["tip"] = { "+20 Stamina", "Requires Level 20" },
			},
			["9998:0"] = {
				["id"] = 9998, ["name"] = "Rogue Ring", ["slot"] = "INVTYPE_FINGER", ["class"] = 4, ["subtype"] = "Miscellaneous",
				["buyout"] = 100, ["listings"] = 1, ["link"] = "|Hitem:9998|h[Rogue Ring]|h",
				["tip"] = { "+20 Stamina", "Classes: Rogue", "Requires Level 10" },
			},
			["9997:0"] = {
				["id"] = 9997, ["name"] = "Big Staff", ["slot"] = "INVTYPE_2HWEAPON", ["class"] = 2, ["subtype"] = "Staves",
				["buyout"] = 100, ["listings"] = 1, ["link"] = "|Hitem:9997|h[Big Staff]|h",
				["tip"] = { "+30 Intellect", "Requires Level 30" },
			},
			["9996:0"] = {
				["id"] = 9996, ["name"] = "Fire Staff", ["slot"] = "INVTYPE_2HWEAPON", ["class"] = 2, ["subtype"] = "Staves",
				["buyout"] = 20000, ["listings"] = 1, ["link"] = "|Hitem:9996|h[Fire Staff]|h",
				["tip"] = { "Equip: Increases damage done by Fire spells and effects by up to 40.", "Equip: Increases damage and healing done by magical spells and effects by up to 30.", "Requires Level 19" },
			},
		},
	},
	["characters"] = {
		["Testmage-Realm"] = {
			["saved_at"] = 1794600000,
			["addon_version"] = 1,
			["identity"] = {
				["name"] = "Testmage",
				["realm"] = "Realm",
				["class"] = "MAGE",
				["race"] = "Skyborne",
				["level"] = 20,
				["xp"] = 1500,
				["xp_max"] = 6000,
				["money"] = 123456,
				["zone"] = "Wetlands",
				["subzone"] = "Menethil Harbor",
				["map_id"] = 1437,
				["x"] = 10.5,
				["y"] = 60.2,
			},
			["stats"] = {
				["health"] = 450,
				["power"] = 900,
				["stats"] = {
					["intellect"] = 60,
					["stamina"] = 30,
				},
				["spell_power"] = {
					["arcane"] = 24,
					["fire"] = 18,
				},
			},
			["gear"] = {
				[16] = {
					["link"] = "|cff0070dd|Hitem:5201::::::::20:::::::|h[Emberstone Staff]|h|r",
					["id"] = 5201,
					["equip"] = "INVTYPE_2HWEAPON",
					["tip"] = { "+8 Stamina", "Equip: Increases damage and healing done by magical spells and effects by up to 24.", "Requires Level 17" },
					["durability"] = {
						80, -- [1]
						90, -- [2]
					},
				},
				[18] = {
					["link"] = "|cff1eff00|Hitem:5198::::::::20:::::::|h[Cookie's Stirring Rod]|h|r",
					["id"] = 5198,
				},
			},
			["bags"] = {
				["at"] = 1794600000,
				["bags"] = {
					[0] = {
						["size"] = 16,
						["free"] = 14,
						["name"] = "Backpack",
						["items"] = {
							[1] = {
								["link"] = "|cffffffff|Hitem:2589::::::::20:::::::|h[Linen Cloth]|h|r",
								["id"] = 2589,
								["count"] = 17,
								["bound"] = false,
							},
							[2] = {
								["link"] = "|cffffffff|Hitem:17031::::::::20:::::::|h[Rune of Teleportation]|h|r",
								["id"] = 17031,
								["count"] = 5,
							},
						},
					},
				},
			},
			["quests"] = {
				["log"] = {
					{
						["id"] = 276,
						["title"] = "Tramping Paws",
						["level"] = 21,
						["objectives"] = {
							{
								["text"] = "Mosshide Gnoll slain: 6/15",
								["type"] = "monster",
								["done"] = false,
								["have"] = 6,
								["need"] = 15,
							}, -- [1]
						},
					}, -- [1]
				},
				["completed"] = {
					161, -- [1]
					455, -- [2]
					783, -- [3]
				},
			},
			["skills"] = {
				{
					["name"] = "Tailoring",
					["rank"] = 112,
					["max"] = 150,
				}, -- [1]
				{
					["name"] = "Staves",
					["rank"] = 95,
					["max"] = 100,
				}, -- [2]
				{
					["name"] = "Cloth",
					["rank"] = 1,
					["max"] = 1,
				}, -- [3]
			},
			["talents"] = {
				["system"] = "traits",
				["unspent"] = 1,
				["nodes"] = {
					{ ["x"] = 1000, ["y"] = 100, ["rank"] = 5, ["max"] = 5, ["name"] = "Arcane Focus" },
					{ ["x"] = 1600, ["y"] = 100, ["rank"] = 3, ["max"] = 3, ["name"] = "Improved Channeling" },
					{ ["x"] = 1600, ["y"] = 700, ["rank"] = 1, ["max"] = 1, ["name"] = "Arcane Blast" },
					{ ["x"] = 4000, ["y"] = 100, ["rank"] = 0, ["max"] = 5, ["name"] = "Improved Fireball" },
					{ ["x"] = 4600, ["y"] = 100, ["rank"] = 0, ["max"] = 2, ["name"] = "Impact" },
					{ ["x"] = 7000, ["y"] = 100, ["rank"] = 2, ["max"] = 3, ["name"] = "Frost Warding" },
					{ ["x"] = 7600, ["y"] = 100, ["rank"] = 0, ["max"] = 5, ["name"] = "Improved Frostbolt" },
				},
			},
			["spells"] = {
				{ ["name"] = "Arcane Missiles", ["rank"] = "Rank 3", ["id"] = 5145, ["tab"] = "Arcane" },
				{ ["name"] = "Blink", ["id"] = 1953, ["tab"] = "Arcane" },
			},
			["reputation"] = {
				{ ["name"] = "Stormwind", ["standing"] = "Friendly", ["value"] = 2500, ["max"] = 6000 },
			},
			["mail"] = {
				["at"] = 1794500000,
				["letters"] = {
					{
						["sender"] = "C",
						["subject"] = "A \"Return\" Customer",
						["money"] = 0,
						["items"] = {},
					},
				},
			},
		},
		["Testrogue-Realm"] = {
			["saved_at"] = 1794500000,
			["identity"] = {
				["name"] = "Testrogue",
				["class"] = "ROGUE",
				["level"] = 33,
			},
			["bank"] = {
				["at"] = 1794400000,
				["bags"] = {
					[-1] = {
						["size"] = 24,
						["items"] = {
							[3] = {
								["link"] = "|cffffffff|Hitem:2589::::::::33:::::::|h[Linen Cloth]|h|r",
								["id"] = 2589,
								["count"] = 40,
							},
						},
					},
				},
			},
		},
	},
}
