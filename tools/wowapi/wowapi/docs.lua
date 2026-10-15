-- Brings the whole documented game API into the simulation, using the data
-- generated from Blizzard's API documentation (wowapi/data/apidocs.lua).
--
--  * Every documented function exists. Hand-written implementations (api.lua)
--    are used where they exist; everything else returns typed "empty game"
--    defaults (0, "", false, {} or a filled-in structure) and can be mocked.
--  * Every call is argument-checked against the documented signature and
--    recorded, so tests can assert on it (sim:Calls / sim:CallCount).
--  * The full Enum and Constants tables are defined.
--  * The event list is known, so registering a misspelled event errors like
--    it does in game.
--  * Every widget type gets its full documented method set.
local M = {}

local data
function M.data()
  if not data then data = require("wowapi.data.apidocs") end
  return data
end

------------------------------------------------------------------ types

local NUMBER = { number = true, luaIndex = true, fileID = true, time_t = true, uiUnit = true,
  uiFontHeight = true, size = true, normalizedValue = true, SingleColorValue = true, BigInteger = true,
  BigUInteger = true, CurrencyID = true, SpellIdentifier = false }
local STRING = { string = true, cstring = true, textureAtlas = true, textureKit = true, WOWGUID = true,
  UnitToken = true, UnitTokenVariant = true, kstringClubMessage = true, FileAsset = true, ClubId = true,
  ClubStreamId = true, ClubInvitationId = true, mouseButton = true, FontAsset = true }
local WIDGET = { ScriptRegion = true, SimpleFrame = true, SimpleRegion = true, SimpleTexture = true,
  SimpleFontString = true, SimpleButton = true, SimpleFont = true, SimpleAnimGroup = true, SimpleMaskTexture = true,
  SimpleLine = true, SimpleObject = true, FrameScriptObject = true, SimpleControlPoint = true, SimpleAnim = true }

local function kind(t)
  local d = M.data()
  if t == "bool" then return "bool" end
  if NUMBER[t] then return "number" end
  if STRING[t] then return "string" end
  if t == "table" or d.structures[t] then return "table" end
  if t == "LuaFunctionReference" or d.callbacks[t] or t:match("Callback$") then return "function" end
  if d.enums[t] then return "enum" end
  if WIDGET[t] then return "widget" end
  return nil
end
M.kind = kind

local function typeOk(k, v)
  local tv = type(v)
  if k == "number" then return tv == "number" or (tv == "string" and tonumber(v) ~= nil) end
  if k == "string" then return tv == "string" or tv == "number" end
  if k == "table" then return tv == "table" end
  if k == "function" then return tv == "function" or (tv == "table" and getmetatable(v) and getmetatable(v).__call ~= nil) end
  if k == "enum" then return tv == "number" or tv == "string" end
  if k == "widget" then return tv == "table" or tv == "string" end
  return true
end

-- Methods whose real signature is looser than the documentation says.
local LOOSE = { SetPoint = true, SetAllPoints = true, SetParent = true, SetText = true, SetFormattedText = true,
  SetTexture = true, SetNormalTexture = true, SetPushedTexture = true, SetHighlightTexture = true,
  SetDisabledTexture = true, SetCheckedTexture = true, SetStatusBarTexture = true, SetThumbTexture = true,
  SetFont = true, SetFontObject = true, SetNormalFontObject = true, SetHighlightFontObject = true,
  SetDisabledFontObject = true, SetScript = true, HookScript = true, SetAttribute = true,
  SetStartPoint = true, SetEndPoint = true }

-- Returns nil or an error message.
local function check(name, args, argv, n, isMethod)
  local offset = isMethod and 1 or 0
  for i, a in ipairs(args) do
    local v = argv[i + offset]
    local k = kind(a[2])
    if v == nil then
      if not isMethod and not a[3] and a[5] == nil and (k == "number" or k == "string" or k == "table" or k == "function") then
        return string.format("bad argument #%d to '%s' (%s expected, got no value)", i, name, a[2])
      end
    elseif k and k ~= "bool" and not typeOk(k, v) then
      return string.format("bad argument #%d to '%s' (%s expected, got %s)", i, name, a[2], type(v))
    end
  end
end
M.check = check

------------------------------------------------------------------ defaults

local function default(t, nilable, inner, depth)
  if nilable then return nil end
  depth = depth or 0
  local d = M.data()
  local k = kind(t)
  if k == "number" then return 0 end
  if k == "string" then return "" end
  if k == "bool" then return false end
  if k == "enum" then
    local lo
    for _, v in pairs(d.enums[t]) do if not lo or v < lo then lo = v end end
    return lo or 0
  end
  if k == "table" then
    if t == "table" or inner or depth > 3 then return {} end
    local s = {}
    for _, f in ipairs(d.structures[t] or {}) do
      if f[5] ~= nil then s[f[1]] = f[5] else s[f[1]] = default(f[2], f[3], f[4], depth + 1) end
    end
    return s
  end
  if k == "function" then return function() end end
  return nil
end
M.default = default

local function defaults(returns)
  local out = {}
  for i, r in ipairs(returns) do
    if r[5] ~= nil then out[i] = r[5] else out[i] = default(r[2], r[3], r[4]) end
  end
  return out, #returns
end
M.defaults = defaults

------------------------------------------------------------------ call log

local MAX_LOG = 200
local function record(sim, key, ...)
  local c = sim.apiCalls[key]
  if not c then c = { n = 0, log = {} }; sim.apiCalls[key] = c end
  c.n = c.n + 1
  if #c.log < MAX_LOG then c.log[#c.log + 1] = { n = select("#", ...), ... } end
end

------------------------------------------------------------------ install: globals

local unpack = unpack or table.unpack
local secrets = require("wowapi.secrets")

function M.install(sim, env)
  local d = M.data()
  sim.apiDocs = d
  sim.apiCalls = {}
  sim.mocks = {}
  local strict = sim.opts.strictArgs ~= false

  -- Enum / Constants: documented values, plus anything api.lua added.
  local enum = rawget(env, "Enum") or {}
  for name, vals in pairs(d.enums) do
    local t = enum[name] or {}
    for k, v in pairs(vals) do t[k] = v end
    enum[name] = t
  end
  rawset(env, "Enum", enum)
  local consts = {}
  for name, vals in pairs(d.constants) do
    local t = {}
    for k, v in pairs(vals) do t[k] = v end
    consts[name] = t
  end
  rawset(env, "Constants", consts)

  -- Functions
  for key, doc in pairs(d.functions) do
    local ns, fname = key:match("^([^.]+)%.(.+)$")
    local holder = env
    if ns then
      holder = rawget(env, ns)
      if not holder then holder = {}; rawset(env, ns, holder) end
    else
      fname = key
    end
    local impl = rawget(holder, fname)
    local rets = doc.r
    local args = doc.a
    local function body(...)
      local mock = sim.mocks[key]
      if mock then return mock(...) end
      if impl then return impl(...) end
      if sim.fakeData then
        local out, n = require("wowapi.faker").returns(key, doc, ...)
        return unpack(out, 1, n)
      end
      local out, n = defaults(rets)
      return unpack(out, 1, n)
    end
    local function wrapAll(...)
      local t = { n = select("#", ...), ... }
      for i = 1, t.n do t[i] = secrets.wrap(t[i]) end
      return unpack(t, 1, t.n)
    end
    rawset(holder, fname, function(...)
      record(sim, key, ...)
      if strict then
        local err = check(key, args, { ... }, select("#", ...), false)
        if err then error(err, 2) end
      end
      if sim.secretRestrictions and secrets.shouldWrap(sim, doc, (...)) then return wrapAll(body(...)) end
      return body(...)
    end)
  end

  -- Events
  local known = {}
  for name in pairs(d.events) do known[name] = true end
  for _, extra in ipairs(sim.opts.extraEvents or {}) do known[extra] = true end
  sim.knownEvents = known
end

------------------------------------------------------------------ install: widgets

-- Widget type -> documented API tables it implements (own tables only;
-- inherited ones come from the parent type).
M.WIDGETS = {
  { "Object", nil, { "SimpleObjectAPI", "SimpleFrameScriptObjectAPI" } },
  { "Region", "Object", { "SimpleScriptRegionAPI", "SimpleScriptRegionResizingAPI", "SimpleAnimatableObjectAPI", "SimpleRegionAPI" } },
  { "TextureBase", "Region", { "SimpleTextureBaseAPI" } },
  { "Texture", "TextureBase", { "SimpleTextureAPI" } },
  { "MaskTexture", "TextureBase", { "SimpleMaskTextureAPI" } },
  { "Line", "TextureBase", { "SimpleLineAPI" } },
  { "FontString", "Region", { "SimpleFontStringAPI" } },
  { "Frame", "Region", { "SimpleFrameAPI" } },
  { "Button", "Frame", { "SimpleButtonAPI" } },
  { "CheckButton", "Button", { "SimpleCheckboxAPI" } },
  { "EditBox", "Frame", { "SimpleEditBoxAPI" } },
  { "Slider", "Frame", { "SimpleSliderAPI" } },
  { "StatusBar", "Frame", { "SimpleStatusBarAPI" } },
  { "ScrollFrame", "Frame", { "SimpleScrollFrameAPI" } },
  { "MessageFrame", "Frame", { "SimpleMessageFrameAPI" } },
  { "ScrollingMessageFrame", "Frame", {} },
  { "ColorSelect", "Frame", { "SimpleColorSelectAPI" } },
  { "Cooldown", "Frame", { "FrameAPICooldown" } },
  { "GameTooltip", "Frame", { "FrameAPITooltip" } },
  { "SimpleHTML", "Frame", { "SimpleHTMLAPI" } },
  { "Browser", "Frame", { "SimpleBrowserAPI" } },
  { "MovieFrame", "Frame", { "SimpleMovieAPI" } },
  { "Minimap", "Frame", { "MinimapFrameAPI" } },
  { "OffScreenFrame", "Frame", { "SimpleOffScreenFrameAPI" } },
  { "MapScene", "Frame", { "SimpleMapSceneAPI" } },
  { "Model", "Frame", { "SimpleModelAPI" } },
  { "PlayerModel", "Model", { "FrameAPICharacterModelBase" } },
  { "DressUpModel", "PlayerModel", { "FrameAPIDressUpModel" } },
  { "CinematicModel", "PlayerModel", { "FrameAPICinematicModel" } },
  { "TabardModel", "PlayerModel", { "FrameAPITabardModelBase", "FrameAPITabardModel" } },
  { "ModelScene", "Frame", { "FrameAPIModelSceneFrame" } },
  { "ModelSceneActor", "Object", { "FrameAPIModelSceneFrameActorBase", "FrameAPIModelSceneFrameActor" } },
  { "UnitPositionFrame", "Frame", { "FrameAPIUnitPositionFrame" } },
  { "FogOfWarFrame", "Frame", { "FrameAPIFogOfWarFrame" } },
  { "Blob", "Frame", { "FrameAPIBlob" } },
  { "ArchaeologyDigSiteFrame", "Blob", { "FrameAPIArchaeologyDigSiteFrame" } },
  { "QuestPOIFrame", "Blob", { "FrameAPIQuestPOI" } },
  { "ScenarioPOIFrame", "Blob", { "FrameAPIScenarioPOI" } },
  { "Checkout", "Frame", { "FrameAPISimpleCheckout" } },
  { "AuraContainer", "Frame", {} },
  { "ItemButton", "Button", {} },
  { "Font", "Object", { "SimpleFontAPI" } },
  { "AnimationGroup", "Object", { "SimpleAnimGroupAPI" } },
  { "Animation", "Object", { "SimpleAnimAPI" } },
  { "Alpha", "Animation", { "SimpleAnimAlphaAPI" } },
  { "Rotation", "Animation", { "SimpleAnimRotationAPI" } },
  { "Scale", "Animation", { "SimpleAnimScaleAPI" } },
  { "LineScale", "Scale", { "SimpleAnimScaleLineAPI" } },
  { "Translation", "Animation", { "SimpleAnimTranslationAPI" } },
  { "LineTranslation", "Translation", { "SimpleAnimTranslationLineAPI" } },
  { "TextureCoordTranslation", "Animation", { "SimpleAnimTextureCoordTranslationAPI" } },
  { "FlipBook", "Animation", { "SimpleAnimFlipBookAPI" } },
  { "Path", "Animation", { "SimpleAnimPathAPI" } },
  { "VertexColor", "Animation", { "SimpleAnimVertexColorAPI" } },
  { "ControlPoint", "Object", { "SimpleControlPointAPI" } },
}

-- Documented methods for a widget type, including inherited ones.
function M.methodsFor(typeName)
  local d = M.data()
  local byName = {}
  for _, w in ipairs(M.WIDGETS) do byName[w[1]] = w end
  local out = {}
  local chain = {}
  local w = byName[typeName]
  while w do table.insert(chain, 1, w); w = byName[w[2]] end
  for _, x in ipairs(chain) do
    for _, api in ipairs(x[3]) do
      for name, doc in pairs(d.scriptObjects[api] or {}) do out[name] = doc end
    end
  end
  return out
end

-- Fill every widget class with its documented methods. `classes` maps a
-- type name to { methods = {}, parent = cls }. Methods already written by
-- hand keep their behaviour but gain argument checks; the rest become
-- getter/setter pairs backed by widget state, or recorded no-ops that
-- return typed defaults.
function M.defineClasses(classes, define)
  for _, w in ipairs(M.WIDGETS) do
    if not classes[w[1]] then define(w[1], w[2] and (classes[w[2]] and w[2]) or nil) end
  end
end

function M.installWidgets(sim, classes, define, state)
  local d = M.data()
  local strict = sim.opts.strictArgs ~= false
  local secure = require("wowapi.secure")
  M.defineClasses(classes, define)
  for _, w in ipairs(M.WIDGETS) do
    local cls = classes[w[1]]
    for _, api in ipairs(w[3]) do
      local methods = d.scriptObjects[api] or {}
      for mname, doc in pairs(methods) do
        local own = rawget(cls.methods, mname)
        local inherited = own or cls.methods[mname]
        local label = w[1] .. ":" .. mname
        local args = doc.a
        local loose = LOOSE[mname]
        local impl = inherited
        if not impl then
          impl = M.autoMethod(sim, mname, doc, methods, state, label)
        end
        local protected = secure.PROTECTED_METHODS[mname]
        for _, f in ipairs(doc.f or {}) do if f == "IsProtectedFunction" then protected = true end end
        local function call(self, ...)
          if strict and not loose then
            local err = check(label, args, { self, ... }, select("#", ...) + 1, true)
            if err then error(err, 2) end
          end
          if protected and sim.lockdown and sim.secureDepth == 0 and secure.isProtected(sim, self) then
            secure.block(sim, (state[self] and state[self].name or label) .. ":" .. mname .. "()")
            return
          end
          return impl(self, ...)
        end
        cls.methods[mname] = function(self, ...)
          if sim.secretValuesEnabled then
            -- widgets accept secret values: they display them without exposing them
            local n = select("#", ...)
            local t = { ... }
            local any = false
            for i = 1, n do if secrets.isSecret(t[i]) then t[i] = secrets.unwrap(t[i]); any = true end end
            if any then return call(self, unpack(t, 1, n)) end
          end
          return call(self, ...)
        end
      end
    end
  end
end

local function suffixOf(name)
  for _, p in ipairs({ "Get", "Set", "Is", "Can", "Has", "Enable", "Disable" }) do
    if name:sub(1, #p) == p and name:sub(#p + 1, #p + 1):match("%u") then return p, name:sub(#p + 1) end
  end
end

function M.autoMethod(sim, name, doc, siblings, state, label)
  local prefix, rest = suffixOf(name)
  local rets = doc.r
  local function props(self)
    local s = state[self]
    if not s then error("not a UI object", 3) end
    s.props = s.props or {}
    return s.props
  end
  local function note(self)
    sim.stubbedCalls[label] = (sim.stubbedCalls[label] or 0) + 1
  end
  if prefix == "Set" then
    return function(self, ...)
      props(self)[rest] = { n = select("#", ...), ... }
    end
  end
  if (prefix == "Get" or prefix == "Is" or prefix == "Can" or prefix == "Has") and
    (siblings["Set" .. rest] or siblings["Enable" .. rest]) then
    return function(self)
      local p = props(self)[rest]
      if p then return unpack(p, 1, math.max(p.n, #rets)) end
      local out, n = defaults(rets)
      return unpack(out, 1, n)
    end
  end
  -- EnableFoo(enable) / IsFooEnabled()
  if prefix == "Enable" then
    return function(self, v) props(self)[rest .. "Enabled"] = { n = 1, v == nil or v and true or false } end
  end
  if prefix == "Is" and rest:match("Enabled$") then
    local key = rest
    return function(self)
      local p = props(self)[key]
      if p then return p[1] end
      return false
    end
  end
  return function(self, ...)
    note(self)
    local out, n = defaults(rets)
    return unpack(out, 1, n)
  end
end

return M
