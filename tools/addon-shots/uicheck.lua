-- UI1: checks a UI document (design/specs/ui-language.md) before it's
-- drawn. Returns a list of problems, each { path, code, message }: the path
-- is a JSON pointer into the document as the agent wrote it (0-based list
-- indices), and the message says how to fix it. An empty list means it
-- draws. FBUI/UI.lua trusts a document that passed.

local M = {}

M.KINDS = { plate = true, bar = true, text = true, pips = true, icon = true, actionbar = true }
M.UNITS = { player = true, target = true, targettarget = true, focus = true, pet = true,
    party1 = true, party2 = true, party3 = true, party4 = true }
M.POINTS = { TOPLEFT = true, TOP = true, TOPRIGHT = true, LEFT = true, CENTER = true, RIGHT = true,
    BOTTOMLEFT = true, BOTTOM = true, BOTTOMRIGHT = true }
M.STATES = { always = true, ooc = true, combat = true, target = true, notarget = true, full = true, low = true,
    stealth = true }
M.FONTS = { Barlow = true, Geist = true, Marcellus = true, Friz = true, ["Arial Narrow"] = true, Morpheus = true,
    Skurri = true }
M.TOKENS = { name = true, level = true, hp = true, ["hp%"] = true, power = true, ["power%"] = true, combo = true }
M.VALUES = { health = true, power = true, combo = true }
M.BINDS = { ACTIONBUTTON = true, MULTIACTIONBAR1BUTTON = true, MULTIACTIONBAR2BUTTON = true,
    MULTIACTIONBAR3BUTTON = true, MULTIACTIONBAR4BUTTON = true }
-- Blizzard frames a document may fade (alpha 0, out of combat).
M.HIDE = { PlayerFrame = true, TargetFrame = true, FocusFrame = true, PetFrame = true, MainActionBar = true,
    MainMenuBar = true, MultiBarBottomLeft = true, MultiBarBottomRight = true, MultiBarRight = true,
    MultiBarLeft = true, StanceBar = true, MicroMenuContainer = true, BagsBar = true, MinimapCluster = true,
    ObjectiveTrackerFrame = true, BuffFrame = true, DebuffFrame = true }
M.LIMITS = { parts = 300 }

-- What each kind takes, besides what every part takes.
local COMMON = { "name", "kind", "unit", "anchor", "size", "level", "alpha", "hidden" }
local TAKES = {
    plate = { "color", "gradient", "border", "borderWidth" },
    bar = { "value", "color", "gradient", "back", "backAlpha", "border", "borderWidth", "vertical", "reverse",
        "ticks", "tickColor" },
    text = { "format", "font", "fontSize", "outline", "stroke", "justify", "color" },
    pips = { "value", "count", "w", "h", "gap", "layout", "from", "to", "radius", "color", "emptyColor", "emptyAlpha",
        "border" },
    icon = { "slot", "aura", "zoom", "border", "borderWidth" },
    actionbar = { "slots", "icon", "gap", "columns", "zoom", "border", "back", "keys", "bind", "bindFrom", "keySize",
        "keyColor", "font", "outline", "stroke" },
}
-- Properties that may differ by state.
local STATEFUL = { size = true, alpha = true, hidden = true, color = true, gradient = true }

local function sorted(set)
    local out = {}
    for k in pairs(set) do
        out[#out + 1] = k
    end
    table.sort(out)
    return table.concat(out, ", ")
end

local function isHex(v)
    return type(v) == "string" and v:match("^#%x%x%x%x%x%x$") ~= nil
end

local function isList(v)
    if type(v) ~= "table" then
        return false
    end
    local n = 0
    for _ in pairs(v) do
        n = n + 1
    end
    return n == #v
end

function M.check(doc)
    local problems = {}
    local function bad(path, code, message)
        problems[#problems + 1] = { path = path, code = code, message = message }
    end
    local function number(path, v, lo, hi, what)
        if type(v) ~= "number" or v ~= v then
            bad(path, "not-a-number", what .. " takes a number from " .. lo .. " to " .. hi .. ".")
            return false
        elseif v < lo or v > hi then
            bad(path, "out-of-range", what .. " is " .. v .. "; it takes " .. lo .. " to " .. hi .. ".")
            return false
        end
        return true
    end
    local function oneOf(path, v, set, what)
        if not set[v] then
            bad(path, "unknown-" .. what, ("%s %s isn't one this version knows. It takes: %s."):format(
                what, type(v) == "string" and ("\"" .. v .. "\"") or tostring(v), sorted(set)))
        end
    end
    local function colour(path, v)
        if isHex(v) or v == "class" or v == "power" or v == "reaction" then
            return
        end
        if isList(v) and #v > 0 then
            for i, s in ipairs(v) do
                if not (isList(s) and #s == 2 and type(s[1]) == "number" and s[1] >= 0 and s[1] <= 1 and isHex(s[2])) then
                    bad(path .. "/" .. (i - 1), "bad-stop", "a colour stop is [how full, 0 to 1, \"#rrggbb\"].")
                end
            end
            return
        end
        bad(path, "bad-colour", "a colour is \"#rrggbb\", \"class\", \"power\", \"reaction\", or stops like [[0.35, \"#d24a3a\"], [1, \"#4fd99a\"]].")
    end

    if type(doc) ~= "table" then
        bad("", "not-a-document", "a UI document is an object with \"ui\": 1 and \"parts\".")
        return problems
    end
    if doc.ui ~= 1 then
        bad("/ui", "version", "\"ui\" is 1 for this version of the language.")
    end
    for k in pairs(doc) do
        if not ({ ui = true, name = true, parts = true, hide = true, lowAt = true, charY = true })[k] then
            bad("/" .. tostring(k), "unknown-key", "a document takes ui, name, parts, hide, lowAt and charY.")
        end
    end
    if doc.name ~= nil and (type(doc.name) ~= "string" or #doc.name > 40) then
        bad("/name", "bad-name", "name is a few words, up to 40 characters.")
    end
    if doc.lowAt ~= nil then
        number("/lowAt", doc.lowAt, 0.05, 0.95, "lowAt")
    end
    if doc.charY ~= nil then
        number("/charY", doc.charY, -540, 540, "charY")
    end
    if doc.hide ~= nil then
        if not isList(doc.hide) then
            bad("/hide", "not-a-list", "hide is a list of Blizzard frame names.")
        else
            for i, name in ipairs(doc.hide) do
                oneOf("/hide/" .. (i - 1), name, M.HIDE, "frame")
            end
        end
    end
    if not isList(doc.parts) or #doc.parts == 0 then
        bad("/parts", "no-parts", "parts is a list of at least one part.")
        return problems
    end
    if #doc.parts > M.LIMITS.parts then
        bad("/parts", "too-many", ("%d parts; a document takes up to %d."):format(#doc.parts, M.LIMITS.parts))
    end

    local names = {}
    for i, p in ipairs(doc.parts) do
        local at = "/parts/" .. (i - 1)
        if type(p) ~= "table" then
            bad(at, "not-a-part", "each part is an object with a kind.")
        else
            if not M.KINDS[p.kind] then
                oneOf(at .. "/kind", p.kind, M.KINDS, "kind")
            end
            local takes = {}
            for _, k in ipairs(COMMON) do
                takes[k] = true
            end
            for _, k in ipairs(TAKES[p.kind] or {}) do
                takes[k] = true
            end
            for k in pairs(p) do
                if not takes[k] then
                    bad(at .. "/" .. tostring(k), "unknown-key",
                        ("a %s doesn't take %s. It takes: %s."):format(tostring(p.kind), tostring(k), sorted(takes)))
                end
            end
            if p.name ~= nil then
                if type(p.name) ~= "string" or not p.name:match("^%a[%w_]*$") or #p.name > 24 then
                    bad(at .. "/name", "bad-name", "a part's name is a word of letters, digits and _, up to 24 long.")
                elseif names[p.name] then
                    bad(at .. "/name", "duplicate-name", ("%s is already the name of part %d."):format(p.name, names[p.name] - 1))
                end
            end
            if p.unit ~= nil then
                oneOf(at .. "/unit", p.unit, M.UNITS, "unit")
            end
            -- state maps: { always = ..., combat = ... }
            for k in pairs(STATEFUL) do
                local v = p[k]
                if type(v) == "table" and not isList(v) then
                    if v.always == nil then
                        bad(at .. "/" .. k, "no-always", k .. " by state needs an \"always\" value to start from.")
                    end
                    for s in pairs(v) do
                        oneOf(at .. "/" .. k .. "/" .. tostring(s), s, M.STATES, "state")
                    end
                end
            end
            local function each(k, fn)
                local v = p[k]
                if v == nil then
                    return
                end
                if STATEFUL[k] and type(v) == "table" and not isList(v) then
                    for s, sv in pairs(v) do
                        fn(at .. "/" .. k .. "/" .. tostring(s), sv)
                    end
                else
                    fn(at .. "/" .. k, v)
                end
            end
            -- anchor: [point, to, toPoint, x, y]
            if p.anchor ~= nil then
                local a = p.anchor
                if not isList(a) or #a < 1 or #a > 5 then
                    bad(at .. "/anchor", "bad-anchor", "anchor is [point, to, toPoint, x, y], e.g. [\"BOTTOM\", \"screen\", \"BOTTOM\", 0, 247].")
                else
                    oneOf(at .. "/anchor/0", a[1], M.POINTS, "point")
                    local to = a[2]
                    if to ~= nil and to ~= "screen" and to ~= "char" and not names[to] then
                        local known = {}
                        for n in pairs(names) do
                            known[#known + 1] = n
                        end
                        table.sort(known)
                        bad(at .. "/anchor/1", "unknown-anchor",
                            ("anchors to %s, but no part before this one has that name. It can be \"screen\", \"char\"%s."):format(
                                tostring(to), #known > 0 and (" or " .. table.concat(known, ", ")) or ""))
                    end
                    if a[3] ~= nil then
                        oneOf(at .. "/anchor/2", a[3], M.POINTS, "point")
                    end
                    if a[4] ~= nil then
                        number(at .. "/anchor/3", a[4], -4000, 4000, "x")
                    end
                    if a[5] ~= nil then
                        number(at .. "/anchor/4", a[5], -4000, 4000, "y")
                    end
                end
            end
            each("size", function(path, v)
                if not isList(v) or #v ~= 2 then
                    bad(path, "bad-size", "size is [width, height] in 1080p pixels.")
                else
                    number(path .. "/0", v[1], 0, 3000, "width")
                    number(path .. "/1", v[2], 0, 3000, "height")
                end
            end)
            if p.level ~= nil then
                number(at .. "/level", p.level, 0, 20, "level")
            end
            each("alpha", function(path, v)
                number(path, v, 0, 1, "alpha")
            end)
            each("hidden", function(path, v)
                if type(v) ~= "boolean" then
                    bad(path, "not-true-or-false", "hidden is true or false.")
                end
            end)
            each("color", colour)
            each("gradient", function(path, v)
                if p.kind == "bar" then
                    if not (isList(v) and #v == 2) then
                        bad(path, "bad-gradient", "a bar's gradient is [lighten the start, darken the end], e.g. [0.38, -0.18].")
                    else
                        number(path .. "/0", v[1], -1, 1, "gradient start")
                        number(path .. "/1", v[2], -1, 1, "gradient end")
                    end
                elseif not (isList(v) and #v == 3 and (v[1] == "HORIZONTAL" or v[1] == "VERTICAL") and isHex(v[2]) and isHex(v[3])) then
                    bad(path, "bad-gradient", "a plate's gradient is [\"HORIZONTAL\" or \"VERTICAL\", \"#from\", \"#to\"].")
                end
            end)
            for _, k in ipairs({ "border", "back", "tickColor", "emptyColor", "keyColor" }) do
                if p[k] ~= nil and not isHex(p[k]) then
                    bad(at .. "/" .. k, "bad-colour", k .. " is a colour, \"#rrggbb\".")
                end
            end
            for k, r in pairs({ borderWidth = { 1, 8 }, backAlpha = { 0, 1 }, emptyAlpha = { 0, 1 }, ticks = { 1, 20 },
                count = { 1, 10 }, w = { 1, 300 }, h = { 1, 300 }, gap = { 0, 100 }, from = { -360, 360 },
                to = { -360, 360 }, radius = { 0, 1000 }, zoom = { 0, 0.3 }, fontSize = { 6, 72 }, stroke = { 0, 3 },
                icon = { 8, 128 }, columns = { 1, 48 }, keySize = { 6, 40 }, bindFrom = { 1, 12 }, slot = { 1, 180 } }) do
                if p[k] ~= nil then
                    number(at .. "/" .. k, p[k], r[1], r[2], k)
                end
            end
            for _, k in ipairs({ "vertical", "reverse", "keys" }) do
                if p[k] ~= nil and type(p[k]) ~= "boolean" then
                    bad(at .. "/" .. k, "not-true-or-false", k .. " is true or false.")
                end
            end
            if p.kind == "bar" and p.value == nil then
                bad(at .. "/value", "no-value", "a bar needs a value: health, power or combo.")
            end
            if p.value ~= nil then
                oneOf(at .. "/value", p.value, M.VALUES, "value")
            end
            if p.layout ~= nil and p.layout ~= "row" and p.layout ~= "arc" then
                bad(at .. "/layout", "bad-layout", "pips are laid out in a \"row\" or an \"arc\".")
            end
            if p.font ~= nil then
                oneOf(at .. "/font", p.font, M.FONTS, "font")
            end
            if p.outline ~= nil then
                oneOf(at .. "/outline", p.outline, { thick = true, thin = true, none = true }, "outline")
            end
            if p.justify ~= nil then
                oneOf(at .. "/justify", p.justify, { LEFT = true, CENTER = true, RIGHT = true }, "justify")
            end
            if p.bind ~= nil then
                oneOf(at .. "/bind", p.bind, M.BINDS, "bind")
            end
            if p.kind == "text" then
                if type(p.format) ~= "string" or #p.format > 120 then
                    bad(at .. "/format", "bad-format", "a text's format is up to 120 characters, e.g. \"{hp} | {hp%}\".")
                else
                    for tok in p.format:gmatch("{([^}]*)}") do
                        if not M.TOKENS[tok] then
                            bad(at .. "/format", "unknown-token",
                                ("{%s} isn't a token. Tokens: {%s}."):format(tok, sorted(M.TOKENS):gsub(", ", "}, {")))
                        end
                    end
                end
            end
            if p.aura ~= nil and (type(p.aura) ~= "string" or #p.aura > 60) then
                bad(at .. "/aura", "bad-aura", "aura is a spell name, up to 60 characters.")
            end
            if p.kind == "icon" and p.slot == nil and p.aura == nil then
                bad(at, "no-icon", "an icon needs a slot (an action slot, 1 to 180) or an aura (a spell name).")
            end
            if p.kind == "actionbar" then
                if not isList(p.slots) or #p.slots == 0 or #p.slots > 48 then
                    bad(at .. "/slots", "bad-slots", "slots is a list of 1 to 48 action slots, e.g. [1, 2, 3, 4, 5, 6].")
                else
                    for j, s in ipairs(p.slots) do
                        if type(s) ~= "number" or s % 1 ~= 0 or s < 1 or s > 180 then
                            bad(at .. "/slots/" .. (j - 1), "bad-slot", "an action slot is a whole number from 1 to 180.")
                        end
                    end
                end
            end
            if type(p.name) == "string" and not names[p.name] then
                names[p.name] = i
            end
        end
    end
    return problems
end

-- The problems as lines, for a log or a tool's text.
function M.lines(problems)
    local out = {}
    for _, p in ipairs(problems) do
        out[#out + 1] = ("%s  %s: %s"):format(p.path == "" and "/" or p.path, p.code, p.message)
    end
    return table.concat(out, "\n")
end

return M
