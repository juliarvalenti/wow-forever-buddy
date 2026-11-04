-- UI1: draws a UI document (design/specs/ui-language.md) in the logged-in
-- scene, for render_ui and the Your UI preview. Grown from the TH1
-- prototype (FBUnitProto's UF.lua), which drew round 4 from a spec.
--
-- The document is data only, in the global FBUISpec, already checked by
-- uicheck.lua (preview.lua does both). Sizes and offsets are 1080p screen
-- pixels. It draws once at login and again when the target changes: enough
-- for a picture of one moment. The in-game interpreter (UI1 phase 3)
-- updates from events instead.
--
-- Display only: our own frames with the mouse off. Blizzard's frames named
-- in `hide` only get alpha 0, out of combat.

local SPEC = FBUISpec
if type(SPEC) ~= "table" then
    return
end

-- One 1080p pixel in UI units.
local PX = 768 / 1080
local MEDIA = "Interface\\AddOns\\ForeverBuddy\\Media\\"
local FONTS = {
    Barlow = MEDIA .. "BarlowSCB.ttf",
    Geist = MEDIA .. "Geist.ttf",
    Marcellus = MEDIA .. "Marcellus.ttf",
    Friz = "Fonts\\FRIZQT__.TTF",
    ["Arial Narrow"] = "Fonts\\ARIALN.TTF",
    Morpheus = "Fonts\\MORPHEUS.TTF",
    Skurri = "Fonts\\SKURRI.TTF",
}
local OUTLINE = { thick = "THICKOUTLINE", thin = "OUTLINE", none = "" }
local POWER = { [0] = "#4a7fdc", [1] = "#d64541", [2] = "#e8935a", [3] = "#f0c94a", [6] = "#36c8f0" }
local REACTION = { hostile = "#d24a3a", neutral = "#e6c34a", friendly = "#4fd99a" }

local function hex(h, a)
    if type(h) == "table" then
        return h
    end
    return { tonumber(h:sub(2, 3), 16) / 255, tonumber(h:sub(4, 5), 16) / 255, tonumber(h:sub(6, 7), 16) / 255, a or 1 }
end

-- The states that hold now; a property given per state takes the last one
-- that holds, in this order.
local ORDER = { "always", "ooc", "combat", "target", "notarget", "full", "low", "stealth" }
-- (From PLAYER_REGEN_DISABLED and _ENABLED: the first fires just before
-- InCombatLockdown turns true.)
local inCombat = InCombatLockdown and InCombatLockdown() or false
local function states(unit, frac)
    local combat = inCombat
    local target = UnitExists and UnitExists("target") or false
    local on = { always = true, combat = combat, ooc = not combat, target = target, notarget = not target }
    on.stealth = FBUIStealth or (IsStealthed and IsStealthed()) or false
    if frac then
        on.full, on.low = frac >= 0.999, frac < (SPEC.lowAt or 0.35)
    end
    return on
end
local function pick(v, on)
    if type(v) ~= "table" or v[1] ~= nil or v.always == nil then
        return v
    end
    local out
    for _, s in ipairs(ORDER) do
        if on[s] and v[s] ~= nil then
            out = v[s]
        end
    end
    return out
end

-- A value as a fraction of its max, and the value; nil for one the client
-- keeps secret (a bar then shows full, text "?").
local function value(unit, what)
    local cur, max
    if what == "health" then
        cur, max = UnitHealth(unit), UnitHealthMax(unit)
    elseif what == "power" then
        cur, max = UnitPower(unit), UnitPowerMax(unit)
    elseif what == "combo" then
        cur, max = UnitPower("player", 4), UnitPowerMax("player", 4)
        if not max or max == 0 then
            max = 5
        end
    end
    if issecretvalue and (issecretvalue(cur) or issecretvalue(max)) then
        return nil, nil
    end
    if type(cur) ~= "number" or type(max) ~= "number" or max == 0 then
        return 1, cur
    end
    return math.max(0, math.min(1, cur / max)), cur
end

-- A colour: hex, a token bound to the unit, or stops by how full.
local function color(c, unit, frac)
    if c == "class" then
        local _, class = UnitClass(unit)
        local k = RAID_CLASS_COLORS and RAID_CLASS_COLORS[class or ""]
        return k and { k.r, k.g, k.b, 1 } or hex("#ffffff")
    elseif c == "power" then
        return hex(POWER[UnitPowerType(unit) or 0] or POWER[0])
    elseif c == "reaction" then
        local r = UnitCanAttack and UnitCanAttack("player", unit) and "hostile"
            or (UnitIsFriend and UnitIsFriend("player", unit) and "friendly") or "neutral"
        return hex(REACTION[r])
    elseif type(c) == "table" then
        local best
        for _, s in ipairs(c) do
            if (frac or 1) <= s[1] then
                return hex(s[2])
            end
            best = s
        end
        return hex(best[2])
    end
    return hex(c or "#ffffff")
end

local function short(v)
    if v >= 1000000 then
        return string.format("%.1f M", v / 1000000)
    elseif v >= 1000 then
        return string.format("%.1f K", v / 1000)
    end
    return tostring(math.floor(v + 0.5))
end

local function format(fmt, unit)
    return (fmt:gsub("{([%w%%]+)}", function(tok)
        if tok == "name" then
            return UnitName(unit) or ""
        elseif tok == "level" then
            return tostring(UnitLevel(unit) or "")
        elseif tok == "hp%" or tok == "power%" then
            local f = value(unit, tok == "hp%" and "health" or "power")
            return f and (math.floor(f * 100 + 0.5) .. "%") or "?"
        elseif tok == "hp" or tok == "power" or tok == "combo" then
            local _, v = value(unit, tok == "hp" and "health" or tok)
            return v and short(v) or "?"
        end
        return ""
    end))
end

local made = {}
local function anchorOf(a)
    local to = a[2] or "screen"
    if to == "char" then
        return UIParent, "CENTER", (a[4] or 0) * PX, (SPEC.charY or -14) * PX + (a[5] or 0) * PX
    elseif to == "screen" then
        return UIParent, a[3] or a[1], (a[4] or 0) * PX, (a[5] or 0) * PX
    end
    return made[to], a[3] or a[1], (a[4] or 0) * PX, (a[5] or 0) * PX
end

local function tex(f, layer, c)
    local t = f:CreateTexture(nil, layer or "ARTWORK")
    t:SetColorTexture(1, 1, 1, 1)
    if c then
        t:SetVertexColor(c[1], c[2], c[3], c[4] or 1)
    end
    return t
end

-- A 1px border, inside the frame's edge.
local function border(f, c, width)
    for _, e in ipairs({ { "TOPLEFT", "TOPRIGHT", true }, { "BOTTOMLEFT", "BOTTOMRIGHT", true },
        { "TOPLEFT", "BOTTOMLEFT", false }, { "TOPRIGHT", "BOTTOMRIGHT", false } }) do
        local l = tex(f, "OVERLAY", c)
        l:SetPoint(e[1])
        l:SetPoint(e[2])
        if e[3] then
            l:SetHeight((width or 1) * PX)
        else
            l:SetWidth((width or 1) * PX)
        end
    end
end

-- Stroked text: black copies around it, r pixels out, then the text.
local function label(f, p, text, point, rel, relPoint, x, y, on)
    local font = FONTS[p.font or "Barlow"]
    local size = (p.fontSize or 14) * PX
    local r = p.stroke or 0
    for dx = -r, r do
        for dy = -r, r do
            if (dx ~= 0 or dy ~= 0) and dx * dx + dy * dy <= r * r + r then
                local k = f:CreateFontString(nil, "OVERLAY", nil, 0)
                k:SetFont(font, size, "")
                k:SetShadowOffset(0, 0)
                k:SetTextColor(0, 0, 0, 1)
                k:SetJustifyH(p.justify or "CENTER")
                k:SetPoint(point, rel, relPoint, x + dx * PX, y + dy * PX)
                k:SetText(text)
            end
        end
    end
    local s = f:CreateFontString(nil, "OVERLAY", nil, 7)
    s:SetFont(font, size, OUTLINE[p.outline or "thick"])
    s:SetShadowOffset(0, 0)
    local c = color(pick(p.color, on) or "#ffffff", p.unit or "player")
    s:SetTextColor(c[1], c[2], c[3], c[4] or 1)
    s:SetJustifyH(p.justify or "CENTER")
    s:SetPoint(point, rel, relPoint, x, y)
    s:SetText(text)
    return s
end

local DRAW = {}

function DRAW.plate(f, p, unit, on)
    local c = color(pick(p.color, on) or "#000000", unit)
    local t = tex(f, "BACKGROUND", c)
    t:SetAllPoints()
    local g = pick(p.gradient, on)
    if type(g) == "table" and type(g[1]) == "string" and CreateColor then
        t:SetVertexColor(1, 1, 1, 1)
        t:SetGradient(g[1], CreateColor(unpack(hex(g[2]))), CreateColor(unpack(hex(g[3]))))
    end
    if p.border then
        border(f, hex(p.border), p.borderWidth)
    end
end

function DRAW.bar(f, p, unit, on)
    local frac = value(unit, p.value) or 1
    local back = tex(f, "BACKGROUND", hex(p.back or "#000000", p.backAlpha or 0.6))
    back:SetAllPoints()
    local c = color(pick(p.color, on) or "#ffffff", unit, frac)
    local fill = tex(f, "ARTWORK", c)
    -- a gentle gradient: lighter at the start, deeper at the end
    local g = pick(p.gradient, on)
    if type(g) == "table" and type(g[1]) == "number" and CreateColor then
        local function shade(k)
            if k >= 0 then
                return c[1] + (1 - c[1]) * k, c[2] + (1 - c[2]) * k, c[3] + (1 - c[3]) * k, 1
            end
            return c[1] * (1 + k), c[2] * (1 + k), c[3] * (1 + k), 1
        end
        fill:SetVertexColor(1, 1, 1, 1)
        fill:SetGradient(p.vertical and "VERTICAL" or "HORIZONTAL", CreateColor(shade(g[1])), CreateColor(shade(g[2])))
    end
    local start = p.reverse and (p.vertical and "TOP" or "RIGHT") or (p.vertical and "BOTTOM" or "LEFT")
    if p.vertical then
        fill:SetPoint(start .. "LEFT")
        fill:SetPoint(start .. "RIGHT")
        fill:SetHeight(math.max(1, f:GetHeight() * frac))
    else
        fill:SetPoint("TOP" .. start)
        fill:SetPoint("BOTTOM" .. start)
        fill:SetWidth(math.max(1, f:GetWidth() * frac))
    end
    for i = 1, (p.ticks or 1) - 1 do
        local t = tex(f, "OVERLAY", hex(p.tickColor or "#000000", 0.7))
        t:SetWidth(PX)
        t:SetPoint("TOP", f, "TOPLEFT", f:GetWidth() * i / p.ticks, 0)
        t:SetPoint("BOTTOM", f, "BOTTOMLEFT", f:GetWidth() * i / p.ticks, 0)
    end
    if p.border then
        border(f, hex(p.border), p.borderWidth)
    end
end

-- n marks lit up to the value: in a row, or around an arc (0 degrees is
-- up, clockwise).
function DRAW.pips(f, p, unit, on)
    local _, cur = value(unit, p.value or "combo")
    local n = p.count or 5
    local w, h, gap = (p.w or 10) * PX, (p.h or 10) * PX, (p.gap or 4) * PX
    local c = color(pick(p.color, on) or "#ffffff", unit)
    for i = 1, n do
        local x, y = (i - (n + 1) / 2) * (w + gap), 0
        if p.layout == "arc" then
            local th = math.rad((p.from or 160) + ((p.to or 200) - (p.from or 160)) * (i - 1) / math.max(1, n - 1))
            x, y = (p.radius or 150) * PX * math.sin(th), (p.radius or 150) * PX * math.cos(th)
        end
        local lit = i <= (cur or 0)
        local t = tex(f, "ARTWORK", lit and c or hex(p.emptyColor or "#2a2a2e", p.emptyAlpha or 1))
        t:SetSize(w, h)
        t:SetPoint("CENTER", f, "CENTER", x, y)
        if p.border then
            local b = CreateFrame("Frame", nil, f)
            b:SetSize(w, h)
            b:SetPoint("CENTER", f, "CENTER", x, y)
            border(b, hex(p.border))
        end
    end
end

function DRAW.text(f, p, unit, on)
    local pt = p.justify == "LEFT" and "LEFT" or p.justify == "RIGHT" and "RIGHT" or "CENTER"
    label(f, p, format(p.format or "", unit), pt, f, pt, 0, 0, on)
end

-- An action slot's icon or an aura's, trimmed, with a border.
function DRAW.icon(f, p, unit, on)
    local i = f:CreateTexture(nil, "ARTWORK")
    local file = p.slot and GetActionTexture(p.slot)
    if p.aura and C_UnitAuras and C_UnitAuras.GetAuraDataBySpellName then
        local a = C_UnitAuras.GetAuraDataBySpellName(unit, p.aura)
        file = a and a.icon
    end
    if file then
        i:SetTexture(file)
    else
        i:SetColorTexture(0.078, 0.078, 0.086, 1)
    end
    local z = p.zoom or 0.08
    i:SetTexCoord(z, 1 - z, z, 1 - z)
    i:SetAllPoints()
    if p.border then
        border(f, hex(p.border), p.borderWidth)
    end
end

-- Action buttons as the player has them: icons in a row or grid, 1px apart
-- by default, with their keybinds. (The in-game interpreter puts the
-- secure buttons here; a picture only needs how they look.)
function DRAW.actionbar(f, p, unit, on)
    local n, sz, gap = #p.slots, (p.icon or 40) * PX, (p.gap or 1) * PX
    local cols = p.columns or n
    local rows = math.ceil(n / cols)
    f:SetSize(cols * sz + (cols - 1) * gap, rows * sz + (rows - 1) * gap)
    if p.back then
        local b = tex(f, "BACKGROUND", hex(p.back))
        b:SetPoint("TOPLEFT", -PX, PX)
        b:SetPoint("BOTTOMRIGHT", PX, -PX)
    end
    for i, slot in ipairs(p.slots) do
        local col, row = (i - 1) % cols, math.floor((i - 1) / cols)
        local b = CreateFrame("Frame", nil, f)
        b:SetSize(sz, sz)
        b:SetPoint("TOPLEFT", f, "TOPLEFT", col * (sz + gap), -row * (sz + gap))
        local icon = b:CreateTexture(nil, "ARTWORK")
        local file = GetActionTexture(slot)
        if file then
            icon:SetTexture(file)
        else
            icon:SetColorTexture(0.078, 0.078, 0.086, 1)
        end
        local z = p.zoom or 0.12
        icon:SetTexCoord(z, 1 - z, z, 1 - z)
        icon:SetAllPoints()
        if p.border then
            border(b, hex(p.border))
        end
        if p.keys ~= false then
            local key = GetBindingKey((p.bind or "ACTIONBUTTON") .. ((p.bindFrom or 1) + i - 1))
            if key then
                key = key:gsub("SHIFT%-", "s"):gsub("CTRL%-", "c"):gsub("ALT%-", "a")
                label(b, { font = p.font, fontSize = p.keySize or 12, outline = p.outline, stroke = p.stroke,
                    color = p.keyColor, justify = "RIGHT" }, key, "TOPRIGHT", b, "TOPRIGHT", -3 * PX, -3 * PX, on)
            end
        end
    end
end

local built = {}
local function build(onlyTarget)
    for i = #built, 1, -1 do
        local e = built[i]
        if not onlyTarget or e.unit == "target" then
            e.frame:Hide()
            table.remove(built, i)
        end
    end
    local hasTarget = UnitExists("target")
    for _, p in ipairs(SPEC.parts) do
        local unit = p.unit or "player"
        if (not onlyTarget or unit == "target") and (unit ~= "target" or hasTarget) then
            local on = states(unit, value(unit, "health"))
            local f = CreateFrame("Frame", nil, UIParent)
            f:EnableMouse(false)
            f:SetFrameStrata("MEDIUM")
            f:SetFrameLevel(10 + (p.level or 0))
            local a = p.anchor or { "CENTER", "char" }
            local rel, relPoint, x, y = anchorOf(a)
            f:SetPoint(a[1], rel or UIParent, relPoint, x, y)
            local size = pick(p.size, on) or { 1, 1 }
            f:SetSize((size[1] or 1) * PX, (size[2] or 1) * PX)
            f:SetAlpha(pick(p.alpha, on) or 1)
            if pick(p.hidden, on) then
                f:Hide()
            end
            if p.name then
                made[p.name] = f
            end
            DRAW[p.kind](f, p, unit, on)
            built[#built + 1] = { frame = f, unit = unit }
        end
    end
end

-- Login draws it; a new target redraws the target's parts; entering or
-- leaving combat and the player's health redraw the lot (their states).
local ev = CreateFrame("Frame")
for _, e in ipairs({ "PLAYER_LOGIN", "PLAYER_TARGET_CHANGED", "PLAYER_REGEN_DISABLED", "PLAYER_REGEN_ENABLED",
    "UNIT_HEALTH" }) do
    ev:RegisterEvent(e)
end
ev:SetScript("OnEvent", function(_, event, unit)
    if event == "UNIT_HEALTH" and unit ~= "player" then
        return
    end
    if event == "PLAYER_REGEN_DISABLED" or event == "PLAYER_REGEN_ENABLED" then
        inCombat = event == "PLAYER_REGEN_DISABLED"
    end
    build(event == "PLAYER_TARGET_CHANGED")
    if event ~= "PLAYER_LOGIN" then
        return
    end
    for _, name in ipairs(SPEC.hide or {}) do
        local b = _G[name]
        if b and b.SetAlpha and not InCombatLockdown() then
            b:SetAlpha(0)
        end
    end
end)
