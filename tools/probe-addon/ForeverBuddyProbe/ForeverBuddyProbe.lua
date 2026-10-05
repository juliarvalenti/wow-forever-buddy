-- ForeverBuddy Probe: checks, on the live Forever client, that the APIs the
-- feature matrix (docs/specs/feature-matrix.md) relies on exist AND return
-- real data, not nil or secret values. Everything is written to
-- ForeverBuddyProbeDB, one top-level table, keyed by character.
--
-- It never changes game state (no item, mail, money, AH or chat actions). It
-- does send these harmless read requests to the server:
--   at login:      RequestTimePlayed (prints "Total time played" in chat),
--                  RequestRaidInfo
--   at a mailbox:  CheckInbox
--   at the AH:     QueryOwnedAuctions
--   on request:    /fbprobe search (one item search), /fbprobe scan (full
--                  scan; uses the account-wide 15-minute throttle)
--
-- Bridge v0.4 spike (probe version 3): /fbprobe reload shows two buttons that
-- reload the UI when clicked, one a plain addon button calling ReloadUI(),
-- one a secure button running the /reload macro, and records which route
-- the client allows. Data.lua is a data slot the app would rewrite: each
-- load records the `stamp` it held, so editing it while the game runs and
-- reloading shows whether /reload re-reads a changed file.
--
-- Privacy: anything that can contain other players' names or text (mail,
-- loot, death, auction house, and every event sample) is stored as shape
-- only: numbers and booleans kept, strings replaced by "<string:LENGTH>".

local ADDON_NAME = ...
local PROBE_VERSION = 3
local MAX_SAMPLES = 5

-- Sections whose strings are redacted (see the header).
local REDACTED = { mail = true, loot = true, death = true, ah = true }

local db -- ForeverBuddyProbeDB
local char -- this character's entry
local unknownEvents = {} -- events this client refused to register

-- Helpers ------------------------------------------------------------------

local function now()
    return GetServerTime and GetServerTime() or time()
end

-- Resolves "C_Item.GetItemInfo" to the function, or nil if any part is missing.
local function api(path)
    local value = _G
    for part in string.gmatch(path, "[^%.]+") do
        if type(value) ~= "table" then
            return nil
        end
        value = value[part]
    end
    if type(value) == "function" then
        return value
    end
    return nil
end

local function isSecret(v)
    return (issecretvalue and issecretvalue(v)) or false
end

-- A SavedVariables-safe copy of v: secrets become markers, depth and size
-- are capped so the file stays small. With `redact`, strings keep only their
-- length, so names and mail text never reach the file.
local function describe(v, redact, depth)
    depth = depth or 0
    if isSecret(v) then
        return "<secret>"
    end
    local t = type(v)
    if t == "string" and redact then
        return "<string:" .. #v .. ">"
    elseif t == "table" then
        if issecrettable and issecrettable(v) then
            return "<secret table>"
        end
        if depth >= 4 then
            return "<table>"
        end
        local out, n = {}, 0
        for k, val in pairs(v) do
            n = n + 1
            if n > 40 then
                out["_truncated"] = true
                break
            end
            if type(k) == "string" or type(k) == "number" then
                out[k] = describe(val, redact, depth + 1)
            end
        end
        return out
    elseif t == "function" or t == "userdata" or t == "thread" then
        return "<" .. t .. ">"
    end
    return v
end

local function pack(...)
    return { n = select("#", ...), ... }
end

-- Calls the API at `path` with the given arguments and records the outcome
-- under checks[section][label]: missing, error, or the returned values.
-- Returns the raw values on success so callers can probe further.
local function try(section, label, path, ...)
    local checks = char.checks
    checks[section] = checks[section] or {}
    local fn = type(path) == "function" and path or api(path)
    if not fn then
        checks[section][label] = { ok = false, err = "missing" }
        return
    end
    local r = pack(pcall(fn, ...))
    if not r[1] then
        checks[section][label] = { ok = false, err = tostring(r[2]) }
        return
    end
    local values, anySecret, anyValue = {}, false, false
    for i = 2, r.n do
        values[i - 1] = describe(r[i], REDACTED[section])
        if isSecret(r[i]) then
            anySecret = true
        elseif r[i] ~= nil then
            anyValue = true
        end
    end
    checks[section][label] = {
        ok = true,
        secret = anySecret or nil,
        empty = (not anyValue) or nil,
        values = values,
    }
    return unpack(r, 2, r.n)
end

local function sample(event, ...)
    local e = char.events[event]
    if not e then
        e = { count = 0, samples = {} }
        char.events[event] = e
    end
    e.count = e.count + 1
    e.last = now()
    if #e.samples < MAX_SAMPLES then
        -- Event args can carry chat text and player names: shape only.
        table.insert(e.samples, { t = now(), args = describe(pack(...), true) })
    end
end

-- Snapshots (rows 3-21, 45, 48) --------------------------------------------

local function probeCharacter()
    try("client", "GetBuildInfo", "GetBuildInfo")
    try("client", "GetLocale", "GetLocale")
    try("client", "GetRealmName", "GetRealmName")
    try("client", "GetNormalizedRealmName", "GetNormalizedRealmName")
    char.checks.client.projectId = { ok = true, values = { WOW_PROJECT_ID } }

    -- 3, 6: identity
    try("identity", "UnitName", "UnitName", "player")
    try("identity", "UnitGUID", "UnitGUID", "player")
    try("identity", "UnitClass", "UnitClass", "player")
    try("identity", "UnitRace", "UnitRace", "player")
    try("identity", "UnitSex", "UnitSex", "player")
    try("identity", "UnitLevel", "UnitLevel", "player")
    try("identity", "UnitFactionGroup", "UnitFactionGroup", "player")
    try("identity", "GetGuildInfo", "GetGuildInfo", "player")

    -- 7: xp / rested
    try("xp", "UnitXP", "UnitXP", "player")
    try("xp", "UnitXPMax", "UnitXPMax", "player")
    try("xp", "GetXPExhaustion", "GetXPExhaustion")
    try("xp", "GetRestState", "GetRestState")
    try("xp", "IsResting", "IsResting")

    -- 8, 9: item level, money
    try("ilvl", "GetAverageItemLevel", "GetAverageItemLevel")
    try("money", "GetMoney", "GetMoney")

    -- 11: location
    try("zone", "GetRealZoneText", "GetRealZoneText")
    try("zone", "GetSubZoneText", "GetSubZoneText")
    local mapID = try("zone", "C_Map.GetBestMapForUnit", "C_Map.GetBestMapForUnit", "player")
    if mapID then
        try("zone", "C_Map.GetMapInfo", "C_Map.GetMapInfo", mapID)
    end

    -- 12: equipped gear (slots 1-19)
    local firstItemID
    for slot = 1, 19 do
        local itemID = try("gear", "GetInventoryItemID." .. slot, "GetInventoryItemID", "player", slot)
        if itemID then
            firstItemID = firstItemID or itemID
            try("gear", "GetInventoryItemLink." .. slot, "GetInventoryItemLink", "player", slot)
            try("gear", "GetCurrentItemLevel." .. slot, function()
                return C_Item.GetCurrentItemLevel(ItemLocation:CreateFromEquipmentSlot(slot))
            end)
        end
    end

    -- 13: bags (backpack 0-4, reagent bag 5)
    for bag = 0, 5 do
        local slots = try("bags", "GetContainerNumSlots." .. bag, "C_Container.GetContainerNumSlots", bag)
        try("bags", "GetContainerNumFreeSlots." .. bag, "C_Container.GetContainerNumFreeSlots", bag)
        try("bags", "GetBagName." .. bag, "C_Container.GetBagName", bag)
        if type(slots) == "number" and slots > 0 then
            try("bags", "GetContainerItemInfo." .. bag .. ".1", "C_Container.GetContainerItemInfo", bag, 1)
        end
    end

    -- 16: professions
    -- Returns up to 5 indices (prof1, prof2, archaeology, fishing, cooking),
    -- any of them nil.
    local profs = pack(try("professions", "GetProfessions", "GetProfessions"))
    for i = 1, profs.n do
        if profs[i] then
            try("professions", "GetProfessionInfo." .. i, "GetProfessionInfo", profs[i])
        end
    end

    -- 19-21: static item data, icon, tooltip (Hearthstone + first equipped item)
    for _, itemID in ipairs({ 6948, firstItemID }) do
        try("items", "GetItemInfo." .. itemID, "C_Item.GetItemInfo", itemID)
        try("items", "GetItemInfoInstant." .. itemID, "C_Item.GetItemInfoInstant", itemID)
        try("items", "GetItemIconByID." .. itemID, "C_Item.GetItemIconByID", itemID)
        try("items", "TooltipInfo.GetItemByID." .. itemID, "C_TooltipInfo.GetItemByID", itemID)
    end

    -- 45: addons
    local numAddOns = try("addons", "GetNumAddOns", "C_AddOns.GetNumAddOns")
    if type(numAddOns) == "number" then
        for i = 1, math.min(numAddOns, 5) do
            try("addons", "GetAddOnInfo." .. i, "C_AddOns.GetAddOnInfo", i)
            try("addons", "GetAddOnMetadata.Version." .. i, "C_AddOns.GetAddOnMetadata", i, "Version")
        end
    end

    -- 48: macros
    try("macros", "GetNumMacros", "GetNumMacros")
    try("macros", "GetMacroInfo.1", "GetMacroInfo", 1)

    -- secret-value restrictions in effect right now
    try("secrets", "HasSecretRestrictions", "C_Secrets.HasSecretRestrictions")
    try("secrets", "ShouldUnitIdentityBeSecret.target", "C_Secrets.ShouldUnitIdentityBeSecret", "target")

    -- 10, 17: answered by events (TIME_PLAYED_MSG, UPDATE_INSTANCE_INFO)
    try("played", "RequestTimePlayed", "RequestTimePlayed")
    try("lockouts", "RequestRaidInfo", "RequestRaidInfo")

    char.probedAt = now()
end

-- Event handlers (rows 10, 14, 15, 17, 22, 29-37, 39-43) --------------------

local handlers = {}

handlers.TIME_PLAYED_MSG = function(total, level)
    char.checks.played = char.checks.played or {}
    char.checks.played.TIME_PLAYED_MSG = { ok = true, values = { describe(total), describe(level) } }
end

handlers.UPDATE_INSTANCE_INFO = function()
    local n = try("lockouts", "GetNumSavedInstances", "GetNumSavedInstances")
    if type(n) == "number" and n > 0 then
        try("lockouts", "GetSavedInstanceInfo.1", "GetSavedInstanceInfo", 1)
    end
end

handlers.PLAYER_MONEY = function()
    try("money", "GetMoney.onEvent", "GetMoney")
end

handlers.LOOT_READY = function()
    local n = try("loot", "GetNumLootItems", "GetNumLootItems")
    if type(n) == "number" then
        for slot = 1, math.min(n, 3) do
            try("loot", "GetLootSlotLink." .. slot, "GetLootSlotLink", slot)
            try("loot", "GetLootSourceInfo." .. slot, "GetLootSourceInfo", slot)
        end
    end
    try("loot", "UnitName.target", "UnitName", "target")
    try("loot", "UnitGUID.target", "UnitGUID", "target")
end

handlers.PLAYER_DEAD = function()
    try("death", "HasRecapEvents", "C_DeathRecap.HasRecapEvents")
    try("death", "UnitName.target", "UnitName", "target")
end

handlers.MERCHANT_SHOW = function()
    try("merchant", "GetRepairAllCost", "GetRepairAllCost")
end

handlers.QUEST_TURNED_IN = function(questID)
    try("quests", "GetTitleForQuestID", "C_QuestLog.GetTitleForQuestID", questID)
end

handlers.BANKFRAME_OPENED = function()
    try("bank", "CanViewBank", "C_Bank.CanViewBank", Enum.BankType and Enum.BankType.Character)
    local tabs = try("bank", "FetchPurchasedBankTabIDs", "C_Bank.FetchPurchasedBankTabIDs",
        Enum.BankType and Enum.BankType.Character)
    local bags = { Enum.BagIndex and Enum.BagIndex.Bank or -1 }
    if type(tabs) == "table" then
        for _, id in ipairs(tabs) do
            table.insert(bags, id)
        end
    end
    for _, bag in ipairs(bags) do
        local slots = try("bank", "GetContainerNumSlots." .. bag, "C_Container.GetContainerNumSlots", bag)
        if type(slots) == "number" and slots > 0 then
            try("bank", "GetContainerItemInfo." .. bag .. ".1", "C_Container.GetContainerItemInfo", bag, 1)
        end
    end
end

handlers.MAIL_SHOW = function()
    try("mail", "CheckInbox", "CheckInbox")
end

handlers.MAIL_INBOX_UPDATE = function()
    local n = try("mail", "GetInboxNumItems", "GetInboxNumItems")
    if type(n) == "number" then
        for i = 1, math.min(n, 3) do
            try("mail", "GetInboxHeaderInfo." .. i, "GetInboxHeaderInfo", i)
            try("mail", "GetInboxInvoiceInfo." .. i, "GetInboxInvoiceInfo", i)
        end
    end
end

handlers.AUCTION_HOUSE_SHOW = function()
    try("ah", "IsThrottledMessageSystemReady", "C_AuctionHouse.IsThrottledMessageSystemReady")
    try("ah", "QueryOwnedAuctions", "C_AuctionHouse.QueryOwnedAuctions", {})
end

handlers.OWNED_AUCTIONS_UPDATED = function()
    local n = try("ah", "GetNumOwnedAuctions", "C_AuctionHouse.GetNumOwnedAuctions")
    if type(n) == "number" and n > 0 then
        try("ah", "GetOwnedAuctionInfo.1", "C_AuctionHouse.GetOwnedAuctionInfo", 1)
    end
end

local scanStarted
handlers.REPLICATE_ITEM_LIST_UPDATE = function()
    local n = try("ah", "GetNumReplicateItems", "C_AuctionHouse.GetNumReplicateItems")
    if scanStarted then
        char.checks.ah.scanSeconds = { ok = true, values = { now() - scanStarted } }
    end
    if type(n) == "number" then
        for i = 0, math.min(n, 3) - 1 do
            try("ah", "GetReplicateItemInfo." .. i, "C_AuctionHouse.GetReplicateItemInfo", i)
        end
    end
end

handlers.COMMODITY_SEARCH_RESULTS_UPDATED = function(itemID)
    try("ah", "GetNumCommoditySearchResults", "C_AuctionHouse.GetNumCommoditySearchResults", itemID)
    try("ah", "GetCommoditySearchResultInfo.1", "C_AuctionHouse.GetCommoditySearchResultInfo", itemID, 1)
end

handlers.ITEM_SEARCH_RESULTS_UPDATED = function(itemKey)
    try("ah", "GetNumItemSearchResults", "C_AuctionHouse.GetNumItemSearchResults", itemKey)
    try("ah", "GetItemSearchResultInfo.1", "C_AuctionHouse.GetItemSearchResultInfo", itemKey, 1)
end

handlers.TRADE_SKILL_SHOW = function()
    try("professions", "GetBaseProfessionInfo", "C_TradeSkillUI.GetBaseProfessionInfo")
end

handlers.PLAYER_LOGOUT = function()
    char.lastLogout = now()
    db.logouts = (db.logouts or 0) + 1
end

-- Every event the matrix's session/timeline rows depend on. Registration is
-- wrapped: registering an event the client doesn't know throws on Forever.
local EVENTS = {
    "PLAYER_LOGIN", "PLAYER_ENTERING_WORLD", "PLAYER_LOGOUT", "PLAYER_MONEY",
    "PLAYER_XP_UPDATE", "PLAYER_LEVEL_UP", "PLAYER_DEAD", "PLAYER_ALIVE",
    "ZONE_CHANGED_NEW_AREA", "ZONE_CHANGED", "CHAT_MSG_LOOT", "CHAT_MSG_MONEY",
    "LOOT_READY", "QUEST_ACCEPTED", "QUEST_TURNED_IN", "BAG_UPDATE_DELAYED",
    "PLAYER_EQUIPMENT_CHANGED", "BANKFRAME_OPENED", "MAIL_SHOW", "MAIL_INBOX_UPDATE",
    "MERCHANT_SHOW", "AUCTION_HOUSE_SHOW", "OWNED_AUCTIONS_UPDATED",
    "REPLICATE_ITEM_LIST_UPDATE", "COMMODITY_SEARCH_RESULTS_UPDATED",
    "ITEM_SEARCH_RESULTS_UPDATED", "TIME_PLAYED_MSG", "UPDATE_INSTANCE_INFO",
    "TRADE_SKILL_SHOW",
}

-- Setup ---------------------------------------------------------------------

local frame = CreateFrame("Frame")

local function onAddonLoaded()
    -- Records whether the client read our file back (the beta bug fixed in
    -- build 70009): a table here means it did.
    local loaded = type(ForeverBuddyProbeDB) == "table"
    db = loaded and ForeverBuddyProbeDB or {}
    ForeverBuddyProbeDB = db
    db.probeVersion = PROBE_VERSION
    db.loads = db.loads or {}
    table.insert(db.loads, { t = now(), fromDisk = loaded })
    while #db.loads > 20 do
        table.remove(db.loads, 1)
    end
    db.characters = db.characters or {}

    -- Bridge: what the data slot held at this load.
    db.dataStamps = db.dataStamps or {}
    local data = ForeverBuddyProbeData
    table.insert(db.dataStamps, { t = now(), stamp = type(data) == "table" and data.stamp or "<missing>" })
    while #db.dataStamps > 20 do
        table.remove(db.dataStamps, 1)
    end
    -- Bridge: a reload attempt still pending when we load again went through.
    local attempts = db.reloadAttempts
    local last = attempts and attempts[#attempts]
    if loaded and last and last.result == "pending" then
        last.result = "reloaded"
        last.reloadedAt = now()
    end
end

-- Bridge v0.4 spike: reload on a click --------------------------------------

-- Each attempt is saved before reloading (a reload writes SavedVariables),
-- so the next load can tell whether it happened.
local function attempt(route)
    db.reloadAttempts = db.reloadAttempts or {}
    table.insert(db.reloadAttempts, {
        route = route,
        t = now(),
        combat = InCombatLockdown and InCombatLockdown() or nil,
        result = "pending",
    })
end

-- Blocked calls are reported as events naming the addon and the function.
local function blocked(event, addon, fn)
    if addon ~= ADDON_NAME or not db or not db.reloadAttempts then
        return
    end
    local last = db.reloadAttempts[#db.reloadAttempts]
    if last and last.result == "pending" then
        last.result = event .. ":" .. tostring(fn)
    end
end

local reloadFrame
local function showReloadButtons()
    if reloadFrame then
        reloadFrame:SetShown(not reloadFrame:IsShown())
        return
    end
    if InCombatLockdown() then
        print("ForeverBuddy Probe: leave combat first (secure buttons can't be made in combat).")
        return
    end
    reloadFrame = CreateFrame("Frame", "ForeverBuddyProbeReload", UIParent, "BasicFrameTemplateWithInset")
    reloadFrame:SetSize(260, 120)
    reloadFrame:SetPoint("CENTER")
    reloadFrame:SetMovable(true)
    reloadFrame:EnableMouse(true)
    reloadFrame:RegisterForDrag("LeftButton")
    reloadFrame:SetScript("OnDragStart", reloadFrame.StartMoving)
    reloadFrame:SetScript("OnDragStop", reloadFrame.StopMovingOrSizing)
    reloadFrame.TitleText:SetText("Probe: reload routes")

    -- A: a plain addon button calling ReloadUI() on the click.
    local plain = CreateFrame("Button", nil, reloadFrame, "UIPanelButtonTemplate")
    plain:SetSize(220, 26)
    plain:SetPoint("TOP", 0, -32)
    plain:SetText("A: ReloadUI()")
    plain:SetScript("OnClick", function()
        attempt("plain")
        ReloadUI()
    end)

    -- B: a secure button running the /reload macro (the player's click).
    local secure = CreateFrame("Button", nil, reloadFrame, "SecureActionButtonTemplate,UIPanelButtonTemplate")
    secure:SetSize(220, 26)
    secure:SetPoint("TOP", plain, "BOTTOM", 0, -8)
    secure:SetText("B: secure /reload")
    -- Both edges are registered because the macro runs only on the edge the
    -- ActionButtonUseKeyDown setting picks, and registering the other one
    -- alone would never fire. PreClick sees both, so record only that edge:
    -- one click, one attempt.
    secure:RegisterForClicks("AnyUp", "AnyDown")
    secure:SetAttribute("type", "macro")
    secure:SetAttribute("macrotext", "/reload")
    secure:SetScript("PreClick", function(_, _, down)
        local onDown = GetCVarBool and GetCVarBool("ActionButtonUseKeyDown")
        if (down and true or false) == (onDown and true or false) then
            attempt("secure")
        end
    end)
end

local function onLogin()
    local name, realm = UnitFullName("player")
    local key = (name or "?") .. "-" .. (realm or GetRealmName() or "?")
    char = db.characters[key] or {}
    db.characters[key] = char
    char.checks, char.events = {}, {}
    char.unknownEvents = unknownEvents
    char.firstLogin = char.firstLogin or now()
    char.lastLogin = now()
    -- Give the client a moment to fill inventory and item caches.
    C_Timer.After(5, probeCharacter)
end

frame:SetScript("OnEvent", function(_, event, ...)
    if event == "ADDON_LOADED" then
        if ... == ADDON_NAME then
            onAddonLoaded()
        end
        return
    end
    if event == "ADDON_ACTION_BLOCKED" or event == "ADDON_ACTION_FORBIDDEN" then
        blocked(event, ...)
        return
    end
    if event == "PLAYER_LOGIN" then
        onLogin()
    end
    if not char then
        return
    end
    sample(event, ...)
    local handler = handlers[event]
    if handler then
        local ok, err = pcall(handler, ...)
        if not ok then
            char.handlerErrors = char.handlerErrors or {}
            char.handlerErrors[event] = tostring(err)
        end
    end
end)

frame:RegisterEvent("ADDON_LOADED")
for _, event in ipairs({ "ADDON_ACTION_BLOCKED", "ADDON_ACTION_FORBIDDEN" }) do
    if not pcall(frame.RegisterEvent, frame, event) then
        table.insert(unknownEvents, event)
    end
end
for _, event in ipairs(EVENTS) do
    if not pcall(frame.RegisterEvent, frame, event) then
        table.insert(unknownEvents, event)
    end
end

-- /fbprobe            summary
-- /fbprobe scan       full auction house scan (AH must be open; throttled)
-- /fbprobe search     one commodity search for Linen Cloth (AH must be open)
-- /fbprobe reload     the two reload buttons (Bridge v0.4 spike)
SLASH_FBPROBE1 = "/fbprobe"
SlashCmdList.FBPROBE = function(msg)
    msg = (msg or ""):lower()
    if msg == "reload" then
        showReloadButtons()
        return
    end
    if not char then
        print("ForeverBuddy Probe: not logged in yet.")
        return
    end
    if msg == "scan" then
        scanStarted = now()
        try("ah", "ReplicateItems", "C_AuctionHouse.ReplicateItems")
        print("ForeverBuddy Probe: full scan requested. Keep the auction house open until it finishes."
            .. " Full scans are throttled account-wide: Auctionator's own full scan won't work for about 15 minutes.")
        return
    elseif msg == "search" then
        try("ah", "SendSearchQuery.linen", function()
            return C_AuctionHouse.SendSearchQuery(C_AuctionHouse.MakeItemKey(2589), {}, false)
        end)
        print("ForeverBuddy Probe: searched Linen Cloth.")
        return
    end
    local ok, failed, secret = 0, 0, 0
    for _, section in pairs(char.checks) do
        for _, entry in pairs(section) do
            if entry.ok then
                ok = ok + 1
                if entry.secret then
                    secret = secret + 1
                end
            else
                failed = failed + 1
            end
        end
    end
    local seen = 0
    for _ in pairs(char.events) do
        seen = seen + 1
    end
    print(string.format(
        "ForeverBuddy Probe: %d checks ok (%d secret), %d failed, %d event types seen, %d unknown events.",
        ok, secret, failed, seen, #unknownEvents))
    print("Play a bit, open your bank, mailbox and the auction house, then log out. That writes the results.")
end
