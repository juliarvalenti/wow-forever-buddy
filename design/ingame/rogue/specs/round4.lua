-- Clean boxes, round 4 (PM's review of round 3): green health with a visible
-- gradient, yellow kept for energy and combo points only, health values as the
-- biggest text in the frames, icons zoomed in a touch with stacks, a proc glow
-- on what's ready, and smaller keybinds. Screen pixels at 1080p, snapped.
local PXU = 768 / 1080
local function P(n) return n * PXU end

local EDGE, BG, TEXT = "#000000", "#141416", "#ffffff"
local HEALTH, HOSTILE, ENERGY, COMBO = "#4fd99a", "#d24a3a", "#f0c94a", "#fff569"
-- class overview: a render can set FBClassTuned = { class colour, power colour, combo points? }
local K = FBClassTuned
local POWER = K and K[2] or ENERGY
local FONT, STROKE = "BarlowSCB.ttf", "THICKOUTLINE"

local ICON, GAP = 46, 1
local GW = 8 * ICON + 7 * GAP           -- grid width, 375
local BOXW = GW + 2                      -- with its outer border
local HALF = (BOXW + 1) / 2              -- frames butt the grid's edge
local Y0 = 200                           -- grid's bottom edge
local ROW2, ROW1 = Y0 + 1, Y0 + 1 + ICON + GAP
local STRIP = ROW1 + ICON + GAP          -- combo strip's bottom
local FRAMEW, FRAMEH = 300, ICON + 2

local function text(anchor, fmt, size, justify, w)
    return { kind = "text", anchor = anchor, size = { P(w or 200), P(size + 4) }, justify = justify,
        format = fmt, font = FONT, fontSize = P(size), outline = STROKE, stroke = 2, color = TEXT, level = 3 }
end

local function row(name, slots, y, extra)
    local r = { name = name, kind = "actionbar", slots = slots, icon = P(ICON), gap = P(GAP), zoom = 0.12,
        anchor = { "BOTTOM", "screen", "BOTTOM", 0, P(y) }, size = { 2, 2 }, strata = "MEDIUM",
        keys = true, font = FONT, keySize = P(12), keyOutline = STROKE, keyColor = TEXT, stroke = 2,
        cdSize = P(22), stackSize = P(16) }
    for k, v in pairs(extra) do r[k] = v end
    return r
end

FBUnitProtoSpec = {
    px = PXU, comboFallback = 3, charY = P(-14), forceCombat = true,
    hide = { "PlayerFrame", "TargetFrame", "MainActionBar", "MultiBarBottomLeft", "MinimapCluster",
             "MicroMenuContainer", "BagsBar", "ObjectiveTrackerFrame",
             "QueueStatusButton", "GeneralDockManager", "ChatFrame1Tab", "ChatFrameMenuButton", "QuickJoinToastButton",
             "ChatFrameChannelButton", "ExpansionLandingPageMinimapButton", "GameTimeFrame", "TimeManagerClockButton" },
    backdrop = {
        -- the grid's black box: the 1px gaps between icons read as black lines
        { name = "gridbox", kind = "plate", art = "flat", color = EDGE,
          anchor = { "BOTTOM", "screen", "BOTTOM", 0, P(Y0) }, size = { P(BOXW), P(STRIP + 8 + 1 - Y0) }, strata = "LOW" },
        row("row1", { 1, 2, 3, 4, 5, 6, 7, 8 }, ROW1, {
            bind = "ACTIONBUTTON", cooldowns = { [3] = { 0.6, "6" } },
            -- Eviscerate (Q, slot 6) is ready at 3 combo points: proc glow
            procs = { [6] = true } }),
        row("row2", { 9, 10, 11, 12, 61, 62, 63, 64 }, ROW2, {
            binds = { "ACTIONBUTTON9", "ACTIONBUTTON10", "ACTIONBUTTON11", "ACTIONBUTTON12",
                "MULTIACTIONBAR1BUTTON1", "MULTIACTIONBAR1BUTTON2", "MULTIACTIONBAR1BUTTON3", "MULTIACTIONBAR1BUTTON4" },
            cooldowns = { [10] = { 0.9, "18" } },
            -- Expose Armor (sE, slot 61) shown with 2 stacks on the target
            stacks = { [61] = "2" } }),
        -- the tracked buff: Slice and Dice, big, above the grid
        { name = "buff", kind = "icon", slot = 8, border = EDGE,
          anchor = { "BOTTOM", "screen", "BOTTOM", 0, P(STRIP + 9 + 36) }, size = { P(40), P(40) }, strata = "MEDIUM" },
        { name = "bufftime", kind = "text", anchor = { "BOTTOM", "screen", "BOTTOM", 0, P(STRIP + 9 + 4) }, size = { P(80), P(34) },
          format = "14", font = FONT, fontSize = P(32), outline = STROKE, stroke = 2, color = TEXT, strata = "MEDIUM" },
    },
    units = {
        { unit = "player", parts = {
            { "box", { kind = "plate", art = "flat", color = EDGE,
                anchor = { "BOTTOMRIGHT", "screen", "BOTTOM", P(-HALF + 1), P(ROW1 - 1) }, size = { P(FRAMEW), P(FRAMEH) } } },
            { "health", { kind = "bar", value = "health", art = "flat", color = HEALTH, gradient = { 0.38, -0.18 },
                back = BG, backAlpha = 0.9, anchor = { "TOPLEFT", "box", "TOPLEFT", P(1), P(-1) }, size = { P(FRAMEW - 2), P(ICON - 7) } } },
            { "power", { kind = "bar", value = "power", art = "flat", color = POWER, gradient = { 0.2, -0.15 },
                back = BG, backAlpha = 0.9, anchor = { "BOTTOMLEFT", "box", "BOTTOMLEFT", P(1), P(1) }, size = { P(FRAMEW - 2), P(5) } } },
            { "hp", text({ "RIGHT", "health", "RIGHT", P(-8), 0 }, "{hpv} | {hp%}", 22, "RIGHT", 220) },
            K and { "name", (function()
                local t = text({ "LEFT", "health", "LEFT", P(8), 0 }, "{name}", 18, "LEFT", 140)
                t.color = K[1]
                return t
            end)() } or nil,
        } },
        { unit = "target", parts = {
            { "box", { kind = "plate", art = "flat", color = EDGE,
                anchor = { "BOTTOMLEFT", "screen", "BOTTOM", P(HALF - 1), P(ROW1 - 1) }, size = { P(FRAMEW), P(FRAMEH) } } },
            { "health", { kind = "bar", value = "health", art = "flat", color = HOSTILE, gradient = { 0.3, -0.18 },
                back = BG, backAlpha = 0.92, anchor = { "TOPLEFT", "box", "TOPLEFT", P(1), P(-1) }, size = { P(FRAMEW - 2), P(ICON - 7) } } },
            { "power", { kind = "bar", value = "power", art = "flat", color = ENERGY, gradient = { 0.2, -0.15 },
                back = BG, backAlpha = 0.9, anchor = { "BOTTOMLEFT", "box", "BOTTOMLEFT", P(1), P(1) }, size = { P(FRAMEW - 2), P(5) } } },
            { "name", text({ "TOPLEFT", "health", "TOPLEFT", P(7), P(-1) }, "{name}", 16, "LEFT", 220) },
            { "hp", text({ "BOTTOMRIGHT", "health", "BOTTOMRIGHT", P(-7), 0 }, "{hpv} | {hp%}", 20, "RIGHT", 220) },
            -- combo points: a segmented strip across the top of the grid
            ((not K or K[3]) or nil) and { "combo", { kind = "pips", anchor = { "BOTTOM", "screen", "BOTTOM", 0, P(STRIP + 4) }, size = { 2, 2 },
                count = 5, spacing = P((GW + 1) / 5), w = P((GW + 1) / 5 - 1), h = P(8), art = "flat", color = COMBO,
                emptyColor = "#2a2a2e", emptyAlpha = 1 } },
        } },
    },
}
