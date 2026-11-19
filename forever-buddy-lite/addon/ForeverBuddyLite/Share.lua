-- Share what you're looking at with your group, like Dota's alt-click: a buff or debuff (who has
-- it, how long is left, stacks) or a cooldown (ready, or how long until it is), as a clickable
-- spell or item link in party/raid chat. Solo, it prints to your own chat instead.
--
-- Two triggers:
-- - Alt+left-click a buff or debuff, anywhere: your buffs, the target's, unit frames from other
--   addons. Clicking an aura does nothing else.
-- - A key bound to "Share what I'm hovering" (Key Bindings > AddOns): for action buttons, bag and
--   equipped items, the spellbook, and auras too. Not a click there: clicking an action button
--   casts it (alt-click self-casts), so the key is the safe way.
--
-- What's under the mouse comes from the game tooltip: whatever frame shows it calls
-- GameTooltip:SetUnitAura / SetAction / SetBagItem / ..., and we remember that call. In combat the
-- client may hide some values from addons ("secret values"), mostly other units' aura timers; the
-- message then leaves out what it couldn't read.
--
-- ForeverBuddyLiteDB.share = { off = true } to turn the alt-click off (/fbl share off).

local _, ns = ...
local read, say = ns.read, ns.say

local THROTTLE = 1 -- seconds between messages
local hovered -- { kind = "aura" | "action" | "spell" | "bag" | "inventory", ... }
local lastSent = 0

local function store()
    local db = ns.db
    if not db then
        return {}
    end
    db.share = type(db.share) == "table" and db.share or {}
    return db.share
end

-- A value the client let us read (not nil, not a hidden "secret").
local function plain(v)
    if v == nil then
        return nil
    end
    local secret = rawget(_G, "issecretvalue")
    if secret and secret(v) then
        return nil
    end
    return v
end

local function duration(seconds)
    seconds = math.floor(seconds + 0.5)
    if seconds < 60 then
        return seconds .. "s"
    end
    return string.format("%dm %02ds", math.floor(seconds / 60), seconds % 60)
end

local function spellLink(id, name)
    return (id and (read("C_Spell.GetSpellLink", id) or read("GetSpellLink", id))) or (name and ("[" .. name .. "]")) or "?"
end

local function unitLabel(unit)
    if unit == "player" then
        return "me"
    end
    local name = plain((read("UnitName", unit)))
    return name or unit
end

-- Aura data by index or by instance id, as one table of readable values.
local function auraData(h)
    local a
    if h.instance then
        a = read("C_UnitAuras.GetAuraDataByAuraInstanceID", h.unit, h.instance)
    elseif h.index then
        a = read("C_UnitAuras.GetAuraDataByIndex", h.unit, h.index, h.filter)
        if type(a) ~= "table" then
            local name, _, count, _, dur, expires, _, _, _, spellId = read("UnitAura", h.unit, h.index, h.filter)
            if name then
                a = { name = name, applications = count, duration = dur, expirationTime = expires, spellId = spellId }
            end
        end
    end
    if type(a) ~= "table" then
        return nil
    end
    return {
        name = plain(a.name),
        spellId = plain(a.spellId),
        stacks = plain(a.applications),
        duration = plain(a.duration),
        expires = plain(a.expirationTime),
        harmful = h.filter and tostring(h.filter):find("HARMFUL") and true or a.isHarmful == true,
    }
end

local function describeAura(h)
    local a = auraData(h)
    if not a or not (a.name or a.spellId) then
        return nil, "can't read that aura right now (hidden in combat?)"
    end
    local parts = { spellLink(a.spellId, a.name) .. " on " .. unitLabel(h.unit) }
    if type(a.stacks) == "number" and a.stacks > 1 then
        parts[#parts + 1] = a.stacks .. " stacks"
    end
    local now = GetTime()
    if type(a.expires) == "number" and a.expires > 0 and a.expires > now then
        parts[#parts + 1] = duration(a.expires - now) .. " left"
    end
    return table.concat(parts, ", ")
end

-- "ready" or "ready in 1m 42s" from a start/duration pair; a global cooldown counts as ready.
local function readiness(start, dur)
    start, dur = plain(start), plain(dur)
    if type(start) ~= "number" or type(dur) ~= "number" then
        return nil
    end
    local left = start + dur - GetTime()
    if dur <= 1.5 or left <= 0 then
        return "READY"
    end
    return "ready in " .. duration(left)
end

local function spellCooldown(id)
    local c = read("C_Spell.GetSpellCooldown", id)
    if type(c) == "table" then
        return readiness(c.startTime, c.duration)
    end
    local start, dur = read("GetSpellCooldown", id)
    return readiness(start, dur)
end

local function itemCooldown(id)
    local start, dur = read("C_Container.GetItemCooldown", id)
    if start == nil then
        start, dur = read("GetItemCooldown", id)
    end
    return readiness(start, dur)
end

local function describeSpell(id)
    local name = ns.spellName(id)
    local cd = spellCooldown(id)
    return spellLink(id, name) .. (cd and (" " .. cd) or "")
end

local function describeItem(id, link)
    link = link or select(2, ns.itemInfo(id)) or ("[" .. (ns.itemName(id) or "item") .. "]")
    local cd = itemCooldown(id)
    local count = read("C_Item.GetItemCount", id) or read("GetItemCount", id)
    local parts = { link }
    if cd then
        parts[1] = parts[1] .. " " .. cd
    end
    if type(count) == "number" and count > 1 then
        parts[#parts + 1] = count .. " left"
    end
    return table.concat(parts, ", ")
end

local function describeAction(slot)
    local kind, id = read("GetActionInfo", slot)
    if kind == "spell" and id then
        return describeSpell(id)
    elseif kind == "item" and id then
        return describeItem(id)
    elseif kind == "macro" and id then
        local spell = read("GetMacroSpell", id)
        if spell then
            return describeSpell(spell)
        end
        local _, link = read("GetMacroItem", id)
        local itemID = link and ns.itemID(link)
        if itemID then
            return describeItem(itemID, link)
        end
    end
    return nil, "nothing to share on that button"
end

local function describe(h)
    if h.kind == "aura" then
        return describeAura(h)
    elseif h.kind == "action" then
        return describeAction(h.slot)
    elseif h.kind == "spell" then
        return describeSpell(h.id)
    elseif h.kind == "bag" then
        local link = read("C_Container.GetContainerItemLink", h.bag, h.slot) or read("GetContainerItemLink", h.bag, h.slot)
        local id = link and ns.itemID(link)
        return id and describeItem(id, link) or nil, "no item there"
    elseif h.kind == "inventory" then
        local link = read("GetInventoryItemLink", h.unit or "player", h.slot)
        local id = link and ns.itemID(link)
        if not id then
            return nil, "no item there"
        end
        local cd = readiness(read("GetInventoryItemCooldown", h.unit or "player", h.slot))
        return link .. (cd and (" " .. cd) or "")
    end
end

local function channel()
    if read("IsInRaid") then
        return "RAID"
    elseif read("IsInGroup", 2) then -- an instance group
        return "INSTANCE_CHAT"
    elseif read("IsInGroup") then
        return "PARTY"
    end
end

local function share(h)
    if not h then
        return
    end
    local now = GetTime()
    if now - lastSent < THROTTLE then
        return
    end
    local ok, msg, why = pcall(describe, h)
    if not ok or not msg then
        say(type(why) == "string" and why or "couldn't read that.")
        return
    end
    lastSent = now
    local to = channel()
    if to then
        local send = ns.resolve("C_ChatInfo.SendChatMessage") or SendChatMessage
        pcall(send, msg, to)
    else
        say(msg .. " |cff9d9d9d(solo: only you see this)|r")
    end
end

function FBL_ShareHovered()
    if GameTooltip:IsShown() then
        share(hovered)
    end
end

-- Remember what the tooltip is showing ------------------------------------------------------

local function hook(method, fn)
    if type(GameTooltip[method]) == "function" then
        hooksecurefunc(GameTooltip, method, function(_, ...)
            hovered = fn(...)
        end)
    end
end

hook("SetUnitAura", function(unit, index, filter)
    return { kind = "aura", unit = unit, index = index, filter = filter }
end)
hook("SetUnitBuff", function(unit, index, filter)
    return { kind = "aura", unit = unit, index = index, filter = filter or "HELPFUL" }
end)
hook("SetUnitDebuff", function(unit, index, filter)
    return { kind = "aura", unit = unit, index = index, filter = filter or "HARMFUL" }
end)
hook("SetUnitBuffByAuraInstanceID", function(unit, instance)
    return { kind = "aura", unit = unit, instance = instance, filter = "HELPFUL" }
end)
hook("SetUnitDebuffByAuraInstanceID", function(unit, instance)
    return { kind = "aura", unit = unit, instance = instance, filter = "HARMFUL" }
end)
hook("SetAction", function(slot)
    return { kind = "action", slot = slot }
end)
hook("SetSpellByID", function(id)
    return { kind = "spell", id = id }
end)
hook("SetBagItem", function(bag, slot)
    return { kind = "bag", bag = bag, slot = slot }
end)
hook("SetInventoryItem", function(unit, slot)
    return { kind = "inventory", unit = unit, slot = slot }
end)
GameTooltip:HookScript("OnHide", function()
    hovered = nil
end)

-- Alt+left-click on an aura: watch the mouse button while an aura tooltip is up. The click
-- itself still goes to whatever frame is under it (on an aura, that does nothing).
local watcher = CreateFrame("Frame")
local wasDown = false
watcher:SetScript("OnUpdate", function()
    local down = IsMouseButtonDown("LeftButton") and true or false
    if down and not wasDown and hovered and hovered.kind == "aura" and IsAltKeyDown() and not store().off
        and GameTooltip:IsShown() then
        share(hovered)
    end
    wasDown = down
end)

BINDING_NAME_FBL_SHARE = "Share what I'm hovering (buff, debuff, cooldown, item)"

ns.command({ name = "share", args = "[on | off]",
    help = "alt-click a buff or debuff (or bind a key) to post it to your group with time left",
    fn = function(rest)
        if rest == "off" then
            store().off = true
            say("alt-click sharing off (the key binding still works).")
        elseif rest == "on" then
            store().off = nil
            say("alt-click sharing on.")
        else
            say("alt+left-click a buff or debuff to share it; bind \"Share what I'm hovering\" in Key Bindings > AddOns for cooldowns and items. /fbl share off turns alt-click off.")
        end
    end })
