-- The groups window: what the group finder last found, in a window that stays up, with who's in
-- each group and filters.
--
-- Forever's group finder runs on the game's group-finder system (C_LFGList). Every search it
-- makes hands its results to every addon, and we keep the exact search it made (category and
-- filters). A search is something the game only sends from a key press or a click, never from an
-- addon on its own, so refreshing stays one action per press: the window's Refresh button, or a
-- key bound under Key Bindings > AddOns > ForeverBuddy Lite. Between presses the game still pushes
-- changes to listed groups (people joining, listings closing), and the window follows them.
--
-- A group's title and comment are protected text: shown as they are, but an addon can't read
-- them, so watch words and the search box match the activity ("Scarlet Monastery"), not titles.
--
-- ForeverBuddyLiteDB.groups = { point, shown, search = { n, ... }, watch = { "word", ... },
--   filters = { text, needs = { TANK, HEALER, DAMAGER }, noArmor, noClass, nearLevel, hideClosed } }

local _, ns = ...
local read, num, now, say = ns.read, ns.num, ns.now, ns.say

local MIN_REFRESH_SECONDS = 2 -- the server throttles faster searches anyway
local ROW_HEIGHT, MEMBER_HEIGHT = 32, 15
local NEAR_LEVELS = 5
-- A five-player group's seats by role.
local SEATS = { TANK = 1, HEALER = 1, DAMAGER = 3 }
-- What each class wears at a level: hunters and shamans take mail at 40, warriors and paladins
-- plate; before that they wear the type below. Whoever wears yours rolls on the same drops.
local ARMOR = { MAGE = "cloth", PRIEST = "cloth", WARLOCK = "cloth", ROGUE = "leather", DRUID = "leather" }
local ARMOR_AT_40 = { HUNTER = { "leather", "mail" }, SHAMAN = { "leather", "mail" }, WARRIOR = { "mail", "plate" },
    PALADIN = { "mail", "plate" } }
local ROLE_SHORT = { TANK = "Tank", HEALER = "Healer", DAMAGER = "Damage" }
local GREY, GOLD, GREEN, RED = "|cff9d9d9d", "|cffffd200", "|cff40ff40", "|cffff6060"

local results = {} -- [id] = { id, activity, name, leader, members, people, roles, level, age, at, delisted, new }
local order = {} -- ids, newest listing first
local seenBefore = {} -- ids from the previous search, to mark what's new
local expanded = {} -- [id] = true: show its members
local searchedAt, lastRefresh, failure = nil, 0, nil
local frame, rows, statusText, list

local function store()
    local db = ns.db
    if not db then
        return nil
    end
    db.groups = type(db.groups) == "table" and db.groups or {}
    local g = db.groups
    g.filters = type(g.filters) == "table" and g.filters or { text = "", nearLevel = false, hideClosed = true }
    g.filters.needs = type(g.filters.needs) == "table" and g.filters.needs or {}
    return g
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

-- Who's in the group: { name, level, class, classFile, role, area, leader } each, as far as the
-- client tells. Roles counted from them; the average level of those whose level is known.
local function readMembers(id, count)
    local people, roles, levels, levelSum = {}, { TANK = 0, HEALER = 0, DAMAGER = 0 }, 0, 0
    local rolesKnown = false
    for i = 1, count do
        local p = read("C_LFGList.GetSearchResultPlayerInfo", id, i)
        if type(p) == "table" then
            local person = { name = p.name, level = num(p.level), class = p.className, classFile = p.classFilename,
                role = p.assignedRole, area = p.areaName, leader = p.isLeader }
            people[#people + 1] = person
            if roles[person.role] then
                roles[person.role] = roles[person.role] + 1
                rolesKnown = true
            end
            if person.level then
                levels, levelSum = levels + 1, levelSum + person.level
            end
        end
    end
    return people, rolesKnown and roles or nil, levels > 0 and (levelSum / levels) or nil
end

local function readResult(id)
    local info = read("C_LFGList.GetSearchResultInfo", id)
    if type(info) ~= "table" then
        return nil
    end
    local count = num(info.numMembers) or 0
    local people, roles, level = readMembers(id, count)
    return {
        id = id,
        activity = activityName(info.activityIDs),
        name = info.name, -- protected text: only ever handed to SetText
        leader = info.leaderName,
        members = count,
        people = people,
        roles = roles,
        level = level,
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

-- Filters -----------------------------------------------------------------------------------

-- Whether a group has a seat for this role: by the roles its members took when the client says,
-- else by headcount.
local function hasRoom(r, role)
    if r.members >= 5 then
        return false
    end
    return not r.roles or r.roles[role] < SEATS[role]
end

local function armorOf(classFile, level)
    if ARMOR[classFile] then
        return ARMOR[classFile]
    end
    local pair = ARMOR_AT_40[classFile]
    return pair and pair[(level and level >= 40) and 2 or 1] or nil
end

-- You: class token ("ROGUE"), its name ("Rogue"), and the armor you wear now.
local function me()
    local className, classFile = read(UnitClass, "player")
    return classFile, className, armorOf(classFile, num(read(UnitLevel, "player")))
end

-- Whether anyone in the group would roll on your armor, or is your class. Members the client
-- doesn't describe can't be judged, so they don't rule a group out.
local function hasMy(r, what)
    local myClass, _, myArmor = me()
    for _, p in ipairs(r.people) do
        if what == "class" and p.classFile and p.classFile == myClass then
            return true
        end
        if what == "armor" and myArmor and armorOf(p.classFile, p.level) == myArmor then
            return true
        end
    end
    return false
end

local function passes(r, f)
    if f.hideClosed and r.delisted then
        return false
    end
    if f.text and f.text ~= "" and not (r.activity or ""):lower():find(f.text:lower(), 1, true) then
        return false
    end
    -- Needs: any ticked role with a seat open.
    local wanted, fits = false, false
    for role in pairs(SEATS) do
        if f.needs[role] then
            wanted = true
            fits = fits or hasRoom(r, role)
        end
    end
    if wanted and not fits then
        return false
    end
    if f.noArmor and hasMy(r, "armor") then
        return false
    end
    if f.noClass and hasMy(r, "class") then
        return false
    end
    if f.nearLevel and r.level then
        local mine = num(read(UnitLevel, "player")) or r.level
        if math.abs(r.level - mine) > NEAR_LEVELS then
            return false
        end
    end
    return true
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

local function summaryText(r)
    local text = string.format("%d/5", r.members)
    if r.roles then
        text = text .. string.format("  %s%dT %dH %dD|r", GREY, r.roles.TANK, r.roles.HEALER, r.roles.DAMAGER)
    end
    if r.level then
        text = text .. string.format("  %slv %d|r", GREY, math.floor(r.level + 0.5))
    end
    return text
end

local function classColour(classFile)
    local c = RAID_CLASS_COLORS and classFile and RAID_CLASS_COLORS[classFile]
    return c and string.format("|cff%02x%02x%02x", c.r * 255, c.g * 255, c.b * 255) or ""
end

-- "Name  lv 37 Rogue  Damage  Stranglethorn  (leader)"
local function memberText(p)
    local parts = { classColour(p.classFile) .. (p.name or "?") .. "|r" }
    if p.level then
        parts[#parts + 1] = "lv " .. p.level
    end
    if p.class then
        parts[#parts + 1] = p.class
    end
    if ROLE_SHORT[p.role] then
        parts[#parts + 1] = ROLE_SHORT[p.role]
    end
    if p.area then
        parts[#parts + 1] = GREY .. p.area .. "|r"
    end
    if p.leader then
        parts[#parts + 1] = GOLD .. "leader|r"
    end
    return table.concat(parts, "  ")
end

local function makeRow()
    local row = CreateFrame("Button", nil, list)
    row:SetHeight(ROW_HEIGHT)
    row:SetPoint("RIGHT", list, "RIGHT", 0, 0)
    row:SetScript("OnClick", function(self)
        expanded[self.id] = not expanded[self.id] or nil
        frame.redraw()
    end)
    local hl = row:CreateTexture(nil, "HIGHLIGHT")
    hl:SetAllPoints()
    hl:SetColorTexture(1, 1, 1, 0.05)
    row.activity = row:CreateFontString(nil, "OVERLAY", "GameFontNormal")
    row.activity:SetPoint("TOPLEFT", 4, -2)
    row.activity:SetPoint("RIGHT", row, "RIGHT", -60, 0)
    row.activity:SetJustifyH("LEFT")
    row.age = row:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    row.age:SetPoint("TOPRIGHT", -4, -3)
    row.title = row:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    row.title:SetPoint("TOPLEFT", 4, -18)
    row.title:SetPoint("RIGHT", row, "RIGHT", -190, 0)
    row.title:SetJustifyH("LEFT")
    row.title:SetWordWrap(false)
    row.summary = row:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    row.summary:SetPoint("TOPRIGHT", -4, -18)
    row.summary:SetJustifyH("RIGHT")
    row.people = row:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    row.people:SetPoint("TOPLEFT", 18, -ROW_HEIGHT)
    row.people:SetPoint("RIGHT", row, "RIGHT", -4, 0)
    row.people:SetJustifyH("LEFT")
    row.people:SetSpacing(2)
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
    local g = store()
    local f = g and g.filters or {}
    local shown, open = {}, 0
    for _, id in ipairs(order) do
        local r = results[id]
        if not r.delisted then
            open = open + 1
        end
        if passes(r, f) then
            shown[#shown + 1] = r
        end
    end

    local when = searchedAt and ("searched " .. ns.ago(searchedAt)) or "no search yet"
    local hint = g and g.search and "Click a group to see who's in it." or "Search once in the group finder; after that Refresh repeats it."
    statusText:SetText(string.format("%sshowing %d of %d open|r, %s.  %s%s|r%s", GOLD, #shown, open, when, GREY, hint,
        failure and ("\n" .. RED .. "Search failed: " .. tostring(failure) .. "|r") or ""))

    rows = rows or {}
    local y = 0
    for i, r in ipairs(shown) do
        local row = rows[i] or makeRow()
        rows[i] = row
        row.id = r.id
        local colour = r.delisted and GREY or (r.new and GREEN or "")
        row.activity:SetText(colour .. (r.activity or "Group") .. (r.delisted and " (closed)" or "") .. (colour ~= "" and "|r" or ""))
        row.title:SetText(r.name or "") -- protected text, shown as is
        row.age:SetText(r.new and not r.delisted and (GREEN .. ageText(r) .. "|r") or ageText(r))
        row.summary:SetText((r.leader and (r.leader .. "  ") or "") .. summaryText(r))
        local height = ROW_HEIGHT
        if expanded[r.id] then
            local lines = {}
            for _, p in ipairs(r.people) do
                lines[#lines + 1] = memberText(p)
            end
            if #lines == 0 then
                lines[1] = GREY .. "The client doesn't say who's in this group.|r"
            end
            row.people:SetText(table.concat(lines, "\n"))
            row.people:Show()
            height = ROW_HEIGHT + #lines * MEMBER_HEIGHT + 6
        else
            row.people:Hide()
        end
        row:SetHeight(height)
        row:ClearAllPoints()
        row:SetPoint("TOPLEFT", list, "TOPLEFT", 0, -y)
        row:SetPoint("RIGHT", list, "RIGHT", 0, 0)
        row:Show()
        y = y + height
    end
    for i = #shown + 1, #rows do
        rows[i]:Hide()
    end
    list:SetHeight(math.max(1, y))
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

-- A ticked box sets filters[key] (or filters.needs[key] when needs is true).
local function checkbox(parent, label, key, anchorTo, x, needs)
    local cb = CreateFrame("CheckButton", nil, parent, "UICheckButtonTemplate")
    cb:SetSize(22, 22)
    cb:SetPoint("LEFT", anchorTo, "RIGHT", x or 8, 0)
    local text = cb:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    text:SetPoint("LEFT", cb, "RIGHT", 0, 1)
    text:SetText(label)
    cb.label = text
    local f = store().filters
    local holder = needs and f.needs or f
    cb:SetChecked(holder[key] and true or false)
    cb:SetScript("OnClick", function(self)
        local filters = store().filters
        local into = needs and filters.needs or filters
        into[key] = self:GetChecked() and true or nil
        redraw()
    end)
    return cb
end

local function build()
    local g = store()
    frame = CreateFrame("Frame", "ForeverBuddyLiteGroups", UIParent, "BasicFrameTemplateWithInset")
    frame:SetSize(560, 480)
    frame:SetFrameStrata("MEDIUM")
    frame:SetClampedToScreen(true)
    frame.redraw = redraw
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
        store().point = { p, rp, x, y }
    end)
    -- Not in UISpecialFrames: Escape leaves it up. The X closes it.
    frame:SetScript("OnHide", function()
        store().shown = false
    end)
    frame:SetScript("OnShow", function()
        store().shown = true
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

    -- Filters.
    local search = CreateFrame("EditBox", nil, frame, "InputBoxTemplate")
    search:SetSize(130, 20)
    search:SetPoint("TOPLEFT", 20, -62)
    search:SetAutoFocus(false)
    search:SetText(g.filters.text or "")
    search:SetScript("OnTextChanged", function(self)
        store().filters.text = self:GetText()
        redraw()
    end)
    search:SetScript("OnEscapePressed", search.ClearFocus)
    search:SetScript("OnEnterPressed", search.ClearFocus)
    local searchHint = search:CreateFontString(nil, "OVERLAY", "GameFontDisableSmall")
    searchHint:SetPoint("LEFT", 2, 0)
    searchHint:SetText("Find an activity")
    searchHint:SetShown((g.filters.text or "") == "")
    search:HookScript("OnTextChanged", function(self)
        searchHint:SetShown(self:GetText() == "")
    end)

    -- Row one: the search box, then which open seats you want.
    local tank = checkbox(frame, "Needs tank", "TANK", search, 8, true)
    local healer = checkbox(frame, "Needs healer", "HEALER", tank.label, 6, true)
    checkbox(frame, "Needs DPS", "DAMAGER", healer.label, 6, true)

    -- Row two: who you'd rather not share loot with, your level, closed listings.
    local _, className, armor = me()
    local anchor = frame:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    anchor:SetPoint("TOPLEFT", 14, -88)
    anchor:SetText("")
    local noArmor = checkbox(frame, armor and ("No " .. armor .. " wearers") or "No same armor", "noArmor", anchor, 0)
    local noClass = checkbox(frame, "No other " .. (className or "of my class") .. "s", "noClass", noArmor.label, 6)
    local near = checkbox(frame, "Near my level", "nearLevel", noClass.label, 6)
    checkbox(frame, "Hide closed", "hideClosed", near.label, 6)

    local scroll = CreateFrame("ScrollFrame", nil, frame, "UIPanelScrollFrameTemplate")
    scroll:SetPoint("TOPLEFT", 12, -114)
    scroll:SetPoint("BOTTOMRIGHT", -32, 12)
    list = CreateFrame("Frame", nil, scroll)
    list:SetSize(510, 1)
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
