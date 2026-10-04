
WeakAurasSaved = {
	["dynamicIconCache"] = {
	},
	["editor_tab_spaces"] = 4,
	["login_squelch_time"] = 10,
	["lastArchiveClear"] = 1727827200,
	["minimap"] = {
		["minimapPos"] = 211.4513817742,
		["hide"] = false,
	},
	["lastUpgrade"] = 1727913599,
	["dbVersion"] = 76,
	["displays"] = {
		["Sunder Armor Tracker"] = {
			["iconSource"] = -1,
			["authorOptions"] = {
			},
			["yOffset"] = -120,
			["anchorPoint"] = "CENTER",
			["cooldownSwipe"] = true,
			["cooldownEdge"] = false,
			["icon"] = true,
			["triggers"] = {
				{
					["trigger"] = {
						["type"] = "aura2",
						["subeventSuffix"] = "_CAST_START",
						["auranames"] = {
							"Sunder Armor", -- [1]
						},
						["event"] = "Health",
						["unit"] = "target",
						["spellIds"] = {
						},
						["custom"] = "function(allstates, event, ...)\n    if event == \"COMBAT_LOG_EVENT_UNFILTERED\" then\n        local _, sub, _, src = ...\n        allstates[\"\"] = { show = true, changed = true, stacks = 5 }\n        return true\n    end\nend",
						["names"] = {
						},
						["debuffType"] = "HARMFUL",
						["useName"] = true,
						["ownOnly"] = false,
					},
					["untrigger"] = {
					},
				}, -- [1]
				["activeTriggerMode"] = -10,
			},
			["internalVersion"] = 76,
			["keepAspectRatio"] = false,
			["selfPoint"] = "CENTER",
			["desaturate"] = false,
			["subRegions"] = {
				{
					["type"] = "subbackground",
				}, -- [1]
				{
					["text_text_format_s_format"] = "none",
					["text_text"] = "%s",
					["text_shadowColor"] = {
						0, -- [1]
						0, -- [2]
						0, -- [3]
						1, -- [4]
					},
					["text_selfPoint"] = "AUTO",
					["text_automaticWidth"] = "Auto",
					["text_fixedWidth"] = 64,
					["anchorYOffset"] = 0,
					["text_justify"] = "CENTER",
					["rotateText"] = "NONE",
					["type"] = "subtext",
					["text_color"] = {
						1, -- [1]
						0.82, -- [2]
						0, -- [3]
						1, -- [4]
					},
					["text_font"] = "Friz Quadrata TT",
					["text_fontSize"] = 12,
					["anchorXOffset"] = 0,
					["text_visible"] = true,
				}, -- [2]
			},
			["height"] = 48,
			["load"] = {
				["class"] = {
					["single"] = "WARRIOR",
					["multi"] = {
						["WARRIOR"] = true,
					},
				},
				["use_class"] = true,
				["size"] = {
					["multi"] = {
					},
				},
			},
			["regionType"] = "icon",
			["conditions"] = {
				{
					["check"] = {
						["trigger"] = 1,
						["variable"] = "stacks",
						["op"] = "<",
						["value"] = "5",
					},
					["changes"] = {
						{
							["value"] = {
								1, -- [1]
								0.2, -- [2]
								0.2, -- [3]
								1, -- [4]
							},
							["property"] = "color",
						}, -- [1]
						{
							["value"] = {
								["sound_type"] = "Play",
								["sound"] = "Interface\\AddOns\\WeakAuras\\Media\\Sounds\\BoxingArenaSound.ogg",
							},
							["property"] = "sound",
						}, -- [2]
					},
				}, -- [1]
			},
			["actions"] = {
				["start"] = {
					["do_custom"] = true,
					["custom"] = "aura_env.count = (aura_env.count or 0) + 1\nprint('|cff00ff00Sunder|r stacks: ' .. tostring(aura_env.count))",
				},
				["finish"] = {
				},
				["init"] = {
					["do_custom"] = false,
				},
			},
			["width"] = 48,
			["zoom"] = 0.3,
			["frameStrata"] = 1,
			["id"] = "Sunder Armor Tracker",
			["alpha"] = 1,
			["uid"] = "f(Rk3K)b9Pq",
			["inverse"] = false,
			["xOffset"] = 0,
			["color"] = {
				1, -- [1]
				1, -- [2]
				1, -- [3]
				1, -- [4]
			},
			["information"] = {
				["forceEvents"] = true,
			},
		},
	},
	["registered"] = {
	},
	["features"] = {
	},
	["historyCutoff"] = 730,
	["migrationCutoff"] = 730,
}
