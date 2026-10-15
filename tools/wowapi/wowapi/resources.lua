-- Fills in everything else the live client defines, from
-- wowapi/data/resources.lua (generated from Ketho/BlizzardInterfaceResources):
--
--  * GlobalStrings for the client locale (TANK, HEALER, ERR_*, ...)
--  * LE_* and other Lua constants, the complete Enum and Constants tables
--  * CVar defaults
--  * every global API / FrameXML function name that isn't emulated becomes a
--    recorded no-op returning nil (it exists in game, so `if Func then`
--    checks behave the same, and calling it doesn't crash)
--  * every Blizzard template name (CreateFrame(..., "SomeBlizzardTemplate")
--    works), every font object, and placeholder frames for the named
--    Blizzard frames (PlayerFrame, ColorPickerFrame, ...)
--  * the full event list
local M = {}

local data
function M.data()
  if not data then data = require("wowapi.data.resources") end
  return data
end

local stringsCache = {}
function M.globalStrings(locale)
  if stringsCache[locale] then return stringsCache[locale] end
  local toc = require("wowapi.toc")
  local base = debug.getinfo(1, "S").source:sub(2):gsub("[^/\\]*$", "")
  local path = base .. "data/globalstrings_" .. locale .. ".lua"
  if not toc.exists(path) then path = base .. "data/globalstrings_enUS.lua" end
  local t = {}
  t._G = t
  local chunk = loadfile(path)
  if chunk then
    if setfenv then setfenv(chunk, t) else chunk = load(io.open(path):read("*a"), "=GlobalStrings", "t", t) end
    pcall(chunk)
  end
  t._G = nil
  stringsCache[locale] = t
  return t
end

-- WOW_PROJECT_* are engine constants. WoW: Forever runs the mainline UI;
-- its project ID isn't published, so it defaults to WOW_PROJECT_MAINLINE
-- and can be overridden with WoW.new({ projectId = n }).
local PROJECTS = { WOW_PROJECT_MAINLINE = 1, WOW_PROJECT_CLASSIC = 2, WOW_PROJECT_BURNING_CRUSADE_CLASSIC = 5,
  WOW_PROJECT_WRATH_CLASSIC = 11, WOW_PROJECT_CATACLYSM_CLASSIC = 14, WOW_PROJECT_MISTS_CLASSIC = 19 }

-- Retail frames addons reference that aren't in the resource frame list.
local EXTRA_FRAMES = { "MicroMenu", "MicroMenuContainer", "BagsBar", "MainActionBar", "MainMenuBar",
  "MultiBarBottomLeft", "MultiBarBottomRight", "MultiBarRight", "MultiBarLeft", "MultiBar5", "MultiBar6",
  "MultiBar7", "StanceBar", "PetActionBar", "PossessActionBar", "OverrideActionBar", "ExtraAbilityContainer",
  "ExtraActionBarFrame", "ZoneAbilityFrame", "StatusTrackingBarManager", "MainStatusTrackingBarContainer",
  "SecondaryStatusTrackingBarContainer", "PlayerCastingBarFrame", "PetCastingBarFrame", "QueueStatusButton",
  "EditModeManagerFrame", "GameMenuFrame", "CharacterMicroButton", "ProfessionMicroButton", "PlayerSpellsMicroButton",
  "AchievementMicroButton", "QuestLogMicroButton", "GuildMicroButton", "LFDMicroButton", "CollectionsMicroButton",
  "EJMicroButton", "StoreMicroButton", "MainMenuMicroButton", "HelpMicroButton", "HousingMicroButton",
  "MainMenuBarBackpackButton", "CharacterBag0Slot", "CharacterBag1Slot", "CharacterBag2Slot", "CharacterBag3Slot",
  "CharacterReagentBag0Slot", "BagBarExpandToggle", "PlayerFrame", "TargetFrame", "FocusFrame", "PartyFrame",
  "BuffFrame", "DebuffFrame", "ObjectiveTrackerFrame", "MinimapCluster", "ChatFrameMenuButton", "UIWidgetTopCenterContainerFrame",
  "MainMenuBarVehicleLeaveButton", "ChatFrameChannelButton", "ContainerFrameCombinedBags", "ContainerFrameContainer",
  "ContainerFrame1", "ContainerFrame2", "ContainerFrame3", "ContainerFrame4", "ContainerFrame5", "ContainerFrame6",
  "ContainerFrame7", "ContainerFrame8", "ContainerFrame9", "ContainerFrame10", "ContainerFrame11", "ContainerFrame12",
  "ContainerFrame13", "BankFrame", "AccountBankPanel", "BankPanel", "GuildBankFrame", "VoidStorageFrame", "AddonListForceLoad", "PaladinPowerBarFrame", "MonkHarmonyBarFrame", "RuneFrame", "TotemFrame" }

local MICRO_BUTTONS = { "CharacterMicroButton", "ProfessionMicroButton", "PlayerSpellsMicroButton",
  "AchievementMicroButton", "QuestLogMicroButton", "HousingMicroButton", "GuildMicroButton", "LFDMicroButton",
  "CollectionsMicroButton", "EJMicroButton", "StoreMicroButton", "MainMenuMicroButton" }

local SHOWN_FRAMES = { PlayerFrame = true, MainMenuBar = true, MainActionBar = true, MinimapCluster = true,
  ChatFrame1 = true, ObjectiveTrackerFrame = true, BuffFrame = true, MicroMenu = true, BagsBar = true }

local FRAME_TYPES = { AddonListForceLoad = "CheckButton" }

local function frameTypeFor(name)
  if FRAME_TYPES[name] then return FRAME_TYPES[name] end
  if name:find("Tooltip$") then return "GameTooltip" end
  if name:find("Button$") then return "Button" end
  if name:find("EditBox$") then return "EditBox" end
  if name:find("ScrollFrame$") then return "ScrollFrame" end
  if name:find("StatusBar$") or name:find("Bar$") and name:find("Casting") then return "StatusBar" end
  return "Frame"
end

-- Lookup sets built once per process.
local setsCache
function M._resetSets() setsCache = nil end
function M.sets()
  if setsCache then return setsCache end
  local r = M.data()
  local s = { frames = {}, fonts = {}, functions = {}, namespaces = {}, mixins = {} }
  for _, n in ipairs(r.frames) do s.frames[n] = true end
  for _, n in ipairs(EXTRA_FRAMES) do s.frames[n] = true end
  for n, t in pairs(r.templates) do if t[1] == "Font" or t[1] == "FontFamily" then s.fonts[n] = true end end
  for _, n in ipairs(r.globalAPI) do if not n:find("%.") then s.functions[n] = true end end
  for _, n in ipairs(r.frameXML) do
    local ns, fname = n:match("^([%w_]+)%.([%w_]+)$")
    if ns then
      s.namespaces[ns] = s.namespaces[ns] or {}
      table.insert(s.namespaces[ns], fname)
    elseif not n:find("[.:]") then
      s.functions[n] = true
    end
  end
  for _, n in ipairs(r.mixins) do s.mixins[n] = true end
  require("wowapi.realclient").extendSets(s)
  setsCache = s
  return s
end

local ACTION_BARS = { ActionButton = true, MultiBarBottomLeftButton = true, MultiBarBottomRightButton = true,
  MultiBarRightButton = true, MultiBarLeftButton = true, MultiBar5Button = true, MultiBar6Button = true,
  MultiBar7Button = true, PetActionButton = true, StanceButton = true }

-- An action button like the ones FrameXML creates (ActionBarButtonTemplate).
function M.actionButton(sim, name, id)
  local widgets = require("wowapi.widgets")
  local env = sim.env
  local b = widgets.create(sim, "CheckButton", name, rawget(env, "UIParent"))
  local s = sim.widgetState[b]
  s.protected = true
  s.width, s.height = 45, 45
  s.shown = name:find("^ActionButton") ~= nil
  s.placeholder = true
  b:SetID(id)
  s.attributes.action = id
  s.attributes.type = "action"
  local function region(t, suffix, key, layer)
    local r = widgets.create(sim, t, name .. suffix, b)
    sim.widgetState[r].layer = layer or "ARTWORK"
    b[key] = r
    return r
  end
  region("Texture", "Icon", "icon", "BACKGROUND"):SetAllPoints()
  b.Icon = b.icon
  region("FontString", "Count", "Count", "OVERLAY")
  region("FontString", "HotKey", "HotKey", "ARTWORK")
  region("FontString", "Name", "Name", "OVERLAY")
  region("Texture", "Border", "Border", "OVERLAY")
  region("Texture", "Flash", "Flash", "ARTWORK")
  local cd = widgets.create(sim, "Cooldown", name .. "Cooldown", b)
  cd:SetAllPoints()
  b.cooldown = cd
  b.Cooldown = cd
  b:SetNormalTexture("Interface\\Buttons\\UI-Quickslot2")
  b.NormalTexture = b:GetNormalTexture()
  b.action = id
  -- the main bar sits at the bottom centre of the screen like the default UI
  if name:find("^ActionButton") then
    b:SetPoint("BOTTOMLEFT", rawget(env, "UIParent"), "BOTTOM", (id - 7) * 48 + 1, 24)
    local ok, tex = pcall(env.GetActionTexture, id)
    if ok and tex then b.icon:SetTexture(tex) end
    b.HotKey:SetPoint("TOPRIGHT", -3, -4)
    b.HotKey:SetText(id == 11 and "-" or id == 12 and "=" or tostring(id % 10))
  end
  return b
end

-- Behaviour of Blizzard frames that addons commonly hook into.
local FRAME_REGISTRIES = { ActionBarButtonEventsFrame = true, ActionBarActionEventsFrame = true,
  ActionBarButtonUpdateFrame = true, ActionBarButtonRangeCheckFrame = true, ActionBarButtonUsableWatcherFrame = true }
local VERB = { "Get", "Set", "Is", "Has", "Show", "Hide", "Update", "On", "Register", "Unregister", "Enable",
  "Disable", "Add", "Remove", "Clear", "Refresh", "Layout", "Can", "Should", "Apply", "Setup", "SetUp", "Init",
  "Reset", "Toggle", "For", "Evaluate", "Acquire", "Release", "Open", "Close", "Play", "Stop", "Mark", "Try",
  "Handle", "Check", "Find", "Select", "Lock", "Unlock", "Begin", "End", "Start", "Cancel", "Load", "Save",
  "Create", "Destroy", "Attach", "Detach", "Process", "Request", "Notify", "Trigger", "Fire", "Invoke", "Run",
  "Generate", "Build", "Calculate", "Sort", "Filter", "Merge", "Queue", "Assign", "Release", "Resize", "Anchor" }
local function looksLikeMethod(k)
  for _, v in ipairs(VERB) do
    if k:sub(1, #v) == v and (#k == #v or k:sub(#v + 1, #v + 1):match("[%u%d_]")) then return true end
  end
  return false
end
M.looksLikeMethod = looksLikeMethod

-- Blizzard's frames have methods and child regions this simulator doesn't
-- know by name; on a placeholder, an unknown method-like key is a recorded
-- no-op and an unknown noun-like key is an empty placeholder child frame.
function M.blizzardFallback(sim, obj, label)
  local mt = getmetatable(obj)
  local base = mt.__index
  sim.widgetState[obj].baseMeta = mt
  setmetatable(obj, { __tostring = mt.__tostring, __index = function(t, k)
    local v = type(base) == "table" and base[k] or nil
    if v ~= nil then return v end
    if type(k) ~= "string" or not k:match("^%u") then return nil end
    if looksLikeMethod(k) then
      local fn = function()
        local key = label .. ":" .. k
        sim.stubbedCalls[key] = (sim.stubbedCalls[key] or 0) + 1
      end
      rawset(t, k, fn)
      return fn
    end
    local child = require("wowapi.widgets").create(sim, "Frame", nil, t)
    sim.widgetState[child].placeholder = true
    sim.widgetState[child].autoChild = true
    sim.widgetState[child].shown = false
    M.blizzardFallback(sim, child, label .. "." .. k)
    rawset(t, k, child)
    return child
  end })
end

function M.decorateFrame(sim, name, f)
  M.blizzardFallback(sim, f, name)
  if name == "NamePlateDriverFrame" then require("wowapi.nameplates").decorateDriver(sim, f) end
  if name == "MicroMenu" then
    -- MicroMenuMixin:GenerateButtonInfos lists the micro buttons in bar order
    function f:GenerateButtonInfos()
      local out = {}
      for _, n in ipairs(MICRO_BUTTONS) do
        local b = sim:Get(n)
        if b then
          local st = sim.widgetState[b]
          st.shown, st.width, st.height = true, 32, 40
          out[#out + 1] = { button = b }
        end
      end
      return out
    end
  end
  if FRAME_REGISTRIES[name] then
    f.frames = {}
    function f:RegisterFrame(frame) table.insert(self.frames, frame) end
    function f:UnregisterFrame(frame)
      for i = #self.frames, 1, -1 do if self.frames[i] == frame then table.remove(self.frames, i) end end
    end
    function f:ForEachFrame(fn) for _, frame in ipairs(self.frames) do fn(frame) end end
    -- the default action bars register their buttons
    if name == "ActionBarButtonEventsFrame" or name == "ActionBarActionEventsFrame" then
      for i = 1, 12 do f:RegisterFrame(sim:Get("ActionButton" .. i)) end
    end
  end
end

function M.install(sim, env)
  local r = M.data()
  local widgets = require("wowapi.widgets")
  sim.resources = r

  -- constants: FrameXML's own (INVSLOT_*, ...) then engine project IDs
  for k, v in pairs(require("wowapi.data.constants")) do
    if rawget(env, k) == nil then
      if type(v) == "table" then
        local copy = {}
        for a, b in pairs(v) do
          if type(b) == "table" then local c = {}; for x, y in pairs(b) do c[x] = y end; copy[a] = c else copy[a] = b end
        end
        v = copy
      end
      rawset(env, k, v)
    elseif type(v) == "table" and type(rawget(env, k)) == "table" and not sim.widgetState[rawget(env, k)] then
      -- a hand-written table (SOUNDKIT, ...) gets FrameXML's missing entries
      local cur = rawget(env, k)
      for a, b in pairs(v) do if rawget(cur, a) == nil and type(b) ~= "table" then rawset(cur, a, b) end end
    end
  end
  for k, v in pairs(PROJECTS) do rawset(env, k, v) end
  rawset(env, "WOW_PROJECT_ID", sim.opts.projectId or PROJECTS.WOW_PROJECT_MAINLINE)
  local enum = rawget(env, "Enum")
  for name, vals in pairs(r.enums) do
    enum[name] = enum[name] or {}
    for k, v in pairs(vals) do if enum[name][k] == nil then enum[name][k] = v end end
  end
  -- Enum.<Name>Meta = { MinValue, MaxValue, NumValues } like the client
  for name, vals in pairs(enum) do
    if type(vals) == "table" and not name:match("Meta$") and enum[name .. "Meta"] == nil then
      local lo, hi, n = nil, nil, 0
      for _, v in pairs(vals) do
        if type(v) == "number" then
          n = n + 1
          if not lo or v < lo then lo = v end
          if not hi or v > hi then hi = v end
        end
      end
      if n > 0 then enum[name .. "Meta"] = { MinValue = lo, MaxValue = hi, NumValues = n } end
    end
  end
  local consts = rawget(env, "Constants")
  for name, vals in pairs(r.constants) do
    if type(vals) == "table" then
      consts[name] = consts[name] or {}
      for k, v in pairs(vals) do if consts[name][k] == nil then consts[name][k] = v end end
    end
  end
  sim.cvarDefaults = r.cvars

  -- Everything below is resolved lazily on first access from the global
  -- environment (see sim.lua's env __index), so a sim only pays for the
  -- globals its addons actually touch.
  local sets = M.sets()
  local function stub(label)
    return function()
      sim.stubbedCalls[label] = (sim.stubbedCalls[label] or 0) + 1
      return nil
    end
  end

  -- templates: every Blizzard template name can be inherited
  setmetatable(sim.templates, { __index = function(t, name)
    local def = r.templates[name]
    if not def or def[1] == "Font" or def[1] == "FontFamily" then return nil end
    local typ, inherits, mixin = def[1], def[2], def[3]
    local entry = { type = typ, blizzard = true, builtin = function(s, obj, create)
      if inherits then
        for _, base in ipairs(require("wowapi.templates").split(inherits)) do
          local bt = s.templates[base]
          if bt and bt.builtin then bt.builtin(s, obj, create) end
        end
      end
      if mixin then
        for _, m in ipairs(require("wowapi.templates").split(mixin)) do
          local mt = s.env[m]
          if type(mt) == "table" then for k, v in pairs(mt) do obj[k] = v end end
        end
      end
    end }
    rawset(t, name, entry)
    return entry
  end })

  local function font(name, seen)
    local existing = rawget(env, name)
    if existing then return existing end
    local t = r.templates[name]
    if not t or (t[1] ~= "Font" and t[1] ~= "FontFamily") then return nil end
    seen = seen or {}
    if seen[name] then return nil end
    seen[name] = true
    local f = widgets.create(sim, "Font", name)
    local fs = sim.widgetState[f]
    local baseName = t[2] and require("wowapi.templates").split(t[2])[1]
    local base = baseName and (rawget(env, baseName) or font(baseName, seen))
    local bs = base and sim.widgetState[base]
    fs.font = bs and bs.font and { bs.font[1], bs.font[2], bs.font[3] } or { "Fonts\\FRIZQT__.TTF", 12, "" }
    fs.textColor = bs and bs.textColor
    fs.justifyH = bs and bs.justifyH
    return f
  end

  local strings = M.globalStrings(sim.locale)
  -- returns value, found
  function sim._lazyGlobal(k)
    local v = strings[k]
    if v ~= nil then return v, true end
    v = r.globals[k]
    if v ~= nil then return v, true end
    if sets.frames[k] then
      local ok, f = pcall(widgets.create, sim, (sets.frameTypes or {})[k] or frameTypeFor(k), k, rawget(env, "UIParent"))
      if not ok then ok, f = pcall(widgets.create, sim, frameTypeFor(k), k, rawget(env, "UIParent")) end
      if ok then
        local st = sim.widgetState[f]
        st.shown = SHOWN_FRAMES[k] or false
        st.placeholder = true
        M.decorateFrame(sim, k, f)
        return f, true
      end
    end
    if sets.fonts[k] then return font(k), true end
    -- Blizzard action bar buttons and their named children
    if type(k) == "string" then
      local bar, idx, suffix = k:match("^(%a-Button)(%d+)(%a*)$")
      if bar and ACTION_BARS[bar] and tonumber(idx) >= 1 and tonumber(idx) <= 12 then
        local b = rawget(env, bar .. idx)
        if not b then
          -- the whole bar exists at once, as in the client
          for i = 1, 12 do if not rawget(env, bar .. i) then M.actionButton(sim, bar .. i, i) end end
          b = rawget(env, bar .. idx)
        end
        if suffix == "" then return b, true end
        local c = rawget(env, k)
        if c then return c, true end
      end
    end
    if sets.functions[k] then
      local fn = stub(k)
      rawset(env, k, fn)
      return fn, true
    end
    if sets.namespaces[k] then
      local t = {}
      for _, fname in ipairs(sets.namespaces[k]) do t[fname] = stub(k .. "." .. fname) end
      rawset(env, k, t)
      return t, true
    end
    if sets.mixins[k] then
      local known = {}
      for _, m in ipairs((sets.mixinMethods or {})[k] or {}) do known[m] = true end
      local t = setmetatable({}, { __index = function(tbl, key)
        if type(key) == "string" and (known[key] or key:match("^%u") and looksLikeMethod(key)) then
          local fn = function() sim.stubbedCalls[k .. "." .. key] = (sim.stubbedCalls[k .. "." .. key] or 0) + 1 end
          rawset(tbl, key, fn)
          return fn
        end
      end })
      sim.blizzardTables[t] = k
      -- methods the real client's mixin has (recorded by SimCheck) exist up
      -- front, so Mixin() copies them and hooksecurefunc finds them
      for _, m in ipairs((sets.mixinMethods or {})[k] or {}) do local _ = t[m] end
      rawset(env, k, t)
      return t, true
    end
    return nil, false
  end

  -- namespaces we already defined by hand (C_*, PixelUtil...) get the
  -- missing FrameXML functions as stubs
  for ns, fnames in pairs(sets.namespaces) do
    local t = rawget(env, ns)
    if type(t) == "table" and not sim.widgetState[t] then
      for _, fname in ipairs(fnames) do
        if rawget(t, fname) == nil then rawset(t, fname, stub(ns .. "." .. fname)) end
      end
    end
  end

  -- events
  for _, e in ipairs(r.events) do sim.knownEvents[e] = true end
  -- the client's own check matches what RegisterEvent accepts
  local eu = rawget(env, "C_EventUtils") or {}
  eu.IsEventValid = function(event) return sim.knownEvents[event] == true end
  rawset(env, "C_EventUtils", eu)

  -- facts recorded from the real client win over everything above
  require("wowapi.realclient").install(sim, env)
end

return M
