-- Data: the SavedVariables, their schema and migrations, and the guard that
-- keeps blank captures from replacing good ones.
--
-- ForeverBuddyLiteDB = {
--   schema = 2,                          -- the data's shape, not the addon version
--   characters = { ["Name-Realm"] = { saved_at, addon_version, errors, identity, stats, gear, bags,
--                  bank, mail, quests, skills, talents, spells, reputation, lockouts, buffs, professions,
--                  money_log = { { t, m }, ... } } },   -- every change of gold, for the wealth chart
--   ah = {...}, items = {...}, trainers = {...}, ladders = {...}, journal = {...},   -- account-wide; the reader reads these keys
--   debug = { ah_error, ah_seen, items_missed, skill_info_keys },
--   ui = { point = {...} },
-- }
--
-- The game writes this file only on logout or /reload.

local _, ns = ...

local SCHEMA = 2

-- Each step takes the data from schema n to n + 1.
local MIGRATIONS = {
    -- 1 -> 2: debug leftovers move under debug; "version" was the addon version
    -- written on every load, so it never told the shape apart.
    [1] = function(db)
        db.debug = {
            ah_error = db.ahError,
            ah_seen = db.ahSeen,
            items_missed = db.itemsMissed,
            skill_info_keys = db.skillInfoKeys,
        }
        db.ahError, db.ahSeen, db.itemsMissed, db.skillInfoKeys, db.version = nil, nil, nil, nil, nil
    end,
}

ns.on("ADDON_LOADED", function(name)
    if name ~= "ForeverBuddyLite" then
        return
    end
    local db = type(ForeverBuddyLiteDB) == "table" and ForeverBuddyLiteDB or {}
    ForeverBuddyLiteDB = db
    local from = ns.num(db.schema) or (next(db) and 1) or SCHEMA
    for n = from, SCHEMA - 1 do
        if MIGRATIONS[n] then
            MIGRATIONS[n](db)
        end
    end
    db.schema = SCHEMA
    db.characters = type(db.characters) == "table" and db.characters or {}
    db.debug = type(db.debug) == "table" and db.debug or {}
    db.ui = type(db.ui) == "table" and db.ui or {}
    ns.db = db
    ns.version = ns.read("C_AddOns.GetAddOnMetadata", "ForeverBuddyLite", "Version")
        or ns.read("GetAddOnMetadata", "ForeverBuddyLite", "Version")
    ns.loadedAt = ns.now() -- anything captured after this isn't on disk until the next reload
end)

ns.on("PLAYER_ENTERING_WORLD", function()
    local name, realm = ns.read(UnitFullName, "player")
    realm = realm or ns.read(GetNormalizedRealmName)
    if name and realm then
        ns.key = name .. "-" .. realm
    end
end)

-- This character's table, created on first use; nil before login.
function ns.char()
    local db, key = ns.db, ns.key
    if not db or not key then
        return nil
    end
    db.characters[key] = db.characters[key] or {}
    return db.characters[key]
end

function ns.debug(k, v)
    if ns.db then
        ns.db.debug[k] = v
    end
end

-- Writes t[field] = value, unless value looks blank and what's there doesn't:
-- while the client logs out or reloads it empties bags, gear, skills and stats
-- before addons save, and that must never replace good data.
function ns.commit(t, field, value, isBlank)
    if value == nil then
        return false
    end
    if isBlank and t[field] ~= nil and isBlank(value) and not isBlank(t[field]) then
        return true -- kept what was there
    end
    t[field] = value
    return true
end
