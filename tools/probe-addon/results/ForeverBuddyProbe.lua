
ForeverBuddyProbeDB = {
["probeVersion"] = 2,
["loads"] = {
{
["fromDisk"] = false,
["t"] = 1791185455,
},
},
["logouts"] = 1,
["characters"] = {
["Ellygie-Vargur"] = {
["probedAt"] = 1791185467,
["events"] = {
["BANKFRAME_OPENED"] = {
["samples"] = {
{
["args"] = {
["n"] = 0,
},
["t"] = 1791185524,
},
},
["count"] = 1,
["last"] = 1791185524,
},
["PLAYER_MONEY"] = {
["samples"] = {
{
["args"] = {
["n"] = 0,
},
["t"] = 1791185683,
},
},
["count"] = 1,
["last"] = 1791185683,
},
["ZONE_CHANGED_NEW_AREA"] = {
["samples"] = {
{
["args"] = {
["n"] = 0,
},
["t"] = 1791185466,
},
},
["count"] = 1,
["last"] = 1791185466,
},
["PLAYER_LOGIN"] = {
["samples"] = {
{
["args"] = {
["n"] = 0,
},
["t"] = 1791185463,
},
},
["count"] = 1,
["last"] = 1791185463,
},
["PLAYER_ENTERING_WORLD"] = {
["samples"] = {
{
["args"] = {
true,
false,
["n"] = 2,
},
["t"] = 1791185463,
},
},
["count"] = 1,
["last"] = 1791185463,
},
["PLAYER_XP_UPDATE"] = {
["samples"] = {
{
["args"] = {
"<string:6>",
["n"] = 1,
},
["t"] = 1791185678,
},
},
["count"] = 1,
["last"] = 1791185678,
},
["PLAYER_LOGOUT"] = {
["samples"] = {
{
["args"] = {
["n"] = 0,
},
["t"] = 1791185694,
},
},
["count"] = 1,
["last"] = 1791185694,
},
["QUEST_TURNED_IN"] = {
["samples"] = {
{
["args"] = {
94774,
210,
0,
["n"] = 3,
},
["t"] = 1791185678,
},
},
["count"] = 1,
["last"] = 1791185678,
},
["MERCHANT_SHOW"] = {
["samples"] = {
{
["args"] = {
["n"] = 0,
},
["t"] = 1791185608,
},
},
["count"] = 1,
["last"] = 1791185608,
},
["BAG_UPDATE_DELAYED"] = {
["samples"] = {
{
["args"] = {
["n"] = 0,
},
["t"] = 1791185465,
},
{
["args"] = {
["n"] = 0,
},
["t"] = 1791185525,
},
{
["args"] = {
["n"] = 0,
},
["t"] = 1791185525,
},
},
["count"] = 3,
["last"] = 1791185525,
},
["MAIL_INBOX_UPDATE"] = {
["samples"] = {
{
["args"] = {
["n"] = 0,
},
["t"] = 1791185533,
},
},
["count"] = 1,
["last"] = 1791185533,
},
["MAIL_SHOW"] = {
["samples"] = {
{
["args"] = {
["n"] = 0,
},
["t"] = 1791185533,
},
},
["count"] = 1,
["last"] = 1791185533,
},
["UPDATE_INSTANCE_INFO"] = {
["samples"] = {
{
["args"] = {
["n"] = 0,
},
["t"] = 1791185463,
},
{
["args"] = {
["n"] = 0,
},
["t"] = 1791185468,
},
},
["count"] = 2,
["last"] = 1791185468,
},
["TIME_PLAYED_MSG"] = {
["samples"] = {
{
["args"] = {
19551,
474,
["n"] = 2,
},
["t"] = 1791185468,
},
},
["count"] = 1,
["last"] = 1791185468,
},
["PLAYER_ALIVE"] = {
["samples"] = {
{
["args"] = {
["n"] = 0,
},
["t"] = 1791185463,
},
},
["count"] = 1,
["last"] = 1791185463,
},
},
["checks"] = {
["played"] = {
["TIME_PLAYED_MSG"] = {
["ok"] = true,
["values"] = {
19551,
474,
},
},
["RequestTimePlayed"] = {
["ok"] = true,
["empty"] = true,
["values"] = {
},
},
},
["ilvl"] = {
["GetAverageItemLevel"] = {
["ok"] = true,
["values"] = {
2.125,
2.125,
2.125,
},
},
},
["secrets"] = {
["ShouldUnitIdentityBeSecret.target"] = {
["ok"] = true,
["values"] = {
false,
},
},
["HasSecretRestrictions"] = {
["ok"] = true,
["values"] = {
true,
},
},
},
["lockouts"] = {
["RequestRaidInfo"] = {
["ok"] = true,
["empty"] = true,
["values"] = {
},
},
["GetNumSavedInstances"] = {
["ok"] = true,
["values"] = {
0,
},
},
},
["bags"] = {
["GetContainerNumSlots.0"] = {
["ok"] = true,
["values"] = {
20,
},
},
["GetContainerItemInfo.0.1"] = {
["ok"] = true,
["values"] = {
{
["itemName"] = "Hearthstone",
["hasLoot"] = false,
["stackCount"] = 1,
["iconFileID"] = 134414,
["hasNoValue"] = true,
["isLocked"] = false,
["itemID"] = 6948,
["isBound"] = true,
["hyperlink"] = "|cnIQ1:|Hitem:6948::::::::11:1487::75:::::::|h[Hearthstone]|h|r",
["isFiltered"] = false,
["isReadable"] = false,
["quality"] = 1,
},
},
},
["GetBagName.0"] = {
["ok"] = true,
["values"] = {
"Backpack",
},
},
["GetBagName.2"] = {
["ok"] = true,
["values"] = {
"Book Bag",
},
},
["GetBagName.1"] = {
["ok"] = true,
["values"] = {
"Apprentice's Herb Pouch",
},
},
["GetContainerNumFreeSlots.2"] = {
["ok"] = true,
["values"] = {
0,
0,
},
},
["GetContainerItemInfo.3.1"] = {
["ok"] = true,
["values"] = {
{
["itemName"] = "Stolen Enchanting Supplies",
["hasLoot"] = false,
["stackCount"] = 2,
["iconFileID"] = 1003590,
["hasNoValue"] = true,
["isLocked"] = false,
["itemID"] = 247805,
["isBound"] = true,
["hyperlink"] = "|cnIQ1:|Hitem:247805::::::::11:1487:::::::::|h[Stolen Enchanting Supplies]|h|r",
["isFiltered"] = false,
["isReadable"] = false,
["quality"] = 1,
},
},
},
["GetContainerNumSlots.1"] = {
["ok"] = true,
["values"] = {
4,
},
},
["GetBagName.5"] = {
["ok"] = true,
["empty"] = true,
["values"] = {
},
},
["GetContainerNumSlots.4"] = {
["ok"] = true,
["values"] = {
6,
},
},
["GetContainerNumFreeSlots.1"] = {
["ok"] = true,
["values"] = {
0,
32,
},
},
["GetContainerNumFreeSlots.5"] = {
["ok"] = true,
["values"] = {
0,
},
},
["GetBagName.3"] = {
["ok"] = true,
["values"] = {
"Linen Bag",
},
},
["GetContainerNumSlots.5"] = {
["ok"] = true,
["values"] = {
0,
},
},
["GetContainerNumFreeSlots.4"] = {
["ok"] = true,
["values"] = {
4,
0,
},
},
["GetContainerItemInfo.4.1"] = {
["ok"] = true,
["empty"] = true,
["values"] = {
},
},
["GetContainerNumFreeSlots.3"] = {
["ok"] = true,
["values"] = {
3,
0,
},
},
["GetContainerNumSlots.3"] = {
["ok"] = true,
["values"] = {
6,
},
},
["GetContainerItemInfo.2.1"] = {
["ok"] = true,
["values"] = {
{
["itemName"] = "Bolt of Linen Cloth",
["hasLoot"] = false,
["stackCount"] = 7,
["iconFileID"] = 132890,
["hasNoValue"] = false,
["isLocked"] = false,
["itemID"] = 2996,
["isBound"] = false,
["hyperlink"] = "|cnIQ1:|Hitem:2996::::::::11:1487:::::::::|h[Bolt of Linen Cloth]|h|r",
["isFiltered"] = false,
["isReadable"] = false,
["quality"] = 1,
},
},
},
["GetBagName.4"] = {
["ok"] = true,
["values"] = {
"Linen Bag",
},
},
["GetContainerNumFreeSlots.0"] = {
["ok"] = true,
["values"] = {
0,
0,
},
},
["GetContainerItemInfo.1.1"] = {
["ok"] = true,
["values"] = {
{
["itemName"] = "Earthroot",
["hasLoot"] = false,
["stackCount"] = 20,
["iconFileID"] = 134187,
["hasNoValue"] = false,
["isLocked"] = false,
["itemID"] = 2449,
["isBound"] = false,
["hyperlink"] = "|cnIQ1:|Hitem:2449::::::::11:1487:::::::::|h[Earthroot]|h|r",
["isFiltered"] = false,
["isReadable"] = false,
["quality"] = 1,
},
},
},
["GetContainerNumSlots.2"] = {
["ok"] = true,
["values"] = {
6,
},
},
},
["identity"] = {
["UnitSex"] = {
["ok"] = true,
["values"] = {
3,
},
},
["GetGuildInfo"] = {
["ok"] = true,
["values"] = {
[3] = 0,
},
},
["UnitFactionGroup"] = {
["ok"] = true,
["values"] = {
"Alliance",
"Alliance",
},
},
["UnitGUID"] = {
["ok"] = true,
["values"] = {
"Player-4613-00CBF893",
},
},
["UnitRace"] = {
["ok"] = true,
["values"] = {
"Human",
"Human",
1,
},
},
["UnitLevel"] = {
["ok"] = true,
["values"] = {
11,
},
},
["UnitName"] = {
["ok"] = true,
["values"] = {
"Ellygie",
"Vargur",
},
},
["UnitClass"] = {
["ok"] = true,
["values"] = {
"Priest",
"PRIEST",
5,
},
},
},
["gear"] = {
["GetInventoryItemID.13"] = {
["ok"] = true,
["empty"] = true,
["values"] = {
},
},
["GetInventoryItemLink.4"] = {
["ok"] = true,
["values"] = {
"|cnIQ1:|Hitem:2575::::::::11:1487::11:::::::|h[Red Linen Shirt]|h|r",
},
},
["GetCurrentItemLevel.8"] = {
["ok"] = true,
["values"] = {
5,
},
},
["GetInventoryItemID.11"] = {
["ok"] = true,
["empty"] = true,
["values"] = {
},
},
["GetInventoryItemLink.7"] = {
["ok"] = true,
["values"] = {
"|cnIQ1:|Hitem:6076::::::::11:1487::11:::::::|h[Tapered Pants]|h|r",
},
},
["GetCurrentItemLevel.9"] = {
["ok"] = true,
["values"] = {
9,
},
},
["GetInventoryItemID.16"] = {
["ok"] = true,
["values"] = {
5580,
},
},
["GetCurrentItemLevel.15"] = {
["ok"] = true,
["values"] = {
5,
},
},
["GetCurrentItemLevel.5"] = {
["ok"] = true,
["values"] = {
5,
},
},
["GetInventoryItemID.14"] = {
["ok"] = true,
["empty"] = true,
["values"] = {
},
},
["GetCurrentItemLevel.16"] = {
["ok"] = true,
["values"] = {
5,
},
},
["GetInventoryItemID.12"] = {
["ok"] = true,
["empty"] = true,
["values"] = {
},
},
["GetCurrentItemLevel.4"] = {
["ok"] = true,
["values"] = {
10,
},
},
["GetCurrentItemLevel.10"] = {
["ok"] = true,
["values"] = {
3,
},
},
["GetCurrentItemLevel.7"] = {
["ok"] = true,
["values"] = {
5,
},
},
["GetInventoryItemID.19"] = {
["ok"] = true,
["empty"] = true,
["values"] = {
},
},
["GetInventoryItemID.18"] = {
["ok"] = true,
["empty"] = true,
["values"] = {
},
},
["GetInventoryItemID.5"] = {
["ok"] = true,
["values"] = {
16605,
},
},
["GetInventoryItemLink.16"] = {
["ok"] = true,
["values"] = {
"|cnIQ1:|Hitem:5580::::::::11:1487::11:::::::|h[Militia Hammer]|h|r",
},
},
["GetInventoryItemLink.5"] = {
["ok"] = true,
["values"] = {
"|cnIQ2:|Hitem:16605::::::::11:1487::11:::::::|h[Friar's Robes of the Light]|h|r",
},
},
["GetInventoryItemID.10"] = {
["ok"] = true,
["values"] = {
1377,
},
},
["GetInventoryItemLink.8"] = {
["ok"] = true,
["values"] = {
"|cnIQ1:|Hitem:80::::::::11:1487::11:::::::|h[Soft Fur-lined Shoes]|h|r",
},
},
["GetInventoryItemLink.6"] = {
["ok"] = true,
["values"] = {
"|cnIQ1:|Hitem:983::::::::11:1487::11:::::::|h[Red Linen Sash]|h|r",
},
},
["GetInventoryItemLink.15"] = {
["ok"] = true,
["values"] = {
"|cnIQ1:|Hitem:11475::::::::11:1487::11:::::::|h[Wine-stained Cloak]|h|r",
},
},
["GetInventoryItemID.8"] = {
["ok"] = true,
["values"] = {
80,
},
},
["GetInventoryItemID.3"] = {
["ok"] = true,
["empty"] = true,
["values"] = {
},
},
["GetInventoryItemLink.10"] = {
["ok"] = true,
["values"] = {
"|cnIQ0:|Hitem:1377::::::::11:1487:::::::::|h[Frayed Gloves]|h|r",
},
},
["GetInventoryItemID.17"] = {
["ok"] = true,
["empty"] = true,
["values"] = {
},
},
["GetInventoryItemID.2"] = {
["ok"] = true,
["empty"] = true,
["values"] = {
},
},
["GetCurrentItemLevel.6"] = {
["ok"] = true,
["values"] = {
9,
},
},
["GetInventoryItemID.15"] = {
["ok"] = true,
["values"] = {
11475,
},
},
["GetInventoryItemID.6"] = {
["ok"] = true,
["values"] = {
983,
},
},
["GetInventoryItemID.9"] = {
["ok"] = true,
["values"] = {
3373,
},
},
["GetInventoryItemID.4"] = {
["ok"] = true,
["values"] = {
2575,
},
},
["GetInventoryItemID.1"] = {
["ok"] = true,
["empty"] = true,
["values"] = {
},
},
["GetInventoryItemLink.9"] = {
["ok"] = true,
["values"] = {
"|cnIQ0:|Hitem:3373::::::::11:1487:::::::::|h[Patchwork Bracers]|h|r",
},
},
["GetInventoryItemID.7"] = {
["ok"] = true,
["values"] = {
6076,
},
},
},
["client"] = {
["GetBuildInfo"] = {
["ok"] = true,
["values"] = {
"1.60.1",
"70205",
"Oct  2 2026",
16001,
"",
" ",
},
},
["GetRealmName"] = {
["ok"] = true,
["values"] = {
"Classic Beta PvP 2",
},
},
["projectId"] = {
["ok"] = true,
["values"] = {
18,
},
},
["GetLocale"] = {
["ok"] = true,
["values"] = {
"enUS",
},
},
["GetNormalizedRealmName"] = {
["ok"] = true,
["values"] = {
"ClassicBetaPvP2",
},
},
},
["bank"] = {
["GetContainerNumSlots.-1"] = {
["ok"] = true,
["values"] = {
32,
},
},
["GetContainerItemInfo.-1.1"] = {
["ok"] = true,
["empty"] = true,
["values"] = {
},
},
["FetchPurchasedBankTabIDs"] = {
["ok"] = true,
["values"] = {
{
},
},
},
["CanViewBank"] = {
["ok"] = true,
["values"] = {
true,
},
},
},
["addons"] = {
["GetAddOnInfo.1"] = {
["ok"] = true,
["values"] = {
"ForeverBuddyProbe",
"ForeverBuddy Probe",
"One-time data check for WoW Forever Buddy. Records which game APIs return real data. Safe to delete afterwards.",
true,
"",
"INSECURE",
},
},
["GetAddOnInfo.5"] = {
["ok"] = true,
["values"] = {
"SpokenQuests",
"Spoken Quests",
"Quest and gossip dialogue read aloud, through the Spoken player. Formerly VoiceOver Redux. Restores quest and gossip voiceovers on supported clients.",
true,
"",
"INSECURE",
},
},
["GetAddOnInfo.3"] = {
["ok"] = true,
["values"] = {
"SpokenContributions",
"|cff888888Spoken Contributions|r",
"Where Spoken keeps the lines it gathered for you. Upload SavedVariables/SpokenContributions.lua at spoken.rusty.one/contribute.",
false,
"DISABLED",
"INSECURE",
},
},
["GetAddOnMetadata.Version.5"] = {
["ok"] = true,
["values"] = {
"2.2.1",
},
},
["GetAddOnMetadata.Version.2"] = {
["ok"] = true,
["values"] = {
"2.1.1",
},
},
["GetAddOnMetadata.Version.4"] = {
["ok"] = true,
["values"] = {
"2.3.2",
},
},
["GetAddOnInfo.2"] = {
["ok"] = true,
["values"] = {
"SpokenBooks",
"Spoken Books",
"Books, letters and notes read aloud through the Spoken player.",
true,
"",
"INSECURE",
},
},
["GetAddOnMetadata.Version.1"] = {
["ok"] = true,
["values"] = {
"2",
},
},
["GetAddOnInfo.4"] = {
["ok"] = true,
["values"] = {
"SpokenPlayer",
"Spoken Player",
"The voice player every Spoken addon speaks through: one queue, one window, one minimap button.",
true,
"",
"INSECURE",
},
},
["GetAddOnMetadata.Version.3"] = {
["ok"] = true,
["values"] = {
"2.3.2",
},
},
["GetNumAddOns"] = {
["ok"] = true,
["values"] = {
12,
},
},
},
["money"] = {
["GetMoney.onEvent"] = {
["ok"] = true,
["values"] = {
1747,
},
},
["GetMoney"] = {
["ok"] = true,
["values"] = {
1947,
},
},
},
["xp"] = {
["GetRestState"] = {
["ok"] = true,
["values"] = {
1,
"Rested",
2,
},
},
["IsResting"] = {
["ok"] = true,
["values"] = {
true,
},
},
["GetXPExhaustion"] = {
["ok"] = true,
["values"] = {
674,
},
},
["UnitXPMax"] = {
["ok"] = true,
["values"] = {
8800,
},
},
["UnitXP"] = {
["ok"] = true,
["values"] = {
715,
},
},
},
["items"] = {
["TooltipInfo.GetItemByID.2575"] = {
["ok"] = true,
["values"] = {
{
["isAzeriteEmpoweredItem"] = false,
["type"] = 0,
["lines"] = {
{
["leftColor"] = {
["b"] = 1,
["GetHSL"] = "<function>",
["g"] = 1,
["GetRGBA"] = "<function>",
["IsRGBEqualTo"] = "<function>",
["SetRGB"] = "<function>",
["GetRGB"] = "<function>",
["OnLoad"] = "<function>",
["GenerateHexColorMarkup"] = "<function>",
["WrapTextInColorCode"] = "<function>",
["GenerateHexColor"] = "<function>",
["IsEqualTo"] = "<function>",
["r"] = 1,
["GenerateHexColorNoAlpha"] = "<function>",
["SetRGBA"] = "<function>",
["GetRGBAsBytes"] = "<function>",
["GetRGBAAsBytes"] = "<function>",
},
["type"] = 22,
["leftText"] = "Red Linen Shirt",
["quality"] = 1,
},
{
["type"] = 21,
["isValidItemType"] = true,
["leftColor"] = {
["b"] = 1,
["GetHSL"] = "<function>",
["g"] = 1,
["GetRGBA"] = "<function>",
["IsRGBEqualTo"] = "<function>",
["SetRGB"] = "<function>",
["GetRGB"] = "<function>",
["OnLoad"] = "<function>",
["GenerateHexColorMarkup"] = "<function>",
["WrapTextInColorCode"] = "<function>",
["GenerateHexColor"] = "<function>",
["IsEqualTo"] = "<function>",
["r"] = 1,
["GenerateHexColorNoAlpha"] = "<function>",
["SetRGBA"] = "<function>",
["GetRGBAsBytes"] = "<function>",
["GetRGBAAsBytes"] = "<function>",
},
["leftText"] = "Shirt",
["isValidInvSlot"] = true,
},
{
["leftText"] = "",
["maxPrice"] = -1,
["leftColor"] = {
["b"] = 0,
["GetHSL"] = "<function>",
["g"] = 0.8235294222831726,
["GetRGBA"] = "<function>",
["IsRGBEqualTo"] = "<function>",
["SetRGB"] = "<function>",
["GetRGB"] = "<function>",
["OnLoad"] = "<function>",
["GenerateHexColorMarkup"] = "<function>",
["WrapTextInColorCode"] = "<function>",
["GenerateHexColor"] = "<function>",
["IsEqualTo"] = "<function>",
["r"] = 1,
["GenerateHexColorNoAlpha"] = "<function>",
["SetRGBA"] = "<function>",
["GetRGBAsBytes"] = "<function>",
["GetRGBAAsBytes"] = "<function>",
},
["type"] = 11,
["price"] = 25,
},
},
["isAzeriteItem"] = false,
["id"] = 2575,
["hyperlink"] = "|cnIQ1:|Hitem:2575::::::::11:1487:::::::::|h[Red Linen Shirt]|h|r",
["isCorruptedItem"] = false,
["dataInstanceID"] = 2,
},
},
},
["GetItemIconByID.2575"] = {
["ok"] = true,
["values"] = {
135029,
},
},
["GetItemIconByID.6948"] = {
["ok"] = true,
["values"] = {
134414,
},
},
["GetItemInfo.2575"] = {
["ok"] = true,
["values"] = {
"Red Linen Shirt",
"|cnIQ1:|Hitem:2575::::::::11:1487:::::::::|h[Red Linen Shirt]|h|r",
1,
10,
0,
"Armor",
"Miscellaneous",
1,
"INVTYPE_BODY",
135029,
25,
4,
0,
0,
0,
[17] = false,
[18] = "",
},
},
["GetItemInfoInstant.2575"] = {
["ok"] = true,
["values"] = {
2575,
"Armor",
"Miscellaneous",
"INVTYPE_BODY",
135029,
4,
0,
},
},
["TooltipInfo.GetItemByID.6948"] = {
["ok"] = true,
["values"] = {
{
["isAzeriteEmpoweredItem"] = false,
["type"] = 0,
["lines"] = {
{
["leftColor"] = {
["b"] = 1,
["GetHSL"] = "<function>",
["g"] = 1,
["GetRGBA"] = "<function>",
["IsRGBEqualTo"] = "<function>",
["SetRGB"] = "<function>",
["GetRGB"] = "<function>",
["OnLoad"] = "<function>",
["GenerateHexColorMarkup"] = "<function>",
["WrapTextInColorCode"] = "<function>",
["GenerateHexColor"] = "<function>",
["IsEqualTo"] = "<function>",
["r"] = 1,
["GenerateHexColorNoAlpha"] = "<function>",
["SetRGBA"] = "<function>",
["GetRGBAsBytes"] = "<function>",
["GetRGBAAsBytes"] = "<function>",
},
["type"] = 22,
["leftText"] = "Hearthstone",
["quality"] = 1,
},
{
["leftColor"] = {
["b"] = 1,
["GetHSL"] = "<function>",
["g"] = 1,
["GetRGBA"] = "<function>",
["IsRGBEqualTo"] = "<function>",
["SetRGB"] = "<function>",
["GetRGB"] = "<function>",
["OnLoad"] = "<function>",
["GenerateHexColorMarkup"] = "<function>",
["WrapTextInColorCode"] = "<function>",
["GenerateHexColor"] = "<function>",
["IsEqualTo"] = "<function>",
["r"] = 1,
["GenerateHexColorNoAlpha"] = "<function>",
["SetRGBA"] = "<function>",
["GetRGBAsBytes"] = "<function>",
["GetRGBAAsBytes"] = "<function>",
},
["type"] = 20,
["leftText"] = "Binds when picked up",
["bonding"] = 6,
},
{
["leftColor"] = {
["b"] = 1,
["GetHSL"] = "<function>",
["g"] = 1,
["GetRGBA"] = "<function>",
["IsRGBEqualTo"] = "<function>",
["SetRGB"] = "<function>",
["GetRGB"] = "<function>",
["OnLoad"] = "<function>",
["GenerateHexColorMarkup"] = "<function>",
["WrapTextInColorCode"] = "<function>",
["GenerateHexColor"] = "<function>",
["IsEqualTo"] = "<function>",
["r"] = 1,
["GenerateHexColorNoAlpha"] = "<function>",
["SetRGBA"] = "<function>",
["GetRGBAsBytes"] = "<function>",
["GetRGBAAsBytes"] = "<function>",
},
["type"] = 0,
["leftText"] = "Unique",
},
},
["isAzeriteItem"] = false,
["id"] = 6948,
["hyperlink"] = "|cnIQ1:|Hitem:6948::::::::11:1487:::::::::|h[Hearthstone]|h|r",
["isCorruptedItem"] = false,
["dataInstanceID"] = 1,
},
},
},
["GetItemInfo.6948"] = {
["ok"] = true,
["values"] = {
"Hearthstone",
"|cnIQ1:|Hitem:6948::::::::11:1487:::::::::|h[Hearthstone]|h|r",
1,
1,
0,
"Miscellaneous",
"Junk",
1,
"INVTYPE_NON_EQUIP_IGNORE",
134414,
0,
15,
0,
1,
0,
[17] = false,
[18] = "",
},
},
["GetItemInfoInstant.6948"] = {
["ok"] = true,
["values"] = {
6948,
"Miscellaneous",
"Junk",
"INVTYPE_NON_EQUIP_IGNORE",
134414,
15,
0,
},
},
},
["zone"] = {
["C_Map.GetBestMapForUnit"] = {
["ok"] = true,
["values"] = {
1453,
},
},
["C_Map.GetMapInfo"] = {
["ok"] = true,
["values"] = {
{
["mapType"] = 3,
["mapID"] = 1453,
["name"] = "Stormwind City",
["parentMapID"] = 1415,
["flags"] = 0,
},
},
},
["GetRealZoneText"] = {
["ok"] = true,
["values"] = {
"Stormwind City",
},
},
["GetSubZoneText"] = {
["ok"] = true,
["values"] = {
"Trade District",
},
},
},
["merchant"] = {
["GetRepairAllCost"] = {
["ok"] = true,
["values"] = {
0,
false,
},
},
},
["professions"] = {
["GetProfessions"] = {
["ok"] = true,
["values"] = {
5,
7,
[5] = 6,
},
},
["GetProfessionInfo.2"] = {
["ok"] = true,
["values"] = {
"Tailoring",
136249,
34,
75,
1,
26,
197,
0,
-1,
0,
"Tailoring",
},
},
["GetProfessionInfo.5"] = {
["ok"] = true,
["values"] = {
"Cooking",
133971,
29,
75,
1,
24,
185,
0,
-1,
0,
"Cooking",
},
},
["GetProfessionInfo.1"] = {
["ok"] = true,
["values"] = {
"Herbalism",
136246,
60,
75,
2,
21,
182,
0,
-1,
0,
"Herbalism",
},
},
},
["mail"] = {
["GetInboxNumItems"] = {
["ok"] = true,
["values"] = {
0,
0,
},
},
["CheckInbox"] = {
["ok"] = true,
["empty"] = true,
["values"] = {
},
},
},
["quests"] = {
["GetTitleForQuestID"] = {
["ok"] = true,
["values"] = {
"Divine Grace",
},
},
},
["macros"] = {
["GetNumMacros"] = {
["ok"] = true,
["values"] = {
0,
0,
},
},
["GetMacroInfo.1"] = {
["ok"] = true,
["empty"] = true,
["values"] = {
},
},
},
},
["unknownEvents"] = {
},
["lastLogout"] = 1791185694,
["lastLogin"] = 1791185463,
["firstLogin"] = 1791185463,
},
},
}
