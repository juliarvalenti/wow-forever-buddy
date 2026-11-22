-- Move the game's reactive-ability alerts ("<Riposte>", "<Overpower>", "<Execute>"...) in the
-- floating combat text, for every class, without moving the rest of the combat text.
--
-- Blizzard_CombatText shows them as a message in angle brackets (messageType SPELL_ACTIVE). We
-- hook CombatText:AddMessage after the game adds one: the newest font string gets an offset on its
-- position and on its scroll function, so it starts and travels from the new spot. Other messages
-- (damage, heals, dodges) are left alone. Combat text isn't protected, so this is safe in combat.
--
-- /fbl reactive <x> <y>   offset in screen points (y up), e.g. /fbl reactive 0 150
-- /fbl reactive test      show a sample alert at the current spot
-- /fbl reactive reset     back to the game's position
--
-- ForeverBuddyLiteDB.reactive = { x, y }

local _, ns = ...
local say = ns.say

local hooked = false

local function store()
    local db = ns.db
    if not db then
        return { x = 0, y = 0 }
    end
    db.reactive = db.reactive or { x = 0, y = 0 }
    return db.reactive
end

local function isReactive(message)
    if type(message) ~= "string" then
        return false
    end
    if issecretvalue and issecretvalue(message) then
        return false
    end
    return message:match("^<.+>$") ~= nil
end

local function hook()
    if hooked or not CombatText or type(CombatText.AddMessage) ~= "function" then
        return
    end
    hooked = true
    hooksecurefunc(CombatText, "AddMessage", function(self, message)
        local o = store()
        if (o.x or 0) == 0 and (o.y or 0) == 0 then
            return
        end
        if not isReactive(message) then
            return
        end
        local list = self.activeFontStrings
        local fs = list and list[#list]
        if not fs or not fs.scrollFunction or fs.startX == nil or fs.startY == nil then
            return
        end
        local base = fs.scrollFunction
        local dx, dy = o.x or 0, o.y or 0
        -- AddMessage sets scrollFunction afresh for every message, so a pooled font string never
        -- keeps this offset into a later, non-reactive message.
        fs.scrollFunction = function(frame, value)
            local x, y = base(frame, value)
            return x + dx, y + dy
        end
        fs:ClearAllPoints()
        fs:SetPoint("TOP", WorldFrame, "BOTTOM", fs.startX + dx, fs.startY + dy)
    end)
end

ns.on("ADDON_LOADED", function(name)
    if name == "Blizzard_CombatText" then
        hook()
    end
end)
ns.on("PLAYER_LOGIN", hook)

local function ensureLoaded()
    if not CombatText and type(CombatText_LoadUI) == "function" then
        pcall(CombatText_LoadUI)
    end
    hook()
    return CombatText ~= nil
end

ns.command({ name = "reactive", args = "[<x> <y>, test, reset]",
    help = "move the <Riposte>/<Overpower> combat text alerts (all classes)",
    fn = function(rest)
        local o = store()
        local x, y = rest:match("^(%-?[%d.]+)%s+(%-?[%d.]+)$")
        if x then
            o.x, o.y = tonumber(x), tonumber(y)
            ensureLoaded()
            say(string.format("reactive alerts moved by %d, %d. /fbl reactive test to see it.", o.x, o.y))
        elseif rest == "reset" then
            o.x, o.y = 0, 0
            say("reactive alerts back at the game's position.")
        elseif rest == "test" then
            if not ensureLoaded() then
                say("floating combat text isn't loaded: turn it on in Interface > Combat.")
                return
            end
            CombatText:AddMessage("<Riposte>", CombatTextUtil and CombatTextUtil.StandardScroll, 1, 0.82, 0, "crit")
        else
            say(string.format("reactive alerts offset %d, %d. /fbl reactive <x> <y> (e.g. 0 150 to move them up), test, reset.",
                o.x or 0, o.y or 0))
        end
    end })
