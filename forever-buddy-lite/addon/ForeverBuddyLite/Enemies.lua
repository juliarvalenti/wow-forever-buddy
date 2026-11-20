-- Battleground enemies (/fbl bg): the other team as a list, class-coloured, with a health bar each,
-- and a click on a row targets that player (/targetexact, a secure button, so it works in combat).
--
-- Who's there comes from the battleground scoreboard (C_PvP.GetScoreInfo: name, GUID, class,
-- faction), not the combat log, which Forever closes to addons. Health comes from any unit the
-- client exposes for that player right now: your target, focus, mouseover, nameplates, and your
-- group's targets, matched by GUID (or name). Health values go straight into the bar
-- (StatusBar:SetValue), so they show even when the client hides the numbers from addons in combat.
-- Players nobody in your group can see keep their last bar, greyed.
--
-- The rows are secure buttons, and the game freezes those in combat: the list itself (who's in it,
-- the order, which name a row targets) changes only out of combat. Health updates any time.
--
-- ForeverBuddyLiteDB.bg = { point, off, scale }

local _, ns = ...
local read, say = ns.read, ns.say

local MAX_ROWS = 40
local ROW_W, ROW_H = 170, 18
local SCORE_EVERY = 5 -- seconds between scoreboard requests
local HEALTH_EVERY = 0.2

local frame, title
local rows = {}
local roster = {} -- { name, short, guid, class }
local byGuid, byName = {}, {}
local pending = false -- roster changed in combat: lay it out when combat ends
local testing = false

local function store()
    local db = ns.db
    if not db then
        return {}
    end
    db.bg = type(db.bg) == "table" and db.bg or {}
    return db.bg
end

local function inBattleground()
    local _, kind = read("IsInInstance")
    return kind == "pvp"
end

-- 0 Horde, 1 Alliance: the side the player is on in this battleground.
local function myFaction()
    local f = read("GetBattlefieldArenaFaction")
    if type(f) == "number" then
        return f
    end
    return read("UnitFactionGroup", "player") == "Horde" and 0 or 1
end

local function classColor(token)
    local c = token and (rawget(_G, "RAID_CLASS_COLORS") or {})[token]
    if c then
        return c.r, c.g, c.b
    end
    return 0.6, 0.6, 0.6
end

-- Rows -----------------------------------------------------------------------------------------

local function makeRow(i)
    local b = CreateFrame("Button", "FBLEnemyRow" .. i, frame, "SecureActionButtonTemplate")
    b:SetSize(ROW_W, ROW_H)
    b:SetPoint("TOPLEFT", 6, -22 - (i - 1) * (ROW_H + 2))
    b:RegisterForClicks("AnyUp", "AnyDown")
    b:SetAttribute("type", "macro")

    local bg = b:CreateTexture(nil, "BACKGROUND")
    bg:SetAllPoints()
    bg:SetColorTexture(0, 0, 0, 0.6)

    local bar = CreateFrame("StatusBar", nil, b)
    bar:SetPoint("TOPLEFT", 1, -1)
    bar:SetPoint("BOTTOMRIGHT", -1, 1)
    bar:SetStatusBarTexture("Interface\\TargetingFrame\\UI-StatusBar")
    bar:SetMinMaxValues(0, 1)
    bar:SetValue(1)
    b.bar = bar

    local name = bar:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    name:SetPoint("LEFT", 4, 0)
    name:SetPoint("RIGHT", -4, 0)
    name:SetJustifyH("LEFT")
    name:SetWordWrap(false)
    b.label = name

    local hl = b:CreateTexture(nil, "HIGHLIGHT")
    hl:SetAllPoints()
    hl:SetColorTexture(1, 1, 1, 0.15)

    b:Hide()
    return b
end

local function setBarLook(row, seen)
    local e = row.enemy
    local r, g, bl = classColor(e and e.class)
    if seen then
        row.bar:SetStatusBarColor(r, g, bl, 1)
        row.label:SetTextColor(1, 1, 1)
    else
        row.bar:SetStatusBarColor(r * 0.35, g * 0.35, bl * 0.35, 1)
        row.label:SetTextColor(0.75, 0.75, 0.75)
    end
end

-- Lay the roster out on the rows. Secure buttons: out of combat only.
local function layout()
    if not frame then
        return
    end
    if InCombatLockdown() then
        pending = true
        return
    end
    pending = false
    for i = 1, MAX_ROWS do
        local e, row = roster[i], rows[i]
        if e then
            if row.enemy ~= e then
                -- Someone else on this row now: forget the last player's health.
                row.seenOnce = nil
                row.bar:SetMinMaxValues(0, 1)
                row.bar:SetValue(1)
            end
            row.enemy = e
            row:SetAttribute("macrotext", "/targetexact " .. e.name)
            row.label:SetText(e.short)
            setBarLook(row, false)
            row:Show()
        else
            row.enemy, row.seenOnce = nil, nil
            row:Hide()
        end
    end
    frame:SetHeight(28 + math.max(1, #roster) * (ROW_H + 2))
    title:SetText(#roster > 0 and ("Enemies: " .. #roster) or "Enemies: waiting for the scoreboard")
end

-- The scoreboard --------------------------------------------------------------------------------

local function sameRoster(a, b)
    if #a ~= #b then
        return false
    end
    for i = 1, #a do
        if a[i].name ~= b[i].name then
            return false
        end
    end
    return true
end

local CLASS_ORDER = { WARRIOR = 1, PALADIN = 2, ROGUE = 3, HUNTER = 4, DRUID = 5, SHAMAN = 6, PRIEST = 7, MAGE = 8, WARLOCK = 9 }

local function readScoreboard()
    if testing then
        return
    end
    local n = read("GetNumBattlefieldScores") or 0
    local mine = myFaction()
    local list = {}
    for i = 1, n do
        local s = read("C_PvP.GetScoreInfo", i)
        if type(s) == "table" and type(s.name) == "string" and s.faction ~= nil and s.faction ~= mine then
            list[#list + 1] = { name = s.name, short = s.name:match("^[^-]+") or s.name, guid = s.guid, class = s.classToken }
        end
    end
    table.sort(list, function(a, b)
        local ca, cb = CLASS_ORDER[a.class] or 99, CLASS_ORDER[b.class] or 99
        if ca ~= cb then
            return ca < cb
        end
        return a.name < b.name
    end)
    if sameRoster(list, roster) then
        return
    end
    roster = list
    byGuid, byName = {}, {}
    for _, e in ipairs(roster) do
        if e.guid then
            byGuid[e.guid] = e
        end
        byName[e.name], byName[e.short] = e, e
    end
    layout()
end

-- Health ------------------------------------------------------------------------------------------

local UNITS = { "target", "focus", "mouseover" }
for i = 1, 40 do
    UNITS[#UNITS + 1] = "nameplate" .. i
end
for i = 1, 4 do
    UNITS[#UNITS + 1] = "party" .. i .. "target"
end
for i = 1, 40 do
    UNITS[#UNITS + 1] = "raid" .. i .. "target"
end

-- Which enemy a unit is: by GUID, else by name. Values the client hides can't be matched.
local function enemyOf(unit)
    local ok, guid = pcall(UnitGUID, unit)
    local secret = rawget(_G, "issecretvalue")
    if ok and guid and not (secret and secret(guid)) and byGuid[guid] then
        return byGuid[guid]
    end
    local okN, name = pcall(GetUnitName, unit, true)
    if okN and name and not (secret and secret(name)) then
        return byName[name] or byName[(name:match("^[^-]+"))]
    end
end

local rowOf = {}
local function updateHealth()
    if not frame or not frame:IsShown() or #roster == 0 then
        return
    end
    wipe(rowOf)
    for i = 1, #roster do
        if rows[i] and rows[i].enemy then
            rowOf[rows[i].enemy] = rows[i]
        end
    end
    local seen = {}
    for _, unit in ipairs(UNITS) do
        if UnitExists(unit) then
            local e = enemyOf(unit)
            local row = e and rowOf[e]
            if row and not seen[row] then
                seen[row] = true
                -- Straight into the bar: a value hidden from the addon still draws.
                pcall(row.bar.SetMinMaxValues, row.bar, 0, UnitHealthMax(unit))
                pcall(row.bar.SetValue, row.bar, UnitHealth(unit))
                row.seenOnce = true
            end
        end
    end
    for i = 1, #roster do
        local row = rows[i]
        if row and row.enemy then
            setBarLook(row, seen[row])
        end
    end
end

-- The window --------------------------------------------------------------------------------------

local function build()
    local s = store()
    frame = CreateFrame("Frame", "ForeverBuddyLiteEnemies", UIParent, "BackdropTemplate")
    frame:SetSize(ROW_W + 12, 60)
    frame:SetFrameStrata("MEDIUM")
    frame:SetClampedToScreen(true)
    frame:SetScale(tonumber(s.scale) or 1)
    if frame.SetBackdrop then
        frame:SetBackdrop({ bgFile = "Interface\\Buttons\\WHITE8x8", edgeFile = "Interface\\Buttons\\WHITE8x8", edgeSize = 1 })
        frame:SetBackdropColor(0, 0, 0, 0.45)
        frame:SetBackdropBorderColor(0, 0, 0, 1)
    end
    local point = s.point
    if type(point) == "table" and point[1] then
        frame:SetPoint(point[1], UIParent, point[2], point[3], point[4])
    else
        frame:SetPoint("LEFT", 60, 60)
    end
    frame:SetMovable(true)
    frame:EnableMouse(true)
    frame:RegisterForDrag("LeftButton")
    frame:SetScript("OnDragStart", function(self)
        if not InCombatLockdown() then
            self:StartMoving()
        end
    end)
    frame:SetScript("OnDragStop", function(self)
        self:StopMovingOrSizing()
        local p, _, rp, x, y = self:GetPoint()
        store().point = { p, rp, x, y }
    end)

    title = frame:CreateFontString(nil, "OVERLAY", "GameFontNormalSmall")
    title:SetPoint("TOPLEFT", 8, -6)
    title:SetText("Enemies")

    for i = 1, MAX_ROWS do
        rows[i] = makeRow(i)
    end
    frame:Hide()

    C_Timer.NewTicker(HEALTH_EVERY, updateHealth)
    C_Timer.NewTicker(SCORE_EVERY, function()
        if frame:IsShown() and not testing then
            pcall(RequestBattlefieldScoreData)
        end
    end)
end

-- Show in battlegrounds (unless turned off), hide elsewhere. Out of combat only (secure rows).
local function update()
    if not frame then
        build()
    end
    if InCombatLockdown() then
        pending = true
        return
    end
    local want = testing or (inBattleground() and not store().off)
    if want and not frame:IsShown() then
        roster = testing and roster or {}
        frame:Show()
        layout()
        if not testing then
            pcall(RequestBattlefieldScoreData)
        end
    elseif not want and frame:IsShown() then
        frame:Hide()
    end
end

ns.on("PLAYER_ENTERING_WORLD", update)
ns.on("ZONE_CHANGED_NEW_AREA", update)
ns.on("UPDATE_BATTLEFIELD_SCORE", readScoreboard)
ns.on("PLAYER_REGEN_ENABLED", function()
    if pending then
        update()
        layout()
    end
end)

-- A fake team to place the window and try a click outside a battleground (/fbl bg test).
local function test(on)
    testing = on
    if on then
        local classes = { "WARRIOR", "PALADIN", "ROGUE", "HUNTER", "DRUID", "SHAMAN", "PRIEST", "MAGE", "WARLOCK", "PRIEST" }
        roster = {}
        local target = read("UnitName", "target")
        for i, c in ipairs(classes) do
            local name = (i == 1 and type(target) == "string") and target or ("Enemy" .. i)
            roster[i] = { name = name, short = name, class = c }
        end
        byGuid, byName = {}, {}
        for _, e in ipairs(roster) do
            byName[e.name] = e
        end
    end
    update()
    layout()
end

ns.command({ name = "bg", args = "[on, off, test, scale <n>]",
    help = "battleground enemies with health bars; click a row to target (shows by itself in battlegrounds)",
    fn = function(rest)
        if InCombatLockdown() then
            say("the enemies list can't change in combat.")
            return
        end
        local scale = tonumber(rest:match("^scale%s+([%d.]+)$"))
        if scale then
            store().scale = math.max(0.5, math.min(2, scale))
            if frame then
                frame:SetScale(store().scale)
            end
            say("enemies list scale " .. store().scale .. ".")
        elseif rest == "off" then
            store().off = true
            testing = false
            update()
            say("enemies list off in battlegrounds.")
        elseif rest == "on" then
            store().off = nil
            update()
            say("enemies list on: it shows by itself in battlegrounds.")
        elseif rest == "test" then
            test(not testing)
            say(testing and "test team shown (row 1 is your target, if you have one). /fbl bg test again to hide."
                or "test team hidden.")
        else
            say("shows by itself in battlegrounds. /fbl bg test shows a fake team to place it; /fbl bg off, on, scale 1.2.")
        end
    end })
