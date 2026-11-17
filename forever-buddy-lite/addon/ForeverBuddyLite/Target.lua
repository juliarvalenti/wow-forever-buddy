-- The target box (/fbl tar, /fbl t): type a name (or part of one), then click Target, or press
-- the key bound to it, as often as you like.
--
-- The button is a secure macro button running /tar <text> (and /cleartarget [dead] with "Alive
-- only", then /ping [@target,exists] with "Ping it"): the game's own targeting by name, at its
-- full range. It doesn't cycle: /tar takes the nearest match, and an addon can't see units beyond
-- nameplate range to pick another. Only a click or key press runs it.
-- The macro is set whenever the text changes (out of combat; a change typed in combat applies when
-- combat ends), so a press does nothing of ours, in or out of combat.
--
-- Below it: your current target as the game shows it to addons (any range): name, level, health,
-- dead, tapped by someone else (no loot or quest credit for you).
--
-- ForeverBuddyLiteDB.target = { text, alive, point, shown }

local _, ns = ...
local say = ns.say

local frame, box, info
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

-- Created when the file loads, so a key binding finds it by name before the window is opened.
local button = CreateFrame("Button", "FBLTargetButton", UIParent, "SecureActionButtonTemplate,UIPanelButtonTemplate")
button:SetSize(80, 22)
button:SetText("Target")
button:RegisterForClicks("AnyUp", "AnyDown") -- the secure template acts on the edge the game's setting names
button:Hide()

local function apply()
    if InCombatLockdown() then
        pending = true
        return
    end
    pending = false
    local t = text()
    if t == "" then
        button:SetAttribute("type", nil)
        return
    end
    local macro = "/tar " .. t
    if store().alive then
        macro = macro .. "\n/cleartarget [dead]"
    end
    if store().ping ~= false then
        macro = macro .. "\n/ping [@target,exists]"
    end
    button:SetAttribute("type", "macro")
    button:SetAttribute("macrotext", macro)
end

ns.on("PLAYER_REGEN_ENABLED", function()
    if pending then
        apply()
    end
end)

-- Your current target, read through pcall: the client may hide some values from addons.
local function describeTarget()
    if not UnitExists("target") then
        return "No target."
    end
    local name = UnitName("target") or "?"
    local level = UnitLevel("target") or 0
    local hp, max = UnitHealth("target") or 0, UnitHealthMax("target") or 0
    local dead = UnitIsDeadOrGhost("target")
    local tapped = UnitIsTapDenied and UnitIsTapDenied("target")
    local reaction = UnitReaction("target", "player") or 4
    local color = (dead or tapped) and "|cff808080" or reaction <= 3 and "|cffff6060" or reaction == 4 and "|cffffd200" or "|cff40ff40"
    local state = dead and "dead" or (max > 0 and string.format("%d%% health", hp * 100 / max) or "")
    return string.format("%s%s|r\n|cff9d9d9dLevel %s|r  %s%s", color, name, level > 0 and level or "??", state,
        (tapped and not dead) and "  |cff808080tapped by someone else|r" or "")
end

local function refresh()
    if not (frame and frame:IsShown()) then
        return
    end
    local ok, line = pcall(describeTarget)
    info:SetText(ok and line or "Target details hidden right now.")
end

local function build()
    local s = store()
    frame = CreateFrame("Frame", "ForeverBuddyLiteTarget", UIParent, "BasicFrameTemplateWithInset")
    frame:SetSize(260, 126)
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
        refresh()
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
        apply()
        if pending then
            info:SetText("In combat: the new name applies when combat ends.")
        end
    end)
    box:SetScript("OnEscapePressed", box.ClearFocus)
    box:SetScript("OnEnterPressed", box.ClearFocus)

    button:SetParent(frame)
    button:ClearAllPoints()
    button:SetPoint("LEFT", box, "RIGHT", 8, 0)
    button:Show()

    local alive = CreateFrame("CheckButton", nil, frame, "UICheckButtonTemplate")
    alive:SetSize(22, 22)
    alive:SetPoint("TOPLEFT", 12, -56)
    local label = alive:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    label:SetPoint("LEFT", alive, "RIGHT", 0, 1)
    label:SetText("Alive only")
    alive:SetChecked(s.alive and true or false)
    alive:SetScript("OnClick", function(self)
        store().alive = self:GetChecked() and true or false
        apply()
    end)

    local ping = CreateFrame("CheckButton", nil, frame, "UICheckButtonTemplate")
    ping:SetSize(22, 22)
    ping:SetPoint("LEFT", label, "RIGHT", 14, -1)
    local pingLabel = ping:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    pingLabel:SetPoint("LEFT", ping, "RIGHT", 0, 1)
    pingLabel:SetText("Ping it")
    ping:SetChecked(s.ping ~= false)
    ping:SetScript("OnClick", function(self)
        store().ping = self:GetChecked() and true or false
        apply()
    end)

    info = frame:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    info:SetPoint("TOPLEFT", 16, -84)
    info:SetPoint("RIGHT", -12, 0)
    info:SetJustifyH("LEFT")

    frame:RegisterEvent("PLAYER_TARGET_CHANGED")
    frame:RegisterUnitEvent("UNIT_HEALTH", "target")
    frame:SetScript("OnEvent", refresh)
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
    apply()
    if store().shown and not frame and not InCombatLockdown() then
        FBL_TargetToggle(true)
    end
end)

BINDING_NAME_FBL_TARGET_TOGGLE = "Show or hide the target box"
_G["BINDING_NAME_CLICK FBLTargetButton:Keybind"] = "Target the name in the target box"

ns.command({ name = "tar", aliases = { "target", "t" }, args = "[name]",
    help = "a box and a Target button: /tar the name you type (and ping it), as often as you press it",
    fn = function(rest)
        if rest ~= "" then
            store().text = rest
            if box then
                box:SetText(rest)
            else
                apply()
            end
            FBL_TargetToggle(true)
            return
        end
        FBL_TargetToggle()
    end })
