-- TH1 prototype: draws unit frames from a spec in the vocabulary
-- (context/specs/unitframes-vocabulary). Display only: our frames, mouse
-- off; on Blizzard's unit frames only alpha, out of combat.

local SPEC = FBUnitProtoSpec
local MEDIA = "Interface\\AddOns\\FBUnitProto\\Media\\"
-- one screen pixel in UI units (768 / screen height when the spec snaps;
-- bible 5: pixel-perfect lines), so a 1px border is really one pixel
local PX = SPEC.px or 1

local function hex(h, a)
    if type(h) == "table" then return h end
    return { tonumber(h:sub(2, 3), 16) / 255, tonumber(h:sub(4, 5), 16) / 255, tonumber(h:sub(6, 7), 16) / 255, a or 1 }
end

-- States, in order; the last one that holds wins (vocabulary §4).
local ORDER = { "always", "ooc", "combat", "full", "low", "stealth" }
local function states(frac)
    local on = { always = true }
    local combat = SPEC.forceCombat or (InCombatLockdown and InCombatLockdown())
    on.combat, on.ooc = combat, not combat
    on.stealth = SPEC.forceStealth or (IsStealthed and IsStealthed())
    if frac then on.full, on.low = frac >= 0.999, frac < (SPEC.lowAt or 0.35) end
    return on
end
local function pick(v, on)
    if type(v) ~= "table" or v[1] ~= nil or v.always == nil then return v end
    local out
    for _, s in ipairs(ORDER) do if on[s] and v[s] ~= nil then out = v[s] end end
    return out
end

-- Values, with the render's fixture fallbacks for what the scene lacks.
local function value(unit, what)
    local o = SPEC.override and SPEC.override[unit] and SPEC.override[unit][what]
    if o then return o, math.floor(o * 100) end
    local cur, max
    if what == "health" then cur, max = UnitHealth(unit), UnitHealthMax(unit)
    elseif what == "power" then cur, max = UnitPower(unit), UnitPowerMax(unit)
    elseif what == "combo" then cur, max = GetComboPoints("player", "target"), 5
        if (cur or 0) == 0 then cur = SPEC.comboFallback or 0 end
    end
    if issecretvalue and (issecretvalue(cur) or issecretvalue(max)) then return nil, nil end
    if not cur or not max or max == 0 then return 1, cur end
    return cur / max, cur
end

local function colorAt(stops, frac)
    if type(stops) == "string" then return hex(stops) end
    local best
    for _, s in ipairs(stops) do if frac <= s[1] then return hex(s[2]) end best = s end
    return hex(best[2])
end

local function format(fmt, unit)
    return (fmt:gsub("{(%w+%%?)}", function(tok)
        if tok == "name" then
            local n = UnitName(unit)
            if (n or "") == "" then n = (SPEC.names or {})[unit] or "" end
            return n
        end
        if tok == "level" then return tostring(UnitLevel(unit) or "") end
        if tok == "hp%" then local f = value(unit, "health"); return f and (math.floor(f * 100 + 0.5) .. "%") or "?" end
        if tok == "power" then return tostring(UnitPower(unit) or "") end
        if tok == "hpv" then
            local v = UnitHealth(unit)
            if issecretvalue and issecretvalue(v) then return "?" end
            v = v or 0
            if v >= 1000000 then return string.format("%.1f M", v / 1000000) end
            if v >= 1000 then return string.format("%.1f K", v / 1000) end
            return tostring(v)
        end
        if tok == "combo" then local _, c = value(unit, "combo"); return tostring(c or 0) end
        return ""
    end))
end

local parts = {}
local function anchorOf(unit, a)
    local to = a[2]
    if to == "char" then return UIParent, "CENTER", (a[4] or 0), (SPEC.charY or -40) + (a[5] or 0) end
    if to == "screen" or to == nil then return UIParent, a[3] or a[1], a[4] or 0, a[5] or 0 end
    return parts[unit .. "." .. to], a[3] or a[1], a[4] or 0, a[5] or 0
end

local function frame(unit, name, p, on)
    local f = CreateFrame("Frame", nil, UIParent)
    f:EnableMouse(false)
    f:SetFrameStrata(p.strata or "MEDIUM")
    f:SetFrameLevel(10 + (p.level or 0))
    local rel, relPoint, x, y = anchorOf(unit, p.anchor or { "CENTER", "char" })
    f:SetPoint(p.anchor and p.anchor[1] or "CENTER", rel, relPoint, x, y)
    f:SetSize(p.size and pick(p.size[1], on) or 1, p.size and pick(p.size[2], on) or 1)
    f:SetAlpha(pick(p.alpha, on) or 1)
    parts[unit .. "." .. name] = f
    return f
end

local function tex(f, art, layer, c, blend)
    local t = f:CreateTexture(nil, layer or "ARTWORK")
    if art and art ~= "flat" then t:SetTexture(MEDIA .. art) else t:SetColorTexture(1, 1, 1, 1) end
    if c then t:SetVertexColor(c[1], c[2], c[3], c[4] or 1) end
    if blend then t:SetBlendMode(blend) end
    return t
end

-- A soft bloom of colour c behind a point, additive.
local function bloom(f, c, x, y, size, alpha)
    local g = tex(f, "Glow", "BACKGROUND", { c[1], c[2], c[3], alpha }, "ADD")
    g:SetSize(size, size)
    g:SetPoint("CENTER", f, "CENTER", x, y)
    return g
end

-- Black copies of a text around it, r screen pixels out: a heavier stroke
-- than the client's outline flag draws in the emulator.
local function strokeCopies(f, font, size, text, point, rel, relPoint, x, y, r, justify)
    for dx = -r, r do
        for dy = -r, r do
            if (dx ~= 0 or dy ~= 0) and dx * dx + dy * dy <= r * r + r then
                local k = f:CreateFontString(nil, "OVERLAY", nil, 0)
                k:SetFont(font, size, "")
                k:SetShadowOffset(0, 0)
                k:SetTextColor(0, 0, 0, 1)
                k:SetJustifyH(justify or "CENTER")
                k:SetPoint(point, rel, relPoint, x + dx * PX, y + dy * PX)
                k:SetText(text)
            end
        end
    end
end

local DRAW = {}
function DRAW.plate(f, p, unit, on)
    local t = tex(f, pick(p.art, on), "BACKGROUND", hex(pick(p.color, on) or "#ffffff"))
    t:SetAllPoints()
    if p.flip then t:SetTexCoord(1, 0, 0, 1) end
    if p.gradient then
        t:SetGradient(p.gradient[1], CreateColor(unpack(hex(p.gradient[2]))), CreateColor(unpack(hex(p.gradient[3]))))
    end
    if p.border then
        local c = hex(p.border, p.borderAlpha)
        for _, e in ipairs({ { "TOPLEFT", "TOPRIGHT", true }, { "BOTTOMLEFT", "BOTTOMRIGHT", true },
            { "TOPLEFT", "BOTTOMLEFT", false }, { "TOPRIGHT", "BOTTOMRIGHT", false } }) do
            if not (p.edges and not p.edges[e[1] .. e[2]]) then
                local l = tex(f, nil, "BORDER", c)
                l:SetPoint(e[1]) l:SetPoint(e[2])
                if e[3] then l:SetHeight(PX) else l:SetWidth(PX) end
            end
        end
    end
end
DRAW.art = DRAW.plate
function DRAW.bar(f, p, unit, on)
    local frac = value(unit, p.value) or 1
    local back = tex(f, p.backArt, "BACKGROUND", hex(p.back or "#000000", p.backAlpha or 0.6))
    back:SetAllPoints()
    local fill = tex(f, pick(p.art, on), "ARTWORK", colorAt(pick(p.color, on), frac), p.blend)
    -- a gentle horizontal gradient: lighter on the left, deeper on the right
    if p.gradient then
        local c = colorAt(pick(p.color, on), frac)
        local function shade(k)
            if k >= 0 then return c[1] + (1 - c[1]) * k, c[2] + (1 - c[2]) * k, c[3] + (1 - c[3]) * k, 1 end
            return c[1] * (1 + k), c[2] * (1 + k), c[3] * (1 + k), 1
        end
        fill:SetColorTexture(1, 1, 1, 1)
        fill:SetGradient("HORIZONTAL", CreateColor(shade(p.gradient[1])), CreateColor(shade(p.gradient[2])))
    end
    if p.leadEdge and not p.vertical then
        local c = colorAt(pick(p.color, on), frac)
        local x = f:GetWidth() * frac - f:GetWidth() / 2
        bloom(f, c, x, 0, f:GetHeight() * 10, 0.5)
        local s = tex(f, "LeadEdge", "OVERLAY", { 1, 1, 1, 0.95 }, "ADD")
        s:SetSize(f:GetHeight() * 3, f:GetHeight() * 6) s:SetPoint("CENTER", f, "CENTER", x, 0)
    end
    if p.bloom and not p.vertical then
        local c = colorAt(pick(p.color, on), frac)
        local w = f:GetWidth() * frac
        for i = 0, 4 do
            bloom(f, c, -f:GetWidth() / 2 + w * (i + 0.5) / 5, 0, p.bloom, p.bloomAlpha or 0.12)
        end
    end
    if p.vertical then
        fill:SetPoint("BOTTOMLEFT") fill:SetPoint("BOTTOMRIGHT")
        fill:SetHeight(math.max(1, f:GetHeight() * frac))
    else
        fill:SetPoint("TOPLEFT") fill:SetPoint("BOTTOMLEFT")
        fill:SetWidth(math.max(1, f:GetWidth() * frac))
        -- crop the art instead of squeezing it, so a texture keeps its grain
        if pick(p.art, on) ~= "flat" then fill:SetTexCoord(0, frac, 0, 1) end
    end
    -- damage trail: the chunk just lost, lingering past the fill
    if p.trail and not p.vertical then
        local tr = tex(f, nil, "BORDER", hex(p.trailColor or "#ffffff", p.trailAlpha or 0.5))
        tr:SetPoint("TOPLEFT", fill, "TOPRIGHT") tr:SetPoint("BOTTOMLEFT", fill, "BOTTOMRIGHT")
        tr:SetWidth(math.max(1, f:GetWidth() * math.min(p.trail, 1 - frac)))
    end
    -- segment ticks: n-1 dark lines over the whole bar
    for i = 1, (p.ticks or 1) - 1 do
        local t = tex(f, nil, "OVERLAY", hex(p.tickColor or "#000000", p.tickAlpha or 0.7))
        t:SetWidth(p.tickWidth or 2)
        t:SetPoint("TOP", f, "TOPLEFT", f:GetWidth() * i / p.ticks, 0)
        t:SetPoint("BOTTOM", f, "BOTTOMLEFT", f:GetWidth() * i / p.ticks, 0)
    end
    if p.glow then
        local g = tex(f, nil, "BORDER", colorAt(pick(p.color, on), frac))
        g:SetPoint("TOPLEFT", fill, "TOPLEFT", -3, 3) g:SetPoint("BOTTOMRIGHT", fill, "BOTTOMRIGHT", 3, -3)
        g:SetAlpha(p.glow)
    end
    if p.border then
        local c = hex(p.border)
        for _, s in ipairs({ { "TOPLEFT", "TOPRIGHT", 1, nil }, { "BOTTOMLEFT", "BOTTOMRIGHT", 1, nil },
            { "TOPLEFT", "BOTTOMLEFT", nil, 1 }, { "TOPRIGHT", "BOTTOMRIGHT", nil, 1 } }) do
            local l = tex(f, nil, "OVERLAY", c)
            l:SetPoint(s[1]) l:SetPoint(s[2])
            if s[3] then l:SetHeight(PX) else l:SetWidth(PX) end
        end
    end
end
-- An arc of n segments around the anchor point, from a0 to a1 degrees
-- (0 is up, clockwise); segments up to the value are lit.
function DRAW.arc(f, p, unit, on)
    local frac = value(unit, p.value) or 1
    local n, r, a0, a1 = p.segments or 24, p.radius or 150, p.from or 200, p.to or 340
    local lit = math.floor(frac * n + 0.5)
    local c = colorAt(pick(p.color, on), frac)
    local lead
    for i = 1, n do
        local deg = a0 + (a1 - a0) * (i - 0.5) / n
        local th = math.rad(deg)
        local x, y = r * math.sin(th), r * math.cos(th)
        local k = p.reverse and (n - i + 1) or i
        local on_ = k <= lit
        if on_ and k == lit then lead = { x, y, th } end
        -- feather: segments fade out toward both ends of the arc
        local fade = 1
        if p.feather then
            local edge = math.min(i - 0.5, n - i + 0.5) / (n * p.feather)
            fade = math.min(1, edge) ^ 1.5
        end
        if on_ and p.glow then
            local g = tex(f, p.lit and "TickLit" or "TickGlow", "BORDER", { c[1], c[2], c[3], p.glow * fade }, p.lit and "ADD")
            g:SetSize((p.thick or 10) * (p.lit and 3.2 or 2), (p.len or 18) * 1.8) g:SetPoint("CENTER", f, "CENTER", x, y)
            g:SetRotation(-th)
        end
        local ec = hex(p.empty or "#ffffff", (p.emptyAlpha or 0.12) * fade)
        local t = tex(f, p.lit and "TickLit" or "Tick", "ARTWORK",
            on_ and { c[1], c[2], c[3], fade } or ec, on_ and p.lit and "ADD")
        t:SetSize(p.thick or 10, p.len or 18)
        t:SetPoint("CENTER", f, "CENTER", x, y)
        t:SetRotation(-th)
    end
    -- bloom along the lit part, and a bright spark at the leading segment
    if p.bloom and lit > 0 then
        for i = 1, lit, math.max(1, math.floor(n / 8)) do
            local k = p.reverse and (n - i + 1) or i
            local th = math.rad(a0 + (a1 - a0) * (k - 0.5) / n)
            bloom(f, c, r * math.sin(th), r * math.cos(th), p.bloom, p.bloomAlpha or 0.18)
        end
    end
    if p.leadEdge and lead then
        bloom(f, c, lead[1], lead[2], (p.len or 18) * 3, 0.55)
        local s = tex(f, "LeadEdge", "OVERLAY", { 1, 1, 1, 0.9 }, "ADD")
        s:SetSize((p.thick or 10) * 2.4, (p.len or 18) * 1.6)
        s:SetPoint("CENTER", f, "CENTER", lead[1], lead[2]) s:SetRotation(-lead[3])
    end
end
function DRAW.pips(f, p, unit, on)
    local _, cur = value(unit, "combo")
    local n = p.count or 5
    local c = hex(pick(p.color, on) or "#ffffff")
    for i = 1, n do
        local x, y, rot = (i - (n + 1) / 2) * (p.spacing or 20), 0, 0
        if p.layout == "arc" then
            local th = math.rad((p.from or 160) + ((p.to or 200) - (p.from or 160)) * (i - 1) / (n - 1))
            x, y, rot = (p.radius or 150) * math.sin(th), (p.radius or 150) * math.cos(th), -th
        end
        local lit = i <= (cur or 0)
        if lit and p.glowArt then
            local g = tex(f, p.glowArt, "BORDER", { c[1], c[2], c[3], p.glowAlpha or 0.8 }, p.blend)
            g:SetSize(p.glowW or p.w * 1.3, p.glowH or p.h * 1.25) g:SetPoint("CENTER", f, "CENTER", x, y) g:SetRotation(rot)
        end
        if p.back then
            local b = tex(f, nil, "BACKGROUND", hex(p.back, p.backAlpha or 1))
            b:SetSize(p.w or 10, p.h or 10) b:SetPoint("CENTER", f, "CENTER", x, y)
        end
        local t = tex(f, lit and p.art or (p.emptyArt or p.art), "ARTWORK",
            lit and c or hex(p.emptyColor or "#ffffff", p.emptyAlpha or 0.35), lit and p.blend)
        t:SetSize(p.w or 10, p.h or 10)
        t:SetPoint("CENTER", f, "CENTER", x, y)
        t:SetRotation(rot)
        if p.border then
            local bc = hex(p.border)
            local hw, hh = (p.w or 10) / 2, (p.h or 10) / 2
            for _, e in ipairs({ { 0, hh, p.w, PX }, { 0, -hh, p.w, PX }, { -hw, 0, PX, p.h }, { hw, 0, PX, p.h } }) do
                local l = tex(f, nil, "OVERLAY", bc)
                l:SetSize(e[3], e[4])
                l:SetPoint("CENTER", f, "CENTER", x + e[1] + (e[1] < 0 and PX / 2 or e[1] > 0 and -PX / 2 or 0),
                    y + e[2] + (e[2] < 0 and PX / 2 or e[2] > 0 and -PX / 2 or 0))
            end
        end
    end
end
function DRAW.text(f, p, unit, on)
    -- a heavier stroke: black copies around the text, p.stroke screen pixels out
    if p.stroke then
        local pt = p.justify or "CENTER"
        strokeCopies(f, p.font and (MEDIA .. p.font) or "Fonts\\FRIZQT__.TTF", p.fontSize or 12,
            format(p.format, unit), pt, f, pt, 0, 0, p.stroke, p.justify)
    end
    local s = f:CreateFontString(nil, "OVERLAY", nil, 7)
    s:SetFont(p.font and (MEDIA .. p.font) or "Fonts\\FRIZQT__.TTF", p.fontSize or 12, p.outline or "")
    s:SetShadowOffset(0, 0)
    if p.shadow then
        s:SetShadowColor(0, 0, 0, p.shadow)
        s:SetShadowOffset(1, -1)
    end
    local c = hex(pick(p.color, on) or "#ffffff")
    s:SetTextColor(c[1], c[2], c[3], c[4] or 1)
    s:SetJustifyH(p.justify or "CENTER")
    s:SetPoint(p.justify or "CENTER")
    s:SetText(format(p.format, unit))
end

-- Action bar overlay: redraws the real spells as trimmed icons in our own
-- frames, mouse off, over Blizzard's buttons (which go to alpha 0 and keep
-- taking the clicks).
function DRAW.actionbar(f, p, unit, on)
    local n, sz, gap = #p.slots, p.icon or 40, p.gap or 4
    f:SetSize(n * sz + (n - 1) * gap, sz)
    for i, slot in ipairs(p.slots) do
        local x = (i - 1) * (sz + gap)
        if p.plate then
            local pl = tex(f, nil, "BACKGROUND", hex(p.plate, p.plateAlpha or 0.6))
            pl:SetPoint("TOPLEFT", x - 1, 1) pl:SetSize(sz + 2, sz + 2)
        end
        local icon = f:CreateTexture(nil, "ARTWORK")
        local file = GetActionTexture(slot)
        if file then icon:SetTexture(file) else icon:SetColorTexture(0, 0, 0, 0.4) end
        local z = p.zoom or 0.08
        icon:SetTexCoord(z, 1 - z, z, 1 - z)
        icon:SetPoint("TOPLEFT", x, 0) icon:SetSize(sz, sz)
        if p.desaturate then icon:SetDesaturated(true) icon:SetAlpha(p.iconAlpha or 0.85) end
        if p.border then
            local c = hex(p.border, p.borderAlpha or 1)
            for _, e in ipairs({ { "TOPLEFT", "TOPRIGHT", true }, { "BOTTOMLEFT", "BOTTOMRIGHT", true },
                { "TOPLEFT", "BOTTOMLEFT", false }, { "TOPRIGHT", "BOTTOMRIGHT", false } }) do
                local l = tex(f, nil, "OVERLAY", c)
                l:SetPoint(e[1], icon, e[1]) l:SetPoint(e[2], icon, e[2])
                if e[3] then l:SetHeight((p.borderWidth or 1) * PX) else l:SetWidth((p.borderWidth or 1) * PX) end
            end
        end
        -- proc: a gold inner border and a soft glow on an ability that's ready to use
        if p.procs and p.procs[slot] then
            local pc = hex(p.procColor or "#ffd23c")
            local g = tex(f, "Glow", "OVERLAY", { pc[1], pc[2], pc[3], 0.55 }, "ADD")
            g:SetPoint("CENTER", icon) g:SetSize(sz * 1.5, sz * 1.5)
            for _, e in ipairs({ { "TOPLEFT", "TOPRIGHT", true }, { "BOTTOMLEFT", "BOTTOMRIGHT", true },
                { "TOPLEFT", "BOTTOMLEFT", false }, { "TOPRIGHT", "BOTTOMRIGHT", false } }) do
                local l = tex(f, nil, "OVERLAY", pc)
                l:SetPoint(e[1], icon, e[1]) l:SetPoint(e[2], icon, e[2])
                if e[3] then l:SetHeight(2 * PX) else l:SetWidth(2 * PX) end
            end
        end
        -- stacks: a count bottom right
        local st = p.stacks and p.stacks[slot]
        if st then
            local font = MEDIA .. (p.font or "Geist.ttf")
            if p.stroke then strokeCopies(f, font, p.stackSize or 14, st, "BOTTOMRIGHT", icon, "BOTTOMRIGHT", -2 * PX, 2 * PX, p.stroke, "RIGHT") end
            local t = f:CreateFontString(nil, "OVERLAY", nil, 7)
            t:SetFont(font, p.stackSize or 14, "OUTLINE")
            t:SetShadowOffset(0, 0)
            t:SetTextColor(1, 1, 1, 1)
            t:SetPoint("BOTTOMRIGHT", icon, "BOTTOMRIGHT", -2 * PX, 2 * PX) t:SetText(st)
        end
        local cd = p.cooldowns and p.cooldowns[slot]
        if cd then
            local shade = tex(f, nil, "OVERLAY", hex("#000000", 0.62))
            shade:SetPoint("TOPLEFT", icon) shade:SetPoint("TOPRIGHT", icon) shade:SetHeight(sz * cd[1])
            local font = MEDIA .. (p.font or "Geist.ttf")
            if p.stroke then strokeCopies(f, font, p.cdSize or 14, cd[2], "CENTER", icon, "CENTER", 0, 0, p.stroke) end
            local t = f:CreateFontString(nil, "OVERLAY", nil, 7)
            t:SetFont(font, p.cdSize or 14, "OUTLINE")
            t:SetShadowOffset(0, 0)
            t:SetTextColor(1, 1, 1, 1)
            t:SetPoint("CENTER", icon) t:SetText(cd[2])
        end
        if p.keys then
            local key = GetBindingKey(p.binds and p.binds[i] or (p.bind .. (p.bindIndex and p.bindIndex[i] or i)))
            if key then
                local label = (key:gsub("SHIFT%-", "s"):gsub("`", "~"))
                local font = MEDIA .. (p.font or "Geist.ttf")
                local pt, rp, kx, ky = "TOPRIGHT", "TOPRIGHT", -2, -2
                if p.keysBelow then pt, rp, kx, ky = "TOP", "BOTTOM", 0, -3 end
                if p.stroke then strokeCopies(f, font, p.keySize or 10, label, pt, icon, rp, kx, ky, p.stroke) end
                local k = f:CreateFontString(nil, "OVERLAY", nil, 7)
                k:SetFont(font, p.keySize or 10, p.keyOutline or "OUTLINE")
                k:SetShadowOffset(0, 0)
                local kc = hex(p.keyColor or "#ffffff")
                k:SetTextColor(kc[1], kc[2], kc[3], kc[4] or 1)
                k:SetPoint(pt, icon, rp, kx, ky)
                k:SetText(label)
            end
        end
    end
end

-- An icon: an action slot's texture (or a file), trimmed, with a 1px border.
function DRAW.icon(f, p, unit, on)
    local i = f:CreateTexture(nil, "ARTWORK")
    local file = p.slot and GetActionTexture(p.slot) or p.file
    if file then i:SetTexture(file) else i:SetColorTexture(0, 0, 0, 0.4) end
    i:SetTexCoord(0.08, 0.92, 0.08, 0.92)
    i:SetAllPoints()
    if p.border then
        local c = hex(p.border)
        for _, e in ipairs({ { "TOPLEFT", "TOPRIGHT", true }, { "BOTTOMLEFT", "BOTTOMRIGHT", true },
            { "TOPLEFT", "BOTTOMLEFT", false }, { "TOPRIGHT", "BOTTOMRIGHT", false } }) do
            local l = tex(f, nil, "OVERLAY", c)
            l:SetPoint(e[1]) l:SetPoint(e[2])
            if e[3] then l:SetHeight(PX) else l:SetWidth(PX) end
        end
    end
end

-- A minimap frame: square plate with a 1px edge, or a ring; zone text.
function DRAW.minimap(f, p, unit, on)
    if p.shape == "ring" then
        if p.mapArt then
            local m = tex(f, p.mapArt, "BACKGROUND", hex("#ffffff", p.mapAlpha or 1))
            m:SetPoint("TOPLEFT", 6, -6) m:SetPoint("BOTTOMRIGHT", -6, 6)
            local you = tex(f, "Glow", "ARTWORK", hex(p.youColor or "#ffffff", 1), "ADD")
            you:SetSize(18, 18) you:SetPoint("CENTER")
        end
        tex(f, "Ring", "ARTWORK", hex(p.edge or "#ffffff", p.edgeAlpha or 0.8)):SetAllPoints()
    else
        local fill = tex(f, nil, "BACKGROUND", hex(p.fill or "#000000", p.fillAlpha or 0.5))
        fill:SetAllPoints()
        fill:SetDrawLayer("BACKGROUND", -8)
        if p.mapArt then
            local m = tex(f, p.mapArt, "BACKGROUND", hex("#ffffff", p.mapAlpha or 1))
            m:SetAllPoints()
            if p.mapCrop then m:SetTexCoord(unpack(p.mapCrop)) end
            local you = tex(f, nil, "OVERLAY", hex(p.youColor or "#ffffff", 1))
            you:SetSize(4 * PX, 4 * PX) you:SetPoint("CENTER")
        end
        local c = hex(p.edge or "#ffffff", p.edgeAlpha or 0.2)
        for _, e in ipairs({ { "TOPLEFT", "TOPRIGHT", true }, { "BOTTOMLEFT", "BOTTOMRIGHT", true },
            { "TOPLEFT", "BOTTOMLEFT", false }, { "TOPRIGHT", "BOTTOMRIGHT", false } }) do
            local l = tex(f, nil, "BORDER", c)
            l:SetPoint(e[1]) l:SetPoint(e[2])
            if e[3] then l:SetHeight(PX) else l:SetWidth(PX) end
        end
    end
    if p.zone then
        local z = f:CreateFontString(nil, "OVERLAY")
        z:SetFont(MEDIA .. (p.font or "Geist.ttf"), p.zoneSize or 12, p.zoneOutline or "")
        z:SetShadowOffset(0, 0)
        local zc = hex(p.zoneColor or "#e8ecf2")
        z:SetTextColor(zc[1], zc[2], zc[3], 1)
        z:SetPoint(p.zoneAt or "TOP", f, p.zoneTo or "BOTTOM", 0, p.zoneY or -6)
        z:SetText(GetZoneText and GetZoneText() ~= "" and GetZoneText() or p.zone)
    end
end

-- Build a unit's parts; the target's are rebuilt when the target changes
-- (a prototype shortcut: the real module pools and refreshes, bible 4.19).
local built = {}
local function build(u)
    for _, f in ipairs(built[u] or {}) do f:Hide() end
    built[u] = {}
    if u.unit == "target" and not UnitExists("target") then return end
    local blizz = ({ player = PlayerFrame, target = TargetFrame })[u.unit]
    if blizz and u.hideBlizzard ~= false and not InCombatLockdown() then blizz:SetAlpha(0) end
    local on = states(value(u.unit, "health"))
    for _, entry in ipairs(u.parts) do
        local name, p = entry[1], entry[2]
        local f = frame(u.unit, name, p, on)
        DRAW[p.kind](f, p, u.unit, on)
        table.insert(built[u], f)
    end
end

local ev = CreateFrame("Frame")
ev:RegisterEvent("PLAYER_LOGIN")
ev:RegisterEvent("PLAYER_TARGET_CHANGED")
ev:SetScript("OnEvent", function(_, event)
    for _, u in ipairs(SPEC.units) do
        if event == "PLAYER_LOGIN" or u.unit == "target" then build(u) end
    end
    if event ~= "PLAYER_LOGIN" then return end
    -- Blizzard's default pieces this spec replaces: alpha only, out of combat
    for _, name in ipairs(SPEC.hide or {}) do
        local b = _G[name]
        if b and not InCombatLockdown() then b:SetAlpha(0) end
    end
    for _, a in ipairs(SPEC.backdrop or {}) do
        local f = frame("screen", a.name, a, states())
        DRAW[a.kind](f, a, a.unit or "player", states())
    end
end)
