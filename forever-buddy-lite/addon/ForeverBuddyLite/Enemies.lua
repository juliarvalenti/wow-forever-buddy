-- Battleground enemies (/fbl bg): the other team as a list, one row each, click a row to target
-- that player (/targetexact, a secure button, so it works in combat). Each row, left to right:
-- a healer mark (+), the name, eyes for how many of your team target them (3, then +N), and health
-- as a number; behind it the health bar in the class colour over a darker shade of it, and a thin,
-- muted power bar (mana, rage, energy) under it. Your target gets a gold border.
--
-- Who's there comes from the battleground scoreboard (C_PvP.GetScoreInfo), not the combat log,
-- which Forever closes to addons. Everything per player comes from whatever unit the client exposes
-- for them right now: your target, focus, mouseover, nameplates, and your group's targets, matched
-- by GUID or name. Health and power go straight into the bars (and the number), so they show even
-- when the client hides the values from addons in combat.
--
-- Range is from you: a spell range check (C_Spell.IsSpellInRange) through any of those units, so it
-- covers enemies your group targets too, not only ones on your screen. In range: full brightness;
-- out of range: faded; nobody can see them (range unknown): slightly faded with a grey health
-- number. If the client hides the range answer, the row fades with SetAlphaFromBoolean, which takes
-- a hidden value without the addon reading it. The spell defaults by class; /fbl bg range <spell>
-- picks another (a melee ability for melee range).
--
-- The rows are secure buttons, and the game freezes those in combat: the list itself (who's in it,
-- the order, which name a row targets) changes only out of combat. Everything drawn on a row
-- updates any time.
--
-- ForeverBuddyLiteDB.bg = { point, off, scale, rangeSpell }

local _, ns = ...
local read, say, num = ns.read, ns.say, ns.num

local MAX_ROWS = 40
local ROW_W, ROW_H = 230, 22
local POWER_H = 3
local HEALER_W, EYES_W, HP_W = 12, 56, 36
local MAX_EYES = 3
local SCORE_EVERY = 5 -- seconds between scoreboard requests
local UPDATE_EVERY = 0.2
local STALE = 10 -- seconds unseen before the health number turns grey
local ALPHA_IN, ALPHA_OUT, ALPHA_UNKNOWN = 1, 0.4, 0.7
local EYE_ICON = "Interface\\Icons\\Spell_Holy_MindVision"

-- Muted power colours (mana, rage, energy, focus): quiet next to the class colours.
local POWER_COLORS = { [0] = { 0.24, 0.33, 0.47 }, [1] = { 0.43, 0.21, 0.2 }, [2] = { 0.45, 0.33, 0.2 }, [3] = { 0.48, 0.42, 0.23 } }

-- A 30-yard-ish spell each class has from early on, for the range check; first one the client
-- answers for wins. Override: /fbl bg range <spell>.
local RANGE_SPELLS = {
    ROGUE = { "Throw", "Shoot", "Shoot Gun", "Shoot Bow", "Shoot Crossbow" },
    WARRIOR = { "Throw", "Shoot", "Shoot Gun", "Shoot Bow", "Shoot Crossbow", "Charge" },
    HUNTER = { "Auto Shot", "Arcane Shot", "Serpent Sting" },
    MAGE = { "Fireball", "Frostbolt", "Arcane Missiles" },
    PRIEST = { "Smite", "Shadow Word: Pain", "Mind Blast" },
    WARLOCK = { "Shadow Bolt", "Corruption", "Immolate" },
    DRUID = { "Wrath", "Moonfire", "Faerie Fire" },
    SHAMAN = { "Lightning Bolt", "Earth Shock" },
    PALADIN = { "Judgement", "Hammer of Wrath", "Exorcism" },
}

local frame, title
local rows = {}
local roster = {} -- { name, short, guid, class, healer, ... }
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

local secretFn = rawget(_G, "issecretvalue")
local function isSecret(v)
    return secretFn ~= nil and v ~= nil and secretFn(v) and true or false
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

-- Four edges just outside the row (an outline: nothing inside moves).
local function border(parent, r, g, b, a, size)
    local function edge(p1, x1, y1, p2, x2, y2, w, h)
        local t = parent:CreateTexture(nil, "OVERLAY", nil, 7)
        t:SetColorTexture(r, g, b, a)
        t:SetPoint(p1, parent, p1, x1, y1)
        t:SetPoint(p2, parent, p2, x2, y2)
        if w then
            t:SetWidth(w)
        else
            t:SetHeight(h)
        end
        t:Hide()
        return t
    end
    return {
        edge("TOPLEFT", -size, size, "TOPRIGHT", size, size, nil, size),
        edge("BOTTOMLEFT", -size, -size, "BOTTOMRIGHT", size, -size, nil, size),
        edge("TOPLEFT", -size, size, "BOTTOMLEFT", -size, -size, size, nil),
        edge("TOPRIGHT", size, size, "BOTTOMRIGHT", size, -size, size, nil),
    }
end

local function showEdges(edges, on)
    for _, t in ipairs(edges) do
        t:SetShown(on)
    end
end

local function makeRow(i)
    local b = CreateFrame("Button", "FBLEnemyRow" .. i, frame, "SecureActionButtonTemplate")
    b:SetSize(ROW_W, ROW_H)
    b:SetPoint("TOPLEFT", 6, -22 - (i - 1) * (ROW_H + 3))
    b:RegisterForClicks("AnyUp", "AnyDown")
    b:SetAttribute("type", "macro")

    -- Black frame; inside it the class colour, darker, behind the health bar (missing health shows it).
    local edge = b:CreateTexture(nil, "BACKGROUND", nil, -8)
    edge:SetAllPoints()
    edge:SetColorTexture(0, 0, 0, 1)
    local back = b:CreateTexture(nil, "BACKGROUND", nil, -7)
    back:SetPoint("TOPLEFT", 1, -1)
    back:SetPoint("BOTTOMRIGHT", -1, POWER_H + 2)
    back:SetColorTexture(0.2, 0.2, 0.2, 1)
    b.back = back

    local bar = CreateFrame("StatusBar", nil, b)
    bar:SetPoint("TOPLEFT", 1, -1)
    bar:SetPoint("BOTTOMRIGHT", -1, POWER_H + 2)
    bar:SetStatusBarTexture("Interface\\TargetingFrame\\UI-StatusBar")
    bar:SetMinMaxValues(0, 1)
    bar:SetValue(1)
    b.bar = bar

    -- The thin power bar along the bottom, under a 1px black line.
    local powerBack = b:CreateTexture(nil, "BACKGROUND", nil, -7)
    powerBack:SetPoint("BOTTOMLEFT", 1, 1)
    powerBack:SetPoint("BOTTOMRIGHT", -1, 1)
    powerBack:SetHeight(POWER_H)
    powerBack:SetColorTexture(0.08, 0.08, 0.08, 1)
    local power = CreateFrame("StatusBar", nil, b)
    power:SetPoint("BOTTOMLEFT", 1, 1)
    power:SetPoint("BOTTOMRIGHT", -1, 1)
    power:SetHeight(POWER_H)
    power:SetStatusBarTexture("Interface\\Buttons\\WHITE8x8")
    power:SetMinMaxValues(0, 1)
    power:SetValue(0)
    power:SetAlpha(0.9)
    b.power = power

    -- Fixed columns, so everything lines up row to row: healer | name | eyes | health.
    local text = CreateFrame("Frame", nil, bar)
    text:SetAllPoints()
    text:SetFrameLevel(bar:GetFrameLevel() + 2)

    local healer = text:CreateFontString(nil, "OVERLAY", "GameFontNormal")
    healer:SetPoint("LEFT", 2, 0)
    healer:SetWidth(HEALER_W - 2)
    healer:SetJustifyH("CENTER")
    healer:SetTextColor(0.3, 1, 0.3)
    healer:SetShadowOffset(1, -1)
    b.healer = healer

    local hp = text:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    hp:SetPoint("RIGHT", -4, 0)
    hp:SetWidth(HP_W)
    hp:SetJustifyH("RIGHT")
    hp:SetShadowOffset(1, -1)
    b.hp = hp

    b.eyes = {}
    for k = 1, MAX_EYES do
        local eye = text:CreateTexture(nil, "OVERLAY")
        eye:SetSize(11, 11)
        eye:SetTexture(EYE_ICON)
        eye:SetTexCoord(0.08, 0.92, 0.08, 0.92)
        eye:SetPoint("RIGHT", hp, "LEFT", -2 - (MAX_EYES - k) * 12 - 14, 0)
        eye:Hide()
        b.eyes[k] = eye
    end
    local more = text:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    more:SetPoint("RIGHT", hp, "LEFT", -2, 0)
    more:SetWidth(14)
    more:SetJustifyH("RIGHT")
    more:SetTextColor(1, 0.35, 0.3)
    more:SetShadowOffset(1, -1)
    b.more = more

    local name = text:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    name:SetPoint("LEFT", HEALER_W + 2, 0)
    name:SetPoint("RIGHT", hp, "LEFT", -EYES_W, 0)
    name:SetJustifyH("LEFT")
    name:SetWordWrap(false)
    name:SetShadowOffset(1, -1)
    b.label = name

    b.targetEdges = border(b, 1, 0.82, 0, 1, 2)

    local hl = b:CreateTexture(nil, "HIGHLIGHT")
    hl:SetAllPoints()
    hl:SetColorTexture(1, 1, 1, 0.12)

    b:Hide()
    return b
end

-- Colours from the class; the health number grey once nobody has seen them for STALE seconds.
local function paint(row)
    local e = row.enemy
    local r, g, bl = classColor(e and e.class)
    row.bar:SetStatusBarColor(r, g, bl, 1)
    row.back:SetColorTexture(r * 0.35, g * 0.35, bl * 0.35, 1)
    local fresh = row.lastSeen and (GetTime() - row.lastSeen) <= STALE
    if fresh then
        row.hp:SetTextColor(1, 1, 1)
    else
        row.hp:SetTextColor(0.55, 0.55, 0.55)
    end
end

-- The health number: a percentage when the addon may read health; when hidden, the client's own
-- percentage formatted by the client, else the raw value (shortened where the client can).
local function setHealthText(row, unit)
    local cur, max = UnitHealth(unit), UnitHealthMax(unit)
    if type(cur) == "number" and type(max) == "number" and not isSecret(cur) and not isSecret(max) then
        if max > 0 then
            row.hp:SetText(math.floor(cur / max * 100 + 0.5) .. "%")
        end
        return
    end
    local curves = rawget(_G, "CurveConstants")
    local percent = rawget(_G, "UnitHealthPercent")
    if percent and curves and curves.ScaleTo100 then
        local ok, pct = pcall(percent, unit, false, curves.ScaleTo100)
        if ok and pct ~= nil and pcall(row.hp.SetFormattedText, row.hp, "%.0f%%", pct) then
            return
        end
    end
    local abbreviate = rawget(_G, "AbbreviateNumbers")
    if not (abbreviate and pcall(function()
        row.hp:SetText(abbreviate(cur))
    end)) then
        pcall(row.hp.SetText, row.hp, cur)
    end
end

local function setPower(row, unit)
    local ok, kind = pcall(UnitPowerType, unit)
    local c = POWER_COLORS[(ok and not isSecret(kind) and kind) or 0] or POWER_COLORS[0]
    row.power:SetStatusBarColor(c[1], c[2], c[3], 1)
    pcall(row.power.SetMinMaxValues, row.power, 0, UnitPowerMax(unit))
    pcall(row.power.SetValue, row.power, UnitPower(unit))
end

local function setEyes(row, n)
    for k = 1, MAX_EYES do
        row.eyes[k]:SetShown(n >= (MAX_EYES - k + 1) and true or false)
    end
    row.more:SetText(n > MAX_EYES and ("+" .. (n - MAX_EYES)) or "")
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
                -- Someone else on this row now: forget the last player's state.
                row.lastSeen = nil
                row.bar:SetMinMaxValues(0, 1)
                row.bar:SetValue(1)
                row.power:SetValue(0)
                row.hp:SetText("")
                setEyes(row, 0)
            end
            row.enemy = e
            row:SetAttribute("macrotext", "/targetexact " .. e.name)
            row.label:SetText(e.short)
            row.healer:SetText(e.healer and "+" or "")
            paint(row)
            row:Show()
        else
            row.enemy = nil
            row:Hide()
        end
    end
    frame:SetHeight(28 + math.max(1, #roster) * (ROW_H + 3))
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

local function plain(v)
    if v == nil or isSecret(v) then
        return nil
    end
    return v
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

-- Units, range, targeting -------------------------------------------------------------------------

-- Your own view first (target, focus, mouseover, nameplates), then your group's targets.
local OWN = { "target", "focus", "mouseover" }
for i = 1, 40 do
    OWN[#OWN + 1] = "nameplate" .. i
end
local GROUP = {}
for i = 1, 4 do
    GROUP[#GROUP + 1] = "party" .. i .. "target"
end
for i = 1, 40 do
    GROUP[#GROUP + 1] = "raid" .. i .. "target"
end

-- Which enemy a unit is: by GUID, else by name. Values the client hides can't be matched.
local function enemyOf(unit)
    local ok, guid = pcall(UnitGUID, unit)
    if ok and guid and not isSecret(guid) and byGuid[guid] then
        return byGuid[guid]
    end
    local okN, name = pcall(GetUnitName, unit, true)
    if okN and name and not isSecret(name) then
        return byName[name] or byName[(name:match("^[^-]+"))]
    end
end

-- The spell to check range with: yours if set, else the first of your class's list the client knows.
local rangeSpell, rangeSpellFor
local function currentRangeSpell()
    local chosen = store().rangeSpell
    if chosen then
        return chosen
    end
    local _, class = read("UnitClass", "player")
    if rangeSpellFor == class and rangeSpell then
        return rangeSpell
    end
    rangeSpellFor = class
    for _, s in ipairs(RANGE_SPELLS[class] or {}) do
        local known = read("C_Spell.GetSpellInfo", s) or read("GetSpellInfo", s)
        if known then
            rangeSpell = s
            return s
        end
    end
    rangeSpell = nil
end

-- true / false, a hidden boolean, or nil when the client can't say for this unit.
local function rangeOf(unit)
    local okV, visible = pcall(UnitIsVisible, unit)
    if okV and visible == false then
        return false -- the client isn't even sending this unit: far
    end
    local spell = currentRangeSpell()
    if not spell then
        return nil
    end
    local f = (C_Spell and C_Spell.IsSpellInRange) or rawget(_G, "IsSpellInRange")
    if not f then
        return nil
    end
    local ok, r = pcall(f, spell, unit)
    if not ok or r == nil then
        return nil
    end
    if isSecret(r) then
        return r
    end
    return r == true or r == 1
end

local function setRangeLook(row, seen, range)
    if not seen then
        row:SetAlpha(ALPHA_UNKNOWN)
    elseif range == nil then
        row:SetAlpha(ALPHA_IN) -- seen, but the client can't say: don't fade a visible enemy
    elseif isSecret(range) then
        if not (row.SetAlphaFromBoolean and pcall(row.SetAlphaFromBoolean, row, range, ALPHA_IN, ALPHA_OUT)) then
            row:SetAlpha(ALPHA_IN)
        end
    else
        row:SetAlpha(range and ALPHA_IN or ALPHA_OUT)
    end
end

local rowOf = {}
local function update()
    if not frame or not frame:IsShown() or #roster == 0 then
        return
    end
    wipe(rowOf)
    for i = 1, #roster do
        if rows[i] and rows[i].enemy then
            rowOf[rows[i].enemy] = rows[i]
        end
    end
    local seen, range, eyes = {}, {}, {}

    local function visit(unit, isGroupTarget)
        if not UnitExists(unit) then
            return
        end
        local e = enemyOf(unit)
        local row = e and rowOf[e]
        if not row then
            return
        end
        if isGroupTarget then
            eyes[row] = (eyes[row] or 0) + 1
        end
        if not seen[row] then
            seen[row] = true
            -- Straight into the bars: a value hidden from the addon still draws.
            pcall(row.bar.SetMinMaxValues, row.bar, 0, UnitHealthMax(unit))
            pcall(row.bar.SetValue, row.bar, UnitHealth(unit))
            pcall(setHealthText, row, unit)
            pcall(setPower, row, unit)
            row.lastSeen = GetTime()
        end
        if range[row] == nil then
            range[row] = rangeOf(unit)
        end
    end

    for _, unit in ipairs(OWN) do
        visit(unit, false)
    end
    -- Eyes: who on your team targets them. In a raid the raid units include you; in a party, add
    -- your own target.
    local inRaid = read("IsInRaid")
    if not inRaid then
        visit("target", true)
    end
    for _, unit in ipairs(GROUP) do
        if inRaid and unit:find("^raid") or (not inRaid and unit:find("^party")) then
            visit(unit, true)
        end
    end

    local targetEnemy = UnitExists("target") and enemyOf("target") or nil
    for i = 1, #roster do
        local row = rows[i]
        if row and row.enemy then
            paint(row)
            setEyes(row, eyes[row] or 0)
            setRangeLook(row, seen[row], range[row])
            showEdges(row.targetEdges, targetEnemy ~= nil and row.enemy == targetEnemy)
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
        frame:SetBackdropColor(0, 0, 0, 0.5)
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

    C_Timer.NewTicker(UPDATE_EVERY, update)
    C_Timer.NewTicker(SCORE_EVERY, function()
        if frame:IsShown() and not testing then
            pcall(RequestBattlefieldScoreData)
            -- Also read what's there now, in case the update event doesn't come.
            readScoreboard()
        end
    end)
end

-- Show in battlegrounds (unless turned off), hide elsewhere. Out of combat only (secure rows).
local function refresh()
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

ns.on("PLAYER_ENTERING_WORLD", refresh)
ns.on("ZONE_CHANGED_NEW_AREA", refresh)
ns.on("UPDATE_BATTLEFIELD_SCORE", readScoreboard)
ns.on("PLAYER_REGEN_ENABLED", function()
    if pending then
        refresh()
        layout()
    end
end)

-- A fake team to place the window and try a click outside a battleground (/fbl bg test).
local function test(on)
    testing = on
    if on then
        local classes = { "PRIEST", "SHAMAN", "WARRIOR", "PALADIN", "ROGUE", "HUNTER", "DRUID", "MAGE", "WARLOCK", "MAGE" }
        roster = {}
        local target = read("UnitName", "target")
        for i, c in ipairs(classes) do
            local name = (i == 1 and type(target) == "string") and target or ("Enemy" .. i)
            roster[i] = { name = name, short = name, class = c, healer = i <= 2 }
        end
        byGuid, byName = {}, {}
        for _, e in ipairs(roster) do
            byName[e.name] = e
        end
    end
    refresh()
    layout()
end

-- /fbl bg debug: what the scoreboard and range check actually give an addon here.
local function debugScoreboard()
    local function show(v)
        if v == nil then
            return "nil"
        elseif isSecret(v) then
            return "<hidden>"
        end
        return tostring(v)
    end
    local _, kind = read("IsInInstance")
    say(string.format("instance %s; GetNumBattlefieldScores %s; my side %s (GetBattlefieldArenaFaction %s, %s)",
        show(kind), show(read("GetNumBattlefieldScores")), show(myFaction()), show(read("GetBattlefieldArenaFaction")),
        show(read("UnitFactionGroup", "player"))))
    say("range spell: " .. tostring(currentRangeSpell() or "none known") .. (UnitExists("target")
        and ("; your target in range: " .. show(rangeOf("target"))) or "; target someone to test range"))
    local curves = rawget(_G, "CurveConstants")
    say("health %: UnitHealthPercent " .. (rawget(_G, "UnitHealthPercent") and "exists" or "MISSING") .. ", ScaleTo100 "
        .. ((curves and curves.ScaleTo100) and "exists" or "MISSING"))
    local f = ns.resolve("C_PvP.GetScoreInfo")
    say("C_PvP.GetScoreInfo " .. (f and "exists" or "MISSING"))
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

ns.command({ name = "bg", args = "[on, off, test, scale <n>, range <spell>, debug]",
    help = "battleground enemies: health, power, range, healers, who your team targets; click a row to target",
    fn = function(rest)
        if rest == "debug" then
            debugScoreboard()
            return
        end
        local spell = rest:match("^range%s+(.+)$")
        if spell then
            spell = spell:match("^%s*(.-)%s*$")
            store().rangeSpell = (spell ~= "" and spell ~= "auto") and spell or nil
            rangeSpell, rangeSpellFor = nil, nil
            say("range check: " .. tostring(currentRangeSpell() or "no spell known") .. ".")
            return
        end
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
            refresh()
            say("enemies list off in battlegrounds.")
        elseif rest == "on" then
            store().off = nil
            refresh()
            say("enemies list on: it shows by itself in battlegrounds.")
        elseif rest == "test" then
            test(not testing)
            say(testing and "test team shown (row 1 is your target, if you have one). /fbl bg test again to hide."
                or "test team hidden.")
        else
            say("shows by itself in battlegrounds. /fbl bg test shows a fake team; scale 1.2; range Kick (melee) or range auto; debug.")
        end
    end })
