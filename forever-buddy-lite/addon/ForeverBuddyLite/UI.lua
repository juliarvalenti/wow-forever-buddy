-- The /fbl window: what's been captured and when, and a button per action.
-- Built the first time it opens. The reader is where the data gets used;
-- this only shows that it's there.

local _, ns = ...
local ago, count = ns.ago, ns.count

local RED, GREY, GOLD = "|cffff6060", "|cff9d9d9d", "|cffffd200"

-- One line per source: "Label   when, how much". Also used by /fbl status.
function ns.statusLines()
    local db, c = ns.db, ns.char()
    local lines = {}
    local function add(label, text)
        lines[#lines + 1] = GOLD .. label .. "|r  " .. (text or GREY .. "never|r")
    end
    if not db then
        add("Data", RED .. "not loaded|r")
        return lines
    end
    local newest = 0
    local function seen(t)
        if ns.num(t) and t > newest then
            newest = t
        end
        return ago(t)
    end
    if c then
        local errs = {}
        for name in pairs(c.errors or {}) do
            errs[#errs + 1] = name
        end
        table.sort(errs)
        add("Character", string.format("%s, %d/%d sections%s", seen(c.saved_at),
            ns.SECTION_COUNT - #errs, ns.SECTION_COUNT, #errs > 0 and (RED .. " (" .. table.concat(errs, ", ") .. ")|r") or ""))
        local bagItems = 0
        for _, b in pairs(c.bags and c.bags.bags or {}) do
            bagItems = bagItems + count(b.items)
        end
        add("Bags", string.format("%d items", bagItems))
        add("Bank", c.bank and (seen(c.bank.at)) or GREY .. "open your bank once|r")
        add("Mail", c.mail and string.format("%s, %d letters", seen(c.mail.at), count(c.mail.letters)) or nil)
        local profs = {}
        for name, p in pairs(c.professions or {}) do
            profs[#profs + 1] = string.format("%s %s", name, seen(p.at))
        end
        table.sort(profs)
        add("Professions", #profs > 0 and table.concat(profs, ", ") or GREY .. "open a profession window|r")
    end
    local ah = db.ah
    add("Auction house", ah and string.format("%s, %d items%s%s", seen(ah.at), ah.count or 0,
        ah.complete == false and " (partial)" or "",
        db.debug.ah_error and (RED .. "  last error: " .. tostring(db.debug.ah_error) .. "|r") or "") or nil)
    local itemsAt, nItems = 0, 0
    for _, it in pairs(db.items or {}) do
        nItems = nItems + 1
        itemsAt = math.max(itemsAt, it.at or 0)
    end
    add("Quest & dungeon gear", nItems > 0 and string.format("%s, %d items", seen(itemsAt), nItems) or nil)
    local trainersAt = 0
    for _, t in pairs(db.trainers or {}) do
        trainersAt = math.max(trainersAt, t.at or 0)
    end
    add("Trainers", count(db.trainers) > 0 and string.format("%s, %d seen", seen(trainersAt), count(db.trainers)) or nil)
    local m = ns.market or {}
    add("Prices", m.state == "scanning" and string.format("scanning, %d%%", math.floor((m.progress or 0) * 100))
        or m.state == "ready" and (GOLD .. "scan done, save to write it to disk|r")
        or ns.lastPriceScan and ns.lastPriceScan() and ("Auctionator scan " .. ago(ns.lastPriceScan())) or nil)
    -- The game writes the file only on reload or logout.
    if ns.loadedAt and newest > ns.loadedAt then
        lines[#lines + 1] = ""
        lines[#lines + 1] = GOLD .. "Captured since the last reload, not on disk yet: Save & Reload.|r"
    end
    return lines
end

local frame, statusText, buttons, priceButton, priceBar

-- The price button's face for each state: label, clickable, tooltip.
local function priceFace()
    local m = ns.market or {}
    if m.state == "ready" then
        return "Save prices & reload", true, "Auctionator's scan is done. This saves it and ours to disk (reloads the UI)."
    elseif m.state == "scanning" then
        return string.format("Scanning prices... %d%%", math.floor((m.progress or 0) * 100)), false,
            "Auctionator is scanning. Keep the auction house open."
    elseif ns.ahIsOpen() then
        return "Start price scan", true, "Runs Auctionator's full scan. When it's done, this button saves it."
    end
    return "Start price scan", false, "Open the auction house to scan prices."
end

local function refresh()
    if not frame or not frame:IsShown() then
        return
    end
    statusText:SetText(table.concat(ns.statusLines(), "\n"))
    local label, on, tip = priceFace()
    priceButton:SetText(label)
    priceButton:SetEnabled(on)
    priceButton.tip = tip
    local m = ns.market or {}
    priceBar:SetValue(m.state == "ready" and 1 or m.state == "scanning" and (m.progress or 0) or 0)
    priceBar:SetShown(m.state ~= "idle")
    local inCombat = InCombatLockdown()
    for _, b in ipairs(buttons) do
        b:SetEnabled(not b.enabled or b.enabled(inCombat))
    end
end

local ACTIONS = {
    { "Snapshot", function() ns.run("snap") end },
    { "Save & Reload", function() ns.run("r") end },
    { "Scan AH", function() ns.run("ah") end, function() return ns.ahIsOpen() end },
    { "Find quest gear", function() ns.run("items") end },
    { "Buff reminders", function() ns.run("wa") end, function(inCombat) return ns.aurasAvailable() and not inCombat end },
    { "Rogue bars", function() ns.run("wa", "bars") end, function(inCombat) return ns.aurasAvailable() and not inCombat end },
}

local function build()
    frame = CreateFrame("Frame", "ForeverBuddyLiteFrame", UIParent, "BasicFrameTemplateWithInset")
    frame:SetSize(380, 420)
    frame:SetFrameStrata("DIALOG")
    local point = ns.db and ns.db.ui.point
    if type(point) == "table" and point[1] then
        frame:SetPoint(point[1], UIParent, point[2], point[3], point[4])
    else
        frame:SetPoint("CENTER")
    end
    frame:SetMovable(true)
    frame:EnableMouse(true)
    frame:RegisterForDrag("LeftButton")
    frame:SetScript("OnDragStart", frame.StartMoving)
    frame:SetScript("OnDragStop", function(self)
        self:StopMovingOrSizing()
        local p, _, rp, x, y = self:GetPoint()
        if ns.db then
            ns.db.ui.point = { p, rp, x, y }
        end
    end)
    frame:SetScript("OnShow", refresh)
    table.insert(UISpecialFrames, "ForeverBuddyLiteFrame") -- Esc closes it

    local title = frame.TitleText or frame:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    if not frame.TitleText then
        title:SetPoint("TOP", 0, -5)
    end
    title:SetText("ForeverBuddy Lite  " .. GREY .. ns.versionString() .. "|r")

    statusText = frame:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    statusText:SetPoint("TOPLEFT", 16, -34)
    statusText:SetPoint("RIGHT", -16, 0)
    statusText:SetJustifyH("LEFT")
    statusText:SetJustifyV("TOP")
    statusText:SetSpacing(3)

    buttons = {}
    local w, h = 165, 24
    local rows = math.ceil(#ACTIONS / 2)

    -- Prices: one wide button that walks through scan -> scanning -> save, with a bar under it.
    priceButton = CreateFrame("Button", nil, frame, "UIPanelButtonTemplate")
    priceButton:SetSize(w * 2 + 8, 28)
    priceButton:SetPoint("BOTTOMLEFT", 16, 14 + rows * (h + 6) + 14)
    priceButton:SetScript("OnClick", function()
        ns.run("market")
        refresh()
    end)
    priceButton:SetMotionScriptsWhileDisabled(true)
    priceButton:SetScript("OnEnter", function(self)
        GameTooltip:SetOwner(self, "ANCHOR_TOP")
        GameTooltip:SetText(self.tip or "", 1, 1, 1, 1, true)
        GameTooltip:Show()
    end)
    priceButton:SetScript("OnLeave", function()
        GameTooltip:Hide()
    end)
    priceBar = CreateFrame("StatusBar", nil, frame)
    priceBar:SetSize(w * 2 + 8, 4)
    priceBar:SetPoint("TOPLEFT", priceButton, "BOTTOMLEFT", 0, -3)
    priceBar:SetStatusBarTexture("Interface\\TargetingFrame\\UI-StatusBar")
    priceBar:SetStatusBarColor(0.25, 0.55, 0.95)
    priceBar:SetMinMaxValues(0, 1)
    local barBack = priceBar:CreateTexture(nil, "BACKGROUND")
    barBack:SetAllPoints()
    barBack:SetColorTexture(1, 1, 1, 0.08)
    local priceLabel = frame:CreateFontString(nil, "OVERLAY", "GameFontNormalSmall")
    priceLabel:SetPoint("BOTTOMLEFT", priceButton, "TOPLEFT", 2, 4)
    priceLabel:SetText("Prices")

    for i, a in ipairs(ACTIONS) do
        local b = CreateFrame("Button", nil, frame, "UIPanelButtonTemplate")
        b:SetSize(w, h)
        local row, col = math.floor((i - 1) / 2), (i - 1) % 2
        b:SetPoint("BOTTOMLEFT", 16 + col * (w + 8), 14 + (rows - 1 - row) * (h + 6))
        b:SetText(a[1])
        b:SetScript("OnClick", function()
            a[2]()
            refresh()
        end)
        b.enabled = a[3]
        buttons[#buttons + 1] = b
    end
end

function ns.toggleWindow()
    if not frame then
        build()
    end
    frame:SetShown(not frame:IsShown())
end

ns.listen("changed", refresh)
ns.on("PLAYER_REGEN_DISABLED", refresh)
ns.on("PLAYER_REGEN_ENABLED", refresh)

ns.command({ name = "status", help = "print what's been captured and when, and which build this is", fn = function()
    ns.say("build " .. ns.versionString())
    for _, line in ipairs(ns.statusLines()) do
        if line ~= "" then
            print(line)
        end
    end
end })
