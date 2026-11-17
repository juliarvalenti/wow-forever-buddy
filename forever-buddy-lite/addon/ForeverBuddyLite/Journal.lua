-- The trade journal: every auction house buy and sale, from the invoice on its mail.
--
-- Each AH mail carries an invoice (GetInboxInvoiceInfo): "buyer" for something you won or bought
-- (what you paid), "seller" for something that sold (the price, the deposit you get back, the AH's
-- cut). When the mailbox shows its contents we record each invoice once, so the reader can say what
-- each item cost you on average, what it sold for after fees, and what's still in stock. Sellers'
-- and buyers' names are never read.
--
-- Pending sales ("seller_temp_invoice", money still on its way) are skipped: the final "seller" mail
-- replaces them. Expired auctions carry no invoice, so a lost deposit isn't recorded.
--
-- ForeverBuddyLiteDB.journal = { entries = { { t, kind = "buy"|"sell", item, count, price, deposit,
--   cut, money, char }, ... oldest first } }

local _, ns = ...
local read, num, now = ns.read, ns.num, ns.now

local KEEP = 3000 -- entries kept; older ones drop off
local MAIL_DAYS = 30 -- AH mail lasts 30 days: from days left we know when it was sent
local SAME_MAIL = 600 -- seconds: a recorded entry this close to a mail's send time is that mail

local function journal()
    local db = ns.db
    if not db then
        return nil
    end
    db.journal = type(db.journal) == "table" and db.journal or {}
    local j = db.journal
    j.entries = type(j.entries) == "table" and j.entries or {}
    j.seen = nil -- an earlier build's dedupe table
    return j
end

-- Every invoice in the mailbox now: { kind, item, count, price, deposit, cut, money, sent }.
local function inbox()
    local mails = {}
    for i = 1, num(read(GetInboxNumItems)) or 0 do
        local kind, itemName, _, bid, buyout, deposit, consignment, _, _, _, count = read(GetInboxInvoiceInfo, i)
        if (kind == "buyer" or kind == "seller") and type(itemName) == "string" then
            local _, _, _, _, money, _, daysLeft = read(GetInboxHeaderInfo, i)
            mails[#mails + 1] = {
                kind = kind == "buyer" and "buy" or "sell",
                item = itemName,
                count = num(count) or 1,
                price = num(bid) or num(buyout) or 0, -- what was paid, or what it sold for (before the cut)
                deposit = num(deposit) or 0,
                cut = num(consignment) or 0,
                money = num(money) or 0, -- gold on the mail (a sale's net)
                sent = math.floor(now() - (MAIL_DAYS - (num(daysLeft) or MAIL_DAYS)) * 86400),
            }
        end
    end
    return mails
end

local function same(a, b)
    return a.kind == b.kind and a.item == b.item and a.count == b.count and a.price == b.price
end

-- Adds each invoice not yet recorded. Identical mails (two stacks sold at one price) are told apart
-- by number: as many get recorded as the mailbox holds, minus those already recorded near that time.
local function recordInbox()
    local j = journal()
    if not j then
        return
    end
    local added = 0
    local mails = inbox()
    for i, mail in ipairs(mails) do
        local inMailbox = 0
        for k = 1, i do
            if same(mails[k], mail) and math.abs(mails[k].sent - mail.sent) < SAME_MAIL then
                inMailbox = inMailbox + 1
            end
        end
        local recorded = 0
        for k = #j.entries, math.max(1, #j.entries - 500), -1 do
            local e = j.entries[k]
            if same(e, mail) and math.abs(e.t - mail.sent) < SAME_MAIL then
                recorded = recorded + 1
            end
        end
        if recorded < inMailbox then
            j.entries[#j.entries + 1] = { t = mail.sent, kind = mail.kind, item = mail.item, count = mail.count,
                price = mail.price, deposit = mail.deposit, cut = mail.cut, money = mail.money, char = ns.key }
            added = added + 1
        end
    end
    if added > 0 then
        while #j.entries > KEEP do
            table.remove(j.entries, 1)
        end
        ns.fire("changed")
    end
end

local soon = ns.debounce(1, recordInbox)
ns.on("MAIL_INBOX_UPDATE", soon)
ns.on("MAIL_SHOW", soon)

function ns.journalCount()
    local j = ns.db and ns.db.journal
    return j and type(j.entries) == "table" and #j.entries or 0
end
