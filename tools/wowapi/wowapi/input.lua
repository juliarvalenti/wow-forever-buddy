-- Keyboard input and key bindings.
--
--   sim:PressKey("CTRL-SHIFT-F")   -- full client dispatch: focused EditBox,
--                                  -- keyboard-enabled frames, UISpecialFrames
--                                  -- on ESCAPE, then override/normal bindings
--   sim:SetBinding("F", "MYADDON_TOGGLE")
local M = {}

local MODS = { ALT = "alt", CTRL = "ctrl", SHIFT = "shift", META = "meta" }

-- "shift-ctrl-f" -> "CTRL-SHIFT-F", { ctrl = true, shift = true }, "F"
function M.normalize(key)
  key = key:upper()
  local mods, base = {}, key
  while true do
    local m, rest = base:match("^(%u+)%-(.+)$")
    if m and MODS[m] then mods[MODS[m]] = true; base = rest else break end
  end
  local parts = {}
  if mods.alt then parts[#parts + 1] = "ALT" end
  if mods.ctrl then parts[#parts + 1] = "CTRL" end
  if mods.shift then parts[#parts + 1] = "SHIFT" end
  if mods.meta then parts[#parts + 1] = "META" end
  parts[#parts + 1] = base
  return table.concat(parts, "-"), mods, base
end

local CHAR = { SPACE = " ", ENTER = nil, TAB = nil }

function M.install(sim, env)
  sim.bindings = {}
  sim.overrideBindings = {}
  sim.bindingLog = {}
  sim.keysDown = {}
  local secure = require("wowapi.secure")

  local function setOverride(owner, priority, key, action)
    key = M.normalize(key)
    for i = #sim.overrideBindings, 1, -1 do
      local o = sim.overrideBindings[i]
      if o.owner == owner and o.key == key then table.remove(sim.overrideBindings, i) end
    end
    if action then
      table.insert(sim.overrideBindings, { owner = owner, priority = priority and true or false, key = key, action = action })
    end
  end
  sim._setOverride = setOverride

  rawset(env, "SetBinding", function(key, action)
    if not key then return false end
    key = M.normalize(key)
    if action == nil or action == "" then sim.bindings[key] = nil else sim.bindings[key] = action end
    sim:FireEvent("UPDATE_BINDINGS")
    return true
  end)
  rawset(env, "SetBindingClick", function(key, button, mouse)
    local name = type(button) == "table" and button:GetName() or button
    return env.SetBinding(key, "CLICK " .. name .. ":" .. (mouse or "LeftButton"))
  end)
  rawset(env, "SetBindingSpell", function(key, spell) return env.SetBinding(key, "SPELL " .. spell) end)
  rawset(env, "SetBindingItem", function(key, item) return env.SetBinding(key, "ITEM " .. item) end)
  rawset(env, "SetBindingMacro", function(key, macro) return env.SetBinding(key, "MACRO " .. macro) end)
  rawset(env, "SetOverrideBinding", function(owner, priority, key, action) setOverride(owner, priority, key, action) end)
  rawset(env, "SetOverrideBindingClick", function(owner, priority, key, button, mouse)
    local name = type(button) == "table" and button:GetName() or button
    setOverride(owner, priority, key, "CLICK " .. name .. ":" .. (mouse or "LeftButton"))
  end)
  rawset(env, "SetOverrideBindingSpell", function(owner, priority, key, spell) setOverride(owner, priority, key, "SPELL " .. spell) end)
  rawset(env, "SetOverrideBindingItem", function(owner, priority, key, item) setOverride(owner, priority, key, "ITEM " .. item) end)
  rawset(env, "SetOverrideBindingMacro", function(owner, priority, key, macro) setOverride(owner, priority, key, "MACRO " .. macro) end)
  rawset(env, "ClearOverrideBindings", function(owner)
    for i = #sim.overrideBindings, 1, -1 do
      if sim.overrideBindings[i].owner == owner then table.remove(sim.overrideBindings, i) end
    end
  end)
  rawset(env, "GetBindingKey", function(action)
    local keys = {}
    for k, a in pairs(sim.bindings) do if a == action then keys[#keys + 1] = k end end
    table.sort(keys)
    return (table.unpack or unpack)(keys)
  end)
  rawset(env, "GetBindingAction", function(key, checkOverride)
    key = M.normalize(key)
    if checkOverride then
      for _, o in ipairs(sim.overrideBindings) do if o.key == key and o.priority then return o.action end end
      for _, o in ipairs(sim.overrideBindings) do if o.key == key then return o.action end end
    end
    return sim.bindings[key] or ""
  end)
  rawset(env, "GetBindingByKey", function(key) return env.GetBindingAction(key, true) end)
  rawset(env, "GetBindingText", function(key, prefix, abbrev) return key and tostring(key) or "" end)
  rawset(env, "GetCurrentBindingSet", function() return 2 end)
  rawset(env, "SaveBindings", function() sim:FireEvent("UPDATE_BINDINGS") end)
  rawset(env, "LoadBindings", function() sim:FireEvent("UPDATE_BINDINGS") end)
  rawset(env, "GetNumBindings", function()
    local n = 0
    for _ in pairs(sim.bindingActions) do n = n + 1 end
    return n
  end)
  rawset(env, "GetBinding", function(i)
    local names = {}
    for n in pairs(sim.bindingActions) do names[#names + 1] = n end
    table.sort(names)
    local n = names[i]
    if not n then return nil end
    return n, sim.bindingActions[n].category, env.GetBindingKey(n)
  end)
  rawset(env, "RunBinding", function(action, keystate) return sim:_runBinding(action, keystate or "down") end)
  rawset(env, "IsKeyDown", function(key) return sim.keysDown[M.normalize(key)] or false end)
  rawset(env, "UISpecialFrames", {})
  rawset(env, "CloseSpecialWindows", function()
    local closed = false
    for _, name in ipairs(env.UISpecialFrames) do
      local f = sim:Get(name)
      if f and f:IsShown() then f:Hide(); closed = true end
    end
    return closed
  end)
end

-- Methods mixed into Sim ------------------------------------------------

local Sim = {}

function Sim:SetBinding(key, action)
  key = M.normalize(key)
  self.bindings[key] = action
end

function Sim:SetOverrideBindingClick(owner, key, buttonName, mouse)
  self._setOverride(owner, false, key, "CLICK " .. buttonName .. ":" .. (mouse or "LeftButton"))
end

function Sim:ClearOverrideBindings(owner)
  self.env.ClearOverrideBindings(owner)
end

-- Hold or release a modifier: sim:SetModifier("shift", true)
function Sim:SetModifier(which, down)
  which = which:lower()
  if self.modifiers[which] == (down and true or nil) then return end
  self.modifiers[which] = down and true or nil
  self:FireEvent("MODIFIER_STATE_CHANGED", "L" .. which:upper(), down and 1 or 0)
end

function Sim:_runBinding(action, keystate)
  local secure = require("wowapi.secure")
  table.insert(self.bindingLog, { action = action, keystate = keystate })
  local click, mb = action:match("^CLICK%s+([^:]+):?(.*)$")
  if click then
    if keystate ~= "down" then return true end
    local b = self:Get(click)
    if b then
      self.hardwareEvent = true
      b:Click(mb ~= "" and mb or "LeftButton", false)
      self.hardwareEvent = false
    end
    return true
  end
  local kind, what = action:match("^(%u+)%s+(.+)$")
  if kind == "SPELL" or kind == "ITEM" or kind == "MACRO" then
    if keystate == "down" then table.insert(self.secureActions, { type = kind:lower(), [kind:lower()] = what }) end
    return true
  end
  local b = self.bindingActions[action]
  if b then
    if keystate == "up" and not b.runOnUp then return true end
    self.hardwareEvent = true
    self:_pcall(b.fn, keystate)
    self.hardwareEvent = false
    return true
  end
  return false
end

-- Press (and release) a key through the client's input chain.
function Sim:PressKey(key, opts)
  opts = opts or {}
  local full, mods, base = M.normalize(key)
  local held = {}
  for m in pairs(mods) do
    if not self.modifiers[m] then self:SetModifier(m, true); held[#held + 1] = m end
  end
  self.keysDown[full] = true
  local handled = false

  -- 1. focused edit box
  local eb = self.keyboardFocus
  if eb and self.widgetState[eb] and eb:IsVisible() then
    handled = true
    if base == "ENTER" then self:_runScript(eb, "OnEnterPressed")
    elseif base == "ESCAPE" then self:_runScript(eb, "OnEscapePressed")
    elseif base == "TAB" then self:_runScript(eb, "OnTabPressed")
    elseif base == "BACKSPACE" then
      local s = self.widgetState[eb]
      s.text = (s.text or ""):sub(1, -2)
      self:_runScript(eb, "OnTextChanged", true)
    elseif base == "UP" or base == "DOWN" or base == "LEFT" or base == "RIGHT" then
      self:_runScript(eb, "OnArrowPressed", base)
    else
      local ch = CHAR[base] or (#base == 1 and (mods.shift and base or base:lower())) or nil
      if ch then
        if base == "SPACE" then self:_runScript(eb, "OnSpacePressed") end
        local s = self.widgetState[eb]
        s.text = (s.text or "") .. ch
        self:_runScript(eb, "OnChar", ch)
        self:_runScript(eb, "OnTextChanged", true)
      end
    end
  end

  -- 2. keyboard-enabled frames, topmost first
  if not handled then
    local layout = require("wowapi.layout")
    local frames = {}
    for obj, s in pairs(self.widgetState) do
      local kb = s.props and s.props.KeyboardEnabled and s.props.KeyboardEnabled[1]
      if s.isFrame and kb and self._isVisible(obj) then frames[#frames + 1] = obj end
    end
    table.sort(frames, function(a, c)
      local s1, l1, q1 = layout.frameOrder(self, a)
      local s2, l2, q2 = layout.frameOrder(self, c)
      if s1 ~= s2 then return s1 > s2 end
      if l1 ~= l2 then return l1 > l2 end
      return q1 > q2
    end)
    for _, f in ipairs(frames) do
      self.hardwareEvent = true
      self:_runScript(f, "OnKeyDown", full)
      self.hardwareEvent = false
      local s = self.widgetState[f]
      local propagate = s.props and s.props.PropagateKeyboardInput and s.props.PropagateKeyboardInput[1]
      if not propagate then handled = true; break end
    end
  end

  -- 3. escape closes special frames
  if not handled and base == "ESCAPE" then
    handled = self.env.CloseSpecialWindows()
    if not handled then table.insert(self.bindingLog, { action = "TOGGLEGAMEMENU", keystate = "down" }); handled = true end
  end

  -- 4. bindings: priority overrides, overrides, then normal
  local action
  if not handled then
    for _, o in ipairs(self.overrideBindings) do if o.key == full and o.priority then action = o.action end end
    if not action then for _, o in ipairs(self.overrideBindings) do if o.key == full then action = o.action end end end
    action = action or self.bindings[full]
    if action then self:_runBinding(action, "down") end
  end

  -- release
  self.keysDown[full] = nil
  if not handled and action then self:_runBinding(action, "up") end
  for _, m in ipairs(held) do self:SetModifier(m, false) end
  self:_afterInput()
  return handled or action ~= nil
end

-- Type text into the focused edit box, one key at a time.
function Sim:TypeText(text)
  for ch in text:gmatch(".") do
    if ch == " " then self:PressKey("SPACE")
    elseif ch == "\n" then self:PressKey("ENTER")
    elseif ch:match("%u") then self:PressKey("SHIFT-" .. ch)
    else
      local eb = self.keyboardFocus
      if eb then
        local s = self.widgetState[eb]
        s.text = (s.text or "") .. ch
        self:_runScript(eb, "OnChar", ch)
        self:_runScript(eb, "OnTextChanged", true)
      end
    end
  end
end

M.SimMethods = Sim
return M
