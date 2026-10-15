-- Widget emulation: CreateFrame and the UI object hierarchy.
--
-- Widgets are plain Lua tables (addons stash fields on them like in the
-- real client). Engine-side state lives in a private side table so it
-- never collides with addon fields.
--
-- Methods either behave like the client (show/hide, scripts, events,
-- text, values...) or are "known stubs": real API names that do nothing
-- here but are recorded in sim.stubbedCalls. Names that are not real API
-- are nil, so a typo errors just like it would in game.
local compat = require("wowapi.compat")
local docs = require("wowapi.docs")
local layout = require("wowapi.layout")
local unpack = compat.unpack

local M = {}

local function isa(cls, target)
  while cls do
    if cls.name == target then return true end
    cls = cls.parent
  end
  return false
end

function M.install(sim, env)
  local state = setmetatable({}, { __mode = "k" })
  local classes = {}
  sim.widgetState = state

  local function S(obj)
    local s = state[obj]
    if not s then error("not a UI object: " .. tostring(obj), 3) end
    return s
  end

  local function define(name, parent, methods)
    local cls = { name = name, parent = parent and classes[parent], methods = methods or {} }
    classes[name] = cls
    setmetatable(cls.methods, { __index = cls.parent and cls.parent.methods or nil })
    return cls.methods
  end

  local stubCache = {}
  local function stub(name)
    if not stubCache[name] then
      stubCache[name] = function(self)
        local key = (state[self] and state[self].type or "?") .. ":" .. name
        sim.stubbedCalls[key] = (sim.stubbedCalls[key] or 0) + 1
      end
    end
    return stubCache[name]
  end

  local visible
  local function fire(obj, script, ...)
    return sim:_runScript(obj, script, ...)
  end

  ---------------------------------------------------------------- Object
  local O = define("Object", nil)
  function O:GetObjectType() return S(self).type end
  function O:IsObjectType(t) return isa(classes[S(self).type], t) end
  function O:IsForbidden() return false end
  function O:GetName() return S(self).name end
  function O:GetDebugName() return S(self).name or tostring(self) end
  function O:GetParent() return S(self).parent end
  function O:GetParentKey() return S(self).parentKey end
  function O:SetParentKey(k)
    local s = S(self)
    s.parentKey = k
    if s.parent then s.parent[k] = self end
  end
  function O:ClearParentKey() S(self).parentKey = nil end

  ---------------------------------------------------------------- Region
  local R = define("Region", "Object")
  function R:SetParent(p)
    local s = S(self)
    if s.parent and state[s.parent] then
      local kids = state[s.parent].children
      for i = #kids, 1, -1 do if kids[i] == self then table.remove(kids, i) end end
    end
    s.parent = p
    if p and state[p] then table.insert(state[p].children, self) end
  end
  function R:Show()
    local s = S(self)
    if s.shown then return end
    s.shown = true
    if visible(self) then fire(self, "OnShow") end
  end
  function R:Hide()
    local s = S(self)
    if not s.shown then return end
    local was = visible(self)
    s.shown = false
    if was then fire(self, "OnHide") end
  end
  function R:SetShown(v) if v then self:Show() else self:Hide() end end
  function R:IsShown() return S(self).shown end
  function R:IsVisible() return visible(self) end
  function R:SetPoint(point, rel, relPoint, x, y)
    local s = S(self)
    if type(point) ~= "string" or not layout.POINTS[point:upper()] then
      error("Invalid region point " .. tostring(point), 2)
    end
    point = point:upper()
    if type(rel) == "number" then rel, relPoint, x, y = nil, nil, rel, relPoint end
    if type(rel) == "string" then
      local name = rel
      rel = sim:Get(name)
      if not rel then error("SetPoint(): Couldn't find region named '" .. name .. "'", 2) end
    end
    if type(relPoint) == "number" and (y == nil) then
      -- SetPoint(point, relativeTo, offsetX, offsetY)
      relPoint, x, y = nil, relPoint, x
    end
    if relPoint ~= nil and (type(relPoint) ~= "string" or not layout.POINTS[relPoint:upper()]) then
      error("Invalid region point " .. tostring(relPoint), 2)
    end
    rel = rel or s.parent
    if rel == self then error("Action[SetPoint] failed because[Cannot anchor to itself]", 2) end
    if rel and layout.dependsOn(sim, rel, self) then
      error("Action[SetPoint] failed because[Cannot anchor to a region dependent on it]", 2)
    end
    for i, p in ipairs(s.points) do
      if p[1] == point then table.remove(s.points, i); break end
    end
    table.insert(s.points, { point, rel, relPoint and relPoint:upper() or point, x or 0, y or 0 })
  end
  function R:GetPoint(i)
    local p = S(self).points[i or 1]
    if p then return p[1], p[2], p[3], p[4], p[5] end
  end
  function R:GetPointByName(name)
    for _, p in ipairs(S(self).points) do
      if p[1] == name then return p[1], p[2], p[3], p[4], p[5] end
    end
  end
  function R:GetNumPoints() return #S(self).points end
  function R:ClearAllPoints() S(self).points = {} end
  function R:SetAllPoints(rel)
    local s = S(self)
    if type(rel) == "string" then rel = rawget(env, rel) end
    rel = rel or s.parent
    if rel == self then error("Action[SetAllPoints] failed because[Cannot anchor to itself]", 2) end
    s.points = { { "TOPLEFT", rel, "TOPLEFT", 0, 0 }, { "BOTTOMRIGHT", rel, "BOTTOMRIGHT", 0, 0 } }
  end
  function R:ClearPoint(point)
    local s = S(self)
    for i, p in ipairs(s.points) do if p[1] == point then table.remove(s.points, i); return end end
  end
  function R:AdjustPointsOffset(dx, dy)
    for _, p in ipairs(S(self).points) do p[4] = p[4] + dx; p[5] = p[5] + dy end
  end
  function R:SetWidth(w) S(self).width = w end
  function R:SetHeight(h) S(self).height = h end
  function R:SetSize(w, h) local s = S(self); s.width = w; s.height = h or w end
  local function resolved(self)
    local l, b, w, h = layout.rect(sim, self)
    if not l then return nil end
    local es = layout.effectiveScale(state, self)
    return l / es, b / es, w / es, h / es
  end
  function R:GetWidth()
    local s = S(self)
    if (s.width or 0) ~= 0 then return s.width end
    local _, _, w = resolved(self)
    return w or 0
  end
  function R:GetHeight()
    local s = S(self)
    if (s.height or 0) ~= 0 then return s.height end
    local _, _, _, h = resolved(self)
    return h or 0
  end
  function R:GetSize() return self:GetWidth(), self:GetHeight() end
  function R:SetScale(sc)
    if type(sc) ~= "number" or sc <= 0 then error("Frame:SetScale(): Scale must be > 0", 2) end
    S(self).scale = sc
  end
  function R:GetScale() return S(self).scale or 1 end
  function R:GetEffectiveScale() return layout.effectiveScale(state, self) end
  function R:GetLeft() local l = resolved(self); return l end
  function R:GetBottom() local _, b = resolved(self); return b end
  function R:GetRight() local l, _, w = resolved(self); return l and l + w end
  function R:GetTop() local _, b, _, h = resolved(self); return b and b + h end
  function R:GetCenter()
    local l, b, w, h = resolved(self)
    if l then return l + w / 2, b + h / 2 end
  end
  function R:GetRect() return resolved(self) end
  function R:GetScaledRect() return layout.rect(sim, self) end
  function R:IsRectValid() return layout.rect(sim, self) ~= nil end
  function R:Intersects(other)
    local l1, b1, w1, h1 = layout.rect(sim, self)
    local l2, b2, w2, h2 = layout.rect(sim, other)
    if not l1 or not l2 then return false end
    return l1 < l2 + w2 and l2 < l1 + w1 and b1 < b2 + h2 and b2 < b1 + h1
  end
  function R:IsMouseOver(top, bottom, left, right)
    local l, b, w, h = layout.rect(sim, self)
    if not l or not sim._isVisible(self) then return false end
    local x, y = sim.cursorX or -1, sim.cursorY or -1
    return x >= l + (left or 0) and x <= l + w + (right or 0) and y >= b + (bottom or 0) and y <= b + h + (top or 0)
  end
  function R:IsMouseMotionFocus() return sim.mouseFocus == self end
  function R:SetAlpha(a) S(self).alpha = a end
  function R:GetAlpha() return S(self).alpha end
  function R:GetEffectiveAlpha()
    local a, o = 1, self
    while o and state[o] do a = a * state[o].alpha; o = state[o].parent end
    return a
  end
  function R:SetVertexColor(r, g, b, a) S(self).color = { r, g, b, a or 1 } end
  function R:GetVertexColor() return unpack(S(self).color or { 1, 1, 1, 1 }) end

  -- scripts
  function R:SetScript(name, fn)
    local s = S(self)
    s.scripts[name] = fn
    s.hooks[name] = nil
    if name == "OnUpdate" then sim:_trackOnUpdate(self, fn ~= nil) end
  end
  function R:GetScript(name) return S(self).scripts[name] end
  function R:HookScript(name, fn)
    local s = S(self)
    if not s.scripts[name] then
      s.scripts[name] = fn
      if name == "OnUpdate" then sim:_trackOnUpdate(self, true) end
      return
    end
    s.hooks[name] = s.hooks[name] or {}
    table.insert(s.hooks[name], fn)
  end
  function R:HasScript() return true end

  ---------------------------------------------------------------- Frame
  local F = define("Frame", "Region")
  local function known(event, method)
    if sim.knownEvents and not sim.knownEvents[event] then
      error(string.format('Frame:%s(): Attempt to register unknown event "%s"', method, tostring(event)), 3)
    end
  end
  function F:RegisterEvent(event)
    if type(event) ~= "string" then error("Usage: frame:RegisterEvent(\"event\")", 2) end
    known(event, "RegisterEvent")
    sim:_registerEvent(self, event)
    return true
  end
  function F:RegisterUnitEvent(event, ...)
    known(event, "RegisterUnitEvent")
    sim:_registerEvent(self, event, select("#", ...) > 0 and { ... } or nil)
    return true
  end
  function F:UnregisterEvent(event) sim:_unregisterEvent(self, event) end
  function F:UnregisterAllEvents()
    for event in pairs(S(self).events) do sim:_unregisterEvent(self, event) end
    S(self).allEvents = false
  end
  function F:RegisterAllEvents() S(self).allEvents = true; sim.allEventFrames[self] = true end
  function F:IsEventRegistered(event) return S(self).events[event] ~= nil or S(self).allEvents end
  local function region(t, self, name, layer, inherits, sublevel)
    local r = M.create(sim, t, name, self, inherits)
    local rs = state[r]
    rs.layer = type(layer) == "string" and layer:upper() or "ARTWORK"
    rs.sublevel = sublevel or 0
    return r
  end
  function F:CreateFontString(name, layer, inherits, sublevel)
    local fs = region("FontString", self, name, layer, nil, sublevel)
    if inherits then fs:SetFontObject(inherits) end
    return fs
  end
  function F:CreateTexture(name, layer, inherits, sublevel) return region("Texture", self, name, layer, inherits, sublevel) end
  function F:CreateMaskTexture(name, layer, inherits, sublevel) return region("MaskTexture", self, name, layer, inherits, sublevel) end
  function F:CreateLine(name, layer, inherits, sublevel) return region("Line", self, name, layer, inherits, sublevel) end
  function F:GetChildren()
    local out = {}
    for _, c in ipairs(S(self).children) do if state[c].isFrame then out[#out + 1] = c end end
    return unpack(out)
  end
  function F:GetNumChildren() return select("#", self:GetChildren()) end
  function F:GetRegions()
    local out = {}
    for _, c in ipairs(S(self).children) do if not state[c].isFrame then out[#out + 1] = c end end
    return unpack(out)
  end
  function F:SetFrameStrata(v)
    v = type(v) == "string" and v:upper() or v
    if not layout.STRATA[v] then error("Frame:SetFrameStrata(): Unknown strata " .. tostring(v), 2) end
    S(self).strata = v
    for _, c in ipairs(S(self).children) do
      if state[c].isFrame and not state[c].fixedStrata then c:SetFrameStrata(v) end
    end
  end
  function F:SetFixedFrameStrata(v) S(self).fixedStrata = v end
  function F:SetFixedFrameLevel(v) S(self).fixedLevel = v end
  function F:Raise()
    local top = 0
    for _, s2 in pairs(state) do if s2.isFrame and s2.strata == S(self).strata and (s2.level or 0) > top then top = s2.level end end
    S(self).level = top + 1
  end
  function F:Lower() S(self).level = 0 end
  function F:GetFrameStrata() return S(self).strata end
  function F:SetFrameLevel(v) S(self).level = v end
  function F:GetFrameLevel() return S(self).level end
  function F:SetID(id) S(self).id = id end
  function F:GetID() return S(self).id end
  function F:EnableMouse(v) S(self).mouse = v and true or false end
  function F:IsMouseEnabled() return S(self).mouse end
  function F:EnableMouseWheel(v) S(self).mouseWheel = v and true or false end
  function F:SetMovable(v) S(self).movable = v and true or false end
  function F:IsMovable() return S(self).movable end
  function F:RegisterForDrag(...) S(self).dragButtons = { ... } end
  function F:StartMoving()
    local s = S(self)
    if not s.movable then error("Frame " .. (s.name or "") .. " is not movable", 2) end
    s.moving = true
  end
  function F:StartSizing(point)
    local s = S(self)
    if not s.resizable then error("Frame " .. (s.name or "") .. " is not resizable", 2) end
    s.sizing = point or "BOTTOMRIGHT"
  end
  function F:StopMovingOrSizing()
    local s = S(self)
    if s.moving or s.sizing then
      -- the client re-anchors a moved frame to a single TOPLEFT point
      local l, b, w, h = layout.rect(sim, self)
      if l then
        local es = layout.effectiveScale(state, self)
        s.points = { { "TOPLEFT", env.UIParent, "BOTTOMLEFT", l / es, (b + h) / es } }
        if s.sizing then s.width, s.height = w / es, h / es end
      end
      s.userPlaced = true
    end
    s.moving, s.sizing = false, nil
  end
  function F:IsDragging() return S(self).moving or false end
  function F:SetResizable(v) S(self).resizable = v and true or false end
  function F:IsResizable() return S(self).resizable or false end
  function F:SetUserPlaced(v) S(self).userPlaced = v and true or false end
  function F:IsUserPlaced() return S(self).userPlaced or false end
  function F:SetResizeBounds(minW, minH, maxW, maxH) S(self).resizeBounds = { minW, minH, maxW, maxH } end
  function F:GetResizeBounds() return unpack(S(self).resizeBounds or { 0, 0, 0, 0 }) end
  function F:GetAttribute(k) return S(self).attributes[k] end
  function F:SetAttribute(k, v)
    S(self).attributes[k] = v
    fire(self, "OnAttributeChanged", k, v)
    require("wowapi.secure").onAttributeChanged(sim, self, k, v)
  end
  function F:SetAttributeNoHandler(k, v) S(self).attributes[k] = v end
  function F:ClearAttribute(k) S(self).attributes[k] = nil end
  function F:ClearAttributes() S(self).attributes = {} end
  function F:GetAttributes() return S(self).attributes end
  function F:IsProtected() local p = S(self).protected or false; return p, p end

  ---------------------------------------------------------------- Button
  local B = define("Button", "Frame")
  function B:SetText(t)
    local s = S(self)
    if not s.fontString then s.fontString = M.create(sim, "FontString", nil, self) end
    s.fontString:SetText(t)
  end
  function B:SetFormattedText(fmt, ...) self:SetText(string.format(fmt, ...)) end
  function B:GetText() local fs = S(self).fontString; return fs and fs:GetText() end
  function B:GetFontString() return S(self).fontString end
  function B:SetFontString(fs) S(self).fontString = fs end
  function B:GetTextWidth() local fs = S(self).fontString; return fs and fs:GetStringWidth() or 0 end
  function B:Enable() S(self).enabled = true; fire(self, "OnEnable") end
  function B:Disable() S(self).enabled = false; fire(self, "OnDisable") end
  function B:SetEnabled(v) if v then self:Enable() else self:Disable() end end
  function B:IsEnabled() return S(self).enabled end
  function B:RegisterForClicks(...) S(self).clicks = { ... } end
  for _, n in ipairs({ "Normal", "Pushed", "Highlight", "Disabled", "Checked", "DisabledChecked" }) do
    B["Set" .. n .. "Texture"] = function(self, tex)
      local s = S(self)
      if type(tex) ~= "table" then
        if tex == nil then s.textures[n] = nil; return end
        local t = M.create(sim, "Texture", nil, self)
        t:SetTexture(tex)
        tex = t
      end
      local ts = state[tex]
      ts.buttonSlot = n
      ts.layer = n == "Highlight" and "HIGHLIGHT" or (n == "Normal" and "BACKGROUND" or "ARTWORK")
      if #ts.points == 0 then tex:SetAllPoints(self) end
      s.textures[n] = tex
    end
    B["Get" .. n .. "Texture"] = function(self) return S(self).textures[n] end
    B["Set" .. n .. "Atlas"] = function(self, atlas)
      local t = M.create(sim, "Texture", nil, self)
      t:SetAtlas(atlas)
      self["Set" .. n .. "Texture"](self, t)
    end
  end
  function B:Click(button, down)
    local s = S(self)
    if not s.enabled then return end
    button = button or "LeftButton"
    fire(self, "PreClick", button, down or false)
    if s.type == "CheckButton" then s.checked = not s.checked end
    fire(self, "OnClick", button, down or false)
    fire(self, "PostClick", button, down or false)
  end
  function B:GetButtonState() return "NORMAL" end

  local CB = define("CheckButton", "Button")
  function CB:SetChecked(v) S(self).checked = v and true or false end
  function CB:GetChecked() return S(self).checked end

  ------------------------------------------------------ StatusBar / Slider
  local function valueWidget(name)
    local W = define(name, "Frame")
    function W:SetMinMaxValues(lo, hi)
      local s = S(self)
      s.min, s.max = lo, hi
      if s.value < lo then self:SetValue(lo) elseif s.value > hi then self:SetValue(hi) end
      fire(self, "OnMinMaxChanged", lo, hi)
    end
    function W:GetMinMaxValues() local s = S(self); return s.min, s.max end
    function W:SetValue(v, userInput)
      local s = S(self)
      if type(v) ~= "number" then error("Usage: " .. name .. ":SetValue(number)", 2) end
      v = math.max(s.min, math.min(s.max, v))
      if s.step and s.step > 0 and name == "Slider" and s.obeyStep then
        v = s.min + math.floor((v - s.min) / s.step + 0.5) * s.step
      end
      if v ~= s.value then
        s.value = v
        fire(self, "OnValueChanged", v, userInput or false)
      end
    end
    function W:GetValue() return S(self).value end
    return W
  end
  local SB = valueWidget("StatusBar")
  function SB:SetStatusBarTexture(t)
    local s = S(self)
    s.barTexture = type(t) == "table" and t or M.create(sim, "Texture", nil, self)
    if type(t) ~= "table" then s.barTexture:SetTexture(t) end
  end
  function SB:GetStatusBarTexture()
    local s = S(self)
    if not s.barTexture then s.barTexture = M.create(sim, "Texture", nil, self) end
    return s.barTexture
  end
  function SB:SetStatusBarColor(r, g, b, a) S(self).color = { r, g, b, a or 1 } end
  function SB:GetStatusBarColor() return unpack(S(self).color or { 1, 1, 1, 1 }) end
  local SL = valueWidget("Slider")
  function SL:SetValueStep(v) S(self).step = v end
  function SL:GetValueStep() return S(self).step end
  function SL:SetObeyStepOnDrag(v) S(self).obeyStep = v end
  function SL:Enable() S(self).enabled = true end
  function SL:Disable() S(self).enabled = false end
  function SL:IsEnabled() return S(self).enabled end

  ---------------------------------------------------------------- EditBox
  local EB = define("EditBox", "Frame")
  function EB:SetText(t)
    S(self).text = tostring(t or "")
    fire(self, "OnTextChanged", false)
  end
  function EB:GetText() return S(self).text end
  function EB:Insert(t) S(self).text = S(self).text .. tostring(t); fire(self, "OnTextChanged", false) end
  function EB:GetNumber() return tonumber(S(self).text) or 0 end
  function EB:SetNumber(n) self:SetText(tostring(n)) end
  function EB:SetNumeric(v) S(self).numeric = v end
  function EB:IsNumeric() return S(self).numeric or false end
  function EB:SetFocus() sim.keyboardFocus = self; fire(self, "OnEditFocusGained") end
  function EB:ClearFocus()
    if sim.keyboardFocus == self then sim.keyboardFocus = nil end
    fire(self, "OnEditFocusLost")
  end
  function EB:HasFocus() return sim.keyboardFocus == self end
  function EB:GetNumLetters() return #S(self).text end
  function EB:GetMaxLetters() return 0 end
  function EB:Enable() S(self).enabled = true end
  function EB:Disable() S(self).enabled = false end
  function EB:IsEnabled() return S(self).enabled end
  function EB:SetTextColor(r, g, b, a) S(self).textColor = { r, g, b, a or 1 } end
  function EB:SetFont(f, sz, fl) S(self).font = { f, sz, fl } return true end
  function EB:GetFont() return unpack(S(self).font or { "Fonts\\FRIZQT__.TTF", 12, "" }) end
  function EB:SetJustifyH(v) S(self).justifyH = v end

  ------------------------------------------------------------ ScrollFrame
  local SF = define("ScrollFrame", "Frame")
  function SF:SetScrollChild(c) S(self).scrollChild = c; if c then c:SetParent(self) end end
  function SF:GetScrollChild() return S(self).scrollChild end
  function SF:SetVerticalScroll(v) S(self).vscroll = v; fire(self, "OnVerticalScroll", v) end
  function SF:GetVerticalScroll() return S(self).vscroll or 0 end
  function SF:GetVerticalScrollRange() return 0 end
  function SF:GetHorizontalScroll() return 0 end

  define("Cooldown", "Frame").SetCooldown = function(self, start, duration)
    S(self).cooldown = { start, duration }
  end
  classes.Cooldown.methods.GetCooldownTimes = function(self)
    local c = S(self).cooldown or { 0, 0 }
    return c[1] * 1000, c[2] * 1000
  end
  define("Model", "Frame")
  define("PlayerModel", "Model")
  define("MessageFrame", "Frame").AddMessage = function(self, msg, r, g, b)
    table.insert(S(self).messages, { text = tostring(msg), r = r, g = g, b = b })
  end
  local SMF = define("ScrollingMessageFrame", "MessageFrame")
  function SMF:AddMessage(msg, r, g, b)
    msg = tostring(msg)
    table.insert(S(self).messages, { text = msg, r = r, g = g, b = b })
    if self == env.DEFAULT_CHAT_FRAME or S(self).isChat then sim:_chat(msg, "SYSTEM") end
  end
  function SMF:GetNumMessages() return #S(self).messages end
  function SMF:GetMessageInfo(i) local m = S(self).messages[i]; if m then return m.text, m.r, m.g, m.b end end
  SMF.Clear = function(self) S(self).messages = {} end
  SMF.SetFading = stub("SetFading")
  SMF.SetInsertMode = stub("SetInsertMode")
  SMF.SetMaxLines = stub("SetMaxLines")
  SMF.SetTimeVisible = stub("SetTimeVisible")

  ------------------------------------------------------------ GameTooltip
  local GT = define("GameTooltip", "Frame")
  local TIP_ANCHORS = {
    ANCHOR_RIGHT = { "BOTTOMLEFT", "TOPRIGHT" }, ANCHOR_LEFT = { "BOTTOMRIGHT", "TOPLEFT" },
    ANCHOR_TOP = { "BOTTOM", "TOP" }, ANCHOR_BOTTOM = { "TOP", "BOTTOM" },
    ANCHOR_TOPLEFT = { "BOTTOMLEFT", "TOPLEFT" }, ANCHOR_TOPRIGHT = { "BOTTOMRIGHT", "TOPRIGHT" },
    ANCHOR_BOTTOMLEFT = { "TOPRIGHT", "BOTTOMLEFT" }, ANCHOR_BOTTOMRIGHT = { "TOPLEFT", "BOTTOMRIGHT" },
  }
  function GT:SetOwner(owner, anchor, x, y)
    local s = S(self)
    s.owner, s.anchor, s.lines = owner, anchor or "ANCHOR_LEFT", {}
    s.width, s.height = 0, 0
    s.points = {}
    local a = TIP_ANCHORS[s.anchor]
    if a and owner then
      table.insert(s.points, { a[1], owner, a[2], x or 0, y or 0 })
    elseif s.anchor == "ANCHOR_CURSOR" then
      table.insert(s.points, { "BOTTOMLEFT", env.UIParent, "BOTTOMLEFT", (sim.cursorX or 0) + 10, (sim.cursorY or 0) + 10 })
    end
  end
  function GT:GetOwner() return S(self).owner, S(self).anchor end
  function GT:GetAnchorType() return S(self).anchor end
  function GT:SetAnchorType(a, x, y) local s = S(self); self:SetOwner(s.owner, a, x, y) end
  function GT:IsOwned(o) return S(self).owner == o end
  function GT:ClearLines() S(self).lines = {} end
  function GT:SetText(t, r, g, b)
    S(self).lines = { { left = tostring(t), r = r, g = g, b = b } }
    self:Show()
  end
  function GT:AddLine(t, r, g, b, wrap) table.insert(S(self).lines, { left = tostring(t), r = r, g = g, b = b }) end
  function GT:AddDoubleLine(l, rt) table.insert(S(self).lines, { left = tostring(l), right = tostring(rt) }) end
  function GT:NumLines() return #S(self).lines end
  function GT:GetLines() return S(self).lines end
  function GT:SetMinimumWidth() end
  function GT:SetPadding() end

  ---------------------------------------------------------------- FontString
  local FS = define("FontString", "Region")
  function FS:SetText(t)
    if t == nil then S(self).text = nil; return end
    S(self).text = tostring(t)
  end
  function FS:GetText() return S(self).text end
  function FS:SetFormattedText(fmt, ...) S(self).text = string.format(fmt, ...) end
  function FS:SetFont(f, sz, flags) S(self).font = { f, sz, flags or "" }; return true end
  function FS:GetFont() return layout.fontOf(sim, self) end
  function FS:SetFontObject(o)
    if type(o) == "string" then
      local name = o
      o = sim:Get(name)
      if not o then error("FontString:SetFontObject(): Couldn't find font object '" .. name .. "'", 2) end
    end
    local s = S(self)
    s.fontObject = o
    s.font = nil
    local fs = o and state[o]
    if fs and fs.justifyH and not s.justifyH then s.justifyH = fs.justifyH end
  end
  function FS:GetFontObject() return S(self).fontObject end
  function FS:SetTextColor(r, g, b, a) S(self).textColor = { r, g, b, a or 1 } end
  function FS:GetTextColor()
    local s = S(self)
    local c = s.textColor or (s.fontObject and state[s.fontObject] and state[s.fontObject].textColor) or { 1, 1, 1, 1 }
    return c[1], c[2], c[3], c[4] or 1
  end
  function FS:SetJustifyH(v) S(self).justifyH = v end
  function FS:GetJustifyH() return S(self).justifyH or "CENTER" end
  function FS:SetJustifyV(v) S(self).justifyV = v end
  function FS:GetJustifyV() return S(self).justifyV or "MIDDLE" end
  -- text metrics from the layout engine (wrapped to the region's width)
  local function wrapped(self)
    local st = S(self)
    local _, size = layout.fontOf(sim, self)
    local width
    if (st.width or 0) > 0 or #st.points >= 2 then
      local _, _, w = layout.rect(sim, self)
      width = w and layout.wrapWidth(sim, self, w)
    end
    return layout.wrap(st.text, size, width), size
  end
  function FS:GetUnboundedStringWidth()
    local _, size = layout.fontOf(sim, self)
    return layout.textWidth(S(self).text, size)
  end
  function FS:GetStringWidth()
    local lines, size = wrapped(self)
    local w = 0
    for _, l in ipairs(lines) do w = math.max(w, layout.textWidth(l, size)) end
    return w
  end
  function FS:GetStringHeight()
    local lines, size = wrapped(self)
    return layout.linesHeight(#lines, size)
  end
  function FS:GetNumLines() return #(wrapped(self)) end
  function FS:IsTruncated() return false end

  ---------------------------------------------------------------- Texture
  define("TextureBase", "Region")
  local T = define("Texture", "TextureBase")
  function T:SetTexture(t) S(self).texture = t; return true end
  function T:GetTexture() return S(self).texture end
  function T:GetTextureFileID() return type(S(self).texture) == "number" and S(self).texture or nil end
  function T:SetColorTexture(r, g, b, a) S(self).texture = "color"; S(self).color = { r, g, b, a or 1 } end
  function T:SetAtlas(a, useAtlasSize)
    local s = S(self)
    local info = require("wowapi.assets").atlasInfo(a)
    s.atlas = a
    s.texture = nil
    if info then
      s.texCoord = nil
      if useAtlasSize then s.width, s.height = info.width, info.height end
      return true
    end
    return false
  end
  function T:GetAtlas() return S(self).atlas end
  function T:SetTexCoord(...) S(self).texCoord = { ... } end
  function T:GetTexCoord() return unpack(S(self).texCoord or { 0, 0, 0, 1, 1, 0, 1, 1 }) end
  function T:SetDesaturated(v) S(self).desaturated = v end
  function T:IsDesaturated() return S(self).desaturated or false end
  function T:SetBlendMode(v) S(self).blend = v end
  function T:GetBlendMode() return S(self).blend or "BLEND" end
  function T:SetDrawLayer(l, sub) S(self).layer = type(l) == "string" and l:upper() or l; if sub then S(self).sublevel = sub end end
  -- gradients: SetGradient("HORIZONTAL"|"VERTICAL", minColor, maxColor)
  local function colorOf(c)
    if type(c) == "table" then return { c.r or c[1] or 1, c.g or c[2] or 1, c.b or c[3] or 1, c.a or c[4] or 1 } end
  end
  function T:SetGradient(orientation, minColor, maxColor)
    S(self).gradient = { orientation = tostring(orientation or "HORIZONTAL"):upper(), colorOf(minColor), colorOf(maxColor) }
  end
  function T:SetGradientAlpha(orientation, r1, g1, b1, a1, r2, g2, b2, a2)
    S(self).gradient = { orientation = tostring(orientation):upper(), { r1, g1, b1, a1 }, { r2, g2, b2, a2 } }
  end
  function T:GetDrawLayer() return S(self).layer or "ARTWORK" end

  ---------------------------------------------------------------- Font
  local Font = define("Font", "Object")
  function Font:GetName() return S(self).name end
  function Font:GetObjectType() return "Font" end
  function Font:IsObjectType(t) return t == "Font" end
  function Font:SetFont(f, sz, fl) S(self).font = { f, sz, fl or "" } end
  function Font:GetFont() return unpack(S(self).font or { "Fonts\\FRIZQT__.TTF", 12, "" }) end
  function Font:SetFontObject(o) if o and state[o] then S(self).font = state[o].font end end
  function Font:CopyFontObject(o) self:SetFontObject(o) end
  function Font:SetTextColor(r, g, b, a) S(self).textColor = { r, g, b, a or 1 } end
  function Font:GetTextColor() return unpack(S(self).textColor or { 1, 0.82, 0, 1 }) end
  function Font:SetJustifyH() end
  function Font:SetJustifyV() end
  function Font:SetShadowColor() end
  function Font:SetShadowOffset() end
  function Font:SetSpacing() end

  sim.widgetClasses = classes
  docs.defineClasses(classes, define)
  require("wowapi.animation").install(sim, classes, define, state, S, M.create)
  -- Every other documented method, generated from Blizzard's API docs.
  docs.installWidgets(sim, classes, define, state)
  -- Like the client, each widget type's metatable __index is one flat
  -- table holding every method (addons iterate it with pairs).
  for _, cls in pairs(classes) do
    local c = cls.parent
    while c do
      for k, v in pairs(c.methods) do
        if rawget(cls.methods, k) == nil then rawset(cls.methods, k, v) end
      end
      c = c.parent
    end
  end
  sim.frameTypes = {}
  for name, cls in pairs(classes) do
    if isa(cls, "Frame") then sim.frameTypes[name:lower()] = name end
  end
  sim.frameTypes.checkbox = "CheckButton"

  function visible(obj)
    local o = obj
    while o do
      local s = state[o]
      if not s then return true end
      if not s.shown then return false end
      o = s.parent
    end
    return true
  end
  sim._isVisible = visible
end

function M.create(sim, otype, name, parent, templates)
  local tname = sim.widgetClasses[otype] and otype or sim.frameTypes[tostring(otype):lower()]
  if not tname then error("CreateFrame: Unknown frame type '" .. tostring(otype) .. "'", 3) end
  local cls = sim.widgetClasses[tname]
  local env = sim.env
  local state = sim.widgetState
  if type(parent) == "string" then parent = env[parent] end
  if name then
    local pname = parent and state[parent] and state[parent].name or ""
    name = name:gsub("%$[Pp]arent", pname)
  end
  local obj = {}
  local s = {
    type = tname, name = name, parent = nil, shown = true, alpha = 1, width = 0, height = 0,
    points = {}, scripts = {}, hooks = {}, events = {}, children = {}, attributes = {},
    enabled = true, checked = false, min = 0, max = 0, value = 0, text = (tname == "EditBox") and "" or nil,
    level = 1, strata = "MEDIUM", textures = {}, lines = {}, messages = {}, mouse = false,
    isFrame = isa(cls, "Frame"),
    templates = templates,
  }
  sim.seq = (sim.seq or 0) + 1
  s.seq = sim.seq
  if parent and state[parent] then
    s.level = (state[parent].level or 0) + 1
    s.strata = state[parent].strata or "MEDIUM"
  end
  if isa(cls, "Button") or isa(cls, "EditBox") or isa(cls, "Slider") then s.mouse = true end
  state[obj] = s
  setmetatable(obj, { __index = cls.methods, __tostring = function() return tname .. ": " .. (name or "anonymous") end })
  -- like the client, every UI object holds its C-side userdata at [0]
  rawset(obj, 0, newproxy and newproxy(false) or {})
  if parent then obj:SetParent(parent) end
  sim.frameCount = sim.frameCount + 1

  if name then env[name] = obj end
  if templates and s.isFrame then
    require("wowapi.templates").apply(sim, obj, templates, M.create)
  end
  return obj
end

return M
