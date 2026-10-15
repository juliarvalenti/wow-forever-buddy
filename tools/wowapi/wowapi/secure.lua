-- Secure execution and combat lockdown.
--
--  * Protected functions (CastSpellByName, TargetUnit, UseAction, ...) can't
--    be called by addon code: the call does nothing and ADDON_ACTION_FORBIDDEN
--    fires, like in game.
--  * Protected frames (secure templates, protected="true") can't be shown,
--    hidden, moved, resized or have attributes changed by addon code while
--    in combat lockdown: ADDON_ACTION_BLOCKED fires and nothing happens.
--  * Secure code paths (SecureActionButtons clicked by the player, state
--    drivers, secure handler snippets) are allowed and perform actions,
--    which are recorded in sim.secureActions.
--  * Macro conditionals (SecureCmdOptionParse), RegisterStateDriver,
--    RegisterAttributeDriver and SecureHandler snippets (_onstate-*,
--    _onclick, _onshow, _onhide, _onattributechanged) work.
local compat = require("wowapi.compat")
local unpack = compat.unpack

local M = {}

-- Undocumented protected globals addons might try to call directly.
M.PROTECTED = {
  "CastSpellByName", "CastSpellByID", "CastSpell", "CastPetAction", "CastShapeshiftForm", "UseAction",
  "UseItemByName", "UseInventoryItem", "UseToy", "UseToyByName", "TargetUnit", "TargetNearestEnemy",
  "TargetNearestFriend", "TargetLastTarget", "TargetLastEnemy", "AssistUnit", "FocusUnit", "ClearFocus",
  "ClearTarget", "SpellStopCasting", "SpellStopTargeting", "SpellTargetUnit", "RunMacro", "RunMacroText",
  "StopMacro", "PetAttack", "PetFollow", "PetStopAttack", "CancelShapeshiftForm", "CancelUnitBuff",
  "MoveForwardStart", "MoveForwardStop", "MoveBackwardStart", "JumpOrAscendStart", "ToggleRun",
  "StartAttack", "StopAttack", "AttackTarget", "PickupAction", "PlaceAction", "ChangeActionBarPage",
  "Stuck", "Logout", "Quit", "ForceQuit", "CameraOrSelectOrMoveStart", "TurnOrActionStart",
  "InteractUnit", "DestroyTotem", "CancelLogout", "SetBinding", "SetBindingClick", "SetBindingItem",
  "SetBindingMacro", "SetBindingSpell", "SetOverrideBinding", "SetOverrideBindingClick",
  "SetOverrideBindingItem", "SetOverrideBindingMacro", "SetOverrideBindingSpell", "ClearOverrideBindings",
}
-- Of those, allowed from addon code outside combat.
M.ALLOWED_OUT_OF_COMBAT = {
  SetBinding = true, SetBindingClick = true, SetBindingItem = true, SetBindingMacro = true,
  SetBindingSpell = true, SetOverrideBinding = true, SetOverrideBindingClick = true,
  SetOverrideBindingItem = true, SetOverrideBindingMacro = true, SetOverrideBindingSpell = true,
  ClearOverrideBindings = true, ClearFocus = false, Logout = true, Quit = true, CancelLogout = true,
}

-- Widget methods that are blocked on protected frames in combat, in
-- addition to those the documentation flags as IsProtectedFunction.
M.PROTECTED_METHODS = { SetAttribute = true, SetAttributeNoHandler = true, ClearAttribute = true }

------------------------------------------------------------------ helpers

-- Which loaded addon's code is on the call stack?
function M.callerAddon(sim)
  for level = 3, 40 do
    local info = debug.getinfo(level, "S")
    if not info then break end
    local src = info.source or ""
    if src:sub(1, 1) == "@" then
      src = src:sub(2):gsub("\\", "/")
      for _, name in ipairs(sim.addonOrder) do
        local a = sim.addons[name]
        local dir = a.dir:gsub("\\", "/"):gsub("^%./", "")
        local s2 = src:gsub("^%./", "")
        if s2:sub(1, #dir + 1) == dir .. "/" then return name end
      end
    end
  end
  return sim.currentAddon or "*** unknown ***"
end

function M.forbid(sim, what)
  local addon = M.callerAddon(sim)
  table.insert(sim.blockedActions, { kind = "FORBIDDEN", addon = addon, action = what, time = sim.time })
  sim:_warn(string.format("AddOn '%s' tried to call the protected function '%s'.", addon, what))
  sim:FireEvent("ADDON_ACTION_FORBIDDEN", addon, what)
end

function M.block(sim, what)
  local addon = M.callerAddon(sim)
  table.insert(sim.blockedActions, { kind = "BLOCKED", addon = addon, action = what, time = sim.time })
  sim:_warn(string.format("AddOn '%s' tried to call the protected function '%s' during combat lockdown.", addon, what))
  sim:FireEvent("ADDON_ACTION_BLOCKED", addon, what)
end

-- Run fn as secure (Blizzard/restricted) code.
function M.secureCall(sim, fn, ...)
  sim.secureDepth = sim.secureDepth + 1
  local r = { pcall(fn, ...) }
  sim.secureDepth = sim.secureDepth - 1
  if not r[1] then sim:_error(tostring(r[2])); return end
  return unpack(r, 2, table.maxn and table.maxn(r) or #r)
end

function M.isProtected(sim, obj)
  local s = sim.widgetState[obj]
  return s ~= nil and s.protected == true
end

------------------------------------------------------------------ macro conditionals

local function modDown(sim, which)
  local m = sim.modifiers
  if which == nil or which == "" then return m.shift or m.ctrl or m.alt or false end
  for w in which:gmatch("[^/]+") do
    w = w:lower()
    if (w == "shift" and m.shift) or ((w == "ctrl" or w == "control") and m.ctrl) or (w == "alt" and m.alt) then return true end
  end
  return false
end

local function evalCondition(sim, cond, unit)
  local neg = false
  local name, arg = cond:match("^%s*([%w@]+)%s*:?%s*(.-)%s*$")
  if not name then return true end
  name = name:lower()
  if name:sub(1, 2) == "no" and name ~= "none" then neg, name = true, name:sub(3) end
  local u = sim.units[unit or "target"]
  local r
  if name == "combat" then r = sim.inCombat
  elseif name == "mod" or name == "modifier" then r = modDown(sim, arg)
  elseif name == "exists" then r = u ~= nil
  elseif name == "dead" then r = u ~= nil and (u.dead or (u.health or 1) <= 0)
  elseif name == "harm" then r = u ~= nil and (u.hostile or false)
  elseif name == "help" then r = u ~= nil and not u.hostile
  elseif name == "group" then
    if arg == "raid" then r = sim.units.raid1 ~= nil
    else r = sim.units.party1 ~= nil or sim.units.raid1 ~= nil end
  elseif name == "mounted" then r = sim.player.mounted or false
  elseif name == "flying" then r = sim.player.flying or false
  elseif name == "swimming" then r = sim.player.swimming or false
  elseif name == "indoors" then r = sim.player.indoors or false
  elseif name == "outdoors" then r = not sim.player.indoors
  elseif name == "stealth" then r = sim.player.stealthed or false
  elseif name == "resting" then r = sim.player.resting or false
  elseif name == "pet" then r = sim.units.pet ~= nil
  elseif name == "form" or name == "stance" then
    local f = sim.player.form or 0
    if arg == "" then r = f ~= 0 else
      r = false
      for n in arg:gmatch("[^/]+") do if tonumber(n) == f then r = true end end
    end
  elseif name == "spec" then
    r = false
    for n in arg:gmatch("[^/]+") do if tonumber(n) == sim.player.spec then r = true end end
  elseif name == "channeling" then r = sim.player.channeling ~= nil
  elseif name == "known" then r = sim.spells[tonumber(arg) or arg] ~= nil
  elseif name == "petbattle" or name == "vehicleui" or name == "overridebar" or name == "possessbar"
    or name == "bonusbar" or name == "extrabar" or name == "canexitvehicle" or name == "flyable" then
    r = sim.player[name] or false
  elseif name == "btn" or name == "button" then
    r = false
    for n in arg:gmatch("[^/]+") do if n == (sim.currentMouseButton or "1") then r = true end end
  elseif name == "actionbar" or name == "bar" then
    r = false
    for n in arg:gmatch("[^/]+") do if tonumber(n) == (sim.actionBarPage or 1) then r = true end end
  else
    r = sim.player[name] and true or false
  end
  if neg then return not r end
  return r and true or false
end

-- Parse "[cond,cond][cond] value; [cond] value; default".
-- Returns value, target unit (or nil).
function M.parseOptions(sim, text)
  for clause in (text .. ";"):gmatch("(.-);") do
    local rest = clause
    local groups = {}
    while true do
      local g, after = rest:match("^%s*%[(.-)%]()")
      if not g then break end
      groups[#groups + 1] = g
      rest = rest:sub(after)
    end
    local value = rest:match("^%s*(.-)%s*$")
    if #groups == 0 then return value, nil end
    for _, g in ipairs(groups) do
      local unit
      local ok = true
      for cond in (g .. ","):gmatch("(.-),") do
        cond = cond:match("^%s*(.-)%s*$")
        local u = cond:match("^@(.+)$") or cond:match("^target%s*=%s*(.+)$")
        if u then unit = u
        elseif cond ~= "" and not evalCondition(sim, cond, unit) then ok = false end
      end
      if ok then return value, unit end
    end
  end
  return nil
end

------------------------------------------------------------------ secure action buttons

local function attr(frame, name, button)
  local s = frame:GetAttribute(name .. (button and ("-" .. button) or ""))
  if s == nil and button then
    local b = button == "LeftButton" and "1" or button == "RightButton" and "2" or button == "MiddleButton" and "3" or nil
    if b then s = frame:GetAttribute(name .. b) end
  end
  if s == nil then s = frame:GetAttribute(name) end
  return s
end
M.attr = attr

function M.performAction(sim, frame, button)
  local typ = attr(frame, "type", button)
  if not typ then return end
  if not sim.hardwareEvent and sim.secureDepth == 0 then
    -- clicking a secure action button from addon code (not a real click) is forbidden
    M.forbid(sim, "SecureActionButton:Click()")
    return
  end
  local unit = attr(frame, "unit", button)
  local rec = { type = typ, frame = frame, button = button, unit = unit }
  if typ == "spell" then rec.spell = attr(frame, "spell", button)
  elseif typ == "item" then rec.item = attr(frame, "item", button)
  elseif typ == "macro" then
    rec.macrotext = attr(frame, "macrotext", button)
    rec.macro = attr(frame, "macro", button)
    if rec.macrotext then
      -- resolve /cast [cond] spell lines
      for line in (rec.macrotext .. "\n"):gmatch("(.-)\n") do
        local cmd, args = line:match("^%s*(/%S+)%s*(.*)$")
        if cmd == "/cast" or cmd == "/use" then
          local v, u = M.parseOptions(sim, args)
          if v and v ~= "" then rec.casts = rec.casts or {}; table.insert(rec.casts, { spell = v, unit = u }) end
        end
      end
    end
  elseif typ == "action" then rec.action = attr(frame, "action", button)
  elseif typ == "target" then
    if unit and sim.units[unit] then sim.units.target = sim.units[unit]; sim:FireEvent("PLAYER_TARGET_CHANGED") end
  elseif typ == "focus" then
    if unit and sim.units[unit] then sim.units.focus = sim.units[unit]; sim:FireEvent("PLAYER_FOCUS_CHANGED") end
  elseif typ == "cancelaura" then rec.spell = attr(frame, "spell", button)
  elseif typ == "click" then
    local target = attr(frame, "clickbutton", button)
    if type(target) == "string" then target = sim:Get(target) end
    if target then M.secureCall(sim, function() target:Click(button) end) end
  elseif typ == "attribute" then
    local f = attr(frame, "attribute-frame", button) or frame
    M.secureCall(sim, function() f:SetAttribute(attr(frame, "attribute-name", button), attr(frame, "attribute-value", button)) end)
  elseif typ == "togglemenu" or typ == "stop" or typ == "multispell" or typ == "pet" then
    rec.spell = attr(frame, "spell", button)
  else
    -- custom type: calls frame[type](frame, unit, button) like SecureActionButton does
    local fn = frame[typ] or attr(frame, "_" .. typ, button)
    if type(fn) == "function" then M.secureCall(sim, fn, frame, unit, button) end
  end
  table.insert(sim.secureActions, rec)
  sim:FireEvent("UNIT_SPELLCAST_SENT", "player", "", "", rec.spell and sim.spells[rec.spell] and sim.spells[rec.spell].id or 0)
end

------------------------------------------------------------------ restricted environment

-- Frame handles for secure snippets: only a safe subset of methods, all
-- executed securely (allowed in combat).
local HANDLE_METHODS = { "Show", "Hide", "SetShown", "IsShown", "IsVisible", "SetAttribute", "GetAttribute",
  "SetPoint", "ClearAllPoints", "SetAllPoints", "SetWidth", "SetHeight", "SetSize", "GetWidth", "GetHeight",
  "SetAlpha", "GetAlpha", "SetScale", "GetName", "GetID", "SetID", "Enable", "Disable", "IsEnabled",
  "GetParent", "GetChildren", "GetFrameLevel", "SetFrameLevel", "RegisterForClicks", "Raise", "Lower",
  "EnableMouse", "IsMouseOver", "GetObjectType", "IsObjectType", "Click", "SetBindingClick", "ClearBindings" }

function M.handle(sim, frame)
  if not frame then return nil end
  sim.handles = sim.handles or setmetatable({}, { __mode = "k" })
  if sim.handles[frame] then return sim.handles[frame] end
  local h = {}
  for _, m in ipairs(HANDLE_METHODS) do
    h[m] = function(self, ...)
      local args = { ... }
      for i, a in ipairs(args) do if type(a) == "table" and a.__frame then args[i] = a.__frame end end
      local f = frame[m]
      if m == "SetBindingClick" then
        local priority, key, target, mb = ...
        if type(target) == "table" then target = target.__frame or target end
        local name = type(target) == "table" and target:GetName() or target
        sim._setOverride(frame, priority, key, "CLICK " .. tostring(name) .. ":" .. (mb or "LeftButton"))
        return
      end
      if m == "ClearBindings" then sim.env.ClearOverrideBindings(frame); return end
      if not f then return end
      local r = { M.secureCall(sim, f, frame, unpack(args, 1, select("#", ...))) }
      for i, v in ipairs(r) do if type(v) == "table" and sim.widgetState[v] then r[i] = M.handle(sim, v) end end
      return unpack(r)
    end
  end
  h.GetFrameRef = function(self, label) return M.handle(sim, frame:GetAttribute("frameref-" .. label)) end
  h.RunAttribute = function(self, name, ...)
    return M.runSnippet(sim, frame, frame:GetAttribute(name), "self, ...", ...)
  end
  h.RunFor = function(self, target, body, ...)
    return M.runSnippet(sim, (type(target) == "table" and target.__frame) or target, body, "self, ...", ...)
  end
  h.Run = function(self, body, ...) return M.runSnippet(sim, frame, body, "self, ...", ...) end
  h.ChildUpdate = function(self, snippetid, message)
    for _, c in ipairs({ frame:GetChildren() }) do
      local body = c:GetAttribute("_childupdate-" .. tostring(snippetid)) or c:GetAttribute("_childupdate")
      if body then M.runSnippet(sim, c, body, "self, scriptid, message", snippetid, message) end
    end
  end
  h.GetChildList = function(self, t)
    t = t or {}
    for _, c in ipairs({ frame:GetChildren() }) do t[#t + 1] = M.handle(sim, c) end
    return t
  end
  h.IsProtected = function() return frame:IsProtected() end
  h.SetWidth = function(self, v) M.secureCall(sim, frame.SetWidth, frame, v) end
  h.__frame = frame
  sim.handles[frame] = h
  return h
end

local function restrictedEnv(sim, frame)
  local e = {
    format = string.format, tostring = tostring, tonumber = tonumber, type = type, select = select,
    strsplit = sim.env.strsplit, strjoin = sim.env.strjoin, strtrim = sim.env.strtrim, strmatch = string.match,
    strfind = string.find, strsub = string.sub, strlower = string.lower, strupper = string.upper, gsub = string.gsub,
    min = math.min, max = math.max, floor = math.floor, ceil = math.ceil, abs = math.abs,
    pairs = pairs, ipairs = ipairs, next = next, unpack = unpack, wipe = sim.env.wipe, tinsert = table.insert,
    tremove = table.remove, newtable = function(...) return { ... } end,
    SecureCmdOptionParse = function(t) return M.parseOptions(sim, t) end,
    GetTime = function() return sim.time end, PlayerInCombat = function() return sim.inCombat end,
    UnitExists = function(u) return sim.units[u] ~= nil end, IsShiftKeyDown = function() return sim.modifiers.shift or false end,
    IsControlKeyDown = function() return sim.modifiers.ctrl or false end, IsAltKeyDown = function() return sim.modifiers.alt or false end,
    print = function(...) sim.env.print(...) end,
  }
  e.owner = M.handle(sim, frame)
  e.self = e.owner
  e.control = {
    RunFor = function(_, target, body, ...) return M.runSnippet(sim, target.__frame or target, body, "self, ...", ...) end,
    Run = function(_, body, ...) return M.runSnippet(sim, frame, body, "self, ...", ...) end,
    CallMethod = function(_, name, ...) local fn = frame[name]; if fn then return M.secureCall(sim, fn, frame, ...) end end,
    ChildUpdate = function(_, snippetid, message)
      for _, c in ipairs({ frame:GetChildren() }) do
        local body = c:GetAttribute("_childupdate-" .. snippetid) or c:GetAttribute("_childupdate")
        if body then M.runSnippet(sim, c, body, "self, scriptid, message", snippetid, message) end
      end
    end,
  }
  return e
end

-- Run a secure snippet with the named arguments bound.
function M.runSnippet(sim, frame, body, argNames, ...)
  if type(body) ~= "string" then return end
  -- compiled as a function of the named arguments (which may end in `...`)
  local src = "return (function(" .. (argNames or "self") .. ") " .. body .. "\nend)(...)"
  local env = restrictedEnv(sim, frame)
  local fn, err = compat.loadstring(src, "=(snippet " .. tostring(frame:GetName() or "?") .. ")", env)
  if not fn then sim:_error(err); return end
  -- the snippet's `self` is a handle to `frame`; the rest are passed through
  local n = select("#", ...)
  local args = { M.handle(sim, frame), ... }
  for i = 2, n + 1 do if type(args[i]) == "table" and sim.widgetState[args[i]] then args[i] = M.handle(sim, args[i]) end end
  return M.secureCall(sim, fn, unpack(args, 1, n + 1))
end

------------------------------------------------------------------ state drivers

function M.evaluateDrivers(sim)
  for frame, drivers in pairs(sim.stateDrivers) do
    for state, d in pairs(drivers) do
      local value = M.parseOptions(sim, d.values)
      if value ~= d.last then
        d.last = value
        M.secureCall(sim, function()
          if state == "visibility" then
            if value == "show" then frame:Show() elseif value == "hide" then frame:Hide() end
          elseif d.attribute then
            frame:SetAttribute(state, value)
          else
            frame:SetAttribute("state-" .. state, value)
          end
        end)
      end
    end
  end
end

function M.install(sim, env)
  sim.secureDepth = 0
  sim.blockedActions = {}
  sim.secureActions = {}
  sim.stateDrivers = setmetatable({}, { __mode = "k" })

  for _, name in ipairs(M.PROTECTED) do
    local existing = rawget(env, name)
    rawset(env, name, function(...)
      if sim.secureDepth > 0 or (M.ALLOWED_OUT_OF_COMBAT[name] and not sim.lockdown) then
        if existing then return existing(...) end
        table.insert(sim.secureActions, { type = "call", func = name, args = { ... } })
        return
      end
      if M.ALLOWED_OUT_OF_COMBAT[name] then M.block(sim, name .. "()") else M.forbid(sim, name .. "()") end
    end)
  end

  rawset(env, "SecureCmdOptionParse", function(text) return M.parseOptions(sim, text) end)
  rawset(env, "RegisterStateDriver", function(frame, state, values)
    if sim.lockdown and sim.secureDepth == 0 then M.block(sim, "RegisterStateDriver()"); return end
    sim.stateDrivers[frame] = sim.stateDrivers[frame] or {}
    sim.stateDrivers[frame][state] = { values = values }
    M.evaluateDrivers(sim)
  end)
  rawset(env, "UnregisterStateDriver", function(frame, state)
    if sim.stateDrivers[frame] then sim.stateDrivers[frame][state] = nil end
  end)
  rawset(env, "RegisterAttributeDriver", function(frame, attribute, values)
    if sim.lockdown and sim.secureDepth == 0 then M.block(sim, "RegisterAttributeDriver()"); return end
    sim.stateDrivers[frame] = sim.stateDrivers[frame] or {}
    sim.stateDrivers[frame][attribute] = { values = values, attribute = true }
    M.evaluateDrivers(sim)
  end)
  rawset(env, "UnregisterAttributeDriver", env.UnregisterStateDriver)
  rawset(env, "RegisterUnitWatch", function(frame, asState)
    sim.stateDrivers[frame] = sim.stateDrivers[frame] or {}
    local unit = frame:GetAttribute("unit") or "target"
    sim.stateDrivers[frame][asState and "unitexists" or "visibility"] = { values = "[@" .. unit .. ",exists] show; hide" }
    M.evaluateDrivers(sim)
  end)
  rawset(env, "UnregisterUnitWatch", function(frame)
    if sim.stateDrivers[frame] then sim.stateDrivers[frame].visibility = nil end
  end)
  rawset(env, "SecureHandlerSetFrameRef", function(frame, label, ref) frame:SetAttribute("frameref-" .. label, ref) end)
  rawset(env, "SecureHandlerExecute", function(frame, body, ...) M.runSnippet(sim, frame, body, "self, ...", ...) end)
  rawset(env, "SecureHandlerWrapScript", function(frame, script, header, pre, post)
    local old = frame:GetScript(script)
    frame:SetScript(script, function(self, ...)
      local message
      if pre then message = M.runSnippet(sim, self, pre, "self, button, down", ...) end
      if message == false then return end
      if old then old(self, ...) end
      if post then M.runSnippet(sim, self, post, "self, message, button, down", message, ...) end
    end)
  end)
  rawset(env, "SecureHandler_OnLoad", function(self) end)
  rawset(env, "IsSecureCmd", function(cmd) return cmd == "/cast" or cmd == "/use" or cmd == "/target" end)
end

-- Frame hooks used by widgets.lua ---------------------------------------

-- Called after a protected frame's attribute changes: run handler snippets.
function M.onAttributeChanged(sim, frame, name, value)
  local s = sim.widgetState[frame]
  if not s or not s.protected then return end
  if type(name) == "string" and name:sub(1, 6) == "state-" then
    local body = s.attributes["_onstate-" .. name:sub(7)]
    if body then M.runSnippet(sim, frame, body, "self, stateid, newstate", name:sub(7), value) end
  end
  local body = s.attributes["_onattributechanged"]
  if body and name:sub(1, 1) ~= "_" then M.runSnippet(sim, frame, body, "self, name, value", name, value) end
end

return M
