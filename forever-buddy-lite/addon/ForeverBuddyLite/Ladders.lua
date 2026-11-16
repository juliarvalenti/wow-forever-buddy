-- Price ladders: every price level of an item and how many are up at it.
-- Auctionator's price scan browses, and a browse gives only the cheapest price
-- and the total. A search for one item (clicking it in Auctionator, the AH's
-- own Buy tab, Point Blank Sniper) hands every addon the whole ladder. We keep
-- it: per item, the last few searches, each "price:qty;price:qty;..." cheapest
-- first, "*" after a level that holds your own listing. Sellers' names are
-- never read. Listening sends nothing to the server.
--
-- ForeverBuddyLiteDB.ladders = { ["itemID" or "itemID:suffix"] = { { at, total, levels }, ... newest last } }

local _, ns = ...

local KEEP = 24          -- searches kept per item
local LEVELS = 40        -- price levels kept per search
local MAX_AGE = 14 * 86400

local function store(key, rows)
    local db = ns.db
    if not db or #rows == 0 then
        return
    end
    -- Group by unit price (several auctions can share one), cheapest first.
    local byPrice, prices, total = {}, {}, 0
    for _, r in ipairs(rows) do
        local p = byPrice[r.price]
        if not p then
            p = { qty = 0 }
            byPrice[r.price] = p
            prices[#prices + 1] = r.price
        end
        p.qty = p.qty + r.qty
        p.mine = p.mine or r.mine
        total = total + r.qty
    end
    table.sort(prices)
    local parts = {}
    for i = 1, math.min(#prices, LEVELS) do
        local p = byPrice[prices[i]]
        parts[i] = prices[i] .. ":" .. p.qty .. (p.mine and "*" or "")
    end
    db.ladders = db.ladders or {}
    local log = db.ladders[key] or {}
    local entry = { at = ns.now(), total = total, levels = table.concat(parts, ";") }
    local last = log[#log]
    if last and last.levels == entry.levels then
        last.at = entry.at -- same ladder, seen again
    else
        log[#log + 1] = entry
        while #log > KEEP do
            table.remove(log, 1)
        end
    end
    db.ladders[key] = log
    ns.fire("changed")
end

-- Commodities (trade goods, consumables): one ladder per item id.
local function commodity(itemID)
    local A = C_AuctionHouse
    local n = A.GetNumCommoditySearchResults(itemID) or 0
    local rows = {}
    for i = 1, n do
        local r = A.GetCommoditySearchResultInfo(itemID, i)
        if r and r.unitPrice and r.quantity then
            rows[#rows + 1] = { price = r.unitPrice, qty = r.quantity, mine = r.containsOwnerItem }
        end
    end
    store(tostring(itemID), rows)
end

-- Other items (gear, bags): per item and suffix; price per unit of a stack.
local function item(itemKey)
    local A = C_AuctionHouse
    local n = A.GetNumItemSearchResults(itemKey) or 0
    local rows = {}
    for i = 1, n do
        local r = A.GetItemSearchResultInfo(itemKey, i)
        if r and r.buyoutAmount and r.quantity and r.quantity > 0 then
            rows[#rows + 1] = { price = math.floor(r.buyoutAmount / r.quantity), qty = r.quantity, mine = r.containsOwnerItem }
        end
    end
    local suffix = itemKey.itemSuffix or 0
    store(itemKey.itemID .. (suffix ~= 0 and (":" .. suffix) or ""), rows)
end

-- Results arrive in pieces; read each item once they've settled.
local pending = {}
local function later(key, fn, arg)
    if pending[key] then
        return
    end
    pending[key] = true
    C_Timer.After(0.5, function()
        pending[key] = nil
        pcall(fn, arg)
    end)
end

local function onCommodity(itemID)
    if itemID then
        later("c" .. itemID, commodity, itemID)
    end
end
local function onItem(itemKey)
    if type(itemKey) == "table" and itemKey.itemID then
        later("i" .. itemKey.itemID .. ":" .. (itemKey.itemSuffix or 0), item, itemKey)
    end
end
ns.on("COMMODITY_SEARCH_RESULTS_UPDATED", onCommodity)
ns.on("COMMODITY_SEARCH_RESULTS_ADDED", onCommodity)
ns.on("ITEM_SEARCH_RESULTS_UPDATED", onItem)
ns.on("ITEM_SEARCH_RESULTS_ADDED", onItem)

-- Old searches go: two weeks is plenty, and it keeps /reload quick.
ns.on("PLAYER_LOGIN", function()
    local db = ns.db
    if not db or type(db.ladders) ~= "table" then
        return
    end
    local cutoff = ns.now() - MAX_AGE
    for key, log in pairs(db.ladders) do
        for i = #log, 1, -1 do
            if (log[i].at or 0) < cutoff then
                table.remove(log, i)
            end
        end
        if #log == 0 then
            db.ladders[key] = nil
        end
    end
end)

function ns.ladderCount()
    local n, newest = 0, 0
    for _, log in pairs(ns.db and ns.db.ladders or {}) do
        n = n + 1
        newest = math.max(newest, log[#log] and log[#log].at or 0)
    end
    return n, newest
end
