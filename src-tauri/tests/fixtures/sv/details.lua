
_detalhes_global = {
	["got_first_run"] = true,
	["update_warning_timeout"] = 10,
	["switchSaved"] = {
		["slots"] = 4,
		["table"] = {
			{
				["atributo"] = 1,
				["sub_atributo"] = 1,
			}, -- [1]
			{
				["atributo"] = 2,
				["sub_atributo"] = 1,
			}, -- [2]
			{
				["atributo"] = 1,
				["sub_atributo"] = 6,
			}, -- [3]
			{
				["atributo"] = 4,
				["sub_atributo"] = 5,
			}, -- [4]
		},
	},
	["latest_news_saw"] = "1.15.4",
	["always_use_profile_name"] = "",
	["custom"] = {
		{
			["source"] = false,
			["author"] = "Details!",
			["desc"] = "Show who in your raid used a potion during the encounter.",
			["tooltip"] = "\t\t\tlocal player, combat, instance = ...\n\n\t\t\tlocal myPotions = {}\n\t\t\treturn \"|cFFFFFF00\" .. player:name() .. \"|r\"\n\t\t",
			["attribute"] = false,
			["name"] = "Potion Used",
			["icon"] = "Interface\\ICONS\\INV_Potion_03",
			["spellid"] = false,
			["target"] = false,
			["script_version"] = 6,
		}, -- [1]
	},
	["details_auras"] = {
	},
	["plugin_window_pos"] = {
		["y"] = -2.288818359375e-05,
		["x"] = 0.0000152587890625,
		["point"] = "CENTER",
		["scale"] = 1,
	},
	["damage_scroll_auto_open"] = true,
	["lastUpdateWarning"] = 1727913600,
	["dungeon_run_id"] = 0,
	["death_recap"] = {
		["show_segments"] = false,
		["relevance_time"] = 7,
		["enabled"] = true,
		["show_life_percent"] = false,
	},
}
_detalhes_database = {
	["savedbuffs"] = {
	},
	["mythic_dungeon_id"] = 0,
	["tabela_historico"] = {
		["tabelas"] = {
			{
				{
					["tipo"] = 2,
					["_ActorTable"] = {
						{
							["flag_original"] = 1297,
							["totalabsorbed"] = 0.006,
							["damage_from"] = {
								["Defias Pillager"] = true,
								["Defias Overseer"] = true,
							},
							["targets"] = {
								["Defias Pillager"] = 4123,
								["Defias Overseer"] = 2210,
							},
							["pets"] = {
							},
							["classe"] = "WARRIOR",
							["total"] = 6333.001,
							["nome"] = "Thrandor",
							["spells"] = {
								["tipo"] = 2,
								["_ActorTable"] = {
									[6603] = {
										["c_amt"] = 3,
										["b_amt"] = 0,
										["c_dmg"] = 612,
										["g_amt"] = 0,
										["n_max"] = 141,
										["targets"] = {
											["Defias Pillager"] = 1290,
										},
										["n_dmg"] = 1013,
										["n_min"] = 0,
										["counter"] = 14,
										["total"] = 1625,
										["id"] = 6603,
										["r_dmg"] = 0,
										["a_dmg"] = 0,
										["m_crit"] = 0,
										["m_amt"] = 0,
									},
									[11567] = {
										["n_max"] = 268,
										["counter"] = 9,
										["total"] = 2116,
										["id"] = 11567,
									},
								},
							},
							["grupo"] = true,
							["last_dps"] = 61.2984352211243,
							["end_time"] = 1727913700,
							["serial"] = "Player-5826-0216A1F3",
							["start_time"] = 1727913596,
						}, -- [1]
					},
				}, -- [1]
			}, -- [1]
		},
	},
	["last_version"] = "1.15.4 5834",
	["character_data"] = {
		["logons"] = 412,
	},
	["active_profile"] = "Thrandor-Forever",
	["last_realversion"] = 158,
	["benchmark_db"] = {
		["frame"] = {
		},
	},
	["last_instance_time"] = -1,
	["combat_id"] = 12087,
	["announce_cooldowns"] = {
		["ignored_cooldowns"] = {
		},
		["enabled"] = false,
		["custom"] = "",
		["channel"] = "RAID",
	},
}
