-- Musca Auras imports. /fbl wa builds the buff reminders, /fbl wa bars the
-- rogue bars, and opens Musca's own Import window with them: nothing is
-- added until you click Import there. The auras are Musca's to draw; this
-- file only builds their data.
--
-- Reminders: one icon per missing buff, out of combat only (the guide's
-- "pre-pull" recipe; classic Aura triggers can't see buffs in combat).

local _, ns = ...

local function spellID(name, fallback)
    if C_Spell and C_Spell.GetSpellInfo then
        local ok, info = pcall(C_Spell.GetSpellInfo, name)
        if ok and type(info) == "table" and info.spellID then
            return info.spellID
        end
    end
    return fallback
end

local function spellIcon(name, fallback)
    if C_Spell and C_Spell.GetSpellTexture then
        local ok, tex = pcall(C_Spell.GetSpellTexture, name)
        if ok and tex then
            return tex
        end
    end
    return fallback
end

local function uid()
    local s = {}
    for i = 1, 11 do
        local n = math.random(1, 62)
        s[i] = string.sub("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789", n, n)
    end
    return table.concat(s)
end

local GROUP = "BuffNag"

-- Out of combat, alive, not on a taxi; plus the class and any extras.
local function load(class, extra)
    local l = {
        use_class = true, class = { single = class, multi = {} },
        use_combat = false, use_alive = true, use_vehicle = false,
        size = { multi = {} }, talent = { multi = {} }, spec = { multi = {} },
    }
    for k, v in pairs(extra or {}) do
        l[k] = v
    end
    return l
end

local function icon(id, label, iconTex, trigger, loadOpts)
    return {
        id = id,
        uid = uid(),
        parent = GROUP,
        regionType = "icon",
        internalVersion = 92,
        width = 30, height = 30,
        iconSource = 0, displayIcon = iconTex or 134400,
        desaturate = false, zoom = 0.15, keepAspectRatio = true,
        cooldown = false,
        triggers = {
            { trigger = trigger, untrigger = {} },
            activeTriggerMode = -10,
        },
        load = loadOpts,
        subRegions = {
            { type = "subbackground" },
            {
                type = "subtext",
                text_visible = true,
                text_text = label,
                text_color = { 1, 0.35, 0.35, 1 },
                text_fontSize = 16,
                text_fontType = "OUTLINE",
                text_selfPoint = "AUTO",
                anchor_point = "OUTER_RIGHT",
                text_anchorXOffset = 4, text_anchorYOffset = 0,
                text_shadowColor = { 0, 0, 0, 1 }, text_shadowXOffset = 1, text_shadowYOffset = -1,
            },
        },
        -- BuffNag's slow breathe.
        animation = {
            start = { type = "none", duration_type = "seconds" },
            main = { type = "preset", preset = "alphaPulse", duration_type = "seconds", duration = "1.8" },
            finish = { type = "none", duration_type = "seconds" },
        },
        conditions = {},
        actions = { start = {}, init = {}, finish = {} },
        config = {}, authorOptions = {},
        information = {},
    }
end

-- "Any one of these" buffs missing.
local function buffMissing(names)
    return { type = "aura2", unit = "player", debuffType = "HELPFUL",
        useName = true, auranames = names, matchesShowOn = "showOnMissing" }
end

local function weaponMissing(hand)
    return { type = "item", event = "Weapon Enchant", weapon = hand, showOn = "showOnMissing" }
end

local function build()
    local c = {}
    local function add(a)
        c[#c + 1] = a
    end
    -- Mage
    add(icon("BuffNag: Arcane Intellect", "Arcane Intellect", spellIcon("Arcane Intellect", 135932),
        buffMissing({ "Arcane Intellect", "Arcane Brilliance" }), load("MAGE")))
    add(icon("BuffNag: Mage armor", "Frost Armor or Ice Armor or Mage Armor",
        spellIcon("Mage Armor") or spellIcon("Ice Armor") or spellIcon("Frost Armor", 135843),
        buffMissing({ "Frost Armor", "Ice Armor", "Mage Armor" }), load("MAGE")))
    -- Rogue: poisons from 20.
    local lvl20 = { use_level = true, level = { "20" }, level_operator = { ">=" } }
    add(icon("BuffNag: Main-hand poison", "Main-hand poison", 132273, weaponMissing("main"), load("ROGUE", lvl20)))
    add(icon("BuffNag: Off-hand poison", "Off-hand poison", 132273, weaponMissing("off"), load("ROGUE", lvl20)))
    -- Gathering trackers, any class, only once you know them.
    for _, t in ipairs({ { "Find Herbs", 2383, 133939 }, { "Find Minerals", 2580, 136025 } }) do
        local name, id, tex = t[1], t[2], t[3]
        local l = load(nil, { use_spellknown = true, spellknown = spellID(name, id) })
        l.use_class = nil
        add(icon("BuffNag: " .. name, name, spellIcon(name, tex), buffMissing({ name }), l))
    end
    -- Well Fed: the mage asked for it.
    add(icon("BuffNag: Well Fed", "Well Fed", 136000, buffMissing({ "Well Fed" }), load("MAGE")))

    local ids = {}
    for _, a in ipairs(c) do
        ids[#ids + 1] = a.id
    end
    local group = {
        id = GROUP,
        uid = uid(),
        regionType = "dynamicgroup",
        internalVersion = 92,
        controlledChildren = ids,
        grow = "DOWN", align = "CENTER", space = 4, stagger = 0,
        sort = "none", animate = false,
        anchorPoint = "CENTER", selfPoint = "TOP", xOffset = 0, yOffset = 260,
        border = false, backdropColor = { 1, 1, 1, 0.5 },
        triggers = { { trigger = { type = "aura2", debuffType = "HELPFUL", unit = "player" }, untrigger = {} } },
        load = { size = { multi = {} }, class = { multi = {} }, talent = { multi = {} }, spec = { multi = {} } },
        conditions = {}, subRegions = {},
        animation = { start = { type = "none", duration_type = "seconds" }, main = { type = "none", duration_type = "seconds" }, finish = { type = "none", duration_type = "seconds" } },
        actions = { start = {}, init = {}, finish = {} },
        config = {}, authorOptions = {}, information = {},
    }
    return { d = group, c = c, v = 2000 }
end

-- Rogue bars ----------------------------------------------------------------
-- /fbl wa bars: one bar per DoT / stun on your target and per buff on you,
-- each in a fixed slot of a plain group, so nothing shifts when one is
-- missing. They use Musca's Aura (Modern) trigger: the game draws those
-- itself, so they keep running in combat (the classic Aura trigger doesn't).

local BARS = "Rogue bars"
local BAR_W, BAR_H, GAP = 220, 20, 3

-- ids: the first rank; Musca expands it to every rank (its Forever rank table).
-- exact: the aura's own IDs instead (Deadly Poison's debuff isn't the spell
-- that makes the poison).
local ROGUE_BARS = {
    { "Garrote", unit = "target", ids = { 703 }, color = { 0.8, 0.15, 0.15, 1 } },
    { "Rupture", unit = "target", ids = { 1943 }, color = { 0.55, 0.05, 0.1, 1 } },
    { "Deadly Poison", unit = "target", exact = { 2818, 2819, 11353, 11354, 25349 }, color = { 0.2, 0.75, 0.25, 1 }, stacks = true },
    { "Cheap Shot", unit = "target", ids = { 1833 }, color = { 0.95, 0.8, 0.2, 1 } },
    { "Kidney Shot", unit = "target", ids = { 408 }, color = { 0.95, 0.5, 0.1, 1 } },
    { "Slice and Dice", unit = "player", ids = { 5171 }, color = { 0.95, 0.75, 0.3, 1 } },
}

local function strings(list)
    local out = {}
    for i, v in ipairs(list) do
        out[i] = tostring(v)
    end
    return out
end

local function text(str, anchor, size)
    return {
        type = "subtext", text_visible = true, text_text = str,
        text_color = { 1, 1, 1, 1 }, text_fontSize = size or 12, text_fontType = "OUTLINE",
        text_selfPoint = "AUTO", anchor_point = anchor,
        text_anchorXOffset = 0, text_anchorYOffset = 0,
        text_shadowColor = { 0, 0, 0, 1 }, text_shadowXOffset = 1, text_shadowYOffset = -1,
    }
end

local function bar(slot, b)
    local trigger = {
        type = "secretAura", unit = b.unit,
        debuffType = b.unit == "player" and "HELPFUL" or "HARMFUL",
        nativeFilters = { PLAYER = true }, -- yours only
    }
    if b.exact then
        trigger.secretUseSpellIDs = true
        trigger.auraspellids = strings(b.exact)
    else
        trigger.secretUseRankSpellIDs = true
        trigger.auraRankSpellIDs = strings(b.ids)
    end
    local sub = { { type = "subbackground" }, { type = "subforeground" },
        text(b[1], "INNER_LEFT"), text("%p", "INNER_RIGHT") }
    if b.stacks then
        sub[#sub + 1] = text("%s", "CENTER", 13) -- on the icon
    end
    return {
        id = "Rogue bar: " .. b[1], uid = uid(), parent = BARS,
        regionType = "aurabar", internalVersion = 92,
        width = BAR_W, height = BAR_H, orientation = "HORIZONTAL",
        texture = "Blizzard", barColor = b.color, barColor2 = b.color,
        backgroundColor = { 0, 0, 0, 0.5 }, enableGradient = false, gradientOrientation = "HORIZONTAL",
        icon = true, icon_side = "LEFT", iconSource = -1, icon_color = { 1, 1, 1, 1 },
        zoom = 0.15, spark = false, inverse = false, smoothProgress = true,
        -- Fixed slot: top to bottom in list order.
        anchorPoint = "TOP", selfPoint = "TOP", xOffset = 0, yOffset = -(slot - 1) * (BAR_H + GAP),
        triggers = { { trigger = trigger, untrigger = {} }, activeTriggerMode = -10 },
        load = { use_class = true, class = { single = "ROGUE", multi = {} },
            size = { multi = {} }, talent = { multi = {} }, spec = { multi = {} } },
        subRegions = sub,
        animation = { start = { type = "none", duration_type = "seconds" }, main = { type = "none", duration_type = "seconds" }, finish = { type = "none", duration_type = "seconds" } },
        conditions = {}, actions = { start = {}, init = {}, finish = {} },
        config = {}, authorOptions = {}, information = {},
    }
end

local function buildBars()
    local c, ids = {}, {}
    for i, b in ipairs(ROGUE_BARS) do
        c[i] = bar(i, b)
        ids[i] = c[i].id
    end
    local group = {
        id = BARS, uid = uid(), regionType = "group", internalVersion = 92,
        controlledChildren = ids,
        anchorPoint = "CENTER", selfPoint = "CENTER", xOffset = 0, yOffset = -180,
        border = false,
        triggers = { { trigger = { type = "aura2", debuffType = "HELPFUL", unit = "player" }, untrigger = {} } },
        load = { size = { multi = {} }, class = { multi = {} }, talent = { multi = {} }, spec = { multi = {} } },
        conditions = {}, subRegions = {},
        animation = { start = { type = "none", duration_type = "seconds" }, main = { type = "none", duration_type = "seconds" }, finish = { type = "none", duration_type = "seconds" } },
        actions = { start = {}, init = {}, finish = {} },
        config = {}, authorOptions = {}, information = {},
    }
    return { d = group, c = c, v = 2000 }
end

function ns.aurasAvailable()
    return WeakAuras and WeakAuras.Import and true or false
end

-- Opens Musca's Import window with the reminders, or with "bars" the rogue bars.
function ns.auras(which)
    if not ns.aurasAvailable() then
        ns.say("Musca Auras isn't loaded.")
        return
    end
    if InCombatLockdown() then
        ns.say("after combat, please.")
        return
    end
    local ok, a, b = pcall(WeakAuras.Import, which == "bars" and buildBars() or build())
    if not ok or a == false or (a == nil and b) then
        ns.say("import failed: " .. tostring(ok and b or a))
    else
        ns.say("check the group in the window, then click Import.")
    end
end

ns.command({ name = "wa", args = "[bars]", help = "Musca Auras import: the buff reminders, or the rogue bars",
    fn = function(rest)
        ns.auras(rest:match("^(%S*)"))
    end })
