-- In-game window, our own look (design prototype, not shipped code).
-- The desktop app's D language carried into the game: stone, stitched
-- hide, a parchment page, the red ribbon, bronze, Marcellus and Geist.
-- Art and fonts are addon media: design/ingame/media, the TGAs made by
-- make.py from _d.css's tokens (python3 make.py <out dir>, needs Pillow and
-- numpy). Marcellus and Geist are OFL; their licences sit beside them.
-- To run it in addon-live: copy media/* into the addon's Media/Proto, then
--   luajit tools/addon-live/live.lua start --art
--   luajit tools/addon-live/live.lua lua "FBProtoPage = 'map'"   (or 'plan')
--   luajit tools/addon-live/live.lua lua "$(grep -v '^[[:space:]]*--' design/ingame/own-look.lua)"
-- Comment lines are stripped before sending, so keep comments on their own
-- lines.
-- Bible ch5: our own frames, one STYLE table, builders return (frame, height).

local MEDIA = "Interface\\AddOns\\ForeverBuddy\\Media\\Proto\\"

local function hex(h, a)
    return { tonumber(h:sub(2, 3), 16) / 255, tonumber(h:sub(4, 5), 16) / 255, tonumber(h:sub(6, 7), 16) / 255, a or 1 }
end

local STYLE = {
    head = MEDIA .. "Marcellus.ttf",
    body = MEDIA .. "Geist.ttf",
    chalk = hex("#ddd5c7"), chalkHi = hex("#f1e8da"), ash = hex("#9a9183"), soot = hex("#6a6358"),
    hideText = hex("#cdbfa9"), hideMute = hex("#8f7d66"), hideDim = hex("#7d6c58"),
    ink = hex("#3a2616"), ink2 = hex("#6e5537"), ink3 = hex("#9a8162"),
    inkRule = hex("#3a2616", 0.55), inkHair = hex("#3a2616", 0.16), margin = hex("#aa281e", 0.35),
    wax = hex("#8e1a16"), ember = hex("#ff7a2e"), ember2 = hex("#ffb15c"),
    bronzeHi = hex("#c9a26a"), bronze = hex("#8a6a3e"), bronzeLo = hex("#4e3a20"),
    okInk = hex("#2f6b1e"), ok = hex("#9fc27a"),
    line = { 1, 1, 1, 0.07 }, black = { 0, 0, 0, 0.6 },
}

local P = {}
FBProto = P

local function rgba(c, a)
    return c[1], c[2], c[3], a or c[4]
end

function P.fill(parent, c, layer)
    local t = parent:CreateTexture(nil, layer or "BACKGROUND")
    t:SetColorTexture(rgba(c))
    return t
end

function P.art(parent, name, layer, l, r, t, b)
    local x = parent:CreateTexture(nil, layer or "BACKGROUND")
    x:SetTexture(MEDIA .. name)
    if l then x:SetTexCoord(l, r, t, b) end
    return x
end

function P.border(f, c, layer)
    local function strip()
        local t = f:CreateTexture(nil, layer or "BORDER")
        t:SetColorTexture(rgba(c))
        return t
    end
    local t, b, l, r = strip(), strip(), strip(), strip()
    t:SetPoint("TOPLEFT") t:SetPoint("TOPRIGHT") t:SetHeight(1)
    b:SetPoint("BOTTOMLEFT") b:SetPoint("BOTTOMRIGHT") b:SetHeight(1)
    l:SetPoint("TOPLEFT") l:SetPoint("BOTTOMLEFT") l:SetWidth(1)
    r:SetPoint("TOPRIGHT") r:SetPoint("BOTTOMRIGHT") r:SetWidth(1)
end

function P.text(parent, font, size, c, justify)
    local s = parent:CreateFontString(nil, "OVERLAY")
    s:SetFont(font, size, "")
    s:SetTextColor(rgba(c))
    s:SetJustifyH(justify or "LEFT")
    s:SetShadowOffset(0, 0)
    return s
end

function P.diamond(parent, c, size)
    local d = parent:CreateTexture(nil, "ARTWORK")
    d:SetTexture("Interface\\Buttons\\WHITE8X8")
    d:SetVertexColor(rgba(c))
    d:SetSize(size, size)
    d:SetRotation(math.rad(45))
    return d
end

function P.hrule(parent, y, x1, x2, c)
    local t = P.fill(parent, c or STYLE.line, "BORDER")
    t:SetPoint("TOPLEFT", x1 or 0, y)
    t:SetPoint("TOPRIGHT", x2 or 0, y)
    t:SetHeight(1)
    return t
end

-- The one primary button: bronze, rivets baked in.
function P.bronze(parent, label, w)
    local b = CreateFrame("Button", nil, parent)
    b:SetSize(w, 28)
    P.art(b, "Bronze", "BACKGROUND"):SetAllPoints()
    local s = P.text(b, STYLE.body, 12, STYLE.chalkHi, "CENTER")
    s:SetPoint("CENTER", 0, 0)
    s:SetText(label)
    return b
end

-- A quiet button on stone: hairline, chalk text.
function P.ghost(parent, label, w)
    local b = CreateFrame("Button", nil, parent)
    b:SetSize(w, 26)
    P.fill(b, { 0, 0, 0, 0.25 }):SetAllPoints()
    P.border(b, hex("#c9a26a", 0.28))
    local s = P.text(b, STYLE.body, 11, STYLE.chalk, "CENTER")
    s:SetPoint("CENTER", 0, 0)
    s:SetText(label)
    return b
end

-- A switch on stone: bronze when on; get/set bound, Refresh redraws.
function P.switch(parent, label, get, set)
    local f = CreateFrame("Button", nil, parent)
    f:SetSize(label ~= "" and 220 or 28, 16)
    local track = P.fill(f, STYLE.soot, "ARTWORK")
    track:SetSize(28, 14)
    track:SetPoint("LEFT", 0, 0)
    local knob = P.fill(f, STYLE.chalk, "OVERLAY")
    knob:SetSize(10, 10)
    local s = P.text(f, STYLE.body, 11, STYLE.chalk)
    s:SetPoint("LEFT", track, "RIGHT", 9, 0)
    s:SetText(label)
    function f.Refresh()
        local on = get()
        track:SetColorTexture(rgba(on and STYLE.bronze or hex("#000000", 0.45)))
        knob:SetColorTexture(rgba(on and STYLE.chalkHi or STYLE.ash))
        knob:ClearAllPoints()
        knob:SetPoint("LEFT", track, "LEFT", on and 16 or 2, 0)
    end
    f:SetScript("OnClick", function() set(not get()) f.Refresh() end)
    f.Refresh()
    return f, 16
end

-- A parchment page with a shadow and the red margin line.
function P.page(parent, marginX)
    local f = CreateFrame("Frame", nil, parent)
    local sh = P.fill(f, { 0, 0, 0, 0.55 }, "BACKGROUND")
    sh:SetPoint("TOPLEFT", 2, -3) sh:SetPoint("BOTTOMRIGHT", 3, -4)
    local paper = P.art(f, "Paper", "BACKGROUND")
    paper:SetAllPoints()
    P.border(f, hex("#3a2616", 0.35))
    if marginX then
        local m = P.fill(f, STYLE.margin, "BORDER")
        m:SetPoint("TOPLEFT", marginX, -1) m:SetPoint("BOTTOMLEFT", marginX, 1) m:SetWidth(1)
    end
    return f
end

-- One plan step on the page. state: done, current, next.
function P.step(parent, n, title, where, state)
    local row = CreateFrame("Frame", nil, parent)
    row:SetHeight(44)
    P.hrule(row, -43, 0, 0, STYLE.inkHair)
    local box = CreateFrame("Frame", nil, row)
    box:SetSize(20, 20)
    box:SetPoint("LEFT", 14, 0)
    if state == "current" then
        P.fill(box, STYLE.wax):SetAllPoints()
    else
        P.border(box, state == "done" and STYLE.inkHair or STYLE.inkRule)
    end
    if state == "done" then
        for _, s in ipairs({ { 5, -45, -3, -1 }, { 10, 55, 2, 1 } }) do
            local tick = box:CreateTexture(nil, "ARTWORK")
            tick:SetTexture("Interface\\Buttons\\WHITE8X8")
            tick:SetVertexColor(rgba(STYLE.okInk))
            tick:SetSize(s[1], 2)
            tick:SetPoint("CENTER", s[3], s[4])
            tick:SetRotation(math.rad(s[2]))
        end
    else
        local num = P.text(box, STYLE.head, 12, state == "current" and STYLE.chalkHi or STYLE.ink, "CENTER")
        num:SetPoint("CENTER", 0, 0)
        num:SetText(tostring(n))
    end
    local t = P.text(row, STYLE.body, 13, state == "done" and STYLE.ink3 or state == "current" and STYLE.wax or STYLE.ink)
    t:SetPoint("TOPLEFT", box, "TOPRIGHT", 22, 3)
    t:SetText(title)
    local w = P.text(row, STYLE.body, 11, STYLE.ink3)
    w:SetPoint("TOPLEFT", t, "BOTTOMLEFT", 0, -4)
    w:SetText(where)
    if state ~= "done" then
        local link = P.text(row, STYLE.body, 11, STYLE.wax, "RIGHT")
        link:SetPoint("RIGHT", -16, 0)
        link:SetText("Show on map  >")
    end
    return row, 44
end

-- A dark card with a hide header strip, like the desktop's side panels.
function P.card(parent, title, h)
    local f = CreateFrame("Frame", nil, parent)
    f:SetHeight(h)
    local sh = P.fill(f, { 0, 0, 0, 0.5 }, "BACKGROUND")
    sh:SetPoint("TOPLEFT", 1, -2) sh:SetPoint("BOTTOMRIGHT", 2, -3)
    P.art(f, "Card", "BACKGROUND"):SetAllPoints()
    local strip = P.art(f, "Hide", "BORDER", 0.05, 0.5, 0.3, 0.36)
    strip:SetPoint("TOPLEFT") strip:SetPoint("TOPRIGHT") strip:SetHeight(32)
    P.hrule(f, -32, 0, 0, hex("#000000", 0.5))
    P.hrule(f, -33, 0, 0, hex("#c9a26a", 0.12))
    P.border(f, hex("#000000", 0.55), "ARTWORK")
    local t = P.text(f, STYLE.body, 13, STYLE.chalkHi)
    t:SetPoint("TOPLEFT", 14, -10)
    t:SetText(title)
    return f
end

-- An option row inside a card: switch, label, help under it.
function P.option(parent, label, help, on)
    local f = CreateFrame("Frame", nil, parent)
    f:SetHeight(help and 42 or 26)
    local sw = P.switch(f, "", function() return on end, function(v) on = v end)
    sw:SetPoint("TOPLEFT", 0, 0)
    local s = P.text(f, STYLE.body, 12, STYLE.chalkHi)
    s:SetPoint("TOPLEFT", 40, -1)
    s:SetText(label)
    if help then
        local h = P.text(f, STYLE.body, 11, STYLE.ash)
        h:SetPoint("TOPLEFT", s, "BOTTOMLEFT", 0, -5)
        h:SetText(help)
    end
    return f, help and 42 or 26
end

-- A slider with a number box beside it.
function P.slider(parent, label, value, shown)
    local f = CreateFrame("Frame", nil, parent)
    f:SetHeight(42)
    local s = P.text(f, STYLE.body, 11, STYLE.ash)
    s:SetPoint("TOPLEFT", 0, 0)
    s:SetText(label)
    local track = P.fill(f, hex("#000000", 0.45), "ARTWORK")
    track:SetPoint("TOPLEFT", 0, -27)
    track:SetPoint("TOPRIGHT", -62, -27)
    track:SetHeight(4)
    local fill = P.fill(f, STYLE.bronze, "OVERLAY")
    fill:SetPoint("TOPLEFT", track, "TOPLEFT")
    fill:SetHeight(4)
    local thumb = P.diamond(f, STYLE.bronzeHi, 10)
    thumb:SetDrawLayer("OVERLAY", 2)
    local box = CreateFrame("Frame", nil, f)
    box:SetSize(50, 24)
    box:SetPoint("TOPRIGHT", 0, -17)
    P.fill(box, hex("#000000", 0.35)):SetAllPoints()
    P.border(box, hex("#c9a26a", 0.28))
    local n = P.text(box, STYLE.body, 12, STYLE.chalkHi, "CENTER")
    n:SetPoint("CENTER")
    function f.Refresh()
        local w = (f:GetWidth() - 62) * (value - 0.1) / 0.8
        fill:SetWidth(math.max(w, 1))
        thumb:ClearAllPoints()
        thumb:SetPoint("CENTER", track, "LEFT", w, 0)
        n:SetText(shown(value))
    end
    return f, 42
end

P.LAYOUT = { W = 620, H = 468, RAIL = 148, RAILH = 452, HEAD = 78, FOOT = 50 }

P.RAIL = {
    { group = "Tonight" },
    { "plan", "Plan", "2" }, { "lists", "Lists", "3" }, { "errands", "Errands", "1" }, { "cleanup", "Cleanup", "" },
    { group = "Modules" },
    { "bags", "Bags", on = true }, { "map", "Map", on = true }, { "quests", "Quests", on = false },
    { "tooltips", "Tooltips", on = true }, { "coach", "Coach", on = false },
}

function P.build(page)
    local L = P.LAYOUT
    local W, H, RAIL, HEAD, FOOT = L.W, L.H, L.RAIL, L.HEAD, L.FOOT

    local win = CreateFrame("Frame", "FBProtoWindow", UIParent)
    win:SetSize(W, H)
    win:SetPoint("CENTER", 0, 40)
    win:SetFrameStrata("DIALOG")
    local drop = P.fill(win, { 0, 0, 0, 0.45 }, "BACKGROUND")
    drop:SetPoint("TOPLEFT", -3, 2) drop:SetPoint("BOTTOMRIGHT", 4, -6)
    P.art(win, "Stone", "BACKGROUND"):SetAllPoints()
    P.border(win, hex("#000000", 0.8))
    local inner = CreateFrame("Frame", nil, win)
    inner:SetPoint("TOPLEFT", 1, -1) inner:SetPoint("BOTTOMRIGHT", -1, 1)
    P.border(inner, hex("#c9a26a", 0.14))

    -- the rail: stitched hide, inset like the desktop sidebar
    local rail = CreateFrame("Frame", nil, win)
    rail:SetSize(RAIL, L.RAILH)
    rail:SetPoint("TOPLEFT", 8, -8)
    P.art(rail, "Hide", "BACKGROUND", 0, RAIL / 256, 0, L.RAILH / 512):SetAllPoints()
    P.border(rail, hex("#000000", 0.55))

    local icon = rail:CreateTexture(nil, "ARTWORK")
    icon:SetTexture("Interface\\AddOns\\ForeverBuddy\\Media\\Icon")
    icon:SetSize(28, 28)
    icon:SetPoint("TOPLEFT", 14, -16)
    local name = P.text(rail, STYLE.head, 15, STYLE.chalkHi)
    name:SetPoint("TOPLEFT", icon, "TOPRIGHT", 8, -1)
    name:SetText("Forever Buddy")
    local who = P.text(rail, STYLE.body, 10, STYLE.hideMute)
    who:SetPoint("TOPLEFT", name, "BOTTOMLEFT", 0, -3)
    who:SetText("Thrandor, level 12")

    local y = -58
    for _, it in ipairs(P.RAIL) do
        if it.group then
            local g = P.text(rail, STYLE.body, 10, STYLE.hideDim)
            g:SetPoint("TOPLEFT", 16, y - 12)
            g:SetText(it.group)
            y = y - 30
        else
            local b = CreateFrame("Button", nil, rail)
            b:SetSize(RAIL, 28)
            b:SetPoint("TOPLEFT", 0, y)
            local here = it[1] == page.id
            if here then
                local rib = P.art(b, "Ribbon", "ARTWORK", 0, 1, 0, 1)
                rib:SetPoint("TOPLEFT", -4, 0)
                rib:SetPoint("BOTTOMRIGHT", 8, 0)
                rib:SetTexCoord(0.3, 1, 0, 1)
            end
            local off = it.on == false
            local s = P.text(b, STYLE.body, 12, here and STYLE.chalkHi or off and STYLE.hideDim or STYLE.hideText)
            s:SetPoint("LEFT", 18, 0)
            s:SetText(it[2])
            if it.on ~= nil then
                local d = P.diamond(b, it.on and STYLE.ember or STYLE.hideDim, 6)
                d:SetDrawLayer("OVERLAY")
                d:SetPoint("RIGHT", -18, 0)
            elseif it[3] ~= "" then
                local c = P.text(b, STYLE.body, 11, here and STYLE.chalkHi or STYLE.hideMute, "RIGHT")
                c:SetPoint("RIGHT", -16, 0)
                c:SetText(it[3])
            end
            y = y - 28
        end
    end
    P.hrule(rail, -(L.RAILH - 46), 10, -10, hex("#000000", 0.35))
    P.hrule(rail, -(L.RAILH - 45), 10, -10, hex("#d6b886", 0.10))
    local set = P.text(rail, STYLE.body, 12, STYLE.hideText)
    set:SetPoint("BOTTOMLEFT", 18, 17)
    set:SetText("Settings")

    -- content header on stone
    local cx = 8 + RAIL + 20
    local title = P.text(win, STYLE.head, 24, STYLE.chalkHi)
    title:SetPoint("TOPLEFT", cx, -20)
    title:SetText(page.title)
    local sub = P.text(win, STYLE.body, 11, STYLE.ash)
    sub:SetPoint("TOPLEFT", title, "BOTTOMLEFT", 0, -6)
    sub:SetText(page.sub)
    P.hrule(win, -HEAD + 8, cx, -20, hex("#000000", 0.45))
    P.hrule(win, -HEAD + 7, cx, -20, STYLE.line)

    local close = CreateFrame("Button", nil, win)
    close:SetSize(24, 24)
    close:SetPoint("TOPRIGHT", -14, -18)
    for _, deg in ipairs({ 45, -45 }) do
        local x = close:CreateTexture(nil, "ARTWORK")
        x:SetTexture("Interface\\Buttons\\WHITE8X8")
        x:SetVertexColor(rgba(STYLE.ash))
        x:SetSize(14, 1.5)
        x:SetPoint("CENTER")
        x:SetRotation(math.rad(deg))
    end

    page.draw(win, cx, L)
    return win
end

P.pages = {}

P.pages.plan = { id = "plan", title = "Tonight's plan", sub = "From the app at 09:53. Elwynn Forest, about an hour." }
function P.pages.plan.draw(win, cx, L)
    local W, H, HEAD, FOOT = L.W, L.H, L.HEAD, L.FOOT
    local pg = P.page(win, 30)
    pg:SetPoint("TOPLEFT", cx, -HEAD - 6)
    pg:SetPoint("BOTTOMRIGHT", -20, FOOT + 8)

    local head = P.text(pg, STYLE.head, 16, STYLE.ink)
    head:SetPoint("TOPLEFT", 44, -14)
    head:SetText("Wanted: Hogger")
    local count = P.text(pg, STYLE.body, 11, STYLE.ink3, "RIGHT")
    count:SetPoint("TOPRIGHT", -16, -17)
    count:SetText("1 of 3 done")
    P.hrule(pg, -40, 30, -12, STYLE.inkRule)

    local track = P.fill(pg, STYLE.inkHair, "ARTWORK")
    track:SetPoint("TOPLEFT", 44, -50)
    track:SetPoint("TOPRIGHT", -16, -50)
    track:SetHeight(3)
    local done = P.fill(pg, STYLE.wax, "OVERLAY")
    done:SetPoint("TOPLEFT", track, "TOPLEFT")
    done:SetSize((W - cx - 20 - 60) / 3, 3)

    local steps = {
        { "Train Tailoring", "Stormwind, Trade District", "done" },
        { "Take Wanted: Hogger", "Goldshire, the board outside the inn", "current" },
        { "Hand it in", "Goldshire, Marshal Dughan", "next" },
    }
    local sy = -60
    for i, s in ipairs(steps) do
        local row, h = P.step(pg, i, s[1], s[2], s[3])
        row:SetPoint("TOPLEFT", 16, sy)
        row:SetPoint("TOPRIGHT", -12, sy)
        sy = sy - h
    end

    local nl = P.text(pg, STYLE.body, 11, STYLE.ink3)
    nl:SetPoint("TOPLEFT", 44, sy - 16)
    nl:SetText("Note")
    local nt = P.text(pg, STYLE.head, 14, STYLE.ink2)
    nt:SetPoint("TOPLEFT", nl, "BOTTOMLEFT", 0, -6)
    nt:SetText("Hogger hits hard at 12. Bring a friend.")

    local on = true
    local sw = P.switch(win, "Show tracker on screen", function() return on end, function(v) on = v end)
    sw:SetPoint("BOTTOMLEFT", cx, 19)
    local sync = P.bronze(win, "Sync", 84)
    sync:SetPoint("BOTTOMRIGHT", -20, 13)
end

P.pages.map = { id = "map", title = "Map", sub = "Coordinates, and a map you can see through. Display only." }
function P.pages.map.draw(win, cx, L)
    local W, H, HEAD, FOOT = L.W, L.H, L.HEAD, L.FOOT
    local cw = W - cx - 20
    local mod = P.switch(win, "", function() return true end, function() end)
    mod:SetPoint("TOPRIGHT", -54, -24)

    -- preview: the map drawn on parchment, at the fade value
    local alpha = 0.5
    local pv = P.page(win)
    pv:SetPoint("TOPLEFT", cx, -HEAD - 6)
    pv:SetSize(cw, 104)
    local ink = { 0.23, 0.15, 0.09 }
    for _, b in ipairs({ { 30, -14, 110, 38 }, { 200, -54, 90, 30 }, { 330, -12, 80, 40 } }) do
        local wood = P.fill(pv, { 0.30, 0.38, 0.18, 0.35 * alpha * 2 }, "BORDER")
        wood:SetPoint("TOPLEFT", b[1], b[2])
        wood:SetSize(b[3], b[4])
    end
    local road = P.fill(pv, { ink[1], ink[2], ink[3], 0.45 }, "ARTWORK")
    road:SetPoint("TOPLEFT", 1, -52) road:SetPoint("TOPRIGHT", -1, -52) road:SetHeight(2)
    local you = P.diamond(pv, STYLE.wax, 9)
    you:SetDrawLayer("OVERLAY")
    you:SetPoint("TOPLEFT", 172, -48)
    local town = P.text(pv, STYLE.head, 12, STYLE.ink2)
    town:SetPoint("TOPLEFT", 186, -32)
    town:SetText("Goldshire")
    P.hrule(pv, -80, 1, -1, STYLE.inkRule)
    local co = P.text(pv, STYLE.body, 11, STYLE.ink)
    co:SetPoint("BOTTOMLEFT", 12, 7)
    co:SetText("You 42.6, 65.3")
    local cu = P.text(pv, STYLE.body, 11, STYLE.ink3, "RIGHT")
    cu:SetPoint("BOTTOMRIGHT", -12, 7)
    cu:SetText("Cursor 51.0, 70.2")

    local card = P.card(win, "World map", 214)
    card:SetPoint("TOPLEFT", cx, -HEAD - 6 - 104 - 14)
    card:SetPoint("TOPRIGHT", -20, -HEAD - 6 - 104 - 14)
    local y = -46
    local rows = {
        { "Coordinates on the world map", "Where you are, and where the cursor points, along the bottom.", true },
        { "Coordinates under the minimap", nil, false },
        { "Fade the map while you move", "It turns see-through while you walk or ride.", true },
    }
    for _, r in ipairs(rows) do
        local f, h = P.option(card, r[1], r[2], r[3])
        f:SetPoint("TOPLEFT", 14, y) f:SetPoint("TOPRIGHT", -14, y)
        y = y - h - 4
    end
    local sl = P.slider(card, "How much shows while moving", alpha, function(v) return math.floor(v * 100 + 0.5) .. "%" end)
    sl:SetPoint("TOPLEFT", 54, y) sl:SetPoint("TOPRIGHT", -14, y)
    sl:SetWidth(cw - 68)
    sl.Refresh()

    local from = P.text(win, STYLE.body, 11, STYLE.ash)
    from:SetPoint("BOTTOMLEFT", cx, 21)
    from:SetText("Helpful preset, 1 change")
    local back = P.ghost(win, "Back to Helpful", 116)
    back:SetPoint("BOTTOMRIGHT", -20, 14)
end

if ForeverBuddyWindow and ForeverBuddyWindow ~= FBProtoWindow then FBProtoOrig = ForeverBuddyWindow end
if FBProtoOrig then FBProtoOrig:Hide() end
if FBProtoWindow then FBProtoWindow:Hide() end
local win = P.build(P.pages[FBProtoPage or "plan"])
ForeverBuddyWindow = win
return "built " .. (FBProtoPage or "plan")
