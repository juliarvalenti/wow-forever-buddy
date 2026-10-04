-- Long strings and escapes, as addons and the %q format write them.
StringsDB = {
	["plain"] = "Hello, Azeroth",
	["quotes"] = "She said \"Ding!\" and 'grats'",
	["single"] = 'it\'s "fine"',
	["backslash"] = "Interface\\AddOns\\MyAddon\\icon.tga",
	["newline_escape"] = "first\nsecond\r\nthird",
	["percent_q_newline"] = "line one\
line two",
	["tabs"] = "a\tb\tc",
	["decimal"] = "\72\105\0331\0",
	["control"] = "\001\002\031\127",
	["bell_etc"] = "\a\b\f\v",
	["item_link"] = "|cffa335ee|Hitem:19019::::::::60:::::|h[Thunderfury, Blessed Blade of the Windseeker]|h|r",
	["utf8"] = "Thrandor — Ëlune's Grace — 龍",
	["emoji"] = "😀 gg",
	["empty"] = "",
	["long"] = [[
Multi-line
	text with "quotes" and 'apostrophes' and \n not an escape]],
	["long_level"] = [==[
contains ]] and ]=] before the end]==],
	["long_empty"] = [[]],
	["huge_line"] = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
	[ [[long key]] ] = 1,
	["numbers"] = {
		0, -- [1]
		-1, -- [2]
		3.14159265358979, -- [3]
		-2.5e-07, -- [4]
		1e+300, -- [5]
		0x7fffffff, -- [6]
		9007199254740993, -- [7]
		-9223372036854775808, -- [8]
	},
	["mixed"] = {
		"positional", -- [1]
		["key"] = "value",
		bare = true;
		[1.5] = false,
		[-3] = "negative key",
		[true] = "bool key",
	},
}
--[[ trailing block comment ]]
StringsVersion = 2
