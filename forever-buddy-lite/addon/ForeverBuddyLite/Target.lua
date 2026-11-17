-- The target box (/fbl target): type a name (or part of one), then click Target, or press the key
-- bound to it, as often as you like.
--
-- Targeting is something the game only does from a click or a key press, through a secure button.
-- The button is one: each press targets, nothing happens on its own.
--
-- A press is /tar <text> (followed by /cleartarget [dead] with "Alive only"): the game's own
-- targeting by name, which reaches far beyond nameplates. Once you're on a match, a press goes
-- to the next match among the nameplates around you instead, so at a pack pressing again cycles
-- through them in a fixed order. Nameplates are what an addon can see of units near you (enemy
-- plates on, up to the game's nameplate distance; friendly NPCs need friendly plates). In combat
-- the game freezes what a secure button does, so every press there is the plain /tar.
--
-- The window also lists what matches near you, live: level, health, dead, tapped by someone else.
-- Reading those is open to any addon; only acting on them (targeting) needs the press.
-- "Untapped only" skips mobs someone else has tagged (no loot or quest credit for you).
--
-- ForeverBuddyLiteDB.target = { text, alive, untapped, point, shown }

local _, ns = ...
local say = ns.say

local MAX_PLATES = 60
local ROWS = 8
local frame, box, status, aliveBox, summary
local rows = {}
local pending = false -- the text changed in combat: apply it when combat ends

local function store()
    local db = ns.db
    if not db then
        return { text = "", alive = true }
    end
    db.target = type(db.target) == "table" and db.target or { text = "", alive = true }
    return db.target
end

local function text()
    return (store().text or ""):gsub("[\r\n]", " "):match("^%s*(.-)%s*$"):sub(1, 100)
end

local function fallback(t)
    local macro = "/tar " .. t
    if store().alive then
        macro = macro .. "\n/cleartarget [dead]"
    end
    return macro
end

local function setStatus(msg)
    if status then
        status:SetText(msg or "")
    end
end

-- The button, created when the file loads so a key binding can find it by name before the window
-- is ever opened. Registered for both key edges; only one acts (see PreClick).
local button = CreateFrame("Button", "FBLTargetButton", UIParent, "SecureActionButtonTemplate,UIPanelButtonTemplate")
button:SetSize(80, 22)
button:SetText("Target")
button:RegisterForClicks("AnyUp", "AnyDown")
button:Hide()

-- What a press does when we don't pick it right then: the fallback macro. Out of combat only.
local function rest()
    if InCombatLockdown() then
        pending = true
        return
    end
    pending = false
    local t = text()
    button:SetAttribute("unit", nil)
    if t == "" then
        button:SetAttribute("type", nil)
    else
        button:SetAttribute("type", "macro")
        button:SetAttribute("macrotext", fallback(t))
    end
end

-- Units on nameplates whose name contains the text (any case), in a fixed order for cycling.
-- filtered: leave out what the options say to skip (dead, tapped).
-- One nameplate, if its name contains needle. In a pcall: in combat the client may hand an addon
-- hidden ("secret") values it can't compare, and one unreadable plate shouldn't stop the rest.
local function readPlate(unit, needle)
    if not UnitExists(unit) then
        return nil
    end
    local name = UnitName(unit)
    if not (name and name:lower():find(needle, 1, true)) then
        return nil
    end
    local guid = UnitGUID(unit) or unit
    local _ = guid < "" -- comparable, for the cycling order
    return {
        unit = unit,
        guid = guid,
        name = name,
        dead = UnitIsDeadOrGhost(unit) and true or false,
        tapped = UnitIsTapDenied and UnitIsTapDenied(unit) and true or false,
    }
end

local function matches(t, filtered)
    local found, needle, s = {}, t:lower(), store()
    for i = 1, MAX_PLATES do
        local ok, m = pcall(readPlate, "nameplate" .. i, needle)
        if ok and m and not (filtered and ((s.alive and m.dead) or (s.untapped and m.tapped))) then
            found[#found + 1] = m
        end
    end
    table.sort(found, function(a, b)
        return a.guid < b.guid
    end)
    return found
end

button:SetScript("PreClick", function(self, mouse, down)
    if InCombatLockdown() then
        return -- the fallback macro set up before combat does it
    end
    -- A click or key sends a down and an up. The game's secure button acts only on the edge its
    -- ActionButtonUseKeyDown setting names (down, by default), for mouse clicks too; prepare that one.
    local acts = (down and true or false) == GetCVarBool("ActionButtonUseKeyDown")
    if not acts then
        self:SetAttribute("type", nil)
        return
    end
    local t = text()
    if t == "" then
        self:SetAttribute("type", nil)
        setStatus("Type a name first.")
        return
    end
    -- Already on a match with others on nameplates: the one after it, wrapping around.
    local list = matches(t, true)
    local current, pick = UnitGUID("target"), nil
    for i, m in ipairs(list) do
        if m.guid == current then
            pick = i % #list + 1
            break
        end
    end
    if pick and #list > 1 then
        self:SetAttribute("type", "target")
        self:SetAttribute("unit", list[pick].unit)
        setStatus(string.format("Next nearby: %d of %d", pick, #list))
        return
    end
    -- Otherwise the game's own targeting by name, at full range.
    self:SetAttribute("type", "macro")
    self:SetAttribute("macrotext", fallback(t))
    setStatus("/tar " .. t)
end)

button:SetScript("PostClick", rest)

ns.on("PLAYER_REGEN_ENABLED", function()
    if pending then
        rest()
    end
end)

-- The live list: every match near you, the options' skips greyed, your target marked.
local function colorOf(m)
    if m.dead or m.tapped then
        return "|cff808080"
    end
    local r = UnitReaction(m.unit, "player") or 4
    return r <= 3 and "|cffff6060" or r == 4 and "|cffffd200" or "|cff40ff40"
end

local function plateRange()
    local ok, v = pcall(GetCVar, "nameplateMaxDistance")
    return ok and tonumber(v) and tostring(math.floor(tonumber(v))) or "?"
end

local function refreshList()
    if not frame or not frame:IsShown() then
        return
    end
    local t = text()
    local list = t ~= "" and matches(t, false) or {}
    local alive, dead, tapped = 0, 0, 0
    for _, m in ipairs(list) do
        if m.dead then
            dead = dead + 1
        else
            alive = alive + 1
        end
        if m.tapped and not m.dead then
            tapped = tapped + 1
        end
    end
    if t == "" then
        summary:SetText("")
    elseif #list == 0 then
        summary:SetText(string.format("None on nameplates (they reach %s yd). /tar still finds farther ones.", plateRange()))
    else
        summary:SetText(string.format("On nameplates (%s yd): %d alive%s, %d dead", plateRange(), alive,
            tapped > 0 and (" (" .. tapped .. " tapped)") or "", dead))
    end
    local current = UnitGUID("target")
    for i = 1, ROWS do
        local m, row = list[i], rows[i]
        if m and not pcall(function()
            local hp, max = UnitHealth(m.unit) or 0, UnitHealthMax(m.unit) or 0
            local health = m.dead and "dead" or (max > 0 and string.format("%d%%", hp * 100 / max) or "")
            local level = UnitLevel(m.unit) or 0
            local tags = (m.tapped and not m.dead) and " tapped" or ""
            row.left:SetText((m.guid == current and "|cffffffff> |r" or "  ") .. colorOf(m) .. m.name .. "|r")
            row.right:SetText(string.format("|cff9d9d9d%s|r %s%s", level > 0 and ("L" .. level) or "L??", health, tags))
        end) then
            row.right:SetText("")
        end
        row:SetShown(m ~= nil)
    end
    if #list > ROWS then
        rows[ROWS].right:SetText(rows[ROWS].right:GetText() .. string.format(" |cff9d9d9d+%d more|r", #list - ROWS))
    end
end

local function build()
    local s = store()
    frame = CreateFrame("Frame", "ForeverBuddyLiteTarget", UIParent, "BasicFrameTemplateWithInset")
    frame:SetSize(270, 150 + ROWS * 14)
    frame:SetFrameStrata("MEDIUM")
    frame:SetClampedToScreen(true)
    local point = s.point
    if type(point) == "table" and point[1] then
        frame:SetPoint(point[1], UIParent, point[2], point[3], point[4])
    else
        frame:SetPoint("CENTER", 0, 200)
    end
    frame:SetMovable(true)
    frame:EnableMouse(true)
    frame:RegisterForDrag("LeftButton")
    frame:SetScript("OnDragStart", frame.StartMoving)
    frame:SetScript("OnDragStop", function(self)
        self:StopMovingOrSizing()
        local p, _, rp, x, y = self:GetPoint()
        store().point = { p, rp, x, y }
    end)
    frame:SetScript("OnShow", function()
        store().shown = true
    end)
    frame:SetScript("OnHide", function()
        store().shown = false
    end)

    local title = frame.TitleText or frame:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    if not frame.TitleText then
        title:SetPoint("TOP", 0, -5)
    end
    title:SetText("Target")

    box = CreateFrame("EditBox", nil, frame, "InputBoxTemplate")
    box:SetSize(130, 20)
    box:SetPoint("TOPLEFT", 20, -32)
    box:SetAutoFocus(false)
    box:SetText(s.text or "")
    box:SetScript("OnTextChanged", function(self)
        store().text = self:GetText()
        rest()
        if pending then
            setStatus("In combat: the new name applies when combat ends.")
        end
    end)
    box:SetScript("OnEscapePressed", box.ClearFocus)
    box:SetScript("OnEnterPressed", box.ClearFocus)

    button:SetParent(frame)
    button:ClearAllPoints()
    button:SetPoint("LEFT", box, "RIGHT", 8, 0)
    button:Show()

    aliveBox = CreateFrame("CheckButton", nil, frame, "UICheckButtonTemplate")
    aliveBox:SetSize(22, 22)
    aliveBox:SetPoint("TOPLEFT", 12, -56)
    local label = aliveBox:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    label:SetPoint("LEFT", aliveBox, "RIGHT", 0, 1)
    label:SetText("Alive only")
    aliveBox:SetChecked(s.alive and true or false)
    aliveBox:SetScript("OnClick", function(self)
        store().alive = self:GetChecked() and true or false
        rest()
    end)

    local untapped = CreateFrame("CheckButton", nil, frame, "UICheckButtonTemplate")
    untapped:SetSize(22, 22)
    untapped:SetPoint("LEFT", label, "RIGHT", 14, -1)
    local untappedLabel = untapped:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    untappedLabel:SetPoint("LEFT", untapped, "RIGHT", 0, 1)
    untappedLabel:SetText("Untapped only")
    untapped:SetChecked(s.untapped and true or false)
    untapped:SetScript("OnClick", function(self)
        store().untapped = self:GetChecked() and true or false
    end)

    status = frame:CreateFontString(nil, "OVERLAY", "GameFontDisableSmall")
    status:SetPoint("TOPLEFT", 16, -84)
    status:SetPoint("RIGHT", -12, 0)
    status:SetJustifyH("LEFT")
    status:SetText("Press: /tar at any range. On a match: next one nearby.")

    summary = frame:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    summary:SetPoint("TOPLEFT", 16, -102)
    summary:SetPoint("RIGHT", -12, 0)
    summary:SetJustifyH("LEFT")
    for i = 1, ROWS do
        local row = CreateFrame("Frame", nil, frame)
        row:SetSize(240, 14)
        row:SetPoint("TOPLEFT", 14, -104 - i * 14)
        row.left = row:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
        row.left:SetPoint("LEFT")
        row.left:SetPoint("RIGHT", -96, 0)
        row.left:SetJustifyH("LEFT")
        row.left:SetWordWrap(false)
        row.right = row:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
        row.right:SetPoint("RIGHT")
        row.right:SetJustifyH("RIGHT")
        row:Hide()
        rows[i] = row
    end
    C_Timer.NewTicker(0.3, refreshList)
end

-- Showing or hiding a window that holds a secure button is blocked in combat.
function FBL_TargetToggle(show)
    if InCombatLockdown() then
        say("the target box can't open or close in combat.")
        return
    end
    if not frame then
        build()
    end
    if show == nil then
        show = not frame:IsShown()
    end
    frame:SetShown(show)
end

ns.on("PLAYER_ENTERING_WORLD", function()
    rest()
    if store().shown and not frame and not InCombatLockdown() then
        FBL_TargetToggle(true)
    end
end)

BINDING_NAME_FBL_TARGET_TOGGLE = "Show or hide the target box"
_G["BINDING_NAME_CLICK FBLTargetButton:Keybind"] = "Target the name in the target box (press again to cycle)"

-- /fbl t debug: what the addon can read of each nameplate, to tell an empty list from hidden names.
local function debugPlates()
    local secret = rawget(_G, "issecretvalue")
    local n = 0
    for i = 1, MAX_PLATES do
        local unit = "nameplate" .. i
        if UnitExists(unit) then
            n = n + 1
            local ok, name = pcall(UnitName, unit)
            local hidden = ok and secret and secret(name)
            local readable = ok and not hidden and pcall(function()
                return name:lower()
            end)
            say(string.format("%s: %s", unit, not ok and "UnitName failed"
                or hidden and "name is hidden (secret value)"
                or readable and ("\"" .. tostring(name) .. "\"")
                or "name unreadable"))
        end
    end
    say(string.format("%d nameplates. Distance setting %s yd. Act on key down: %s.", n, plateRange(),
        tostring(GetCVarBool("ActionButtonUseKeyDown"))))
end

ns.command({ name = "target", aliases = { "tar", "t" }, args = "[name | debug]",
    help = "a box and a Target button: targets the name you type; press again to cycle",
    fn = function(rest_)
        if rest_ == "debug" then
            debugPlates()
            return
        end
        if rest_ ~= "" then
            store().text = rest_
            if box then
                box:SetText(rest_)
            else
                rest()
            end
            FBL_TargetToggle(true)
            return
        end
        FBL_TargetToggle()
    end })
