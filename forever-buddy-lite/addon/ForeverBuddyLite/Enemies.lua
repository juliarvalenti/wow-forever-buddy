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
local read, say, num = ns.read, ns.say, ns.num

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

    -- Behind the bar: the class colour, darker (the missing health shows it, not black).
    local bg = b:CreateTexture(nil, "BACKGROUND")
    bg:SetAllPoints()
    bg:SetColorTexture(0.2, 0.2, 0.2, 0.9)
    b.back = bg

    local bar = CreateFrame("StatusBar", nil, b)
    bar:SetPoint("TOPLEFT", 1, -1)
    bar:SetPoint("BOTTOMRIGHT", -1, 1)
    bar:SetStatusBarTexture("Interface\\TargetingFrame\\UI-StatusBar")
    bar:SetMinMaxValues(0, 1)
    bar:SetValue(1)
    b.bar = bar

    local name = bar:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    name:SetPoint("LEFT", 4, 0)
    name:SetPoint("RIGHT", -40, 0)
    name:SetJustifyH("LEFT")
    name:SetWordWrap(false)
    b.label = name

    local hp = bar:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    hp:SetPoint("RIGHT", -4, 0)
    hp:SetJustifyH("RIGHT")
    b.hp = hp

    local hl = b:CreateTexture(nil, "HIGHLIGHT")
    hl:SetAllPoints()
    hl:SetColorTexture(1, 1, 1, 0.15)

    b:Hide()
    return b
end

-- How a row looks: full colour while someone in your group can see them, greyed when nobody can
-- (the bar keeps their last health). Range isn't shown: an addon can only check it for enemies on
-- your screen, which made it follow the camera more than distance.
-- Always the class colour (the missing health a darker shade of it). Only the health number says
-- how fresh it is: white while someone in your group sees them, grey once nobody has for STALE
-- seconds. (Dimming whole rows followed your camera: nameplates exist only on screen.)
local STALE = 10
local function setBarLook(row)
    local e = row.enemy
    local r, g, bl = classColor(e and e.class)
    row.bar:SetStatusBarColor(r, g, bl, 1)
    row.back:SetColorTexture(r * 0.35, g * 0.35, bl * 0.35, 0.9)
    row.label:SetTextColor(1, 1, 1)
    local fresh = row.lastSeen and (GetTime() - row.lastSeen) <= STALE
    if fresh then
        row.hp:SetTextColor(1, 1, 1)
    else
        row.hp:SetTextColor(0.55, 0.55, 0.55)
    end
end

-- The health number: a percentage when the addon may read health; otherwise the raw value goes
-- straight to the text (the client draws a value it hides from the addon).
local function setHealthText(row, unit)
    local cur, max = UnitHealth(unit), UnitHealthMax(unit)
    local secret = rawget(_G, "issecretvalue")
    local readable = type(cur) == "number" and type(max) == "number" and not (secret and (secret(cur) or secret(max)))
    if readable and max > 0 then
        row.hp:SetText(math.floor(cur / max * 100 + 0.5) .. "%")
        return
    end
    -- Hidden: the client's own percentage (0-100 with its ScaleTo100 curve), formatted by the client.
    local curves = rawget(_G, "CurveConstants")
    local percent = rawget(_G, "UnitHealthPercent")
    if percent and curves and curves.ScaleTo100 then
        local ok, pct = pcall(percent, unit, false, curves.ScaleTo100)
        if ok and pct ~= nil and pcall(row.hp.SetFormattedText, row.hp, "%.0f%%", pct) then
            return
        end
    end
    -- Else the raw health, shortened by the client where it can (2373 -> 2.4K).
    local abbreviate = rawget(_G, "AbbreviateNumbers")
    if not (abbreviate and pcall(function()
        row.hp:SetText(abbreviate(cur))
    end)) then
        pcall(row.hp.SetText, row.hp, cur)
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
                row.seenOnce, row.lastSeen = nil, nil
                row.bar:SetMinMaxValues(0, 1)
                row.bar:SetValue(1)
                row.hp:SetText("")
            end
            row.enemy = e
            row:SetAttribute("macrotext", "/targetexact " .. e.name)
            row.label:SetText((e.healer and "|cff40ff40+|r " or "") .. e.short)
            setBarLook(row)
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
        if a[i].name ~= b[i].name or a[i].healer ~= b[i].healer then
            return false
        end
    end
    return true
end

local CLASS_ORDER = { WARRIOR = 1, PALADIN = 2, ROGUE = 3, HUNTER = 4, DRUID = 5, SHAMAN = 6, PRIEST = 7, MAGE = 8, WARLOCK = 9 }

-- Healers. A healing spec on the scoreboard decides it when Forever fills one in; otherwise the
-- numbers: a class that can heal, healing at least HEAL_OVER_DAMAGE times its damage, and at least
-- HEAL_SHARE of the most healing anyone in the battleground has done (relative, so it works a minute
-- in and an hour in, in any battleground size).
local CAN_HEAL = { PRIEST = true, DRUID = true, SHAMAN = true, PALADIN = true }
local HEALING_SPECS = { Holy = true, Restoration = true, Discipline = true }
local HEAL_OVER_DAMAGE, HEAL_SHARE = 1.5, 0.3

local function isHealer(e, maxHealing)
    if type(e.spec) == "string" and e.spec ~= "" then
        return HEALING_SPECS[e.spec] or false
    end
    local heal, dmg = tonumber(e.healing) or 0, tonumber(e.damage) or 0
    return CAN_HEAL[e.class] == true and heal > 0 and heal >= HEAL_OVER_DAMAGE * dmg and heal >= HEAL_SHARE * maxHealing
end

local function readScoreboard()
    if testing then
        return
    end
    local n = num(read("GetNumBattlefieldScores")) or 0
    local f = ns.resolve("C_PvP.GetScoreInfo")
    if not f or n == 0 then
        return
    end
    local mine = myFaction()
    local secret = rawget(_G, "issecretvalue")
    local function plain(v)
        if v == nil or (secret and secret(v)) then
            return nil
        end
        return v
    end
    local list = {}
    local maxHealing = 0
    for i = 1, n do
        local ok, s = pcall(f, i)
        if ok and type(s) == "table" then
            local name, faction = plain(s.name), plain(s.faction)
            local healing = tonumber(plain(s.healingDone)) or 0
            maxHealing = math.max(maxHealing, healing) -- both teams: the match's scale
            if type(name) == "string" and name ~= "" and faction ~= nil and faction ~= mine then
                list[#list + 1] = { name = name, short = name:match("^[^-]+") or name, guid = plain(s.guid), class = plain(s.classToken),
                    healing = healing, damage = tonumber(plain(s.damageDone)) or 0, spec = plain(s.talentSpec) }
            end
        end
    end
    for _, e in ipairs(list) do
        e.healer = isHealer(e, maxHealing)
    end
    table.sort(list, function(a, b)
        if a.healer ~= b.healer then
            return a.healer -- healers first
        end
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
            if row then
                if not seen[row] then
                    -- Straight into the bar: a value hidden from the addon still draws.
                    pcall(row.bar.SetMinMaxValues, row.bar, 0, UnitHealthMax(unit))
                    pcall(row.bar.SetValue, row.bar, UnitHealth(unit))
                    pcall(setHealthText, row, unit)
                    row.seenOnce, row.lastSeen = true, GetTime()
                    seen[row] = true
                end
            end
        end
    end
    for i = 1, #roster do
        local row = rows[i]
        if row and row.enemy then
            setBarLook(row)
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
            -- Also read what's there now, in case the update event doesn't come.
            readScoreboard()
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

-- /fbl bg debug: what the scoreboard actually gives an addon here.
local function debugScoreboard()
    local secret = rawget(_G, "issecretvalue")
    local function show(v)
        if v == nil then
            return "nil"
        elseif secret and secret(v) then
            return "<hidden>"
        end
        return tostring(v)
    end
    local _, kind = read("IsInInstance")
    say(string.format("instance %s; GetNumBattlefieldScores %s; my side %s (GetBattlefieldArenaFaction %s, %s)",
        show(kind), show(read("GetNumBattlefieldScores")), show(myFaction()), show(read("GetBattlefieldArenaFaction")),
        show(read("UnitFactionGroup", "player"))))
    local curves = rawget(_G, "CurveConstants")
    say("health %: UnitHealthPercent " .. (rawget(_G, "UnitHealthPercent") and "exists" or "MISSING") .. ", ScaleTo100 "
        .. ((curves and curves.ScaleTo100) and "exists" or "MISSING"))
    say("C_PvP.GetScoreInfo " .. (ns.resolve("C_PvP.GetScoreInfo") and "exists" or "MISSING")
        .. "; RequestBattlefieldScoreData " .. (ns.resolve("RequestBattlefieldScoreData") and "exists" or "MISSING"))
    local f = ns.resolve("C_PvP.GetScoreInfo")
    local sides = {}
    for i = 1, math.min(num(read("GetNumBattlefieldScores")) or 0, 80) do
        local ok, s = pcall(f, i)
        if ok and type(s) == "table" then
            local side = show(s.faction)
            sides[side] = (sides[side] or 0) + 1
            if i <= 3 then
                say(string.format("  #%d name %s, faction %s, class %s, guid %s, spec %s, role %s, healing %s, damage %s", i,
                    show(s.name), side, show(s.classToken), s.guid and "yes" or "no", show(s.talentSpec), show(s.roleAssigned),
                    show(s.healingDone), show(s.damageDone)))
            end
        elseif i <= 3 then
            say(string.format("  #%d: %s", i, ok and type(s) or ("error " .. tostring(s))))
        end
    end
    local parts = {}
    for k, v in pairs(sides) do
        parts[#parts + 1] = "faction " .. k .. ": " .. v
    end
    say("by side: " .. (#parts > 0 and table.concat(parts, ", ") or "none") .. "; enemies listed now: " .. #roster)
end

ns.command({ name = "bg", args = "[on, off, test, scale <n>, debug]",
    help = "battleground enemies with health bars; click a row to target (shows by itself in battlegrounds)",
    fn = function(rest)
        if InCombatLockdown() then
            say("the enemies list can't change in combat.")
            return
        end
        if rest == "debug" then
            debugScoreboard()
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
