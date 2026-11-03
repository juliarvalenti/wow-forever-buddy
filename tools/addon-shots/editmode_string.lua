-- Decodes an Edit Mode layout export string (the text the game's Share >
-- Copy to Clipboard gives) into the systems of a layout, so hud.lua can
-- show a player's own layout. The format is plain space-separated tokens,
-- as written up by ui-studio (github.com/jacobdolph/ui-studio, web/FORMAT.md):
--
--   <version> [<interfaceStyle> | <numSystems>] <numSystems> system*
--   system := <system> <systemIndex> <isInDefaultPosition> anchor ("-1" | anchor) <settings>
--   anchor := <point> <relativePoint> <relativeTo> <offsetX> <offsetY>
--
-- systemIndex is 0-based (-1 for a single-instance system), points are
-- indices into POINTS, and settings are character pairs chr(35 + id)
-- chr(35 + digit), a value of 90 or more taking one pair per base-90 digit,
-- least significant first ("#" when there are none).

local M = {}

local POINTS = { [0] = "TOPLEFT", "TOP", "TOPRIGHT", "LEFT", "CENTER", "RIGHT", "BOTTOMLEFT", "BOTTOM", "BOTTOMRIGHT" }

-- A few real exports have an odd-length blob; like ui-studio, the stray
-- last character is ignored. Only a repeat of the id just before is another
-- digit.
local function settingsOf(blob)
    local values = {}
    if blob == "#" then
        return values
    end
    local lastId, place = nil, 0
    for i = 1, #blob - 1, 2 do
        local id = blob:byte(i) - 35
        local digit = blob:byte(i + 1) - 35
        place = (id == lastId) and place + 1 or 0
        values[id] = (place > 0 and values[id] or 0) + digit * 90 ^ place
        lastId = id
    end
    return values
end

-- Parses the systems from token n on, expecting count of them; nil unless
-- that uses up every token.
local function systemsFrom(tokens, n, count)
    local systems = {}
    local function anchorAt(k)
        local point, relativePoint = POINTS[tonumber(tokens[k])], POINTS[tonumber(tokens[k + 1])]
        local x, y = tonumber(tokens[k + 3]), tonumber(tokens[k + 4])
        if not (point and relativePoint and tokens[k + 2] and x and y) then
            return nil
        end
        return { point = point, relativePoint = relativePoint, relativeTo = tokens[k + 2], offsetX = x, offsetY = y }
    end
    for _ = 1, count do
        local system, index, default = tonumber(tokens[n]), tonumber(tokens[n + 1]), tonumber(tokens[n + 2])
        if not (system and index and default) then
            return nil
        end
        local anchor = anchorAt(n + 3)
        if not anchor then
            return nil
        end
        n = n + 8
        local anchor2
        if tokens[n] == "-1" then
            n = n + 1
        else
            anchor2 = anchorAt(n)
            if not anchor2 then
                return nil
            end
            n = n + 5
        end
        local blob = tokens[n]
        if not blob then
            return nil
        end
        n = n + 1
        systems[#systems + 1] = {
            system = system,
            systemIndex = index >= 0 and index + 1 or nil,
            isInDefaultPosition = default == 1,
            anchorInfo = anchor,
            anchorInfo2 = anchor2,
            settings = settingsOf(blob),
        }
    end
    if tokens[n] ~= nil then
        return nil
    end
    return systems
end

-- { version = n, systems = { { system, systemIndex, isInDefaultPosition,
-- anchorInfo, anchorInfo2, settings = { [id] = value } } } }
function M.decode(text)
    local tokens = {}
    for t in text:gmatch("%S+") do
        tokens[#tokens + 1] = t
    end
    local version = assert(tonumber(tokens[1]), "not an Edit Mode layout string")
    -- The count is second (old strings) or third (after Forever's interface
    -- style, or repeated in recent live strings): take the reading that
    -- accounts for every token.
    for _, at in ipairs({ 2, 3 }) do
        local count = tonumber(tokens[at])
        local systems = count and systemsFrom(tokens, at + 1, count)
        if systems then
            return { version = version, systems = systems }
        end
    end
    error("couldn't read this Edit Mode layout string (version " .. version .. ")")
end

return M
