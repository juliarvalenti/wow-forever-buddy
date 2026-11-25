-- Auction house gear.
-- When a scan runs (ours via /fbl ah, Auctionator's, anyone's), the client
-- hands the results to every addon. We keep the equippable ones, for any
-- class: link (suffix included), slot, type, required level, stat lines and
-- the lowest buyout, merged by item and suffix. Sellers' names are never read.
-- Listening sends nothing to the server; only /fbl ah asks the AH for pages.

local _, ns = ...
local read, num, now, say = ns.read, ns.num, ns.now, ns.say

local building -- { at, total, items = { [key] = {...} }, full, complete } while a pass runs
local ahOpen = false

function ns.ahIsOpen()
    return ahOpen or (AuctionHouseFrame and AuctionHouseFrame:IsShown()) or false
end

-- item:id:enchant:gem1..4:suffix:... -> "id:suffix"
local function listingKey(link)
    local id, suffix = link:match("|Hitem:(%d+):[^:]*:[^:]*:[^:]*:[^:]*:[^:]*:([^:]*)")
    return id and (id .. ":" .. (suffix ~= "" and suffix or "0")) or nil
end

-- The kept items are stored packed: CBOR, deflated, base64. Several times
-- smaller than a Lua table on disk, so /reload stays quick. The reader
-- unpacks it. A client without C_EncodingUtil stores the plain table.
local function packItems(items)
    local E = C_EncodingUtil
    if type(E) ~= "table" or not E.SerializeCBOR or not E.CompressString or not E.EncodeBase64 then
        return nil
    end
    local ok, s = pcall(function()
        local method = Enum and Enum.CompressionMethod and Enum.CompressionMethod.Deflate
        return E.EncodeBase64(E.CompressString(E.SerializeCBOR(items), method))
    end)
    return ok and type(s) == "string" and s or nil
end

local function unpackItems(ah)
    if type(ah) ~= "table" then
        return nil
    end
    if type(ah.items) == "table" then
        return ah.items
    end
    local E = C_EncodingUtil
    if type(ah.packed) ~= "string" or type(E) ~= "table" then
        return nil
    end
    local ok, t = pcall(function()
        local method = Enum and Enum.CompressionMethod and Enum.CompressionMethod.Deflate
        return E.DeserializeCBOR(E.DecompressString(E.DecodeBase64(ah.packed), method))
    end)
    return ok and type(t) == "table" and t or nil
end

-- Two sources feed one collector. Each returns true when the result is done
-- with (kept or skipped), false when the item isn't loaded yet.

-- A browse result: { itemKey = { itemID, itemLevel, itemSuffix }, minPrice, totalQuantity }.
local function fromBrowse(r)
    if type(r) ~= "table" or type(r.itemKey) ~= "table" or not num(r.minPrice) or r.minPrice <= 0
        or r.totalQuantity == 0 then
        return true
    end
    local itemKey = r.itemKey
    local itemID, suffix = itemKey.itemID, num(itemKey.itemSuffix) or 0
    -- Removed or unknown items still show up in browses now and then.
    if read("C_Item.DoesItemExistByID", itemID) == false then
        return true
    end
    local _, itemType, subType, equipLoc, _, classID, subClassID = read("C_Item.GetItemInfoInstant", itemID)
    if not ns.isGear(equipLoc, classID) then
        return true
    end
    local id = itemID .. ":" .. suffix
    if building.items[id] then
        return true
    end
    local link = "item:" .. itemID .. "::::::" .. (suffix ~= 0 and suffix or "")
    -- The tooltip carries the stats; it's empty until the client has the item,
    -- so ask for it and come back.
    local data = read("C_TooltipInfo.GetItemKey", itemID, itemKey.itemLevel or 0, suffix)
    local tip = type(data) == "table" and type(data.lines) == "table" and #data.lines > 1 and ns.tooltipStatLines(data)
    if not tip then
        tip = ns.statLines(link)
        if not tip or #tip == 0 then
            read("C_Item.RequestLoadItemDataByID", itemID)
            return false
        end
    end
    -- The AH's tooltip leaves out requirements ("Requires Level 20",
    -- "Requires Engineering (120)"); the item's own tooltip has them.
    local seen = {}
    for _, t in ipairs(tip) do
        seen[t] = true
    end
    for _, t in ipairs(ns.statLines(link) or {}) do
        if t:find("^Requires ") and not seen[t] then
            tip[#tip + 1] = t
        end
    end
    -- The AH's own name for the key (it has it from the browse), else the item cache.
    local keyInfo = read("C_AuctionHouse.GetItemKeyInfo", itemKey)
    local name = type(keyInfo) == "table" and keyInfo.itemName or read("C_Item.GetItemNameByID", itemID)
    local quality = type(keyInfo) == "table" and keyInfo.quality or nil
    local _, _, _, _, minLevel = ns.itemInfo(itemID)
    building.items[id] = { id = itemID, link = link, name = name, quality = quality, slot = equipLoc,
        type = itemType, subtype = subType, class = classID, subclass = subClassID,
        req = num(minLevel), ilvl = num(itemKey.itemLevel),
        buyout = r.minPrice, listings = r.totalQuantity, tip = tip }
    return true
end

-- A replicate (whole-AH) listing by index; several listings merge into one item.
local function fromReplicate(i)
    local name, _, count, quality, _, _, _, _, _, buyout, _, _, _, _, _, _, itemID, hasAll =
        read("C_AuctionHouse.GetReplicateItemInfo", i)
    if not num(itemID) or not num(buyout) or buyout <= 0 then
        return true -- nothing to keep (bid only, or empty)
    end
    local _, itemType, subType, equipLoc, _, classID, subClassID = read("C_Item.GetItemInfoInstant", itemID)
    if not ns.isGear(equipLoc, classID) then
        return true
    end
    local link = read("C_AuctionHouse.GetReplicateItemLink", i)
    if not link or hasAll == false then
        read("C_Item.RequestLoadItemDataByID", itemID)
        return false
    end
    local id = listingKey(link)
    if not id then
        return true
    end
    local each = math.floor(buyout / math.max(1, num(count) or 1))
    local it = building.items[id]
    if it then
        it.listings = it.listings + 1
        if each < it.buyout then
            it.buyout = each
        end
        return true
    end
    local _, _, _, _, minLevel = ns.itemInfo(link)
    building.items[id] = { id = itemID, link = link, name = name, quality = quality, slot = equipLoc,
        type = itemType, subtype = subType, class = classID, subclass = subClassID, req = minLevel,
        buyout = each, listings = 1, tip = ns.statLines(link) }
    return true
end

local function finish(unloaded)
    local db = ns.db
    -- A small browse (a search you typed) updates what's there; a full scan
    -- (/fbl ah, or a whole-AH browse of 500+) replaces it.
    local items = building.items
    -- Someone else's whole-AH browse counts as a full scan too, but only when it holds both
    -- weapons and armor: a big search in one category (all one-handers) must not wipe the rest.
    local weapons, armor = false, false
    for _, v in pairs(items) do
        weapons = weapons or v.class == 2
        armor = armor or v.class == 4
    end
    local full = building.full or (building.total >= 500 and weapons and armor)
    local old = not full and unpackItems(db.ah)
    if old then
        for k, v in pairs(items) do
            old[k] = v
        end
        items = old
    end
    local n = ns.count(items)
    local packed = packItems(items)
    db.ah = { at = building.at, listings = old and db.ah.listings or building.total, unloaded = unloaded, count = n,
        packed = packed, items = not packed and items or nil, complete = building.complete }
    building = nil
    if full then
        say(string.format("kept %d equippable items from the auction house scan. /fbl r to save.", n))
    end
    ns.fire("changed")
end

-- A few hundred results a frame, so the client doesn't stall. Only unloaded
-- items left: wait a second, a few times.
local function work(jobs, tries)
    local left, budget = {}, 300
    for _, job in ipairs(jobs) do
        if budget > 0 then
            budget = budget - 1
            local ok, done = pcall(job)
            if ok and not done then
                left[#left + 1] = job
            end
        else
            left[#left + 1] = job
        end
    end
    if #left > 0 and (budget <= 0 or tries < 5) then
        C_Timer.After(budget > 0 and 1 or 0, function()
            local ok, err = pcall(work, left, budget > 0 and tries + 1 or tries)
            if not ok then
                ns.debug("ah_error", tostring(err))
                building = nil
            end
        end)
        return
    end
    finish(#left)
end

local function safely(fn, ...)
    -- A failure here would be silent (Lua errors are hidden by default): keep it for the reader.
    local ok, err = pcall(fn, ...)
    if not ok then
        ns.debug("ah_error", tostring(err))
        building = nil
    end
end

-- A whole-AH (replicate) scan's results, once they've settled.
local function replicated()
    local n = read("C_AuctionHouse.GetNumReplicateItems")
    if not num(n) or n <= 0 or building or not ns.db then
        return
    end
    building = { at = now(), total = n, items = {} }
    local jobs = {}
    for i = 0, n - 1 do -- replicate indices are 0-based
        jobs[#jobs + 1] = function() return fromReplicate(i) end
    end
    work(jobs, 0)
end

-- Auctionator's "Full Scan" on Forever is a browse scan: it pages through
-- C_AuctionHouse.GetBrowseResults. One pass, 3 s after the last page arrives.
local browseTimer
local function browsed(scan)
    browseTimer = nil
    local results = read("C_AuctionHouse.GetBrowseResults")
    if type(results) ~= "table" or #results == 0 or not ns.db then
        return
    end
    if building then
        -- Still working through an earlier page: come back, don't drop this one.
        browseTimer = C_Timer.NewTimer(3, function()
            browsed(scan)
        end)
        return
    end
    ns.debug("ah_seen", { at = now(), results = #results }) -- what the last browse looked like
    building = { at = now(), total = #results, items = {}, full = scan ~= nil, complete = scan and scan.complete }
    local jobs = {}
    for i = 1, #results do
        local r = results[i]
        jobs[#jobs + 1] = function() return fromBrowse(r) end
    end
    work(jobs, 0)
end

-- /fbl ah: our own scan. One empty search, weapons and armor only, then
-- "more results" page after page until the AH says that's all. The AH
-- throttles requests; when it isn't ready we wait for it to say so.
local active -- { pages, failures, asked, waiting, watch, complete }

local function scanDone(complete, why)
    local scan = active
    active = nil
    if scan.watch then
        scan.watch:Cancel()
    end
    if why then
        say(why)
    end
    scan.complete = complete
    if browseTimer then
        browseTimer:Cancel()
    end
    safely(browsed, scan)
end

local function scanAsk()
    local scan = active
    if not scan then
        return
    end
    if read("C_AuctionHouse.IsThrottledMessageSystemReady") == false then
        scan.waiting = true -- AUCTION_HOUSE_THROTTLED_SYSTEM_READY sends it
        return
    end
    scan.waiting = false
    scan.asked = GetTime()
    if scan.pages == 0 then
        C_AuctionHouse.SendBrowseQuery({ searchString = "", sorts = {}, filters = {},
            itemClassFilters = { { classID = 2 }, { classID = 4 } } }) -- weapons, armor
    else
        C_AuctionHouse.RequestMoreBrowseResults()
    end
end

local function scanStart()
    if active then
        say("already scanning (" .. active.pages .. " pages so far).")
        return
    end
    if not ns.ahIsOpen() then
        say("open the auction house first, then /fbl ah.")
        return
    end
    if not (C_AuctionHouse and C_AuctionHouse.SendBrowseQuery) then
        say("this client has no auction house browse; use Auctionator's full scan instead.")
        return
    end
    active = { pages = 0, failures = 0, asked = GetTime() }
    say("scanning weapons and armor...")
    -- No answer for 15 s: ask again. Nothing for a minute: keep what we have.
    active.watch = C_Timer.NewTicker(5, function()
        local scan = active
        if not scan then
            return
        end
        local quiet = GetTime() - scan.asked
        if quiet > 60 then
            scanDone(false, "the auction house stopped answering; keeping the " .. scan.pages .. " pages it sent.")
        elseif quiet > 15 and not scan.waiting then
            scanAsk()
        end
    end)
    scanAsk()
end

local function onBrowse()
    if active then
        local scan = active
        scan.pages = scan.pages + 1
        scan.asked = GetTime()
        if read("C_AuctionHouse.HasFullBrowseResults") then
            scanDone(true)
        else
            if scan.pages % 10 == 0 then
                local r = read("C_AuctionHouse.GetBrowseResults")
                say(string.format("%d items so far...", type(r) == "table" and #r or 0))
            end
            scanAsk()
        end
        return
    end
    if browseTimer then
        browseTimer:Cancel()
    end
    browseTimer = C_Timer.NewTimer(3, function()
        safely(browsed)
    end)
end

ns.on("AUCTION_HOUSE_BROWSE_RESULTS_UPDATED", onBrowse)
ns.on("AUCTION_HOUSE_BROWSE_RESULTS_ADDED", onBrowse)

ns.on("AUCTION_HOUSE_THROTTLED_SYSTEM_READY", function()
    if active and active.waiting then
        scanAsk()
    end
end)

-- A failed browse of ours used to stall without a word: retry twice, then say so.
ns.on("AUCTION_HOUSE_BROWSE_FAILURE", function()
    local scan = active
    if not scan then
        return -- someone else's search
    end
    ns.debug("ah_error", "browse failed at " .. date("%Y-%m-%d %H:%M:%S"))
    scan.failures = scan.failures + 1
    if scan.failures <= 2 then
        C_Timer.After(2, scanAsk)
    else
        scanDone(false, "the auction house refused the scan three times; keeping the " .. scan.pages .. " pages it sent.")
    end
end)

ns.on("AUCTION_HOUSE_SHOW", function()
    ahOpen = true
    ns.fire("changed")
end)
ns.on("AUCTION_HOUSE_CLOSED", function()
    ahOpen = false
    if active then
        scanDone(false, "auction house closed mid-scan; keeping the " .. active.pages .. " pages it sent.")
    end
    ns.fire("changed")
end)

-- Fires as a whole-AH scan's results arrive; one pass once they've settled.
ns.on("REPLICATE_ITEM_LIST_UPDATE", function()
    if not building then
        C_Timer.After(2, replicated)
    end
end)

ns.command({ name = "ah", aliases = { "scan" }, help = "with the auction house open: scan every weapon and armor listing",
    fn = scanStart })

-- /fbl market: one key for a price scan. First press starts Auctionator's full
-- scan (its own button's code: browse or replicate, as its settings say); when
-- Auctionator says its prices are in, the next press saves and reloads, which
-- writes Auctionator's data and ours to disk. The game only sends the AH our
-- requests from a key or click, so this stays one action per press.
-- ns.market is the state the window shows: idle, scanning (with progress), ready.
ns.market = { state = "idle", progress = 0 }

local function setMarket(state, progress)
    ns.market.state, ns.market.progress = state, progress or ns.market.progress
    if state == "ready" then
        ns.market.at = now()
    end
    ns.fire("changed")
end

function ns.marketReady()
    return ns.market.state == "ready"
end

-- When Auctionator last scanned, by its own record (browse or replicate).
function ns.lastPriceScan()
    local s = _G.Auctionator and Auctionator.SavedState
    return ns.market.at or (s and (s.TimeOfLastBrowseScan or s.TimeOfLastReplicateScan))
end

local function auctionatorScan()
    local A = _G.Auctionator
    local state = A and A.State
    if not state then
        say("Auctionator isn't loaded.")
        return
    end
    local replicate = A.Config and A.Config.Get and A.Config.Options and read(A.Config.Get, A.Config.Options.REPLICATE_SCAN)
    local frame = replicate and state.FullScanFrameRef or state.IncrementalScanFrameRef
    if not frame or not frame.InitiateScan then
        say("Auctionator's scanner isn't ready; open its tab in the auction house once.")
        return
    end
    frame:InitiateScan()
    setMarket("scanning", 0)
end

-- Auctionator's event bus: start, progress, done (prices written to its database), failed.
ns.on("PLAYER_LOGIN", function()
    local A = _G.Auctionator
    if not (A and A.EventBus and A.IncrementalScan and A.IncrementalScan.Events) then
        return
    end
    local I, F = A.IncrementalScan.Events, A.FullScan and A.FullScan.Events or {}
    local events = {}
    for _, e in ipairs({ I.ScanStart, I.ScanProgress, I.PricesProcessed, I.ScanFailed, F.ScanStart, F.ScanProgress, F.ScanComplete, F.ScanFailed }) do
        if e then
            events[#events + 1] = e
        end
    end
    pcall(A.EventBus.Register, A.EventBus, {
        ReceiveEvent = function(_, event, progress)
            if event == I.ScanStart or event == F.ScanStart then
                setMarket("scanning", 0)
            elseif event == I.ScanProgress or event == F.ScanProgress then
                setMarket("scanning", ns.num(progress) or ns.market.progress)
            elseif event == I.PricesProcessed or event == F.ScanComplete then
                setMarket("ready", 1)
                say("Auctionator's prices are in. Press again (or Save prices & reload) to write them to disk.")
            elseif event == I.ScanFailed or event == F.ScanFailed then
                setMarket("idle", 0)
                say("Auctionator's scan failed; try again in a moment.")
            end
        end,
    }, events)
end)

ns.on("AUCTION_HOUSE_CLOSED", function()
    if ns.market.state == "scanning" then
        setMarket("idle", 0) -- Auctionator stops when the AH closes
    end
end)

ns.command({ name = "market", aliases = { "mk" }, help = "one key for prices: start Auctionator's scan, press again when done to save",
    fn = function()
        if ns.market.state == "ready" then
            ns.run("r")
        elseif ns.market.state == "scanning" then
            say(string.format("still scanning (%d%%).", math.floor((ns.market.progress or 0) * 100)))
        elseif ns.ahIsOpen() then
            auctionatorScan()
        else
            say("open the auction house first.")
        end
    end })
