-- The WoW: Forever global API (mainline 12.x style, Interface 16001).
--
-- Everything an addon can see lives in `env`. The standard Lua library is
-- trimmed to what the client exposes (no io/os/require), and WoW's own
-- globals are layered on top.
local compat = require("wowapi.compat")
local widgets = require("wowapi.widgets")
local unpack = compat.unpack

local M = {}

-- math.random that is deterministic per Sim (seed via opts.seed).
local function opts_random(sim)
  local seed = sim.opts.seed or 12345
  local state = seed % 2147483647
  if state <= 0 then state = state + 2147483646 end
  return function(m, n)
    state = (state * 16807) % 2147483647
    local r = (state - 1) / 2147483646
    if m == nil then return r end
    if n == nil then m, n = 1, m end
    return m + math.floor(r * (n - m + 1))
  end
end


local CLASSES = {
  { "WARRIOR", "Warrior", { 0.78, 0.61, 0.43 } },
  { "PALADIN", "Paladin", { 0.96, 0.55, 0.73 } },
  { "HUNTER", "Hunter", { 0.67, 0.83, 0.45 } },
  { "ROGUE", "Rogue", { 1.00, 0.96, 0.41 } },
  { "PRIEST", "Priest", { 1.00, 1.00, 1.00 } },
  { "DEATHKNIGHT", "Death Knight", { 0.77, 0.12, 0.23 } },
  { "SHAMAN", "Shaman", { 0.00, 0.44, 0.87 } },
  { "MAGE", "Mage", { 0.25, 0.78, 0.92 } },
  { "WARLOCK", "Warlock", { 0.53, 0.53, 0.93 } },
  { "MONK", "Monk", { 0.00, 1.00, 0.60 } },
  { "DRUID", "Druid", { 1.00, 0.49, 0.04 } },
  { "DEMONHUNTER", "Demon Hunter", { 0.64, 0.19, 0.79 } },
  { "EVOKER", "Evoker", { 0.20, 0.58, 0.50 } },
}

local RACES = {
  Human = 1, Orc = 2, Dwarf = 3, NightElf = 4, Scourge = 5, Tauren = 6, Gnome = 7, Troll = 8,
  Goblin = 9, BloodElf = 10, Draenei = 11, Worgen = 22, Pandaren = 24,
}
local RACE_NAMES = { Scourge = "Undead", NightElf = "Night Elf", BloodElf = "Blood Elf" }

local POWER_TOKENS = { [0] = "MANA", "RAGE", "FOCUS", "ENERGY", "COMBO_POINTS", "RUNES",
  "RUNIC_POWER", "SOUL_SHARDS", "LUNAR_POWER", "HOLY_POWER", "ALTERNATE", "MAELSTROM",
  "CHI", "INSANITY", [16] = "ARCANE_CHARGES", [17] = "FURY", [18] = "PAIN", [19] = "ESSENCE" }

local QUALITY_COLORS = {
  [0] = { 0.62, 0.62, 0.62 }, { 1, 1, 1 }, { 0.12, 1, 0 }, { 0, 0.44, 0.87 },
  { 0.64, 0.21, 0.93 }, { 1, 0.5, 0 }, { 0.9, 0.8, 0.5 }, { 0, 0.8, 1 },
}

local GLOBAL_STRINGS = {
  OKAY = "Okay", CANCEL = "Cancel", ACCEPT = "Accept", DECLINE = "Decline", YES = "Yes", NO = "No",
  CLOSE = "Close", SETTINGS = "Settings", OPTIONS = "Options", ENABLE = "Enable", DISABLE = "Disable",
  DEFAULT = "Default", NONE = "None", UNKNOWN = "Unknown", LEVEL = "Level", NAME = "Name",
  ADDONS = "AddOns", RESET = "Reset", SAVE = "Save", DELETE = "Delete", EDIT = "Edit",
  GOLD_AMOUNT = "%d Gold", SILVER_AMOUNT = "%d Silver", COPPER_AMOUNT = "%d Copper",
  HEALTH = "Health", MANA = "Mana", RAGE = "Rage", ENERGY = "Energy", FOCUS = "Focus",
  ERR_NOT_IN_COMBAT = "You can't do that while in combat",
  SPELL_FAILED_NOT_IN_COMBAT = "You can't do that while in combat",
}

local SOUNDKIT = {
  IG_MAINMENU_OPEN = 850, IG_MAINMENU_CLOSE = 851, IG_MAINMENU_OPTION = 852,
  IG_MAINMENU_OPTION_CHECKBOX_ON = 856, IG_MAINMENU_OPTION_CHECKBOX_OFF = 857,
  IG_CHARACTER_INFO_TAB = 841, IG_CHARACTER_INFO_OPEN = 839, IG_CHARACTER_INFO_CLOSE = 840,
  IG_QUEST_LIST_OPEN = 875, IG_QUEST_LIST_CLOSE = 876, U_CHAT_SCROLL_BUTTON = 1115,
  TELL_MESSAGE = 3081, RAID_WARNING = 8959, READY_CHECK = 8960, ALARM_CLOCK_WARNING_3 = 12889,
  IG_PLAYER_INVITE = 880, LOOT_WINDOW_COIN_SOUND = 120, UI_BNET_TOAST = 18019,
}

function M.install(sim, env)
  env._G = env

  ------------------------------------------------------------ Lua stdlib
  for _, k in ipairs({ "assert", "error", "ipairs", "pairs", "next", "pcall", "xpcall", "select",
    "tonumber", "tostring", "type", "setmetatable", "getmetatable", "rawget", "rawset", "rawequal",
    "coroutine" }) do
    rawset(env, k, _G[k])
  end
  rawset(env, "unpack", unpack)
  -- a Blizzard placeholder frame answers unknown keys through a fallback;
  -- addons still see the widget's real metatable, as in the client
  rawset(env, "getmetatable", function(o)
    local st = sim.widgetState[o]
    if st and st.baseMeta then return st.baseMeta end
    return getmetatable(o)
  end)
  rawset(env, "newproxy", rawget(_G, "newproxy") or function(mt)
    local p = {}
    if mt then setmetatable(p, {}) end
    return p
  end)
  -- WoW's Lua 5.1 xpcall passes extra arguments to the function (like 5.2+);
  -- stock 5.1 drops them.
  if compat.is51 and not compat.isJIT then
    rawset(env, "xpcall", function(f, handler, ...)
      local n = select("#", ...)
      if n == 0 then return xpcall(f, handler) end
      local args = { ... }
      return xpcall(function() return f(unpack(args, 1, n)) end, handler)
    end)
  end
  rawset(env, "_VERSION", "Lua 5.1")
  rawset(env, "bit", compat.bit)
  rawset(env, "collectgarbage", function(opt)
    if opt == "count" then return collectgarbage("count") end
    return 0
  end)
  rawset(env, "gcinfo", function() return collectgarbage("count") end)
  rawset(env, "loadstring", function(src, name) return compat.loadstring(src, name or src, env) end)
  if compat.is51 then
    rawset(env, "getfenv", function(f)
      if f == nil or f == 0 or f == 1 then return env end
      return getfenv(f)
    end)
    rawset(env, "setfenv", setfenv)
  end

  -- string library + WoW extensions (also reachable as ("x"):trim()).
  local S = string
  local function strtrim(s, chars)
    chars = chars and ("[" .. chars:gsub("[%]%[%%%^%-]", "%%%0") .. "]") or "%s"
    return (s:gsub("^" .. chars .. "+", ""):gsub(chars .. "+$", ""))
  end
  local function strsplit(delims, s, pieces)
    if type(s) ~= "string" then error("bad argument #2 to 'strsplit' (string expected)", 2) end
    local set = "[" .. delims:gsub("[%]%[%%%^%-]", "%%%0") .. "]"
    local out, pos = {}, 1
    while true do
      if pieces and #out == pieces - 1 then break end
      local a, b = s:find(set, pos)
      if not a then break end
      out[#out + 1] = s:sub(pos, a - 1)
      pos = b + 1
    end
    out[#out + 1] = s:sub(pos)
    return unpack(out)
  end
  local function strjoin(delim, ...)
    local t = { ... }
    for i = 1, select("#", ...) do t[i] = tostring(t[i]) end
    return table.concat(t, delim)
  end
  local function strsplittable(delims, s, pieces) return { strsplit(delims, s, pieces) } end
  S.trim, S.split, S.join, S.splittable = strtrim, strsplit, strjoin, strsplittable
  rawset(env, "string", S)
  local G = {
    strlen = S.len, strsub = S.sub, strupper = S.upper, strlower = S.lower, strfind = S.find,
    strmatch = S.match, gsub = S.gsub, gmatch = S.gmatch, format = S.format, strrep = S.rep,
    strbyte = S.byte, strchar = S.char, strrev = S.reverse, strtrim = strtrim, strsplit = strsplit,
    strjoin = strjoin, strsplittable = strsplittable,
    strconcat = function(...) return strjoin("", ...) end,
    tostringall = function(...)
      local t = { ... }
      for i = 1, select("#", ...) do t[i] = tostring(t[i]) end
      return unpack(t, 1, select("#", ...))
    end,
    strlenutf8 = function(s) local _, n = s:gsub("[^\128-\191]", ""); return n end,
    strcmputf8i = function(a, b) a, b = a:lower(), b:lower(); return a < b and -1 or (a > b and 1 or 0) end,
  }
  for k, v in pairs(G) do rawset(env, k, v) end

  -- table
  local T = {}
  for k, v in pairs(table) do T[k] = v end
  local function wipe(t) for k in pairs(t) do t[k] = nil end return t end
  T.wipe = wipe
  T.getn = T.getn or function(t) return #t end
  T.unpack = unpack
  T.maxn = T.maxn or function(t) local n = 0; for k in pairs(t) do if type(k) == "number" and k > n then n = k end end return n end
  T.removemulti = function(t, pos, count) for _ = 1, count or 1 do table.remove(t, pos) end end
  rawset(env, "table", T)
  rawset(env, "wipe", wipe)
  rawset(env, "tinsert", table.insert)
  rawset(env, "tremove", table.remove)
  rawset(env, "sort", table.sort)
  rawset(env, "getn", T.getn)
  rawset(env, "tContains", function(t, v) for _, x in pairs(t) do if x == v then return true end end return false end)
  rawset(env, "tIndexOf", function(t, v) for i, x in ipairs(t) do if x == v then return i end end end)
  rawset(env, "tInvert", function(t) local o = {}; for k, v in pairs(t) do o[v] = k end return o end)
  rawset(env, "tDeleteItem", function(t, v)
    local n = 0
    for i = #t, 1, -1 do if t[i] == v then table.remove(t, i); n = n + 1 end end
    return n
  end)
  rawset(env, "tAppendAll", function(t, src) for _, v in ipairs(src) do t[#t + 1] = v end end)
  rawset(env, "tFilter", function(t, pred, isIndexed)
    local o = {}
    for k, v in pairs(t) do if pred(v) then if isIndexed then o[#o + 1] = v else o[k] = v end end end
    return o
  end)
  rawset(env, "tCount", function(t) local n = 0; for _ in pairs(t) do n = n + 1 end return n end)
  local function CopyTable(t, shallow)
    local c = {}
    for k, v in pairs(t) do
      if type(v) == "table" and not shallow then c[k] = CopyTable(v) else c[k] = v end
    end
    return c
  end
  rawset(env, "CopyTable", CopyTable)
  -- more of SharedXML's TableUtil
  rawset(env, "GetOrCreateTableEntry", function(t, k, default)
    local v = t[k]
    if v == nil then v = default or {}; t[k] = v end
    return v
  end)
  rawset(env, "FindInTable", function(t, v) for k, x in pairs(t) do if x == v then return k end end end)
  rawset(env, "MergeTable", function(dest, src) for k, v in pairs(src or {}) do dest[k] = v end return dest end)
  rawset(env, "FindInTableIf", function(t, pred) for k, v in pairs(t) do if pred(v) then return k, v end end end)
  rawset(env, "ContainsIf", function(t, pred) for _, v in pairs(t) do if pred(v) then return true end end return false end)
  -- TooltipDataProcessor: addons post-hook tooltip data by type
  local postCalls = {}
  rawset(env, "TooltipDataProcessor", {
    AllTypes = "ALL",
    AddTooltipPostCall = function(kind, fn) postCalls[kind] = postCalls[kind] or {}; table.insert(postCalls[kind], fn) end,
    AddTooltipPreCall = function() end,
    AddLinePreCall = function() end,
    AddLinePostCall = function() end,
    _postCalls = postCalls,
  })
  rawset(env, "GetAutoCompleteRealms", function() return { (sim.player.realm or ""):gsub("[%s%-]", "") } end)
  rawset(env, "GetKeysArray", function(t) local o = {}; for k in pairs(t) do o[#o + 1] = k end return o end)
  rawset(env, "GetValuesArray", function(t) local o = {}; for _, v in pairs(t) do o[#o + 1] = v end return o end)
  rawset(env, "GetPairsArray", function(t) local o = {}; for k, v in pairs(t) do o[#o + 1] = { key = k, value = v } end return o end)
  rawset(env, "GetKeysArraySortedByValue", function(t)
    local o = {}; for k in pairs(t) do o[#o + 1] = k end
    table.sort(o, function(a, b) return t[a] < t[b] end)
    return o
  end)
  rawset(env, "tCompare", function(a, b, depth)
    local function eq(x, y, d)
      if type(x) ~= "table" or type(y) ~= "table" then return x == y end
      if d and d <= 0 then return true end
      for k, v in pairs(x) do if not eq(v, y[k], d and d - 1) then return false end end
      for k in pairs(y) do if x[k] == nil then return false end end
      return true
    end
    return eq(a, b, depth)
  end)
  rawset(env, "tUnorderedRemove", function(t, i) t[i] = t[#t]; t[#t] = nil end)
  rawset(env, "tDeleteItem", function(t, v)
    local n = 0
    for i = #t, 1, -1 do if t[i] == v then table.remove(t, i); n = n + 1 end end
    return n
  end)
  rawset(env, "MergeTable", function(dst, src) for k, v in pairs(src) do dst[k] = v end return dst end)

  -- math (WoW's global trig functions take degrees)
  local Mth = {}
  for k, v in pairs(math) do Mth[k] = v end
  Mth.mod = math.fmod
  rawset(env, "math", Mth)
  local rng = opts_random(sim)
  Mth.random = rng
  for k, v in pairs({
    abs = math.abs, ceil = math.ceil, floor = math.floor, max = math.max, min = math.min,
    sqrt = math.sqrt, exp = math.exp, log = math.log, log10 = function(x) return math.log(x, 10) end,
    frexp = math.frexp, ldexp = math.ldexp, mod = math.fmod, random = rng, fastrandom = rng,
    deg = math.deg, rad = math.rad, PI = math.pi,
    sin = function(d) return math.sin(math.rad(d)) end, cos = function(d) return math.cos(math.rad(d)) end,
    tan = function(d) return math.tan(math.rad(d)) end, asin = function(x) return math.deg(math.asin(x)) end,
    acos = function(x) return math.deg(math.acos(x)) end, atan = function(x) return math.deg(math.atan(x)) end,
    atan2 = function(y, x) return math.deg((math.atan2 or math.atan)(y, x)) end,
    Round = function(v) return math.floor(v + 0.5) end,
    Clamp = function(v, lo, hi) return math.max(lo, math.min(hi, v)) end,
    Saturate = function(v) return math.max(0, math.min(1, v)) end,
    Lerp = function(a, b, t) return a + (b - a) * t end,
    ApproximatelyEqual = function(a, b, eps) return math.abs(a - b) <= (eps or 1e-6) end,
  }) do rawset(env, k, v) end

  ------------------------------------------------------------ time & debug
  local function serverTime() return math.floor(sim.epoch + (sim.time - (sim.opts.startTime or 100.0))) end
  rawset(env, "GetTime", function() return sim.time end)
  rawset(env, "GetTimePreciseSec", function() return sim.time + (os.clock() - clock0) end)
  rawset(env, "GetServerTime", serverTime)
  rawset(env, "time", function(t) if t then return os.time(t) end return serverTime() end)
  rawset(env, "date", function(fmt, t) return os.date(fmt, t or serverTime()) end)
  rawset(env, "difftime", os.difftime)
  -- debugprofilestop measures real elapsed time (it advances within a
  -- frame, unlike GetTime); addons use it for time-budgeted loops.
  local clock0 = os.clock()
  rawset(env, "debugprofilestop", function() return sim.time * 1000 + (os.clock() - clock0) * 1000 end)
  rawset(env, "debugprofilestart", function() end)
  rawset(env, "debugstack", function(start, count1, count2) return debug.traceback("", (start or 1) + 1) end)
  rawset(env, "debuglocals", function() return "" end)
  rawset(env, "GetFramerate", function() return 60 end)
  rawset(env, "GetNetStats", function() return 0, 0, 20, 20 end)
  rawset(env, "C_DateAndTime", {
    GetServerTimeLocal = serverTime,
    GetCurrentCalendarTime = function()
      local d = os.date("*t", serverTime())
      return { year = d.year, month = d.month, monthDay = d.day, weekday = d.wday, hour = d.hour, minute = d.min }
    end,
  })

  ------------------------------------------------------------ errors & security
  -- The default handler is the client's error display: the error counts,
  -- whether it came from our dispatch or an addon's xpcall(f, geterrorhandler()).
  local function defaultHandler(msg)
    local tb = debug.traceback(tostring(msg), 2)
    table.insert(sim.errors, { message = tostring(msg), traceback = tb })
    if not sim.quiet then io.stderr:write("|cffff0000Lua error|r: " .. tostring(msg) .. "\n") end
    return msg
  end
  rawset(env, "geterrorhandler", function()
    return sim.errorHandler or defaultHandler
  end)
  rawset(env, "seterrorhandler", function(fn) sim.errorHandler = fn end)
  rawset(env, "CallErrorHandler", function(...) return (sim.errorHandler or defaultHandler)(...) end)
  rawset(env, "securecall", function(fn, ...)
    if type(fn) == "string" then fn = rawget(env, fn) end
    local r = { n = 0 }
    local function pack(...) r = { n = select("#", ...), ... } end
    pack(pcall(fn, ...))
    if not r[1] then sim:_error(tostring(r[2])); return end
    return unpack(r, 2, r.n)
  end)
  rawset(env, "securecallfunction", function(fn, ...) return env.securecall(fn, ...) end)
  rawset(env, "secureexecuterange", function(t, fn, ...) for k, v in pairs(t) do env.securecall(fn, k, v, ...) end end)
  rawset(env, "issecure", function() return false end)
  -- addon code taints what it writes: a table field or a global an addon
  -- created reports that addon (TaintLess finds its host addon this way)
  rawset(env, "issecurevariable", function(t, k)
    if k == nil then t, k = env, t end
    if t == env then
      local by = sim.createdGlobals and sim.createdGlobals[k]
      if by then return false, by end
      return true, nil
    end
    if sim.currentAddon then return false, sim.currentAddon end
    return true, nil
  end)
  rawset(env, "InCombatLockdown", function() return sim.lockdown end)
  rawset(env, "hooksecurefunc", function(tbl, name, hook)
    if type(tbl) == "string" then tbl, name, hook = env, tbl, name end
    local orig = tbl[name]
    if type(orig) ~= "function" then
      -- a Blizzard method the simulator only knows by its owner (a mixin or
      -- placeholder frame): hook a no-op, as the real one would exist
      local st = sim.widgetState[tbl]
      -- (also a widget method this simulator doesn't implement, e.g. a
      -- GameTooltip:SetQuestItem an addon hooks)
      local isStub = sim.blizzardTables[tbl] or (st and st.placeholder)
        or (st and type(name) == "string" and require("wowapi.resources").looksLikeMethod(name))
      local ost = type(orig) == "table" and sim.widgetState[orig]
      if isStub and (orig == nil or (ost and ost.autoChild)) then
        orig = function() end
      end
    end
    if type(orig) ~= "function" then
      error("hooksecurefunc(): " .. tostring(name) .. " is not a function", 2)
    end
    tbl[name] = function(...)
      local r = { n = 0 }
      local function pack(...) r = { n = select("#", ...), ... } end
      pack(orig(...))
      local ok, err = pcall(hook, ...)
      if not ok then sim:_error(tostring(err)) end
      return unpack(r, 1, r.n)
    end
  end)

  ------------------------------------------------------------ build / client
  rawset(env, "GetBuildInfo", function()
    local b = sim.build
    return b.version, b.build, b.date, b.interface, b.version, "Release"
  end)
  rawset(env, "GetLocale", function() return sim.locale end)
  rawset(env, "GetCurrentRegion", function() return 1 end)
  rawset(env, "GetCurrentRegionName", function() return "US" end)
  rawset(env, "IsLoggedIn", function() return sim.loggedIn end)
  rawset(env, "IsPublicBuild", function() return true end)
  rawset(env, "IsTestBuild", function() return false end)
  rawset(env, "GetScreenWidth", function() return 1920 end)
  rawset(env, "GetScreenHeight", function() return 1080 end)
  rawset(env, "GetPhysicalScreenSize", function() return 1920, 1080 end)
  rawset(env, "GetCursorPosition", function() return sim.cursorX or 0, sim.cursorY or 0 end)
  rawset(env, "IsShiftKeyDown", function() return sim.modifiers.shift or false end)
  rawset(env, "IsControlKeyDown", function() return sim.modifiers.ctrl or false end)
  rawset(env, "IsAltKeyDown", function() return sim.modifiers.alt or false end)
  rawset(env, "IsModifierKeyDown", function() return (sim.modifiers.shift or sim.modifiers.ctrl or sim.modifiers.alt) or false end)
  rawset(env, "ReloadUI", function() sim.reloadRequested = true end)
  rawset(env, "C_UI", { Reload = function() sim.reloadRequested = true end })
  rawset(env, "Enum", {
    PowerType = { HealthCost = -2, None = -1, Mana = 0, Rage = 1, Focus = 2, Energy = 3, ComboPoints = 4,
      Runes = 5, RunicPower = 6, SoulShards = 7, LunarPower = 8, HolyPower = 9, Alternate = 10,
      Maelstrom = 11, Chi = 12, Insanity = 13, ArcaneCharges = 16, Fury = 17, Pain = 18, Essence = 19 },
    ItemQuality = { Poor = 0, Common = 1, Uncommon = 2, Rare = 3, Epic = 4, Legendary = 5,
      Artifact = 6, Heirloom = 7, WoWToken = 8 },
  })
  for k, v in pairs(GLOBAL_STRINGS) do rawset(env, k, v) end
  rawset(env, "SOUNDKIT", SOUNDKIT)
  rawset(env, "PlaySound", function(id, channel)
    if id == nil then error("PlaySound: soundKitID is nil (typo in SOUNDKIT name?)", 2) end
    table.insert(sim.sounds, { id = id, channel = channel })
    return true, #sim.sounds
  end)
  rawset(env, "PlaySoundFile", function(file, channel)
    table.insert(sim.sounds, { file = file, channel = channel })
    return true, #sim.sounds
  end)
  rawset(env, "StopSound", function() end)
  rawset(env, "PlayMusic", function() end)
  rawset(env, "StopMusic", function() end)

  -- CVars
  local function cvarGet(n)
    local v = sim.cvars[n]
    if v == nil and sim.cvarDefaults then v = sim.cvarDefaults[n] end
    if v ~= nil then return tostring(v) end
    return nil
  end
  local cvar = {
    GetCVar = cvarGet,
    SetCVar = function(n, v)
      if cvarGet(n) == nil and not (sim.registeredCVars or {})[n] then return false end
      local old = cvarGet(n)
      sim.cvars[n] = v ~= nil and tostring(v) or nil
      if tostring(old) ~= tostring(v) then sim:FireEvent("CVAR_UPDATE", n, tostring(v)) end
      return true
    end,
    GetCVarBool = function(n) return cvarGet(n) == "1" end,
    GetCVarNumberOrDefault = function(n) return tonumber(cvarGet(n)) or 0 end,
    GetCVarDefault = function(n) local d = sim.cvarDefaults and sim.cvarDefaults[n]; return d ~= nil and tostring(d) or nil end,
    RegisterCVar = function(n, v)
      sim.registeredCVars = sim.registeredCVars or {}
      sim.registeredCVars[n] = true
      if sim.cvars[n] == nil then sim.cvars[n] = v ~= nil and tostring(v) or "" end
    end,
    GetCVarInfo = function(n)
      local v = cvarGet(n)
      if v == nil then return nil end
      local d = sim.cvarDefaults and sim.cvarDefaults[n]
      return v, d ~= nil and tostring(d) or v, false, false, false, false, false
    end,
  }
  rawset(env, "C_CVar", cvar)
  -- SharedXML's CvarUtil exposes these as globals in the mainline client
  for _, n in ipairs({ "GetCVar", "SetCVar", "GetCVarBool", "GetCVarDefault", "RegisterCVar", "GetCVarInfo" }) do
    rawset(env, n, cvar[n])
  end

  ------------------------------------------------------------ mixins & colors
  local function Mixin(obj, ...)
    for i = 1, select("#", ...) do
      local m = select(i, ...)
      for k, v in pairs(m) do obj[k] = v end
    end
    return obj
  end
  rawset(env, "Mixin", Mixin)
  rawset(env, "CreateFromMixins", function(...) return Mixin({}, ...) end)
  rawset(env, "CreateAndInitFromMixin", function(m, ...)
    local o = Mixin({}, m)
    o:Init(...)
    return o
  end)

  local ColorMixin = {}
  function ColorMixin:OnLoad(r, g, b, a) self:SetRGBA(r, g, b, a) end
  function ColorMixin:SetRGBA(r, g, b, a) self.r, self.g, self.b, self.a = r, g, b, a end
  function ColorMixin:SetRGB(r, g, b) self:SetRGBA(r, g, b, nil) end
  function ColorMixin:GetRGB() return self.r, self.g, self.b end
  function ColorMixin:GetRGBA() return self.r, self.g, self.b, self.a or 1 end
  function ColorMixin:GetRGBAsBytes() return math.floor(self.r * 255 + 0.5), math.floor(self.g * 255 + 0.5), math.floor(self.b * 255 + 0.5) end
  function ColorMixin:IsEqualTo(o) return self.r == o.r and self.g == o.g and self.b == o.b and (self.a or 1) == (o.a or 1) end
  function ColorMixin:GenerateHexColor()
    return string.format("ff%02x%02x%02x", self:GetRGBAsBytes())
  end
  function ColorMixin:GenerateHexColorMarkup() return "|c" .. self:GenerateHexColor() end
  function ColorMixin:WrapTextInColorCode(text) return "|c" .. self:GenerateHexColor() .. tostring(text) .. "|r" end
  rawset(env, "ColorMixin", ColorMixin)
  local function CreateColor(r, g, b, a)
    local c = Mixin({}, ColorMixin)
    c:OnLoad(r, g, b, a)
    return c
  end
  rawset(env, "CreateColor", CreateColor)
  rawset(env, "CreateColorFromHexString", function(hex)
    local a, r, g, b = hex:match("^(%x%x)(%x%x)(%x%x)(%x%x)$")
    if not a then return nil end
    return CreateColor(tonumber(r, 16) / 255, tonumber(g, 16) / 255, tonumber(b, 16) / 255, tonumber(a, 16) / 255)
  end)
  rawset(env, "WrapTextInColorCode", function(text, hex) return "|c" .. hex .. tostring(text) .. "|r" end)
  for name, c in pairs({
    NORMAL_FONT_COLOR = { 1, 0.82, 0 }, HIGHLIGHT_FONT_COLOR = { 1, 1, 1 }, RED_FONT_COLOR = { 1, 0.1, 0.1 },
    GREEN_FONT_COLOR = { 0.1, 1, 0.1 }, GRAY_FONT_COLOR = { 0.5, 0.5, 0.5 }, YELLOW_FONT_COLOR = { 1, 1, 0 },
    WHITE_FONT_COLOR = { 1, 1, 1 }, ORANGE_FONT_COLOR = { 1, 0.5, 0.25 }, DISABLED_FONT_COLOR = { 0.5, 0.5, 0.5 },
    LIGHTBLUE_FONT_COLOR = { 0.53, 0.67, 1 }, BLUE_FONT_COLOR = { 0, 0.67, 1 },
    LIGHTGRAY_FONT_COLOR = { 0.749, 0.749, 0.749 }, DARKGRAY_FONT_COLOR = { 0.4, 0.4, 0.4 },
    LIGHTYELLOW_FONT_COLOR = { 1, 1, 0.6 }, DARKYELLOW_FONT_COLOR = { 1, 0.82, 0 },
  }) do rawset(env, name, CreateColor(c[1], c[2], c[3], 1)) end

  local raidColors, sortOrder, maleNames = {}, {}, {}
  for _, c in ipairs(CLASSES) do
    local col = CreateColor(c[3][1], c[3][2], c[3][3], 1)
    col.colorStr = col:GenerateHexColor()
    raidColors[c[1]] = col
    sortOrder[#sortOrder + 1] = c[1]
    maleNames[c[1]] = c[2]
  end
  rawset(env, "RAID_CLASS_COLORS", raidColors)
  rawset(env, "CLASS_SORT_ORDER", sortOrder)
  rawset(env, "LOCALIZED_CLASS_NAMES_MALE", maleNames)
  rawset(env, "LOCALIZED_CLASS_NAMES_FEMALE", CopyTable(maleNames))
  rawset(env, "GetClassColor", function(cls)
    local c = raidColors[cls] or raidColors.PRIEST
    return c.r, c.g, c.b, c.colorStr
  end)
  rawset(env, "C_ClassColor", { GetClassColor = function(cls) return raidColors[cls] end })
  rawset(env, "GetNumClasses", function() return #CLASSES end)
  rawset(env, "GetClassInfo", function(id)
    local c = CLASSES[id]
    if c then return c[2], c[1], id end
  end)
  local itemColors = {}
  for q, c in pairs(QUALITY_COLORS) do
    local col = CreateColor(c[1], c[2], c[3], 1)
    col.hex = "|c" .. col:GenerateHexColor()
    col.color = col
    itemColors[q] = col
  end
  rawset(env, "ITEM_QUALITY_COLORS", itemColors)
  rawset(env, "C_ColorOverrides", { GetColorForQuality = function(q) return itemColors[q] end })

  ------------------------------------------------------------ numbers & money
  local function BreakUpLargeNumbers(n)
    local s = tostring(math.floor(n))
    local neg = s:sub(1, 1) == "-"
    if neg then s = s:sub(2) end
    s = s:reverse():gsub("(%d%d%d)", "%1,"):reverse():gsub("^,", "")
    return (neg and "-" or "") .. s
  end
  rawset(env, "BreakUpLargeNumbers", BreakUpLargeNumbers)
  rawset(env, "AbbreviateNumbers", function(n)
    if n >= 1e9 then return (string.format("%.1fB", n / 1e9):gsub("%.0B", "B")) end
    if n >= 1e6 then return (string.format("%.1fM", n / 1e6):gsub("%.0M", "M")) end
    if n >= 1e3 then return (string.format("%.1fK", n / 1e3):gsub("%.0K", "K")) end
    return tostring(n)
  end)
  rawset(env, "AbbreviateLargeNumbers", env.AbbreviateNumbers)
  local function coinText(copper, icons)
    local g, s, c = math.floor(copper / 10000), math.floor(copper / 100) % 100, copper % 100
    local parts = {}
    if icons then
      if g > 0 then parts[#parts + 1] = BreakUpLargeNumbers(g) .. "|TInterface\\MoneyFrame\\UI-GoldIcon:0:0:2:0|t" end
      if s > 0 or g > 0 then parts[#parts + 1] = s .. "|TInterface\\MoneyFrame\\UI-SilverIcon:0:0:2:0|t" end
      parts[#parts + 1] = c .. "|TInterface\\MoneyFrame\\UI-CopperIcon:0:0:2:0|t"
    else
      if g > 0 then parts[#parts + 1] = g .. " Gold" end
      if s > 0 then parts[#parts + 1] = s .. " Silver" end
      if c > 0 or #parts == 0 then parts[#parts + 1] = c .. " Copper" end
    end
    return table.concat(parts, icons and " " or ", ")
  end
  rawset(env, "GetMoney", function() return sim.player.money end)
  rawset(env, "GetCoinText", function(c) return coinText(c, false) end)
  rawset(env, "GetMoneyString", function(c) return coinText(c, true) end)
  rawset(env, "C_CurrencyInfo", {
    GetCoinTextureString = function(c) return coinText(c, true) end,
    GetCoinText = function(c) return coinText(c, false) end,
  })

  ------------------------------------------------------------ units
  local function U(unit)
    if type(unit) ~= "string" then return nil end
    unit = unit:lower()
    if sim.units[unit] then return sim.units[unit] end
    for _, u in pairs(sim.units) do if u.name == unit or (u.name and u.name:lower() == unit) then return u end end
  end
  local function classOf(u)
    for i, c in ipairs(CLASSES) do if c[1] == u.class then return c[2], c[1], i end end
    return u.class, u.class, 0
  end
  local unitApi = {
    UnitExists = function(u) return U(u) ~= nil end,
    UnitName = function(u)
      local x = U(u); if not x then return nil end
      if x.realm ~= sim.player.realm then return x.name, x.realm end
      return x.name, nil
    end,
    UnitNameUnmodified = function(u) return env.UnitName(u) end,
    UnitFullName = function(u)
      local x = U(u)
      if not x then return end
      -- the player's own realm comes back normalized (no spaces or dashes)
      local realm = x.realm or (x == sim.player and sim.player.realm) or nil
      if realm then realm = realm:gsub("[%s%-]", "") end
      return x.name, realm
    end,
    UnitGUID = function(u) local x = U(u); return x and x.guid end,
    UnitClass = function(u) local x = U(u); if x then return classOf(x) end end,
    UnitClassBase = function(u) local x = U(u); if x then local _, f, id = classOf(x); return f, id end end,
    UnitRace = function(u)
      local x = U(u); if not x then return end
      return RACE_NAMES[x.race] or x.race, x.race, RACES[x.race] or 0
    end,
    UnitFactionGroup = function(u) local x = U(u); if x then return x.faction, x.faction end end,
    UnitSex = function(u) local x = U(u); return x and x.sex end,
    UnitLevel = function(u) local x = U(u); return x and x.level or 0 end,
    UnitEffectiveLevel = function(u) local x = U(u); return x and x.level or 0 end,
    UnitHealth = function(u) local x = U(u); return x and x.health or 0 end,
    UnitHealthMax = function(u) local x = U(u); return x and x.healthMax or 0 end,
    UnitPower = function(u, t) local x = U(u); return x and x.power or 0 end,
    UnitPowerMax = function(u, t) local x = U(u); return x and x.powerMax or 0 end,
    UnitPowerType = function(u) local x = U(u); if x then return x.powerType, POWER_TOKENS[x.powerType] end end,
    UnitIsDead = function(u) local x = U(u); return x and (x.dead or (x.health or 1) <= 0) or false end,
    UnitIsGhost = function(u) local x = U(u); return x and x.ghost or false end,
    UnitIsDeadOrGhost = function(u) return env.UnitIsDead(u) or env.UnitIsGhost(u) end,
    UnitIsPlayer = function(u) local x = U(u); return x ~= nil and (x.isPlayer ~= false) and (x.guid or ""):match("^Player") ~= nil end,
    UnitIsUnit = function(a, b) local x, y = U(a), U(b); return x ~= nil and x == y end,
    UnitAffectingCombat = function(u)
      local x = U(u); if x == sim.player then return sim.inCombat end
      return x and x.inCombat or false
    end,
    UnitIsFriend = function(a, b) local x, y = U(a), U(b); return x and y and x.faction == y.faction or false end,
    UnitIsEnemy = function(a, b) local x, y = U(a), U(b); return x and y and (x.hostile or y.hostile) or false end,
    UnitCanAttack = function(a, b) local y = U(b); return y and y.hostile or false end,
    UnitReaction = function(a, b) local y = U(b); if y then return y.hostile and 2 or 5 end end,
    UnitIsAFK = function(u) local x = U(u); return x and x.afk or false end,
    UnitIsDND = function(u) local x = U(u); return x and x.dnd or false end,
    UnitIsConnected = function(u) return U(u) ~= nil end,
    UnitInRange = function(u) local x = U(u); return x ~= nil, x ~= nil end,
    UnitIsVisible = function(u) return U(u) ~= nil end,
    UnitClassification = function(u) local x = U(u); return x and x.classification or "normal" end,
    UnitCreatureType = function(u) local x = U(u); return x and x.creatureType end,
    UnitIsGroupLeader = function(u) local x = U(u); return x and x.leader or false end,
    UnitGroupRolesAssigned = function(u) local x = U(u); return x and x.role or "NONE" end,
    UnitXP = function() return sim.player.xp or 0 end,
    UnitXPMax = function() return sim.player.xpMax or 1000 end,
    GetXPExhaustion = function() return sim.player.rested end,
    UnitInParty = function(u) local x = U(u); if not x then return false end
      for i = 1, 4 do if sim.units["party" .. i] == x then return true end end
      return x == sim.player and env.IsInGroup()
    end,
    UnitInRaid = function(u) local x = U(u); for i = 1, 40 do if sim.units["raid" .. i] == x then return i end end end,
    GetUnitName = function(u, showServer)
      local n, r = env.UnitName(u)
      if n and r and showServer then return n .. "-" .. r end
      return n
    end,
    GetRealmName = function() return sim.player.realm end,
    GetNormalizedRealmName = function() return (sim.player.realm:gsub("[%s%-]", "")) end,
    GetPlayerInfoByGUID = function(guid)
      for _, x in pairs(sim.units) do
        if x.guid == guid then
          local cls, file = classOf(x)
          return cls, file, RACE_NAMES[x.race] or x.race, x.race, x.sex, x.name, x.realm
        end
      end
    end,
    IsInGroup = function() return sim.units.party1 ~= nil or sim.units.raid1 ~= nil end,
    IsInRaid = function() return sim.units.raid1 ~= nil end,
    GetNumGroupMembers = function()
      local n = 0
      for i = 1, 40 do if sim.units["raid" .. i] then n = n + 1 end end
      if n > 0 then return n end
      for i = 1, 4 do if sim.units["party" .. i] then n = n + 1 end end
      return n > 0 and n + 1 or 0
    end,
    GetNumSubgroupMembers = function()
      local n = 0
      for i = 1, 4 do if sim.units["party" .. i] then n = n + 1 end end
      return n
    end,
    IsInGuild = function() return sim.player.guild ~= nil end,
    GetGuildInfo = function(u) local x = U(u); if x and x.guild then return x.guild, x.guildRank or "Member", 0 end end,
    IsInInstance = function() return sim.player.instanceType ~= nil and sim.player.instanceType ~= "none",
      sim.player.instanceType or "none" end,
    IsResting = function() return sim.player.resting or false end,
    IsMounted = function() return sim.player.mounted or false end,
    IsFlying = function() return sim.player.flying or false end,
    IsSwimming = function() return sim.player.swimming or false end,
    IsIndoors = function() return sim.player.indoors or false end,
    IsOutdoors = function() return not sim.player.indoors end,
    GetZoneText = function() return sim.player.zone end,
    GetRealZoneText = function() return sim.player.zone end,
    GetSubZoneText = function() return sim.player.subZone or "" end,
    GetMinimapZoneText = function() return sim.player.subZone or sim.player.zone end,
    GetSpecialization = function() return sim.player.spec end,
    GetSpecializationInfoByID = function(id)
      for cls, info in pairs(require("wowapi.faker").CLASSES) do
        for _, sp in ipairs(info.specs) do
          if sp[1] == id then return sp[1], sp[2], sp[2] .. " specialization.", 136243, sp[3], cls, cls end
        end
      end
    end,
    GetSpecializationRole = function(i) local s = (sim.player.specs or {})[i]; return s and s.role end,
    GetRaidRosterInfo = function(i)
      local u = sim.units["raid" .. i]
      if not u then return nil end
      local cls, file = classOf(u)
      return u.name, (u.leader and 2 or 0), u.subgroup or 1, u.level, cls, file, u.zone or sim.player.zone,
        true, u.dead or false, u.role == "TANK" and "MAINTANK" or nil, false, u.role or "NONE"
    end,
    UnitCastingInfo = function(unit)
      local x = U(unit)
      local c = x and x.casting
      if not c then return nil end
      return c.spell.name, c.spell.name, c.spell.icon, c.startTime * 1000, c.endTime * 1000, false, c.castGUID, false, c.spell.id
    end,
    UnitChannelInfo = function(unit) return nil end,
    GetNumSpecializations = function() return sim.player.numSpecs or 3 end,
    GetSpecializationInfo = function(i)
      local s = (sim.player.specs or {})[i]
      if s then return s.id, s.name, s.description or "", s.icon or 0, s.role or "DAMAGER" end
    end,
  }
  for k, v in pairs(unitApi) do rawset(env, k, v) end

  -- auras: sim.units[unit].auras = { { name=, spellId=, isHelpful=, ... } }
  local function auraList(unit, filter)
    local x = U(unit)
    local out = {}
    if not x then return out end
    local harmful = filter and filter:find("HARMFUL")
    for _, a in ipairs(x.auras or {}) do
      local helpful = a.isHelpful ~= false and not a.isHarmful
      if (harmful and not helpful) or (not harmful and helpful) then
        local d = {}
        for k, v in pairs(a) do d[k] = v end
        d.applications = d.applications or d.count or 0
        d.duration = d.duration or 0
        d.expirationTime = d.expirationTime or 0
        d.icon = d.icon or 136243
        d.isHelpful = helpful
        d.isHarmful = not helpful
        d.auraInstanceID = d.auraInstanceID or (#out + 1)
        out[#out + 1] = d
      end
    end
    return out
  end
  rawset(env, "C_UnitAuras", {
    GetAuraDataByIndex = function(unit, i, filter) return auraList(unit, filter)[i] end,
    GetBuffDataByIndex = function(unit, i, filter) return auraList(unit, "HELPFUL")[i] end,
    GetDebuffDataByIndex = function(unit, i, filter) return auraList(unit, "HARMFUL")[i] end,
    GetPlayerAuraBySpellID = function(id)
      for _, f in ipairs({ "HELPFUL", "HARMFUL" }) do
        for _, a in ipairs(auraList("player", f)) do if a.spellId == id then return a end end
      end
    end,
    GetAuraDataBySpellName = function(unit, name, filter)
      for _, a in ipairs(auraList(unit, filter)) do if a.name == name then return a end end
    end,
  })
  rawset(env, "AuraUtil", {
    ForEachAura = function(unit, filter, maxCount, fn, usePackedAura)
      for _, a in ipairs(auraList(unit, filter)) do
        local stop
        if usePackedAura then stop = fn(a) else stop = fn(a.name, a.icon, a.applications, nil, a.duration, a.expirationTime, a.sourceUnit, nil, nil, a.spellId) end
        if stop then return end
      end
    end,
    FindAuraByName = function(name, unit, filter)
      for _, a in ipairs(auraList(unit, filter)) do if a.name == name then return a.name, a.icon, a.applications end end
    end,
  })

  ------------------------------------------------------------ items & spells
  -- Items/spells registered with sim:AddItem/AddSpell win; any other ID
  -- is a generated (fake but consistent) item or spell when fakeData is on.
  local faker = require("wowapi.faker")
  local function fakeItem(id)
    if not sim.fakeData or type(id) ~= "number" or id <= 0 then return nil end
    local known = false
    for _, k in ipairs(faker.KNOWN_ITEMS) do if k[1] == id then known = true end end
    if not known and not faker.exists("item", id) then return nil end
    local i = faker.item(id, sim.player.level)
    sim.items[id] = i
    return i
  end
  local function itemRef(ref)
    if type(ref) == "string" then
      local id = ref:match("item:(%d+)")
      if id then id = tonumber(id); return sim.items[id] or fakeItem(id) end
      if sim.items[ref] then return sim.items[ref] end
      local n = tonumber(ref)
      return n and (sim.items[n] or fakeItem(n)) or nil
    end
    return sim.items[ref] or fakeItem(ref)
  end
  local function GetItemInfo(ref)
    local i = itemRef(ref)
    if not i then return nil end
    return i.name, i.link, i.quality, i.itemLevel or 1, i.minLevel or 1, i.type or "Miscellaneous",
      i.subType or "Junk", i.stackCount or 1, i.equipLoc or "", i.icon or 134400, i.sellPrice or 0,
      i.classID or 15, i.subclassID or 0
  end
  rawset(env, "C_Item", {
    GetItemInfo = GetItemInfo,
    GetItemInfoInstant = function(ref)
      local i = itemRef(ref); if not i then return nil end
      return i.id, i.type or "Miscellaneous", i.subType or "Junk", i.equipLoc or "", i.icon or 134400, i.classID or 15, i.subclassID or 0
    end,
    GetItemNameByID = function(id) local i = itemRef(id); return i and i.name end,
    GetItemQualityByID = function(id) local i = itemRef(id); return i and i.quality end,
    GetItemIconByID = function(id) local i = itemRef(id); return i and (i.icon or 134400) end,
    GetItemCount = function(ref)
      local i = itemRef(ref); if not i then return 0 end
      local n = 0
      for _, bag in pairs(sim.bags) do
        for k, slot in pairs(bag) do
          if type(k) == "number" and type(slot) == "table" and slot.itemID == i.id then n = n + (slot.stackCount or 1) end
        end
      end
      return n
    end,
    DoesItemExistByID = function(id) return itemRef(id) ~= nil end,
    RequestLoadItemDataByID = function() end,
    IsItemDataCachedByID = function(id) return itemRef(id) ~= nil end,
  })
  local function spellRef(ref)
    local s = sim.spells[ref] or sim.spells[tonumber(ref) or -1]
    if s then return s end
    if type(ref) == "string" and not tonumber(ref) then
      for _, k in ipairs(faker.KNOWN_SPELLS) do
        if k[2]:lower() == ref:lower() then ref = k[1]; break end
      end
      if type(ref) == "string" then return nil end
    end
    local id = tonumber(ref)
    local known = false
    for _, k in ipairs(faker.KNOWN_SPELLS) do if k[1] == id then known = true end end
    if sim.fakeData and id and id > 0 and (known or faker.exists("spell", id)) then
      s = faker.spell(id)
      sim.spells[id] = s
      sim.spells[s.name] = sim.spells[s.name] or s
      return s
    end
  end
  rawset(env, "C_Spell", {
    GetSpellInfo = function(ref)
      local s = spellRef(ref); if not s then return nil end
      return { name = s.name, iconID = s.icon or 136243, originalIconID = s.icon or 136243,
        castTime = s.castTime or 0, minRange = s.minRange or 0, maxRange = s.maxRange or 0, spellID = s.id }
    end,
    GetSpellName = function(ref) local s = spellRef(ref); return s and s.name end,
    GetSpellTexture = function(ref) local s = spellRef(ref); if s then return s.icon or 136243, s.icon or 136243 end end,
    GetSpellCooldown = function(ref)
      local s = spellRef(ref); if not s then return nil end
      local cd = sim.spellCooldowns and sim.spellCooldowns[s.id] or s.cooldown or { startTime = 0, duration = 0 }
      return { startTime = cd.startTime or 0, duration = cd.duration or 0, isEnabled = true, modRate = 1 }
    end,
    DoesSpellExist = function(ref) return spellRef(ref) ~= nil end,
    IsSpellUsable = function(ref) local s = spellRef(ref); return s ~= nil and s.usable ~= false, false end,
    GetSpellLink = function(ref)
      local s = spellRef(ref)
      if s then return string.format("|cff71d5ff|Hspell:%d:0|h[%s]|h|r", s.id, s.name) end
    end,
    GetSpellDescription = function(ref) local s = spellRef(ref); return s and s.description or "" end,
    IsCurrentSpell = function() return false end,
  })
  rawset(env, "IsPlayerSpell", function(id)
    local s = sim.spells[id]
    if s then return s.known ~= false end
    -- with fake data, the player knows their class's well-known spells
    for _, k in ipairs(faker.KNOWN_SPELLS) do
      if k[1] == id then return sim.fakeData and (k[4] == nil or k[4] == sim.player.class) or false end
    end
    return false
  end)
  rawset(env, "IsSpellKnown", env.IsPlayerSpell)

  -- bags: sim.bags[bag][slot] = { itemID = n, stackCount = n }
  rawset(env, "C_Container", {
    GetContainerNumSlots = function(bag) local b = sim.bags[bag]; return b and (b.size or 16) or 0 end,
    GetContainerItemInfo = function(bag, slot)
      local b = sim.bags[bag]; local s = b and b[slot]
      if not s then return nil end
      local i = itemRef(s.itemID) or { name = "?", link = "", quality = 1 }
      return { itemID = s.itemID, stackCount = s.stackCount or 1, hyperlink = i.link, quality = i.quality,
        iconFileID = i.icon or 134400, isLocked = false, isBound = s.isBound or false, itemName = i.name }
    end,
    GetContainerItemID = function(bag, slot) local b = sim.bags[bag]; return b and b[slot] and b[slot].itemID end,
    GetContainerItemLink = function(bag, slot)
      local b = sim.bags[bag]; local s = b and b[slot]
      local i = s and itemRef(s.itemID)
      return i and i.link
    end,
    GetContainerNumFreeSlots = function(bag)
      local b = sim.bags[bag]; if not b then return 0, 0 end
      local used = 0
      for k in pairs(b) do if type(k) == "number" then used = used + 1 end end
      return (b.size or 16) - used, 0
    end,
    UseContainerItem = function(bag, slot) table.insert(sim.sentChat, { used = { bag, slot } }) end,
  })
  rawset(env, "NUM_BAG_SLOTS", 4)
  rawset(env, "NUM_CONTAINER_FRAMES", 13)
  rawset(env, "BACKPACK_CONTAINER", 0)

  ------------------------------------------------------------ spellbook
  -- The player's spellbook: their class's spells (sim.spellbook overrides).
  local function spellbook()
    if sim.spellbook then return sim.spellbook end
    local list = {}
    if sim.fakeData then
      for _, k in ipairs(faker.KNOWN_SPELLS) do
        if k[4] == sim.player.class or k[4] == nil then list[#list + 1] = k[1] end
      end
    end
    return list
  end
  local function bookItem(i)
    local id = spellbook()[i]
    if not id then return nil end
    local s = spellRef(id)
    return s
  end
  rawset(env, "C_SpellBook", {
    GetNumSpellBookSkillLines = function() return #spellbook() > 0 and 1 or 0 end,
    GetSpellBookSkillLineInfo = function(i)
      if i ~= 1 or #spellbook() == 0 then return nil end
      return { name = env.UnitClass("player"), iconID = 136243, itemIndexOffset = 0, numSpellBookItems = #spellbook(),
        isGuild = false, shouldHide = false, specID = sim.player.specID or 0, offSpecID = 0 }
    end,
    GetSpellBookItemInfo = function(i)
      local s = bookItem(i)
      if not s then return nil end
      return { actionID = s.id, spellID = s.id, itemType = 1, name = s.name, subName = "", iconID = s.icon,
        isPassive = false, isOffSpec = false, skillLineIndex = 1 }
    end,
    GetSpellBookItemName = function(i) local s = bookItem(i); if s then return s.name, "" end end,
    GetSpellBookItemTexture = function(i) local s = bookItem(i); return s and s.icon end,
    GetSpellBookItemType = function(i) local s = bookItem(i); if s then return 1, s.id, s.id end end,
    HasPetSpells = function() return nil end,
    IsSpellBookItemPassive = function() return false end,
    FindSpellBookSlotForSpell = function(id)
      for i, sid in ipairs(spellbook()) do if sid == id then return i, 0 end end
    end,
  })

  ------------------------------------------------------------ textures
  rawset(env, "C_Texture", {
    GetAtlasInfo = function(atlas)
      local a = require("wowapi.assets").atlasInfo(atlas)
      if not a then return nil end
      return { width = a.width, height = a.height, rawSize = { x = a.width, y = a.height },
        leftTexCoord = a.left, rightTexCoord = a.right, topTexCoord = a.top, bottomTexCoord = a.bottom,
        tilesHorizontally = a.tilesH, tilesVertically = a.tilesV, file = a.fileID, filename = a.path }
    end,
  })

  ------------------------------------------------------------ map
  rawset(env, "C_Map", {
    GetBestMapForUnit = function(u) local x = U(u); return x and x.mapID end,
    GetMapInfo = function(id) return { mapID = id, name = sim.player.zone, mapType = 3, parentMapID = 0 } end,
    GetPlayerMapPosition = function(mapID, u)
      local x = U(u); if not x then return nil end
      local px, py = x.x or 0.5, x.y or 0.5
      return { x = px, y = py, GetXY = function() return px, py end }
    end,
  })

  ------------------------------------------------------------ timers
  local function makeHandle(t)
    local h = {}
    function h:Cancel() t.cancelled = true end
    function h:IsCancelled() return t.cancelled or false end
    function h:Invoke(...) return t.fn(h, ...) end
    t.handle = h
    return h
  end
  local function checkTimerArgs(name, seconds, fn)
    if type(seconds) ~= "number" then error("Usage: C_Timer." .. name .. "(seconds, callback)", 3) end
    if type(fn) ~= "function" and not (type(fn) == "table" and getmetatable(fn) and getmetatable(fn).__call) then
      error("C_Timer." .. name .. ": callback must be a function", 3)
    end
  end
  rawset(env, "C_Timer", {
    After = function(seconds, fn)
      checkTimerArgs("After", seconds, fn)
      local t = sim:_addTimer(seconds, function() fn() end)
      makeHandle(t)
    end,
    NewTimer = function(seconds, fn)
      checkTimerArgs("NewTimer", seconds, fn)
      local t = sim:_addTimer(seconds, fn)
      return makeHandle(t)
    end,
    NewTicker = function(seconds, fn, iterations)
      checkTimerArgs("NewTicker", seconds, fn)
      local t = sim:_addTimer(seconds, fn, seconds, iterations)
      return makeHandle(t)
    end,
  })

  ------------------------------------------------------------ addons
  local function addonByRef(ref) return sim:_addonInfo(ref) end
  local addonsApi = {
    GetNumAddOns = function() return #sim:_installedAddons() end,
    GetAddOnMetadata = function(ref, field)
      local a = addonByRef(ref)
      if not a then error("Couldn't find addon " .. tostring(ref), 2) end
      return a.toc.metadata[field] or a.toc.metadata[field:lower()]
    end,
    GetAddOnInfo = function(ref)
      local a = addonByRef(ref)
      if not a then return nil, nil, nil, false, "MISSING" end
      local reason = a.reason
      if not a.loaded and not reason and a.toc.loadOnDemand then reason = "DEMAND_LOADED" end
      return a.name, a.toc.title, a.toc.metadata.notes, reason == nil or reason == "DEMAND_LOADED", a.loaded and nil or reason, "INSECURE", false
    end,
    IsAddOnLoaded = function(ref)
      local a = addonByRef(ref)
      if not a then return false, false end
      return (a.loaded or a.loading) and true or false, a.loaded and true or false
    end,
    LoadAddOn = function(ref)
      local a = addonByRef(ref)
      if a and a.loaded then return true end
      local ok, reason = sim:LoadAddon(a and (a.dir or a.name) or ref)
      if ok then return true end
      return false, reason
    end,
    DoesAddOnExist = function(ref) return addonByRef(ref) ~= nil or sim:_resolveAddon(ref) ~= nil end,
    IsAddOnLoadOnDemand = function(ref) local a = addonByRef(ref); return a and a.toc.loadOnDemand or false end,
    EnableAddOn = function() end,
    DisableAddOn = function() end,
    GetAddOnEnableState = function(ref) return addonByRef(ref) and 2 or 0 end,
    IsAddonVersionCheckEnabled = function() return true end,
  }
  rawset(env, "C_AddOns", addonsApi)

  ------------------------------------------------------------ backdrops (BackdropTemplateMixin)
  local backdropMixin = {}
  local function bstate(self) return sim.widgetState[self] end
  function backdropMixin:SetBackdrop(info) bstate(self).backdrop = info; bstate(self).backdropColor = nil; bstate(self).backdropBorderColor = nil end
  function backdropMixin:GetBackdrop() return bstate(self).backdrop end
  function backdropMixin:ClearBackdrop() bstate(self).backdrop = nil end
  function backdropMixin:ApplyBackdrop() end
  function backdropMixin:OnBackdropLoaded() end
  function backdropMixin:OnBackdropSizeChanged() end
  function backdropMixin:HasBackdropInfo(info) return bstate(self).backdrop == info end
  function backdropMixin:SetBackdropColor(r, g, b, a)
    if not bstate(self).backdrop then return end
    bstate(self).backdropColor = { r, g, b, a or 1 }
  end
  function backdropMixin:GetBackdropColor()
    local c = bstate(self).backdropColor
    if c then return c[1], c[2], c[3], c[4] end
    if bstate(self).backdrop then return 1, 1, 1, 1 end
  end
  function backdropMixin:SetBackdropBorderColor(r, g, b, a)
    if not bstate(self).backdrop then return end
    bstate(self).backdropBorderColor = { r, g, b, a or 1 }
  end
  function backdropMixin:GetBackdropBorderColor()
    local c = bstate(self).backdropBorderColor
    if c then return c[1], c[2], c[3], c[4] end
    if bstate(self).backdrop then return 1, 1, 1, 1 end
  end
  rawset(env, "BackdropTemplateMixin", backdropMixin)
  rawset(env, "BACKDROP_TOOLTIP_16_16_5555", { bgFile = "Interface\\Tooltips\\UI-Tooltip-Background",
    edgeFile = "Interface\\Tooltips\\UI-Tooltip-Border", tile = true, tileEdge = true, tileSize = 16, edgeSize = 16,
    insets = { left = 5, right = 5, top = 5, bottom = 5 } })
  rawset(env, "BACKDROP_DIALOG_32_32", { bgFile = "Interface\\DialogFrame\\UI-DialogBox-Background",
    edgeFile = "Interface\\DialogFrame\\UI-DialogBox-Border", tile = true, tileEdge = true, tileSize = 32, edgeSize = 32,
    insets = { left = 11, right = 12, top = 12, bottom = 11 } })
  rawset(env, "BACKDROP_DARK_DIALOG_32_32", env.BACKDROP_DIALOG_32_32)

  ------------------------------------------------------------ frames
  rawset(env, "CreateFrame", function(otype, name, parent, template, id)
    if type(otype) ~= "string" then error("CreateFrame: frameType must be a string", 2) end
    if not sim.frameTypes[otype:lower()] then error("CreateFrame: Unknown frame type '" .. otype .. "'", 2) end
    local f = widgets.create(sim, otype, name, parent, template)
    if id and f.SetID then f:SetID(id) end
    return f
  end)
  rawset(env, "CreateFont", function(name)
    local f = widgets.create(sim, "Font", name)
    return f
  end)
  local function frame(t, name, parent) return widgets.create(sim, t, name, parent) end
  local UIParent = frame("Frame", "UIParent")
  UIParent:SetSize(1920, 1080)
  sim.widgetState[UIParent].fixedRect = { 0, 0, 1920, 1080 }
  sim.widgetState[UIParent].level = 0
  local world = frame("Frame", "WorldFrame")
  sim.widgetState[world].fixedRect = { 0, 0, 1920, 1080 }
  sim.widgetState[world].strata = "WORLD"
  frame("Frame", "Minimap", UIParent)
  frame("Frame", "AddonCompartmentFrame", UIParent)
  local tooltip = frame("GameTooltip", "GameTooltip", UIParent)
  tooltip:Hide()
  frame("GameTooltip", "ItemRefTooltip", UIParent):Hide()
  local chat = frame("ScrollingMessageFrame", "ChatFrame1", UIParent)
  rawset(env, "DEFAULT_CHAT_FRAME", chat)
  rawset(env, "SELECTED_CHAT_FRAME", chat)
  rawset(env, "NUM_CHAT_WINDOWS", 10)
  local uiErrors = frame("MessageFrame", "UIErrorsFrame", UIParent)
  uiErrors.AddMessage = function(self, msg, r, g, b)
    table.insert(sim.uiErrors, tostring(msg))
  end
  uiErrors.AddExternalErrorMessage = uiErrors.AddMessage
  local rw = frame("MessageFrame", "RaidWarningFrame", UIParent)
  rawset(env, "RaidNotice_AddMessage", function(f, msg) table.insert(sim.uiErrors, tostring(msg)) end)
  rawset(env, "RaidBossEmoteFrame", rw)
  local GOLD, WHITE, GRAY, RED, GREEN = { 1, 0.82, 0, 1 }, { 1, 1, 1, 1 }, { 0.5, 0.5, 0.5, 1 }, { 1, 0.1, 0.1, 1 }, { 0.1, 1, 0.1, 1 }
  for n, def in pairs({
    GameFontNormal = { 12, GOLD }, GameFontHighlight = { 12, WHITE }, GameFontNormalSmall = { 10, GOLD },
    GameFontHighlightSmall = { 10, WHITE }, GameFontNormalLarge = { 16, GOLD }, GameFontHighlightLarge = { 16, WHITE },
    GameFontDisable = { 12, GRAY }, GameFontDisableSmall = { 10, GRAY }, GameFontRed = { 12, RED },
    GameFontGreen = { 12, GREEN }, GameFontWhite = { 12, WHITE }, GameFontNormalHuge = { 20, GOLD },
    NumberFontNormal = { 14, WHITE, "OUTLINE" }, NumberFontNormalSmall = { 12, WHITE, "OUTLINE" },
    ChatFontNormal = { 14, WHITE }, SystemFont_Med1 = { 12, WHITE }, SystemFont_Small = { 10, WHITE },
    GameFontNormalMed3 = { 14, GOLD }, GameFontHighlightMedium = { 14, WHITE }, GameTooltipText = { 12, WHITE },
    GameTooltipHeaderText = { 14, WHITE }, Tooltip_Med = { 12, WHITE }, QuestFont = { 13, { 0.18, 0.12, 0.06, 1 } },
    GameFontNormalOutline = { 12, GOLD, "OUTLINE" }, GameFontHighlightOutline = { 12, WHITE, "OUTLINE" },
    GameFontNormalLeft = { 12, GOLD }, GameFontHighlightLeft = { 12, WHITE }, GameFontBlack = { 12, { 0, 0, 0, 1 } },
    GameFontNormalMed1 = { 13, GOLD }, GameFontNormalMed2 = { 14, GOLD }, GameFontHighlightMed2 = { 14, WHITE },
    GameFontNormalTiny = { 9, GOLD }, GameFontHighlightExtraSmall = { 9, WHITE }, SystemFont_Large = { 16, WHITE },
    SystemFont_Huge1 = { 20, WHITE }, Game15Font = { 15, WHITE }, Game18Font = { 18, WHITE },
  }) do
    local f = widgets.create(sim, "Font", n)
    local fs = sim.widgetState[f]
    fs.font = { "Fonts\\FRIZQT__.TTF", def[1], def[3] or "" }
    fs.textColor = def[2]
    if n:find("Left$") then fs.justifyH = "LEFT" end
  end
  rawset(env, "STANDARD_TEXT_FONT", "Fonts\\FRIZQT__.TTF")
  rawset(env, "UNIT_NAME_FONT", "Fonts\\FRIZQT__.TTF")
  rawset(env, "DAMAGE_TEXT_FONT", "Fonts\\FRIZQT__.TTF")
  rawset(env, "NAMEPLATE_FONT", "Fonts\\FRIZQT__.TTF")
  rawset(env, "GetMouseFoci", function() return { sim.mouseFocus } end)
  rawset(env, "GetMouseFocus", nil)
  rawset(env, "GameTooltip_Hide", function() tooltip:Hide() end)
  rawset(env, "GameTooltip_SetDefaultAnchor", function(tt, owner) tt:SetOwner(owner, "ANCHOR_NONE") end)
  rawset(env, "AddonCompartmentFrame", env.AddonCompartmentFrame)
  env.AddonCompartmentFrame.registeredAddons = {}
  env.AddonCompartmentFrame.RegisterAddon = function(self, info) table.insert(self.registeredAddons, info) end

  ------------------------------------------------------------ chat & slash
  rawset(env, "SlashCmdList", {})
  rawset(env, "hash_SlashCmdList", {})
  rawset(env, "print", function(...)
    local n = select("#", ...)
    local t = {}
    for i = 1, n do t[i] = tostring((select(i, ...))) end
    chat:AddMessage(table.concat(t, " "))
  end)
  rawset(env, "SendChatMessage", function(msg, chatType, lang, target)
    if type(msg) ~= "string" then error("Usage: SendChatMessage(msg [, chatType, languageID, target])", 2) end
    if #msg > 255 then error("SendChatMessage: message too long (" .. #msg .. " > 255)", 2) end
    table.insert(sim.sentChat, { msg = msg, chatType = (chatType or "SAY"):upper(), target = target })
  end)
  rawset(env, "C_ChatInfo", {
    RegisterAddonMessagePrefix = function(prefix)
      if #prefix > 16 then return false end
      sim.addonMessagePrefixes[prefix] = true
      return true
    end,
    IsAddonMessagePrefixRegistered = function(p) return sim.addonMessagePrefixes[p] or false end,
    GetRegisteredAddonMessagePrefixes = function()
      local o = {}; for p in pairs(sim.addonMessagePrefixes) do o[#o + 1] = p end; return o
    end,
    SendAddonMessage = function(prefix, msg, chatType, target)
      if #msg > 255 then error("SendAddonMessage: message too long", 2) end
      table.insert(sim.addonMessages, { prefix = prefix, msg = msg, chatType = chatType, target = target })
      return 0
    end,
    SendChatMessage = env.SendChatMessage,
  })
  rawset(env, "ChatFrame_AddMessageEventFilter", function(event, fn)
    sim.chatFilters[event] = sim.chatFilters[event] or {}
    table.insert(sim.chatFilters[event], fn)
  end)
  rawset(env, "ChatFrame_RemoveMessageEventFilter", function(event, fn)
    local l = sim.chatFilters[event] or {}
    for i = #l, 1, -1 do if l[i] == fn then table.remove(l, i) end end
  end)
  rawset(env, "ChatFrame_GetMessageEventFilters", function(event) return sim.chatFilters[event] end)
  rawset(env, "ChatFrame_OpenChat", function(text) sim.openedChat = text end)
  rawset(env, "ChatEdit_InsertLink", function(link) sim.openedChat = (sim.openedChat or "") .. link; return true end)

  ------------------------------------------------------------ popups
  rawset(env, "StaticPopupDialogs", {})
  rawset(env, "StaticPopup_Show", function(which, text1, text2, data)
    local def = env.StaticPopupDialogs[which]
    if not def then return nil end
    local dialog = frame("Frame", nil, UIParent)
    dialog.which, dialog.data = which, data
    dialog.text = frame("FontString", nil, dialog)
    dialog.text:SetText(string.format(def.text or "", text1 or "", text2 or ""))
    if def.hasEditBox then dialog.editBox = frame("EditBox", nil, dialog); dialog.EditBox = dialog.editBox end
    table.insert(sim.popups, dialog)
    if def.OnShow then sim:_pcall(def.OnShow, dialog, data) end
    return dialog
  end)
  rawset(env, "StaticPopup_Hide", function(which)
    for i = #sim.popups, 1, -1 do
      if sim.popups[i].which == which then
        local d = table.remove(sim.popups, i)
        local def = env.StaticPopupDialogs[which]
        if def and def.OnHide then sim:_pcall(def.OnHide, d, d.data) end
      end
    end
  end)
  rawset(env, "StaticPopup_Visible", function(which)
    for _, d in ipairs(sim.popups) do if d.which == which then return d end end
  end)

  ------------------------------------------------------------ event helpers (mainline)
  local registry = { callbacks = {} }
  function registry:RegisterFrameEventAndCallback(event, fn, owner)
    sim.eventCallbacks[event] = sim.eventCallbacks[event] or {}
    table.insert(sim.eventCallbacks[event], { fn = fn, owner = owner })
    return owner
  end
  function registry:UnregisterFrameEventAndCallback(event, owner)
    local l = sim.eventCallbacks[event] or {}
    for i = #l, 1, -1 do if l[i].owner == owner then table.remove(l, i) end end
  end
  function registry:RegisterCallback(event, fn, owner)
    self.callbacks[event] = self.callbacks[event] or {}
    table.insert(self.callbacks[event], { fn = fn, owner = owner })
  end
  function registry:UnregisterCallback(event, owner)
    local l = self.callbacks[event] or {}
    for i = #l, 1, -1 do if l[i].owner == owner then table.remove(l, i) end end
  end
  function registry:TriggerEvent(event, ...)
    for _, cb in ipairs(self.callbacks[event] or {}) do
      if cb.owner ~= nil then sim:_pcall(cb.fn, cb.owner, ...) else sim:_pcall(cb.fn, ...) end
    end
  end
  rawset(env, "EventRegistry", registry)
  -- Same semantics as Blizzard_SharedXML/EventUtil.lua
  local EventUtil = {}
  function EventUtil.RegisterOnceFrameEventAndCallback(frameEvent, callback, ...)
    local required = { n = select("#", ...), ... }
    local owner = {}
    registry:RegisterFrameEventAndCallback(frameEvent, function(_, ...)
      for i = 1, required.n do
        if select(i, ...) ~= required[i] then return end
      end
      registry:UnregisterFrameEventAndCallback(frameEvent, owner)
      callback(...)
    end, owner)
  end
  function EventUtil.ContinueOnAddOnLoaded(addOnName, callback)
    if sim.addons[addOnName] and sim.addons[addOnName].loaded then callback(); return end
    EventUtil.RegisterOnceFrameEventAndCallback("ADDON_LOADED", callback, addOnName)
  end
  function EventUtil.ContinueOnPlayerLogin(callback)
    if sim.loggedIn then callback(); return end
    EventUtil.RegisterOnceFrameEventAndCallback("PLAYER_LOGIN", callback)
  end
  function EventUtil.ContinueAfterAllEvents(callback, ...)
    local left = select("#", ...)
    for i = 1, left do
      EventUtil.RegisterOnceFrameEventAndCallback((select(i, ...)), function()
        left = left - 1
        if left == 0 then callback() end
      end)
    end
  end
  function EventUtil.RegisterForCallbacks() end
  rawset(env, "EventUtil", EventUtil)


  ------------------------------------------------------------ settings panel (mainline)
  local function newCategory(name, frame_, parent)
    local cat = { name = name, frame = frame_, parent = parent, ID = name, settings = {}, subcategories = {} }
    function cat:GetID() return self.ID end
    function cat:GetName() return self.name end
    function cat:SetName(n) self.name = n end
    return cat
  end
  local function newSetting(category, variable, tbl, key, vtype, name, default)
    local setting = { variable = variable, name = name, default = default, varType = vtype, category = category, callbacks = {} }
    function setting:GetValue()
      local v = tbl and tbl[key]
      if v == nil then return self.default end
      return v
    end
    function setting:SetValue(v)
      if tbl then tbl[key] = v end
      for _, cb in ipairs(self.callbacks) do sim:_pcall(cb, self, v) end
    end
    function setting:GetDefaultValue() return self.default end
    function setting:GetVariable() return self.variable end
    function setting:GetName() return self.name end
    function setting:SetValueChangedCallback(cb) table.insert(self.callbacks, cb) end
    category.settings[variable] = setting
    return setting
  end
  -- a control in a vertical-layout category (drawn by wowapi.settingspanel)
  local function control(category, kind, setting, options, tooltip)
    local c = { kind = kind, setting = setting, options = options, tooltip = tooltip }
    function c:AddShownPredicate() end
    function c:AddModifyPredicate() end
    function c:SetParentInitializer() end
    function c:GetSetting() return self.setting end
    if type(category) == "table" then
      category.controls = category.controls or {}
      table.insert(category.controls, c)
    end
    return c
  end
  rawset(env, "CreateSettingsListSectionHeaderInitializer", function(name)
    return { kind = "header", name = name }
  end)
  rawset(env, "Settings", {
    VarType = { Boolean = "boolean", Number = "number", String = "string" },
    RegisterCanvasLayoutCategory = function(frame_, name)
      local c = newCategory(name, frame_)
      return c, {}
    end,
    RegisterCanvasLayoutSubcategory = function(parent, frame_, name)
      local c = newCategory(name, frame_, parent)
      table.insert(parent.subcategories, c)
      return c, {}
    end,
    RegisterVerticalLayoutCategory = function(name)
      local c = newCategory(name)
      local layout = { initializers = {} }
      function layout:AddInitializer(i)
        table.insert(self.initializers, i)
        if type(i) == "table" and i.kind == "header" then
          c.controls = c.controls or {}
          table.insert(c.controls, i)
        end
      end
      return c, layout
    end,
    RegisterVerticalLayoutSubcategory = function(parent, name)
      local c = newCategory(name, nil, parent)
      table.insert(parent.subcategories, c)
      return c, { AddInitializer = function() end }
    end,
    RegisterAddOnCategory = function(cat) table.insert(sim.settingsCategories, cat) end,
    OpenToCategory = function(id)
      sim.openedSettingsCategory = id
      local function find(list)
        for _, c in ipairs(list) do
          if type(c) == "table" and (c == id or c.ID == id or c.name == id) then return c end
          local sub = type(c) == "table" and c.subcategories and find(c.subcategories)
          if sub then return sub end
        end
      end
      local cat = find(sim.settingsCategories)
      require("wowapi.settingspanel").open(sim, cat)
      return cat ~= nil
    end,
    GetCategory = function(id) for _, c in ipairs(sim.settingsCategories) do if c.ID == id then return c end end end,
    RegisterAddOnSetting = function(category, variable, key, tbl, vtype, name, default)
      return newSetting(category, variable, tbl, key, vtype, name, default)
    end,
    RegisterProxySetting = function(category, variable, vtype, name, default, getter, setter)
      local proxy = setmetatable({}, { __index = function() return getter() end, __newindex = function(_, _, v) setter(v) end })
      return newSetting(category, variable, proxy, "v", vtype, name, default)
    end,
    CreateCheckbox = function(category, setting, tooltip) return control(category, "checkbox", setting, nil, tooltip) end,
    CreateCheckBox = function(category, setting, tooltip) return control(category, "checkbox", setting, nil, tooltip) end,
    CreateSlider = function(category, setting, options, tooltip) return control(category, "slider", setting, options, tooltip) end,
    CreateDropdown = function(category, setting, options, tooltip) return control(category, "dropdown", setting, options, tooltip) end,
    CreateSliderOptions = function(lo, hi, step)
      local o = { minValue = lo, maxValue = hi, steps = step }
      function o:SetLabelFormatter(kind, fn) self.formatter = fn end
      return o
    end,
    CreateControlTextContainer = function()
      local c = { data = {} }
      function c:Add(value, label) table.insert(self.data, { value = value, label = label }) end
      function c:GetData() return self.data end
      return c
    end,
    SetValue = function() end,
  })
  rawset(env, "MinimalSliderWithSteppersMixin", { Label = { Right = "RIGHT", Left = "LEFT", Top = "TOP" } })

  ------------------------------------------------------------ legacy globals
  -- Removed from the mainline client. Only defined with { legacyGlobals = true }
  -- so addons don't accidentally depend on API that doesn't exist in game.
  if sim.opts.legacyGlobals then
    rawset(env, "GetAddOnMetadata", addonsApi.GetAddOnMetadata)
    rawset(env, "IsAddOnLoaded", addonsApi.IsAddOnLoaded)
    rawset(env, "LoadAddOn", addonsApi.LoadAddOn)
    rawset(env, "GetNumAddOns", addonsApi.GetNumAddOns)
    rawset(env, "GetAddOnInfo", addonsApi.GetAddOnInfo)
    rawset(env, "GetItemInfo", GetItemInfo)
    rawset(env, "GetSpellInfo", function(ref)
      local s = spellRef(ref); if not s then return nil end
      return s.name, nil, s.icon or 136243, s.castTime or 0, s.minRange or 0, s.maxRange or 0, s.id
    end)
    rawset(env, "GetCoinTextureString", env.C_CurrencyInfo.GetCoinTextureString)
    rawset(env, "InterfaceOptions_AddCategory", function(panel) table.insert(sim.settingsCategories, panel) end)
    rawset(env, "InterfaceOptionsFrame_OpenToCategory", function() end)
  end
end

return M
