-- Fake game data: a believable, deterministic world for the simulator.
--
-- Anything the game server would normally supply - items, spells, NPCs,
-- group members, bags, auras, and the return values of every documented
-- API function nobody hand-wrote - is generated here from a seed, so the
-- same item ID is always the same item, and tests are repeatable.
-- Real facts (classes, specs, races, zones, a set of well-known spells and
-- items) are used where they matter; names and numbers are invented.
local M = {}

------------------------------------------------------------------ hashing / rng

-- Fast deterministic string hash (polynomial, 31-bit)
local function hash(s)
  s = tostring(s)
  local h = 5381
  for i = 1, #s do h = (h * 33 + s:byte(i)) % 2147483647 end
  return h
end
M.hash = hash

-- Small deterministic generator from a seed string.
function M.rng(seed)
  local state = hash(seed) % 2147483647
  if state == 0 then state = 1 end
  local r = {}
  function r.float()
    state = (state * 48271) % 2147483647
    return state / 2147483647
  end
  function r.int(lo, hi) return lo + math.floor(r.float() * (hi - lo + 1)) end
  function r.pick(t) return t[r.int(1, #t)] end
  function r.chance(p) return r.float() < p end
  return r
end

------------------------------------------------------------------ facts

M.CLASSES = {
  WARRIOR = { id = 1, power = 1, specs = { { 71, "Arms", "DAMAGER" }, { 72, "Fury", "DAMAGER" }, { 73, "Protection", "TANK" } },
    armor = "plate", weapons = { "sword", "axe", "mace" } },
  PALADIN = { id = 2, power = 0, specs = { { 65, "Holy", "HEALER" }, { 66, "Protection", "TANK" }, { 70, "Retribution", "DAMAGER" } },
    armor = "plate", weapons = { "mace", "sword", "hammer" } },
  HUNTER = { id = 3, power = 2, specs = { { 253, "Beast Mastery", "DAMAGER" }, { 254, "Marksmanship", "DAMAGER" }, { 255, "Survival", "DAMAGER" } },
    armor = "mail", weapons = { "bow", "gun", "polearm" } },
  ROGUE = { id = 4, power = 3, specs = { { 259, "Assassination", "DAMAGER" }, { 260, "Outlaw", "DAMAGER" }, { 261, "Subtlety", "DAMAGER" } },
    armor = "leather", weapons = { "dagger", "sword" } },
  PRIEST = { id = 5, power = 0, specs = { { 256, "Discipline", "HEALER" }, { 257, "Holy", "HEALER" }, { 258, "Shadow", "DAMAGER" } },
    armor = "cloth", weapons = { "staff", "wand", "mace" } },
  DEATHKNIGHT = { id = 6, power = 6, specs = { { 250, "Blood", "TANK" }, { 251, "Frost", "DAMAGER" }, { 252, "Unholy", "DAMAGER" } },
    armor = "plate", weapons = { "sword", "axe" } },
  SHAMAN = { id = 7, power = 0, specs = { { 262, "Elemental", "DAMAGER" }, { 263, "Enhancement", "DAMAGER" }, { 264, "Restoration", "HEALER" } },
    armor = "mail", weapons = { "mace", "axe", "staff" } },
  MAGE = { id = 8, power = 0, specs = { { 62, "Arcane", "DAMAGER" }, { 63, "Fire", "DAMAGER" }, { 64, "Frost", "DAMAGER" } },
    armor = "cloth", weapons = { "staff", "wand" } },
  WARLOCK = { id = 9, power = 0, specs = { { 265, "Affliction", "DAMAGER" }, { 266, "Demonology", "DAMAGER" }, { 267, "Destruction", "DAMAGER" } },
    armor = "cloth", weapons = { "staff", "wand", "dagger" } },
  MONK = { id = 10, power = 3, specs = { { 268, "Brewmaster", "TANK" }, { 269, "Windwalker", "DAMAGER" }, { 270, "Mistweaver", "HEALER" } },
    armor = "leather", weapons = { "staff", "polearm" } },
  DRUID = { id = 11, power = 0, specs = { { 102, "Balance", "DAMAGER" }, { 103, "Feral", "DAMAGER" }, { 104, "Guardian", "TANK" }, { 105, "Restoration", "HEALER" } },
    armor = "leather", weapons = { "staff", "mace" } },
  DEMONHUNTER = { id = 12, power = 17, specs = { { 577, "Havoc", "DAMAGER" }, { 581, "Vengeance", "TANK" } },
    armor = "leather", weapons = { "glaive" } },
  EVOKER = { id = 13, power = 0, specs = { { 1467, "Devastation", "DAMAGER" }, { 1468, "Preservation", "HEALER" }, { 1473, "Augmentation", "DAMAGER" } },
    armor = "mail", weapons = { "staff", "axe" } },
}
M.CLASS_LIST = { "WARRIOR", "PALADIN", "HUNTER", "ROGUE", "PRIEST", "DEATHKNIGHT", "SHAMAN", "MAGE", "WARLOCK",
  "MONK", "DRUID", "DEMONHUNTER", "EVOKER" }

M.RACES = {
  Alliance = { "Human", "Dwarf", "NightElf", "Gnome", "Draenei", "Worgen" },
  Horde = { "Orc", "Scourge", "Tauren", "Troll", "BloodElf", "Goblin" },
}

M.ZONES = {
  Alliance = { { "Elwynn Forest", 1429, "Goldshire" }, { "Stormwind City", 1453, "Trade District" }, { "Dun Morogh", 1426, "Kharanos" },
    { "Teldrassil", 1438, "Dolanaar" }, { "Westfall", 1436, "Sentinel Hill" }, { "Duskwood", 1431, "Darkshire" } },
  Horde = { { "Durotar", 1411, "Razor Hill" }, { "Orgrimmar", 1454, "Valley of Strength" }, { "Mulgore", 1412, "Bloodhoof Village" },
    { "Tirisfal Glades", 1420, "Brill" }, { "The Barrens", 1413, "The Crossroads" }, { "Stonetalon Mountains", 1442, "Sun Rock Retreat" } },
}

-- Well-known spells: id, name, icon, class, cast time (ms), cooldown (s)
M.KNOWN_SPELLS = {
  { 133, "Fireball", "spell_fire_flamebolt", "MAGE", 2500, 0 },
  { 116, "Frostbolt", "spell_frost_frostbolt02", "MAGE", 2000, 0 },
  { 118, "Polymorph", "spell_nature_polymorph", "MAGE", 1700, 0 },
  { 1953, "Blink", "spell_arcane_blink", "MAGE", 0, 15 },
  { 1459, "Arcane Intellect", "spell_holy_magicalsentry", "MAGE", 0, 0 },
  { 2061, "Flash Heal", "spell_holy_flashheal", "PRIEST", 1500, 0 },
  { 17, "Power Word: Shield", "spell_holy_powerwordshield", "PRIEST", 0, 0 },
  { 139, "Renew", "spell_holy_renew", "PRIEST", 0, 0 },
  { 589, "Shadow Word: Pain", "spell_shadow_shadowwordpain", "PRIEST", 0, 0 },
  { 100, "Charge", "ability_warrior_charge", "WARRIOR", 0, 20 },
  { 6673, "Battle Shout", "ability_warrior_battleshout", "WARRIOR", 0, 0 },
  { 1784, "Stealth", "ability_stealth", "ROGUE", 0, 2 },
  { 1752, "Sinister Strike", "spell_shadow_ritualofsacrifice", "ROGUE", 0, 0 },
  { 8921, "Moonfire", "spell_nature_starfall", "DRUID", 0, 0 },
  { 774, "Rejuvenation", "spell_nature_rejuvenation", "DRUID", 0, 0 },
  { 8936, "Regrowth", "spell_nature_resistnature", "DRUID", 1500, 0 },
  { 403, "Lightning Bolt", "spell_nature_lightning", "SHAMAN", 2000, 0 },
  { 421, "Chain Lightning", "spell_nature_chainlightning", "SHAMAN", 2000, 6 },
  { 686, "Shadow Bolt", "spell_shadow_shadowbolt", "WARLOCK", 2000, 0 },
  { 172, "Corruption", "spell_shadow_abominationexplosion", "WARLOCK", 0, 0 },
  { 5782, "Fear", "spell_shadow_possession", "WARLOCK", 1700, 0 },
  { 642, "Divine Shield", "spell_holy_divineshield", "PALADIN", 0, 300 },
  { 19750, "Flash of Light", "spell_holy_flashheal", "PALADIN", 1500, 0 },
  { 3044, "Arcane Shot", "ability_impalingbolt", "HUNTER", 0, 0 },
  { 5384, "Feign Death", "ability_rogue_feigndeath", "HUNTER", 0, 30 },
  { 49998, "Death Strike", "spell_deathknight_butcher2", "DEATHKNIGHT", 0, 0 },
  { 100780, "Tiger Palm", "ability_monk_tigerpalm", "MONK", 0, 0 },
  { 162794, "Chaos Strike", "ability_demonhunter_chaosstrike", "DEMONHUNTER", 0, 0 },
  { 361469, "Living Flame", "ability_evoker_livingflame", "EVOKER", 2000, 0 },
  { 8690, "Hearthstone", "inv_misc_rune_01", nil, 10000, 900 },
}

-- Well-known items: id, name, icon, quality, class, subclass, equipLoc, ilvl
M.KNOWN_ITEMS = {
  { 6948, "Hearthstone", "inv_misc_rune_01", 1, 15, 0, "", 1 },
  { 19019, "Thunderfury, Blessed Blade of the Windseeker", "inv_sword_39", 5, 2, 7, "INVTYPE_WEAPON", 80 },
  { 18803, "Finkle's Lava Dredger", "inv_hammer_05", 4, 2, 5, "INVTYPE_2HWEAPON", 70 },
  { 2589, "Linen Cloth", "inv_fabric_linen_01", 1, 7, 5, "", 10 },
  { 4306, "Silk Cloth", "inv_fabric_silk_01", 1, 7, 5, "", 25 },
  { 118, "Minor Healing Potion", "inv_potion_49", 1, 0, 1, "", 5 },
  { 2459, "Swiftness Potion", "inv_potion_95", 1, 0, 1, "", 10 },
  { 4540, "Tough Hunk of Bread", "inv_misc_food_11", 1, 0, 5, "", 5 },
  { 159, "Refreshing Spring Water", "inv_drink_07", 1, 0, 5, "", 5 },
  { 2901, "Mining Pick", "inv_pick_02", 1, 2, 14, "", 10 },
}

------------------------------------------------------------------ icons by category

local assets = require("wowapi.assets")
local iconCats = {}
local function icons(prefixes)
  local key = table.concat(prefixes, ",")
  if iconCats[key] then return iconCats[key] end
  local _, _, list = assets.icons()
  local out = {}
  for _, e in ipairs(list) do
    for _, p in ipairs(prefixes) do
      if e[2]:sub(1, #p) == p then out[#out + 1] = e[1]; break end
    end
  end
  if #out == 0 then out = { 134400 } end
  iconCats[key] = out
  return out
end
M.icons = icons

function M.iconID(name)
  local _, byName = assets.icons()
  return byName[tostring(name):lower()]
end

------------------------------------------------------------------ word lists

local ADJ = { "Gleaming", "Ancient", "Savage", "Runed", "Blessed", "Cursed", "Stormforged", "Emberglow", "Frostwoven",
  "Shadowstalker's", "Gilded", "Ironbound", "Moonlit", "Venomous", "Thunderous", "Sunfire", "Twilight", "Rugged",
  "Veteran's", "Arcanist's", "Tidecaller's", "Wildheart", "Grim", "Noble", "Tempered", "Spellwoven", "Bloodstained" }
local SUFFIX = { "of the Bear", "of the Eagle", "of the Tiger", "of the Monkey", "of the Owl", "of the Whale",
  "of the Falcon", "of the Wolf", "of the Boar", "of Healing", "of Fiery Wrath", "of Frozen Wrath", "of Arcane Wrath",
  "of Power", "of Agility", "of Stamina", "of the Sorcerer", "of the Champion", "of Nimbleness", "of Defense" }
local WEAPON = {
  sword = { { "Blade", "Longsword", "Saber", "Broadsword", "Claymore" }, { "inv_sword_" }, 7, "INVTYPE_WEAPON" },
  axe = { { "Axe", "Cleaver", "Hatchet", "Waraxe" }, { "inv_axe_" }, 0, "INVTYPE_WEAPON" },
  mace = { { "Mace", "Cudgel", "Morningstar", "Scepter" }, { "inv_mace_" }, 4, "INVTYPE_WEAPON" },
  hammer = { { "Warhammer", "Maul", "Greathammer" }, { "inv_hammer_" }, 5, "INVTYPE_2HWEAPON" },
  dagger = { { "Dagger", "Dirk", "Shiv", "Kris" }, { "inv_weapon_shortblade_" }, 15, "INVTYPE_WEAPON" },
  staff = { { "Staff", "Greatstaff", "Spire", "Crook" }, { "inv_staff_" }, 10, "INVTYPE_2HWEAPON" },
  bow = { { "Longbow", "Shortbow", "Recurve" }, { "inv_weapon_bow_" }, 2, "INVTYPE_RANGED" },
  gun = { { "Rifle", "Blunderbuss", "Boomstick" }, { "inv_weapon_rifle_" }, 3, "INVTYPE_RANGED" },
  polearm = { { "Halberd", "Glaive", "Spear", "Pike" }, { "inv_spear_", "inv_polearm_" }, 6, "INVTYPE_2HWEAPON" },
  wand = { { "Wand", "Rod", "Focus" }, { "inv_wand_" }, 19, "INVTYPE_RANGEDRIGHT" },
  glaive = { { "Warglaive", "Glaive" }, { "inv_glaive_" }, 9, "INVTYPE_WEAPON" },
}
local ARMOR_SLOTS = {
  { "Helm", { "inv_helmet_" }, "INVTYPE_HEAD" }, { "Shoulderpads", { "inv_shoulder_" }, "INVTYPE_SHOULDER" },
  { "Chestguard", { "inv_chest_" }, "INVTYPE_CHEST" }, { "Gloves", { "inv_gauntlets_" }, "INVTYPE_HAND" },
  { "Belt", { "inv_belt_" }, "INVTYPE_WAIST" }, { "Leggings", { "inv_pants_" }, "INVTYPE_LEGS" },
  { "Boots", { "inv_boots_" }, "INVTYPE_FEET" }, { "Bracers", { "inv_bracer_" }, "INVTYPE_WRIST" },
  { "Cloak", { "inv_misc_cape_" }, "INVTYPE_CLOAK" }, { "Ring", { "inv_jewelry_ring_" }, "INVTYPE_FINGER" },
  { "Amulet", { "inv_jewelry_necklace_" }, "INVTYPE_NECK" },
}
local ARMOR_TYPES = { cloth = { 1, "Cloth" }, leather = { 2, "Leather" }, mail = { 3, "Mail" }, plate = { 4, "Plate" } }
local CONSUMABLES = {
  { { "Healing Potion", "Mana Potion", "Elixir of Fortitude", "Flask of Endurance" }, { "inv_potion_", "inv_alchemy_" }, 1, "Potion" },
  { { "Roasted Boar", "Spiced Bread", "Fish Stew", "Honeycake", "Mountain Water" }, { "inv_misc_food_", "inv_drink_" }, 5, "Food & Drink" },
  { { "Scroll of Protection", "Scroll of Intellect", "Scroll of Strength" }, { "inv_scroll_" }, 8, "Other" },
}
local TRADE = {
  { { "Copper Ore", "Iron Ore", "Mithril Ore", "Thorium Ore" }, { "inv_ore_" }, 7, "Metal & Stone" },
  { { "Peacebloom", "Silverleaf", "Mageroyal", "Briarthorn" }, { "inv_misc_herb_" }, 9, "Herb" },
  { { "Light Leather", "Heavy Leather", "Rugged Hide" }, { "inv_misc_leatherscrap_", "inv_misc_pelt_" }, 6, "Leather" },
  { { "Arcane Dust", "Mystic Essence", "Radiant Shard" }, { "inv_enchant_" }, 12, "Enchanting" },
}
local SPELL_WORDS = {
  fire = { { "Flame", "Fire", "Ember", "Inferno", "Blaze", "Scorch" }, { "spell_fire_" } },
  frost = { { "Frost", "Ice", "Glacial", "Winter's", "Rime" }, { "spell_frost_" } },
  nature = { { "Thorn", "Storm", "Earthen", "Wild", "Lightning" }, { "spell_nature_" } },
  shadow = { { "Shadow", "Void", "Dread", "Soul", "Umbral" }, { "spell_shadow_" } },
  holy = { { "Holy", "Divine", "Radiant", "Blessed", "Sacred" }, { "spell_holy_" } },
  arcane = { { "Arcane", "Astral", "Mystic", "Starlight" }, { "spell_arcane_" } },
  physical = { { "Savage", "Crushing", "Rending", "Brutal", "Precise" }, { "ability_warrior_", "ability_rogue_", "ability_hunter_" } },
}
local SPELL_NOUNS = { "Bolt", "Strike", "Nova", "Shield", "Barrage", "Ward", "Blast", "Wave", "Rush", "Burst", "Touch", "Lance", "Surge" }
local SCHOOLS = { physical = 1, holy = 2, fire = 4, nature = 8, frost = 16, shadow = 32, arcane = 64 }

local SYL1 = { "Ar", "Bel", "Cor", "Dra", "El", "Fen", "Gar", "Hal", "Is", "Jor", "Kel", "Lor", "Mor", "Nar", "Or",
  "Per", "Quel", "Ral", "Syl", "Thal", "Ul", "Val", "Wyn", "Zul", "Tor", "Ka", "Li", "Ma", "Ny", "Rha" }
local SYL2 = { "adin", "an", "ath", "dor", "eth", "ia", "ic", "in", "ion", "is", "ok", "on", "or", "os", "ra", "ric",
  "ros", "th", "wen", "yn", "ar", "ius", "ella", "grim", "mir", "ka", "zan", "ea" }
local NPC_ADJ = { "Mangy", "Rabid", "Dire", "Young", "Elder", "Vile", "Corrupted", "Feral", "Frenzied", "Hulking" }
local NPC_NOUN = { "Wolf", "Kobold Miner", "Murloc", "Boar", "Gnoll", "Spider", "Bandit", "Skeleton", "Harpy", "Ogre",
  "Stag", "Bear", "Raptor", "Cultist", "Elemental" }
local BOSS = { "Hogger", "Van Cleef", "Mutanus the Devourer", "Lord Serpentis", "Bazzalan", "Taragaman", "Arugal", "Mekgineer Thermaplugg" }
local GUILD_A = { "Knights", "Order", "Circle", "Brotherhood", "Covenant", "Legion", "Sons", "Wardens", "Keepers" }
local GUILD_B = { "of the Silver Hand", "of Stormwind", "of Elune", "of the Iron Forge", "of the Wild", "of Dawn", "of Ashes", "of the Deep" }
local SENTENCES = { "A relic of an older age.", "Smells faintly of murloc.", "It hums with arcane power.",
  "Warm to the touch.", "Crafted by a master smith.", "Its edge never dulls.", "Found in the depths of the Deadmines.",
  "Rumored to have belonged to a king.", "Still covered in kobold candle wax." }

------------------------------------------------------------------ generators

function M.playerName(seed)
  local r = M.rng("name:" .. tostring(seed))
  return r.pick(SYL1) .. r.pick(SYL2)
end

function M.guildName(seed)
  local r = M.rng("guild:" .. tostring(seed))
  return r.pick(GUILD_A) .. " " .. r.pick(GUILD_B)
end

function M.sentence(seed) return M.rng("text:" .. tostring(seed)).pick(SENTENCES) end

local QUALITY_WEIGHTS = { { 0, 0.12 }, { 1, 0.38 }, { 2, 0.28 }, { 3, 0.15 }, { 4, 0.06 }, { 5, 0.01 } }
local QUALITY_HEX = { [0] = "ff9d9d9d", "ffffffff", "ff1eff00", "ff0070dd", "ffa335ee", "ffff8000", "ffe6cc80", "ff00ccff" }

function M.itemLink(id, name, quality)
  return string.format("|c%s|Hitem:%d::::::::60:::::::::|h[%s]|h|r", QUALITY_HEX[quality] or "ffffffff", id, name)
end

-- A complete fake item for any ID.
function M.item(id, level)
  level = level or 60
  for _, k in ipairs(M.KNOWN_ITEMS) do
    if k[1] == id then
      local i = { id = id, name = k[2], icon = M.iconID(k[3]) or 134400, quality = k[4], classID = k[5], subclassID = k[6],
        equipLoc = k[7], itemLevel = k[8], minLevel = 1, stackCount = k[5] == 0 and 20 or (k[5] == 7 and 200 or 1),
        sellPrice = 0, type = ({ [0] = "Consumable", [2] = "Weapon", [7] = "Tradeskill", [15] = "Miscellaneous" })[k[5]] or "Miscellaneous" }
      i.subType = i.type
      i.link = M.itemLink(id, i.name, i.quality)
      return i
    end
  end
  local r = M.rng("item:" .. id)
  local roll, quality = r.float(), 1
  local acc = 0
  for _, q in ipairs(QUALITY_WEIGHTS) do acc = acc + q[2]; if roll <= acc then quality = q[1]; break end end
  local kind = r.float()
  local i = { id = id, quality = quality }
  if kind < 0.3 then
    local wkeys = { "sword", "axe", "mace", "hammer", "dagger", "staff", "bow", "gun", "polearm", "wand" }
    local wk = r.pick(wkeys)
    local w = WEAPON[wk]
    local base = r.pick(w[1])
    i.name = (quality >= 2 and r.pick(ADJ) .. " " or "") .. base .. (quality >= 2 and quality <= 3 and r.chance(0.6) and (" " .. r.pick(SUFFIX)) or "")
    i.classID, i.subclassID, i.equipLoc, i.type, i.subType = 2, w[3], w[4], "Weapon", base
    i.icon = r.pick(icons(w[2]))
    i.stackCount = 1
  elseif kind < 0.6 then
    local slot = r.pick(ARMOR_SLOTS)
    local atype = r.pick({ "cloth", "leather", "mail", "plate" })
    local sub = (slot[3] == "INVTYPE_CLOAK" or slot[3] == "INVTYPE_FINGER" or slot[3] == "INVTYPE_NECK") and { 1, "Miscellaneous" } or ARMOR_TYPES[atype]
    i.name = (quality >= 2 and r.pick(ADJ) .. " " or "") .. slot[1] .. (quality >= 2 and quality <= 3 and r.chance(0.6) and (" " .. r.pick(SUFFIX)) or "")
    i.classID, i.subclassID, i.equipLoc, i.type, i.subType = 4, sub[1], slot[3], "Armor", sub[2]
    i.icon = r.pick(icons(slot[2]))
    i.stackCount = 1
  elseif kind < 0.8 then
    local c = r.pick(CONSUMABLES)
    i.name = r.pick(c[1])
    i.classID, i.subclassID, i.equipLoc, i.type, i.subType = 0, c[3], "", "Consumable", c[4]
    i.icon = r.pick(icons(c[2]))
    i.stackCount = 20
    i.quality = math.min(quality, 2)
  else
    local t = r.pick(TRADE)
    i.name = r.pick(t[1])
    i.classID, i.subclassID, i.equipLoc, i.type, i.subType = 7, t[3], "", "Tradeskill", t[4]
    i.icon = r.pick(icons(t[2]))
    i.stackCount = 200
    i.quality = math.min(quality, 2)
  end
  i.itemLevel = math.max(1, level + r.int(-8, 8) + i.quality * 3)
  i.minLevel = math.max(1, math.min(level, i.itemLevel - 5))
  i.sellPrice = math.floor(i.itemLevel * (i.quality + 1) * r.int(20, 120))
  i.description = r.chance(0.2) and M.sentence(id) or nil
  i.bindType = i.quality >= 3 and 1 or (i.classID == 2 or i.classID == 4) and 2 or 0
  i.link = M.itemLink(id, i.name, i.quality)
  return i
end

-- Does a (fake) spell/item with this ID exist? Roughly 1 in 4 IDs do, like
-- the sparse real ID space; well-known ones always do.
function M.exists(kind, id)
  if type(id) ~= "number" or id <= 0 or id ~= math.floor(id) then return false end
  return hash(kind .. ":exists:" .. id) % 4 == 0
end

-- A complete fake spell for any ID.
function M.spell(id)
  for _, k in ipairs(M.KNOWN_SPELLS) do
    if k[1] == id then
      return { id = id, name = k[2], icon = M.iconID(k[3]) or 136243, class = k[4], castTime = k[5],
        cooldown = { startTime = 0, duration = 0 }, cooldownSeconds = k[6], minRange = 0, maxRange = k[5] > 0 and 40 or 5,
        description = "Deals damage or heals. You know how it goes." }
    end
  end
  local r = M.rng("spell:" .. id)
  local schoolNames = { "fire", "frost", "nature", "shadow", "holy", "arcane", "physical" }
  local school = r.pick(schoolNames)
  local w = SPELL_WORDS[school]
  local s = { id = id, name = r.pick(w[1]) .. " " .. r.pick(SPELL_NOUNS), icon = r.pick(icons(w[2])),
    school = SCHOOLS[school], castTime = r.pick({ 0, 0, 1500, 2000, 2500, 3000 }),
    minRange = 0, maxRange = r.pick({ 5, 30, 40 }), cooldownSeconds = r.pick({ 0, 0, 0, 6, 12, 30, 60, 120 }),
    cooldown = { startTime = 0, duration = 0 } }
  s.description = string.format("Deals %d %s damage to an enemy.", r.int(100, 2000), school)
  return s
end

-- A player-character unit.
function M.character(seed, opts)
  opts = opts or {}
  local r = M.rng("char:" .. tostring(seed))
  local class = opts.class or r.pick(M.CLASS_LIST)
  local info = M.CLASSES[class]
  local faction = opts.faction or r.pick({ "Alliance", "Horde" })
  local race = opts.race or r.pick(M.RACES[faction])
  local level = opts.level or 60
  local specIndex = opts.spec or r.int(1, #info.specs)
  local spec = info.specs[specIndex]
  local healthMax = opts.healthMax or math.floor(level * level * (spec[3] == "TANK" and 2.2 or 1.4) + 200)
  local u = {
    name = opts.name or M.playerName(seed), realm = opts.realm or "Forever", class = class, race = race, faction = faction,
    level = level, sex = r.pick({ 2, 3 }), healthMax = healthMax, health = opts.health or healthMax,
    powerType = info.power, powerMax = info.power == 0 and level * 60 or (info.power == 1 or info.power == 6 or info.power == 17) and 100 or 100,
    guid = string.format("Player-1-%08X", hash("guid:" .. tostring(seed)) % 4294967295),
    role = spec[3], spec = specIndex, specID = spec[1], isPlayer = true,
  }
  u.power = (info.power == 1 or info.power == 6 or info.power == 17) and 0 or u.powerMax
  local specs = {}
  for _, sp in ipairs(info.specs) do specs[#specs + 1] = { id = sp[1], name = sp[2], role = sp[3], icon = 136243,
    description = sp[2] .. " specialization." } end
  u.specs = specs
  u.numSpecs = #specs
  return u
end

local CREATURE_TYPES = { "Beast", "Humanoid", "Undead", "Elemental", "Demon", "Dragonkin", "Mechanical", "Giant" }

-- An enemy NPC.
function M.npc(seed, opts)
  opts = opts or {}
  local r = M.rng("npc:" .. tostring(seed))
  local boss = opts.boss or (opts.classification == "worldboss")
  local level = opts.level or (boss and 63 or r.int(55, 62))
  local name = opts.name or (boss and r.pick(BOSS) or (r.pick(NPC_ADJ) .. " " .. r.pick(NPC_NOUN)))
  local hp = opts.healthMax or (boss and level * 20000 or level * r.int(60, 140))
  return { name = name, level = level, hostile = opts.hostile ~= false, health = opts.health or hp, healthMax = hp,
    powerType = 0, power = 0, powerMax = 0, classification = opts.classification or (boss and "worldboss" or r.pick({ "normal", "normal", "normal", "elite", "rare" })),
    creatureType = opts.creatureType or r.pick(CREATURE_TYPES), faction = "Neutral", isPlayer = false,
    guid = string.format("Creature-0-1-2-3-%d-%08X", r.int(100, 250000), hash("npcguid:" .. tostring(seed)) % 4294967295),
    class = "WARRIOR", race = "Human", realm = "Forever" }
end

-- An aura table (C_UnitAuras AuraData shape) for a spell.
function M.aura(spell, opts)
  opts = opts or {}
  return { name = spell.name, spellId = spell.id, icon = spell.icon, applications = opts.stacks or 0,
    duration = opts.duration or 60, expirationTime = opts.expirationTime or ((opts.now or 0) + (opts.duration or 60)),
    isHelpful = opts.harmful ~= true, isHarmful = opts.harmful == true, sourceUnit = opts.source or "player",
    dispelName = opts.dispelName, canApplyAura = true, isStealable = false, isBossAura = false,
    isFromPlayerOrPlayerPet = (opts.source or "player") == "player", nameplateShowAll = false, timeMod = 1 }
end

------------------------------------------------------------------ generic API values

local docs -- lazy
local NAMEISH = { name = true, title = true, label = true, displayName = true, fullName = true }

local function fakeScalar(r, fieldName, typ, kind, ctx)
  local n = (fieldName or ""):lower()
  if kind == "bool" then
    if n:find("^is") or n:find("^has") or n:find("^can") then return r.chance(0.35) end
    return r.chance(0.5)
  end
  if kind == "enum" then
    local vals = {}
    for _, v in pairs(docs.data().enums[typ]) do vals[#vals + 1] = v end
    table.sort(vals)
    return #vals > 0 and r.pick(vals) or 0
  end
  if kind == "number" then
    if typ == "fileID" or n:find("icon") or n:find("texture") or n:find("fileid") then return r.pick(icons({ "inv_", "spell_", "ability_" })) end
    if typ == "time_t" or n:find("timestamp") then return 1790000000 - r.int(0, 86400 * 30) end
    if n == "level" or n:find("level$") then return r.int(1, 60) end
    if n:find("itemid$") then return r.int(1000, 200000) end
    if n:find("spellid$") then return r.int(100, 400000) end
    if n:find("id$") or typ == "luaIndex" and n:find("index") then return r.int(1, 99999) end
    if n:find("percent") or n:find("pct") then return r.int(0, 100) end
    if n:find("num") or n:find("count") or n:find("quantity") or n:find("stack") or n:find("amount") then return r.int(0, 8) end
    if n:find("duration") or n:find("time") or n:find("seconds") then return r.int(0, 600) end
    if n:find("quality") then return r.int(1, 4) end
    if n:find("price") or n:find("cost") or n:find("money") or n:find("copper") then return r.int(1, 500000) end
    if n == "x" or n == "y" or n:find("position") then return math.floor(r.float() * 1000) / 1000 end
    if n:find("max") then return r.int(10, 100) end
    return r.int(0, 100)
  end
  if kind == "string" then
    if typ == "WOWGUID" or n:find("guid") then return string.format("Player-1-%08X", r.int(1, 2147483646)) end
    if n:find("link") then local it = M.item(r.int(1000, 200000)); return it.link end
    if n:find("atlas") then return "UI-HUD-UnitFrame-Player-PortraitOn" end
    if n:find("unit") then return "player" end
    if n:find("realm") then return "Forever" end
    if n:find("zone") or n:find("map") then return r.pick(M.ZONES.Alliance)[1] end
    if n:find("guild") then return M.guildName(r.int(1, 1e6)) end
    if n:find("desc") or n:find("text") or n:find("tooltip") or n:find("message") or n:find("body") then return M.sentence(r.int(1, 1e6)) end
    if NAMEISH[fieldName or ""] or n:find("name") or n:find("title") then
      if ctx and ctx:lower():find("item") then return M.item(r.int(1000, 200000)).name end
      if ctx and ctx:lower():find("spell") then return M.spell(r.int(100, 400000)).name end
      if ctx and (ctx:lower():find("unit") or ctx:lower():find("friend") or ctx:lower():find("guild") or ctx:lower():find("club")) then
        return M.playerName(r.int(1, 1e6))
      end
      return r.pick(ADJ):gsub("'s$", "") .. " " .. r.pick({ "Expedition", "Challenge", "Reward", "Task", "Journey", "Feat", "Trial" })
    end
    return r.pick({ "Forever", "Azeroth", "Elwynn", "Durotar", "Dalaran" })
  end
  if kind == "function" then return function() end end
  return nil
end

-- Generate a plausible value of documented type `typ`.
function M.value(r, typ, nilable, inner, fieldName, ctx, depth)
  docs = docs or require("wowapi.docs")
  depth = depth or 0
  local kind = docs.kind(typ)
  if kind == "table" then
    if depth > 3 then return {} end
    local d = docs.data()
    if typ == "table" then
      if not inner then return {} end
      local out = {}
      for i = 1, r.int(1, 3) do out[i] = M.value(r, inner, false, nil, fieldName, ctx, depth + 1) end
      return out
    end
    local s = {}
    for _, f in ipairs(d.structures[typ] or {}) do
      if f[5] ~= nil then s[f[1]] = f[5]
      elseif not f[3] or r.chance(0.7) then s[f[1]] = M.value(r, f[2], f[3], f[4], f[1], typ, depth + 1) end
    end
    return s
  end
  if kind == nil then return nil end
  return fakeScalar(r, fieldName, typ, kind, ctx)
end

-- Fake returns for a documented function call.
function M.returns(key, doc, ...)
  local args = { ... }
  -- index-based lookups that may return nothing end after a plausible count
  local mayBeEmpty = false
  for _, f in ipairs(doc.f or {}) do if f == "MayReturnNothing" then mayBeEmpty = true end end
  for i, a in ipairs(doc.a) do
    local v = args[i]
    if type(v) == "number" and (a[2] == "luaIndex" or a[1]:lower():find("index")) then
      local cap = 3 + hash(key .. ":count") % 8
      if v > cap or v < 1 then
        if mayBeEmpty then return {}, 0 end
        local out = {}
        for j, r in ipairs(doc.r) do if not r[3] then out[j] = M.value(M.rng(key), r[2], r[3], r[4], r[1], key) end end
        return out, #doc.r
      end
    end
  end
  local parts = { key }
  for i = 1, select("#", ...) do parts[#parts + 1] = tostring(args[i]) end
  local r = M.rng(table.concat(parts, "|"))
  local out = {}
  for i, ret in ipairs(doc.r) do
    if ret[5] ~= nil then out[i] = ret[5]
    else out[i] = M.value(r, ret[2], ret[3], ret[4], ret[1], key) end
  end
  return out, #doc.r
end

return M
