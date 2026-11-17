-- The groups window: what the group finder last found, in a window that stays up.
--
-- Forever's group finder runs on the game's group-finder system (C_LFGList). Every search it
-- makes hands its results to every addon, and we keep the exact search it made (category and
-- filters). A search is something the game only sends from a key press or a click, never from an
-- addon on its own, so refreshing stays one action per press: the window's Refresh button, or a
-- key bound under Key Bindings > AddOns > ForeverBuddy Lite. Between presses the game still pushes
-- changes to listed groups (people joining, listings closing), and the window follows them.
--
-- A group's title and comment are protected text: shown as they are, but an addon can't read
-- them, so watch words match the activity ("Scarlet Monastery"), not titles.
--
-- ForeverBuddyLiteDB.groups = { point, shown, search = { n, ... }, watch = { "word", ... } }

local _, ns = ...
local read, num, now, say = ns.read, ns.num, ns.now, ns.say

local MIN_REFRESH_SECONDS = 2 -- the server throttles faster searches anyway
local ROW_HEIGHT = 32
local GREY, GOLD, GREEN, RED = "|cff9d9d9d", "|cffffd200", "|cff40ff40", "|cffff6060"

local results = {} -- [id] = { id, activity, name, leader, members, roles, age, at, delisted, new }
local order = {} -- ids, newest listing first
local seenBefore = {} -- ids from the previous search, to mark what's new
local searchedAt, lastRefresh, failure = nil, 0, nil
local frame, rows, statusText, list

local function store()
    local db = ns.db
    if not db then
        return nil
    end
    db.groups = type(db.groups) == "table" and db.groups or {}
    return db.groups
end

-- Reading results ---------------------------------------------------------------------------

local function activityName(ids)
    for _, id in ipairs(type(ids) == "table" and ids or {}) do
        local info = read("C_LFGList.GetActivityInfoTable", id)
        local name = type(info) == "table" and (info.fullName or info.shortName)
        if type(name) == "string" and name ~= "" then
            return name
        end
    end
    return nil
end

-- "1 tank, 1 healer, 2 damage" when the members' roles can be read.
local function roleCounts(id, members)
    local counts = { TANK = 0, HEALER = 0, DAMAGER = 0 }
    local known = false
    for i = 1, members or 0 do
        local role = read("C_LFGList.GetSearchResultMemberInfo", id, i)
        if counts[role] then
            counts[role] = counts[role] + 1
            known = true
        end
    end
    return known and counts or nil
end

local function readResult(id)
    local info = read("C_LFGList.GetSearchResultInfo", id)
    if type(info) ~= "table" then
        return nil
    end
    local members = num(info.numMembers) or 0
    return {
        id = id,
        activity = activityName(info.activityIDs),
        name = info.name, -- protected text: only ever handed to SetText
        leader = info.leaderName,
        members = members,
        roles = roleCounts(id, members),
        age = num(info.age) or 0,
        at = now(),
        delisted = info.isDelisted or false,
        new = not seenBefore[id],
    }
end

local function sortOrder()
    order = {}
    for id in pairs(results) do
        order[#order + 1] = id
    end
    table.sort(order, function(a, b)
        local ra, rb = results[a], results[b]
        if ra.delisted ~= rb.delisted then
            return not ra.delisted
        end
        return ra.age < rb.age
    end)
end

-- A sound and a line in chat when a new group's activity has a watch word.
local function alert(newOnes)
    local g = store()
    local words = g and g.watch or {}
    if #words == 0 then
        return
    end
    for _, r in ipairs(newOnes) do
        local activity = (r.activity or ""):lower()
        for _, word in ipairs(words) do
            if word ~= "" and activity:find(word:lower(), 1, true) then
                pcall(PlaySound, SOUNDKIT and SOUNDKIT.RAID_WARNING or 8959)
                say(string.format("%snew group:|r %s%s", GOLD, r.activity or "?", r.leader and (" (" .. r.leader .. ")") or ""))
                break
            end
        end
    end
end

-- Showing them ------------------------------------------------------------------------------

local function ageText(r)
    local seconds = r.age + (now() - r.at)
    if seconds < 60 then
        return "new"
    elseif seconds < 3600 then
        return string.format("%dm", seconds / 60)
    end
    return string.format("%dh", seconds / 3600)
end

local function membersText(r)
    local text = string.format("%d in group", r.members)
    if r.roles then
        text = text .. string.format("  %s%dT %dH %dD|r", GREY, r.roles.TANK, r.roles.HEALER, r.roles.DAMAGER)
    end
    return text
end

local function makeRow(i)
    local row = CreateFrame("Frame", nil, list)
    row:SetHeight(ROW_HEIGHT)
    row:SetPoint("TOPLEFT", 0, -(i - 1) * ROW_HEIGHT)
    row:SetPoint("RIGHT", list, "RIGHT", 0, 0)
    row.activity = row:CreateFontString(nil, "OVERLAY", "GameFontNormal")
    row.activity:SetPoint("TOPLEFT", 4, -2)
    row.activity:SetPoint("RIGHT", row, "RIGHT", -60, 0)
    row.activity:SetJustifyH("LEFT")
    row.age = row:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    row.age:SetPoint("TOPRIGHT", -4, -3)
    row.title = row:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    row.title:SetPoint("TOPLEFT", row.activity, "BOTTOMLEFT", 0, -2)
    row.title:SetPoint("RIGHT", row, "RIGHT", -150, 0)
    row.title:SetJustifyH("LEFT")
    row.title:SetWordWrap(false)
    row.members = row:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    row.members:SetPoint("TOPRIGHT", row.age, "BOTTOMRIGHT", 0, -3)
    row.members:SetJustifyH("RIGHT")
    local line = row:CreateTexture(nil, "BACKGROUND")
    line:SetColorTexture(1, 1, 1, 0.06)
    line:SetHeight(1)
    line:SetPoint("BOTTOMLEFT")
    line:SetPoint("BOTTOMRIGHT")
    return row
end

local function redraw()
    if not frame or not frame:IsShown() then
        return
    end
    local open = 0
    for _, id in ipairs(order) do
        if not results[id].delisted then
            open = open + 1
        end
    end
    local when = searchedAt and ("searched " .. ns.ago(searchedAt)) or "no search yet"
    local g = store()
    local hint = g and g.search and "Refresh or your bound key searches again."
        or "Search once in the group finder; after that Refresh repeats it."
    statusText:SetText(string.format("%s%d open|r, %s.  %s%s|r%s", GOLD, open, when, GREY, hint,
        failure and ("\n" .. RED .. "Search failed: " .. tostring(failure) .. "|r") or ""))

    rows = rows or {}
    for i, id in ipairs(order) do
        local r = results[id]
        local row = rows[i] or makeRow(i)
        rows[i] = row
        local colour = r.delisted and GREY or (r.new and GREEN or "")
        row.activity:SetText(colour .. (r.activity or "Group") .. (r.delisted and " (closed)" or "") .. (colour ~= "" and "|r" or ""))
        if r.name then
            row.title:SetText(r.name) -- protected text, shown as is
        else
            row.title:SetText("")
        end
        row.age:SetText(r.new and not r.delisted and (GREEN .. ageText(r) .. "|r") or ageText(r))
        row.members:SetText((r.leader and (r.leader .. "  ") or "") .. membersText(r))
        row:Show()
    end
    for i = #order + 1, #rows do
        rows[i]:Hide()
    end
    list:SetHeight(math.max(1, #order * ROW_HEIGHT))
end

-- The search ------------------------------------------------------------------------------

-- The group finder's own searches: keep the exact call, to repeat it.
if C_LFGList and C_LFGList.Search then
    pcall(hooksecurefunc, C_LFGList, "Search", function(...)
        local g = store()
        if g then
            g.search = { n = select("#", ...), ... }
        end
        lastRefresh = now()
    end)
end

-- One search per press: the window's button or the bound key.
function FBL_GroupsRefresh()
    local g = store()
    if not g or type(g.search) ~= "table" then
        say("search once in the group finder first; after that this repeats the same search.")
        return
    end
    if now() - lastRefresh < MIN_REFRESH_SECONDS then
        return
    end
    lastRefresh = now()
    failure = nil
    local ok, err = pcall(C_LFGList.Search, unpack(g.search, 1, g.search.n or #g.search))
    if not ok then
        say("couldn't search: " .. tostring(err))
    end
end

ns.on("LFG_LIST_SEARCH_RESULTS_RECEIVED", function()
    local _, ids = read("C_LFGList.GetSearchResults")
    local previous = {}
    for id in pairs(results) do
        previous[id] = true
    end
    seenBefore = previous
    results = {}
    local newOnes = {}
    for _, id in ipairs(type(ids) == "table" and ids or {}) do
        local r = readResult(id)
        if r then
            results[id] = r
            if r.new and not r.delisted then
                newOnes[#newOnes + 1] = r
            end
        end
    end
    -- The first search of a session has nothing to compare with: nothing is "new".
    if not next(previous) then
        for _, r in pairs(results) do
            r.new = false
        end
        newOnes = {}
    end
    searchedAt, failure = now(), nil
    sortOrder()
    alert(newOnes)
    redraw()
end)

ns.on("LFG_LIST_SEARCH_RESULT_UPDATED", function(id)
    if not results[id] then
        return
    end
    local r = readResult(id)
    if r then
        r.new = results[id].new
        results[id] = r
        sortOrder()
        redraw()
    end
end)

ns.on("LFG_LIST_SEARCH_FAILED", function(reason)
    failure = reason or "unknown"
    redraw()
end)

-- The window ------------------------------------------------------------------------------

local function build()
    frame = CreateFrame("Frame", "ForeverBuddyLiteGroups", UIParent, "BasicFrameTemplateWithInset")
    frame:SetSize(470, 380)
    frame:SetFrameStrata("MEDIUM")
    frame:SetClampedToScreen(true)
    local g = store()
    local point = g and g.point
    if type(point) == "table" and point[1] then
        frame:SetPoint(point[1], UIParent, point[2], point[3], point[4])
    else
        frame:SetPoint("RIGHT", -40, 0)
    end
    frame:SetMovable(true)
    frame:EnableMouse(true)
    frame:RegisterForDrag("LeftButton")
    frame:SetScript("OnDragStart", frame.StartMoving)
    frame:SetScript("OnDragStop", function(self)
        self:StopMovingOrSizing()
        local p, _, rp, x, y = self:GetPoint()
        local s = store()
        if s then
            s.point = { p, rp, x, y }
        end
    end)
    -- Not in UISpecialFrames: Escape leaves it up. The X closes it.
    frame:SetScript("OnHide", function()
        local s = store()
        if s then
            s.shown = false
        end
    end)
    frame:SetScript("OnShow", function()
        local s = store()
        if s then
            s.shown = true
        end
        redraw()
    end)

    local title = frame.TitleText or frame:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    if not frame.TitleText then
        title:SetPoint("TOP", 0, -5)
    end
    title:SetText("Groups")

    statusText = frame:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    statusText:SetPoint("TOPLEFT", 14, -32)
    statusText:SetPoint("RIGHT", -110, 0)
    statusText:SetJustifyH("LEFT")

    local refresh = CreateFrame("Button", nil, frame, "UIPanelButtonTemplate")
    refresh:SetSize(90, 22)
    refresh:SetPoint("TOPRIGHT", -12, -30)
    refresh:SetText("Refresh")
    refresh:SetScript("OnClick", FBL_GroupsRefresh)

    local scroll = CreateFrame("ScrollFrame", nil, frame, "UIPanelScrollFrameTemplate")
    scroll:SetPoint("TOPLEFT", 12, -64)
    scroll:SetPoint("BOTTOMRIGHT", -32, 12)
    list = CreateFrame("Frame", nil, scroll)
    list:SetSize(420, 1)
    scroll:SetScrollChild(list)

    -- Ages tick on screen; no server calls.
    C_Timer.NewTicker(15, function()
        if frame:IsShown() then
            redraw()
        end
    end)
end

function FBL_GroupsToggle()
    if not frame then
        build()
    end
    frame:SetShown(not frame:IsShown())
end

-- Reopen it after a reload if it was up.
ns.on("PLAYER_ENTERING_WORLD", function()
    local g = store()
    if g and g.shown and not frame then
        build()
        frame:Show()
    end
end)

BINDING_HEADER_FBL = "ForeverBuddy Lite"
BINDING_NAME_FBL_GROUPS_REFRESH = "Refresh the group search"
BINDING_NAME_FBL_GROUPS_TOGGLE = "Show or hide the groups window"

ns.command({ name = "groups", aliases = { "g" }, args = "[watch word,word | watch off]",
    help = "the groups window; 'watch sm,cath' plays a sound when a new group's activity matches",
    fn = function(rest)
        local g = store()
        local words = rest:match("^watch%s+(.+)$")
        if g and words then
            if words == "off" then
                g.watch = {}
                say("group alerts off.")
            else
                g.watch = {}
                for word in words:gmatch("[^,]+") do
                    g.watch[#g.watch + 1] = word:match("^%s*(.-)%s*$")
                end
                say("alerting on new groups for: " .. table.concat(g.watch, ", "))
            end
            return
        end
        FBL_GroupsToggle()
    end })
