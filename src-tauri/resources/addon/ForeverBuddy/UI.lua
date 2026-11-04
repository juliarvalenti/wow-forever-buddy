-- UI1: draws a UI document (design/specs/ui-language.md), live. Grown from
-- the TH1 prototype (FBUnitProto's UF.lua), which drew round 4 from a spec.
--
-- One interpreter for the game and the pictures, so a draft never shows
-- what the game wouldn't draw: render_ui and the Your UI preview run this
-- same file in the scene (tools/addon-shots/preview.lua), with the document
-- in the global FBUISpec. In game the document is the one the player chose
-- with Use this, in the slot Data/UI.lua (ForeverBuddyData_UI.doc). Either
-- way it's data only, and in game it's checked again here with the addon's
-- own UICheck.lua (the folder is writable by anything on the machine): a
-- missing or bad document draws nothing and the standard HUD stays.
--
-- Frames are made once, out of combat, then repainted from events: health,
-- power, combo points, auras, the target, combat and stealth. One shared
-- OnUpdate ticks the aura bars' timers. Our frames have the mouse off and
-- are never moved or resized in combat (a part's size for a state waits for
-- it to end); in game a state hides a part by alpha, so nothing here can be
-- refused in combat. Blizzard's player and target frames, and the ones named
-- in `hide`, only get alpha 0.
--
-- Values the game hides (secret values) are never compared or summed: a bar
-- keeps its last fill, text shows "?".
--
-- Action bars: in a picture, icons as they look. In game the `actionbar`
-- parts place TH3's secure buttons (ForeverBuddy.lua, U.bars), which lays
-- them out only out of combat.

local ns = select(2, ...)
if type(ns) ~= "table" then
    ns = {}
end
local SPEC, LIVE = rawget(_G, "FBUISpec"), false
if type(SPEC) ~= "table" then
    SPEC = nil
    local slot, checker = rawget(_G, "ForeverBuddyData_UI"), ns.uicheck
    if type(slot) == "table" and slot.schema == 1 and type(slot.doc) == "table" and type(checker) == "table" then
        local ok, problems = pcall(checker.check, slot.doc)
        if ok and type(problems) == "table" and #problems == 0 then
            SPEC, LIVE = slot.doc, true
        end
    end
end
if not SPEC then
    return
end
-- (ForeverBuddy.lua's HUD stands aside while a document draws.)
ns.uiActive = LIVE

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

local function isSecret(v)
    return issecretvalue ~= nil and issecretvalue(v) == true
end

local function locked()
    return LIVE and InCombatLockdown ~= nil and InCombatLockdown() == true
end

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
-- keeps secret.
local function value(unit, what)
    local cur, max
    if what == "health" then
        cur, max = UnitHealth(unit), UnitHealthMax(unit)
    elseif what == "power" then
        cur, max = UnitPower(unit), UnitPowerMax(unit)
    elseif what == "combo" then
        cur, max = UnitPower("player", 4), UnitPowerMax("player", 4)
        if isSecret(max) then
            return nil, nil
        end
        if not max or max == 0 then
            max = 5
        end
    end
    if isSecret(cur) or isSecret(max) then
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
        local kind = UnitPowerType(unit)
        return hex(POWER[not isSecret(kind) and kind or 0] or POWER[0])
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
            local n = UnitName(unit)
            return type(n) == "string" and not isSecret(n) and n:gsub("|", "||") or ""
        elseif tok == "level" then
            local l = UnitLevel(unit)
            return type(l) == "number" and not isSecret(l) and tostring(l) or ""
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

-- Each frame's textures, text and child frames, made the first time a paint
-- asks for them and handed out again, in the same order, on every paint
-- after; the ones a paint didn't ask for hide. So a repaint makes nothing
-- new once a part has drawn its fullest.
local pools = {}
local cur -- the part being painted
local function poolOf(f)
    local p = pools[f]
    if not p then
        p = { tex = {}, fs = {}, fr = {}, nt = 0, nf = 0, nr = 0 }
        pools[f] = p
    end
    return p
end

local function tex(f, layer, c)
    local p = poolOf(f)
    p.nt = p.nt + 1
    local t = p.tex[p.nt]
    if t then
        t:ClearAllPoints()
        t:SetDrawLayer(layer or "ARTWORK", 0)
        t:SetRotation(0)
        t:Show()
    else
        t = f:CreateTexture(nil, layer or "ARTWORK")
        p.tex[p.nt] = t
    end
    t:SetTexCoord(0, 1, 0, 1)
    t:SetColorTexture(1, 1, 1, 1)
    c = c or { 1, 1, 1, 1 }
    t:SetVertexColor(c[1], c[2], c[3], c[4] or 1)
    return t
end

local function fontString(f, sub)
    local p = poolOf(f)
    p.nf = p.nf + 1
    local s = p.fs[p.nf]
    if s then
        s:ClearAllPoints()
        s:SetDrawLayer("OVERLAY", sub)
        s:Show()
    else
        s = f:CreateFontString(nil, "OVERLAY", nil, sub)
        p.fs[p.nf] = s
    end
    return s
end

local function child(f)
    local p = poolOf(f)
    p.nr = p.nr + 1
    local c = p.fr[p.nr]
    if c then
        c:ClearAllPoints()
        c:Show()
    else
        c = CreateFrame("Frame", nil, f)
        p.fr[p.nr] = c
        cur.frames[#cur.frames + 1] = c
    end
    return c
end

local function begin(e)
    for _, f in ipairs(e.frames) do
        local p = poolOf(f)
        p.nt, p.nf, p.nr = 0, 0, 0
    end
end

local function finish(e)
    for _, f in ipairs(e.frames) do
        local p = pools[f]
        for _, k in ipairs({ { "tex", p.nt }, { "fs", p.nf }, { "fr", p.nr } }) do
            local list = p[k[1]]
            for i = k[2] + 1, #list do
                list[i]:Hide()
            end
        end
    end
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
                local k = fontString(f, 0)
                k:SetFont(font, size, "")
                k:SetShadowOffset(0, 0)
                k:SetTextColor(0, 0, 0, 1)
                k:SetJustifyH(p.justify or "CENTER")
                k:SetPoint(point, rel, relPoint, x + dx * PX, y + dy * PX)
                k:SetText(text)
            end
        end
    end
    local s = fontString(f, 7)
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
    -- turned about its middle (a diamond, a slash); a turned plate has no border
    if p.rotate then
        t:SetRotation(-math.rad(p.rotate))
    elseif p.border then
        border(f, hex(p.border), p.borderWidth)
    end
end

-- (A value the game hides keeps the bar where it last was.)
function DRAW.bar(f, p, unit, on, e)
    local frac = value(unit, p.value)
    if frac then
        e.fill = frac
    end
    frac = e.fill or 1
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
        fill:SetHeight(math.max(1, e.h * frac))
    else
        fill:SetPoint("TOP" .. start)
        fill:SetPoint("BOTTOM" .. start)
        fill:SetWidth(math.max(1, e.w * frac))
    end
    for i = 1, (p.ticks or 1) - 1 do
        local t = tex(f, "OVERLAY", hex(p.tickColor or "#000000", 0.7))
        t:SetWidth(PX)
        t:SetPoint("TOP", f, "TOPLEFT", e.w * i / p.ticks, 0)
        t:SetPoint("BOTTOM", f, "BOTTOMLEFT", e.w * i / p.ticks, 0)
    end
    if p.border then
        border(f, hex(p.border), p.borderWidth)
    end
end

-- n marks lit up to the value: in a row, a column, or around an arc (0
-- degrees is up, clockwise). Combo points light one each; health and power
-- light their share of n, so 30 pips make a segmented gauge. `rotate` turns
-- each mark (45 makes diamonds); on an arc, `orient` turns each to face out.
function DRAW.pips(f, p, unit, on, e)
    local what = p.value or "combo"
    local frac, now = value(unit, what)
    local n = p.count or 5
    if frac then
        e.lit = what == "combo" and (now or 0) or math.floor(frac * n + 0.5)
    end
    local lit = e.lit or (what == "combo" and 0 or n)
    local w, h, gap = (p.w or 10) * PX, (p.h or 10) * PX, (p.gap or 4) * PX
    for i = 1, n do
        local x, y, rot = (i - (n + 1) / 2) * (w + gap), 0, math.rad(p.rotate or 0)
        if p.layout == "column" then
            x, y = 0, ((n + 1) / 2 - i) * (h + gap)
        elseif p.layout == "arc" then
            local deg = (p.from or 160) + ((p.to or 200) - (p.from or 160)) * (i - 1) / math.max(1, n - 1)
            local th = math.rad(deg)
            x, y = (p.radius or 150) * PX * math.sin(th), (p.radius or 150) * PX * math.cos(th)
            if p.orient then
                rot = rot + th
            end
        end
        -- (colour stops by each pip's own place, so a gauge can run red to green)
        local c = color(pick(p.color, on) or "#ffffff", unit, i / n)
        local t = tex(f, "ARTWORK", i <= lit and c or hex(p.emptyColor or "#2a2a2e", p.emptyAlpha or 1))
        t:SetSize(w, h)
        t:SetPoint("CENTER", f, "CENTER", x, y)
        if rot ~= 0 then
            t:SetRotation(-rot)
        elseif p.border then
            local b = child(f)
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
    local i = tex(f, "ARTWORK")
    local file = p.slot and GetActionTexture(p.slot)
    if p.aura and C_UnitAuras and C_UnitAuras.GetAuraDataBySpellName then
        local ok, a = pcall(C_UnitAuras.GetAuraDataBySpellName, unit, p.aura)
        file = ok and type(a) == "table" and not (issecrettable and issecrettable(a)) and a.icon or nil
    end
    if file then
        i:SetTexture(file)
    else
        i:SetVertexColor(0.078, 0.078, 0.086, 1)
    end
    local z = p.zoom or 0.08
    i:SetTexCoord(z, 1 - z, z, 1 - z)
    i:SetAllPoints()
    if p.border then
        border(f, hex(p.border), p.borderWidth)
    end
end

-- Action buttons in a row, a grid or an arc, 1px apart by default. Where
-- each goes, from the part's frame: TOPLEFT offsets in a grid, CENTER
-- offsets on an arc.
local function places(p, n, sz, gap)
    local cols = p.columns or n
    local out = {}
    for i = 1, n do
        if p.layout == "arc" then
            local th = math.rad((p.from or 120) + ((p.to or 240) - (p.from or 120)) * (i - 1) / math.max(1, n - 1))
            out[i] = { "CENTER", (p.radius or 200) * PX * math.sin(th), (p.radius or 200) * PX * math.cos(th) }
        else
            local col, row = (i - 1) % cols, math.floor((i - 1) / cols)
            out[i] = { "TOPLEFT", col * (sz + gap), -row * (sz + gap) }
        end
    end
    return out
end

-- In a picture: icons with their keybinds. In game: TH3's secure buttons
-- go there (ns.uiBars, laid out by U.bars out of combat), and this only
-- draws the plate behind a grid.
local bars = {}
function DRAW.actionbar(f, p, unit, on, e)
    local n, sz, gap = #p.slots, (p.icon or 40) * PX, (p.gap or 1) * PX
    local cols = p.columns or n
    local rows = math.ceil(n / cols)
    local arc = p.layout == "arc"
    if not locked() then
        if arc then
            -- around the frame's middle, like pips on an arc
            f:SetSize(sz, sz)
        else
            f:SetSize(cols * sz + (cols - 1) * gap, rows * sz + (rows - 1) * gap)
        end
    end
    if p.back and not arc then
        local b = tex(f, "BACKGROUND", hex(p.back))
        b:SetPoint("TOPLEFT", -PX, PX)
        b:SetPoint("BOTTOMRIGHT", PX, -PX)
    end
    local at = places(p, n, sz, gap)
    if LIVE then
        if not e.placed then
            e.placed = true
            local list = {}
            for i, slot in ipairs(p.slots) do
                list[i] = { slot = slot, point = at[i][1], x = at[i][2], y = at[i][3] }
            end
            bars[#bars + 1] = { frame = f, size = sz, buttons = list }
            ns.uiBars = bars
        end
        return
    end
    for i, slot in ipairs(p.slots) do
        local b = child(f)
        b:SetSize(sz, sz)
        b:SetPoint(at[i][1], f, at[i][1], at[i][2], at[i][3])
        if arc and p.back then
            local bk = tex(b, "BACKGROUND", hex(p.back))
            bk:SetPoint("TOPLEFT", -PX, PX)
            bk:SetPoint("BOTTOMRIGHT", PX, -PX)
        end
        local icon = tex(b, "ARTWORK")
        local file = GetActionTexture(slot)
        if file then
            icon:SetTexture(file)
        else
            icon:SetVertexColor(0.078, 0.078, 0.086, 1)
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

-- Aura bars (UI1, Julia's ask): one timer bar per aura on a unit, each with
-- its icon, name, stacks, a fill that drains and the time left. Which ones:
-- `filter` (mine, the default: helpful or harmful, cast by you; or
-- helpful, harmful, all), `show` (only these spell names or IDs, in that
-- order of priority), `max`, `sort` (time left or name) and `grow` (down
-- or up from the anchor).
--
-- Secret values: the client can hide an aura's timing in combat, and code
-- can't do sums on a hidden value. A bar whose timing is hidden shows full
-- with no time, never a guess; and if the aura list itself is refused, the
-- last bars stay up.
local DISPEL = { Magic = "#3c8cff", Curse = "#a24dff", Poison = "#3fbf5f", Disease = "#b8862b" }

local function auraList(unit, p)
    local out = {}
    if not (C_UnitAuras and C_UnitAuras.GetAuraDataByIndex) then
        return out
    end
    local filter = p.filter or "mine"
    for _, kind in ipairs({ "HELPFUL", "HARMFUL" }) do
        if filter == "all" or filter == "mine" or (filter == "helpful") == (kind == "HELPFUL") then
            for i = 1, 40 do
                local ok, a = pcall(C_UnitAuras.GetAuraDataByIndex, unit, i, kind)
                if not ok or (type(a) == "table" and issecrettable and issecrettable(a)) then
                    return nil
                end
                if type(a) ~= "table" then
                    break
                end
                local mine = a.sourceUnit == "player" or a.isFromPlayerOrPlayerPet
                if isSecret(mine) then
                    mine = false
                end
                -- maxDuration leaves out long and endless auras (a poison's
                -- hour); one whose duration is hidden stays, never a guess.
                local dur = a.duration
                local long = p.maxDuration and type(dur) == "number" and not isSecret(dur)
                    and (dur <= 0 or dur > p.maxDuration)
                if (filter ~= "mine" or mine) and not long then
                    -- (wrapped: the client's own table is never written to)
                    out[#out + 1] = { aura = a, harmful = kind == "HARMFUL" }
                end
            end
        end
    end
    if p.show then
        local rank = {}
        for i, s in ipairs(p.show) do
            rank[s] = i
        end
        local kept = {}
        for _, e in ipairs(out) do
            local id, name = e.aura.spellId, e.aura.name
            local r = (not isSecret(id) and rank[id]) or (not isSecret(name) and rank[name]) or nil
            if r then
                e.rank = r
                kept[#kept + 1] = e
            end
        end
        out = kept
    end
    local now = GetTime and GetTime() or 0
    for _, entry in ipairs(out) do
        local e = entry.aura.expirationTime
        entry.ends = type(e) == "number" and not isSecret(e) and e > 0 and e or nil
        entry.name = not isSecret(entry.aura.name) and entry.aura.name or ""
    end
    table.sort(out, function(x, y)
        if p.sort == "name" then
            return x.name < y.name
        elseif p.show and p.sort == nil then
            return x.rank < y.rank
        end
        return (x.ends or math.huge) < (y.ends or math.huge)
    end)
    for i = #out, (p.max or 8) + 1, -1 do
        out[i] = nil
    end
    return out
end

-- "14", "1:05", "40m", "2h"; nothing for an aura that doesn't run out.
local function timeLeft(s)
    if s == math.huge or s <= 0 then
        return ""
    elseif s >= 3600 then
        return math.ceil(s / 3600) .. "h"
    elseif s >= 600 then
        return math.ceil(s / 60) .. "m"
    elseif s >= 60 then
        return ("%d:%02d"):format(math.floor(s / 60), math.floor(s % 60))
    end
    return tostring(math.ceil(s))
end

function DRAW.auras(f, p, unit, on, e)
    local w, h, gap = (p.w or 220) * PX, (p.h or 22) * PX, (p.gap or 2) * PX
    local iconW = p.icon == false and 0 or h
    local up = p.grow == "up"
    if not locked() then
        f:SetSize(w, h)
    end
    local now = GetTime and GetTime() or 0
    for i, entry in ipairs(e.list) do
        local a = entry.aura
        local row = child(f)
        row:SetSize(w, h)
        row:SetPoint(up and "BOTTOMLEFT" or "TOPLEFT", f, up and "BOTTOMLEFT" or "TOPLEFT", 0,
            (up and 1 or -1) * (i - 1) * (h + gap))
        if iconW > 0 then
            local ic = tex(row, "ARTWORK")
            if not isSecret(a.icon) then
                ic:SetTexture(a.icon)
            end
            ic:SetTexCoord(0.08, 0.92, 0.08, 0.92)
            ic:SetSize(iconW, h)
            ic:SetPoint("LEFT", row, "LEFT", 0, 0)
            local stacks = a.applications
            if type(stacks) == "number" and not isSecret(stacks) and stacks > 1 then
                label(row, { font = p.font, fontSize = math.max(8, (p.fontSize or 13) - 2), outline = p.outline,
                    stroke = p.stroke, justify = "RIGHT" }, tostring(stacks), "BOTTOMRIGHT", ic, "BOTTOMRIGHT",
                    -PX, PX, on)
            end
        end
        local bar = child(row)
        bar:SetPoint("TOPLEFT", row, "TOPLEFT", iconW > 0 and iconW + PX or 0, 0)
        bar:SetPoint("BOTTOMRIGHT", row, "BOTTOMRIGHT", 0, 0)
        local bw = w - (iconW > 0 and iconW + PX or 0)
        tex(bar, "BACKGROUND", hex(p.back or "#000000", p.backAlpha or 0.6)):SetAllPoints()
        -- the share left: full while the timing is hidden or it never runs out
        local s, dur = entry.ends and entry.ends - now or math.huge, a.duration
        local frac = 1
        if s ~= math.huge and type(dur) == "number" and not isSecret(dur) and dur > 0 then
            frac = math.max(0, math.min(1, s / dur))
        end
        local dispel = a.dispelName
        if isSecret(dispel) then
            dispel = nil
        end
        local c = p.color == "dispel" and hex(entry.harmful and (DISPEL[dispel] or "#d24a3a") or "#4fd99a")
            or color(pick(p.color, on) or (entry.harmful and "#d24a3a" or "#4fd99a"), unit, frac)
        local fill = tex(bar, "ARTWORK", c)
        fill:SetPoint("TOPLEFT")
        fill:SetPoint("BOTTOMLEFT")
        fill:SetWidth(math.max(1, bw * frac))
        if p.border then
            border(row, hex(p.border))
        end
        local text = { font = p.font, fontSize = p.fontSize or 13, outline = p.outline, stroke = p.stroke,
            color = p.nameColor }
        text.justify = "LEFT"
        label(bar, text, entry.name, "LEFT", bar, "LEFT", 5 * PX, 0, on)
        text.justify = "RIGHT"
        label(bar, text, timeLeft(s), "RIGHT", bar, "RIGHT", -5 * PX, 0, on)
    end
end

-- The parts, made once: { p, unit, f, frames, w, h, ... }.
local parts = {}
ns.uiParts = parts -- (for the harness)
local built, pending = false, false

-- One part as things are now. A target part with no target, or a part its
-- state hides, is hidden (in game by alpha: see the top).
local function paint(e)
    local p, f, unit = e.p, e.f, e.unit
    local here = unit ~= "target" or (UnitExists and UnitExists("target")) or false
    local hp = here and value(unit, "health") or nil
    if hp then
        e.hp = hp
    end
    local on = states(unit, e.hp)
    local show = here and not pick(p.hidden, on)
    local alpha = pick(p.alpha, on) or 1
    if LIVE then
        f:SetAlpha(show and alpha or 0)
    else
        f:SetShown(show)
        f:SetAlpha(alpha)
    end
    if not show then
        return
    end
    if not locked() then
        local size = pick(p.size, on) or { 1, 1 }
        e.w, e.h = (size[1] or 1) * PX, (size[2] or 1) * PX
        f:SetSize(e.w, e.h)
    end
    if p.kind == "auras" then
        local list = auraList(unit, p)
        if not list then
            return
        end
        e.list = list
    end
    cur = e
    begin(e)
    DRAW[p.kind](f, p, unit, on, e)
    finish(e)
    cur = nil
end

local function paintAll(match)
    for _, e in ipairs(parts) do
        if not match or match(e) then
            paint(e)
        end
    end
end

-- Blizzard's player and target frames, and the ones the document names:
-- alpha 0 (only in game; a picture draws the scene's own way).
local function fade()
    if not LIVE then
        return
    end
    local names = { "PlayerFrame", "TargetFrame" }
    for _, n in ipairs(SPEC.hide or {}) do
        names[#names + 1] = n
    end
    for _, n in ipairs(names) do
        local b = rawget(_G, n)
        if type(b) == "table" and type(b.SetAlpha) == "function" then
            b:SetAlpha(0)
        end
    end
end

local timers = CreateFrame("Frame")
timers:Hide()
ns.uiTimers = timers -- (for the harness)

-- Every part's frame, made and anchored once, out of combat.
local function build()
    if built then
        return
    end
    if locked() then
        pending = true
        return
    end
    built, pending = true, false
    local timed = false
    for _, p in ipairs(SPEC.parts) do
        local f = CreateFrame("Frame", nil, UIParent)
        f:EnableMouse(false)
        f:SetFrameStrata("MEDIUM")
        f:SetFrameLevel(10 + (p.level or 0))
        local a = p.anchor or { "CENTER", "char" }
        local rel, relPoint, x, y = anchorOf(a)
        f:SetPoint(a[1], rel or UIParent, relPoint, x, y)
        if p.name then
            made[p.name] = f
        end
        local e = { p = p, unit = p.unit or "player", f = f, frames = { f }, w = PX, h = PX }
        parts[#parts + 1] = e
        timed = timed or p.kind == "auras"
        paint(e)
    end
    fade()
    if timed then
        timers:Show()
    end
end

-- The one OnUpdate: aura bars' time, five times a second.
local since = 0
timers:SetScript("OnUpdate", function(_, elapsed)
    since = since + (type(elapsed) == "number" and elapsed or 0)
    if since < 0.2 then
        return
    end
    since = 0
    paintAll(function(e)
        return e.p.kind == "auras"
    end)
end)

local function ofUnit(unit, kinds)
    return function(e)
        return e.unit == unit and (not kinds or kinds[e.p.kind])
    end
end
local AURA_KINDS = { auras = true, icon = true }

local ev = CreateFrame("Frame")
for _, e in ipairs({ "PLAYER_LOGIN", "PLAYER_TARGET_CHANGED", "PLAYER_REGEN_DISABLED", "PLAYER_REGEN_ENABLED",
    "UPDATE_STEALTH", "UNIT_HEALTH", "UNIT_MAXHEALTH", "UNIT_POWER_UPDATE", "UNIT_POWER_FREQUENT",
    "UNIT_MAXPOWER", "UNIT_DISPLAYPOWER", "UNIT_AURA" }) do
    pcall(ev.RegisterEvent, ev, e)
end
ev:SetScript("OnEvent", function(_, event, unit)
    if event == "PLAYER_REGEN_DISABLED" or event == "PLAYER_REGEN_ENABLED" then
        inCombat = event == "PLAYER_REGEN_DISABLED"
    end
    if event == "PLAYER_LOGIN" then
        build()
        return
    end
    if not built then
        -- (made at the end of combat when the login came in it, then TH3's
        -- buttons placed too)
        if event == "PLAYER_REGEN_ENABLED" and pending then
            build()
            if ns.barsApply and ns.uiBars then
                ns.barsApply()
            end
        end
        return
    end
    if unit ~= nil and unit ~= "player" and unit ~= "target" then
        return
    end
    if event == "UNIT_AURA" then
        paintAll(ofUnit(unit, AURA_KINDS))
    elseif event:match("^UNIT_") then
        paintAll(ofUnit(unit))
    else
        -- the target, combat or stealth: every part's states
        paintAll()
        -- (Blizzard's target frame may set its own alpha as it changes.)
        if event == "PLAYER_TARGET_CHANGED" then
            fade()
        end
    end
end)
