-- The simulated game client. One Sim = one WoW client session with its
-- own global environment, frames, events, clock and SavedVariables.
local compat = require("wowapi.compat")
local toc = require("wowapi.toc")
local serialize = require("wowapi.serialize")
local widgets = require("wowapi.widgets")
local api = require("wowapi.api")
local docs = require("wowapi.docs")
local layout = require("wowapi.layout")
local render = require("wowapi.render")
local unpack = compat.unpack

local Sim = {}
Sim.__index = Sim

local DEFAULT_PLAYER = {
  name = "Tester", realm = "Forever", class = "WARRIOR", race = "Human", faction = "Alliance",
  sex = 2, level = 60, health = 5000, healthMax = 5000, money = 1234567, guild = nil,
  zone = "Elwynn Forest", subZone = "Goldshire", guid = "Player-1-00000001", mapID = 1429,
}
local faker = require("wowapi.faker")

local DEFAULT_BUILD = { version = "1.60.1", build = "60101", date = "Sep 1 2026", interface = 16001 }

local listeners = {}
-- Called with every new Sim (the test runner uses this to police errors).
function Sim.onNew(fn) table.insert(listeners, fn) end

function Sim.new(opts)
  opts = opts or {}
  local self = setmetatable({}, Sim)
  self.opts = opts
  -- facts recorded from the real client (SimCheck) win over the defaults
  local realclient = require("wowapi.realclient")
  realclient.applyGlobals()
  self.build = setmetatable(opts.build or {}, { __index = realclient.build() or DEFAULT_BUILD })
  self.locale = opts.locale or "enUS"
  self.addonPaths = {}
  for _, p in ipairs(opts.addonPaths or {}) do self.addonPaths[#self.addonPaths + 1] = p end
  self.savedVariablesDir = opts.savedVariablesDir
  -- Survives /reload: { account = { [addon] = { var = value } }, character = {...} }
  self.store = { account = {}, character = {} }
  self.quiet = opts.quiet
  self:_reset()
  for _, fn in ipairs(listeners) do fn(self) end
  return self
end

-- (Re)build all client state except SavedVariables. Used by new and /reload.
function Sim:_reset()
  local opts = self.opts
  self.time = opts.startTime or 100.0
  self.epoch = opts.now or 1790000000 -- deterministic server time()
  self.loggedIn = false
  self.inCombat = false
  self.lockdown = false
  self.hardwareEvent = false
  self.menus = {}
  self.menuModifiers = {}
  self.eventFrames = {}
  self.allEventFrames = setmetatable({}, { __mode = "k" })
  self.onUpdateFrames = {}
  self.timers = {}
  self.timerSeq = 0
  self.chat = {}
  self.sentChat = {}
  self.addonMessages = {}
  self.firedEvents = {}
  self.errors = {}
  self.warnings = {}
  self.stubbedCalls = {}
  self.blizzardTables = setmetatable({}, { __mode = "k" })
  self.undefinedGlobals = {}
  self.createdGlobals = {}
  self.sounds = {}
  self.frameCount = 0
  self.addons = {}
  self.addonOrder = {}
  self.eventCallbacks = {}
  self.units = {}
  self.fakeData = opts.fakeData ~= false
  local op = opts.player or {}
  local p = {}
  for k, v in pairs(DEFAULT_PLAYER) do p[k] = v end
  -- class-derived data (power type, specs, role) always matches the class
  local class = op.class or p.class
  local char = faker.character(op.name or p.name, { class = class, faction = op.faction or p.faction,
    race = op.race or p.race, level = op.level or p.level, spec = op.spec, healthMax = op.healthMax or p.healthMax })
  for _, k in ipairs({ "powerType", "power", "powerMax", "specs", "numSpecs", "spec", "specID", "role" }) do p[k] = char[k] end
  if self.fakeData then
    p.guild = faker.guildName(op.name or p.name)
    local zones = faker.ZONES[op.faction or p.faction] or faker.ZONES.Alliance
    if op.faction and op.faction ~= "Alliance" and not op.zone then
      local z = zones[1]
      p.zone, p.mapID, p.subZone = z[1], z[2], z[3]
    end
  end
  for k, v in pairs(op) do p[k] = v end
  self.units.player = p
  self.player = p
  self.items = {}
  self.spells = {}
  for id, v in pairs(opts.items or {}) do self:AddItem(id, v) end
  for id, v in pairs(opts.spells or {}) do self:AddSpell(id, v) end
  self.cvars = {}
  self.spellCooldowns = {}
  self.auraSeq = 0
  self.castSeq = 0
  self.popups = {}
  self.chatFilters = {}
  self.bags = {}
  self.modifiers = {}
  self.uiErrors = {}
  self.settingsCategories = {}
  self.addonMessagePrefixes = {}
  self.errorHandler = nil
  self.reloadRequested = false

  local env = {}
  self.env = env
  local sim = self
  setmetatable(env, {
    __index = function(_, k)
      if sim._lazyGlobal then
        local v, found = sim._lazyGlobal(k)
        if found then return v end
      end
      if type(k) == "string" then sim.undefinedGlobals[k] = (sim.undefinedGlobals[k] or 0) + 1 end
      return nil
    end,
    __newindex = function(t, k, v)
      if sim.currentAddon and type(k) == "string" then
        sim.createdGlobals[k] = sim.currentAddon
      end
      rawset(t, k, v)
    end,
  })
  require("wowapi.templates").install(self, env)
  widgets.install(self, env)
  api.install(self, env)
  docs.install(self, env)
  require("wowapi.input").install(self, env)
  require("wowapi.framexml").install(self, env)
  if self.fakeData then require("wowapi.fakeapi").install(self, env) end
  require("wowapi.nameplates").install(self, env)
  require("wowapi.secure").install(self, env)
  require("wowapi.xml").install(self)
  require("wowapi.resources").install(self, env)
  require("wowapi.secrets").install(self, env)
  if self.fakeData and not opts.bags then self:_fakeBags() end
  for bag, contents in pairs(opts.bags or {}) do self.bags[bag] = contents end
end

-------------------------------------------------------------------- utils

local function traceback(err)
  local tb = debug.traceback(tostring(err), 2)
  return { message = tostring(err), traceback = tb }
end

-- Run fn protected, capture errors the way the client's error frame would.
function Sim:_pcall(fn, ...)
  local args, n = { ... }, select("#", ...)
  local function pack(...) return { n = select("#", ...), ... } end
  local res = pack(xpcall(function() return fn(unpack(args, 1, n)) end, traceback))
  if not res[1] then
    local e = res[2]
    table.insert(self.errors, e)
    if self.errorHandler then pcall(self.errorHandler, e.message) end
    if not self.quiet then io.stderr:write("|cffff0000Lua error|r: " .. e.message .. "\n") end
    return false, e.message
  end
  return true, unpack(res, 2, res.n)
end

function Sim:_warn(msg)
  table.insert(self.warnings, msg)
end

function Sim:_runScript(obj, script, ...)
  local s = self.widgetState[obj]
  if not s then return end
  local fn = s.scripts[script]
  if not fn then return end
  self:_pcall(fn, obj, ...)
  local hooks = s.hooks[script]
  if hooks then
    for _, h in ipairs(hooks) do self:_pcall(h, obj, ...) end
  end
end

-------------------------------------------------------------------- events

function Sim:_registerEvent(frame, event, units)
  local s = self.widgetState[frame]
  if s.events[event] == nil then
    self.eventFrames[event] = self.eventFrames[event] or {}
    table.insert(self.eventFrames[event], frame)
  end
  s.events[event] = units or true
end

function Sim:_unregisterEvent(frame, event)
  local s = self.widgetState[frame]
  if s.events[event] == nil then return end
  s.events[event] = nil
  local list = self.eventFrames[event]
  for i = #list, 1, -1 do if list[i] == frame then table.remove(list, i) end end
end

-- Fire a game event at every frame registered for it.
function Sim:FireEvent(event, ...)
  if self.knownEvents and not self.knownEvents[event] and not (self.warnedEvents or {})[event] then
    self.warnedEvents = self.warnedEvents or {}
    self.warnedEvents[event] = true
    self:_warn("FireEvent: '" .. tostring(event) .. "' is not a documented game event")
  end
  table.insert(self.firedEvents, { event = event, args = { n = select("#", ...), ... } })
  local list = self.eventFrames[event]
  if list then
    local snapshot = { unpack(list) }
    local unit = ...
    for _, frame in ipairs(snapshot) do
      local reg = self.widgetState[frame].events[event]
      if reg == true then
        self:_runScript(frame, "OnEvent", event, ...)
      elseif type(reg) == "table" then
        for _, u in ipairs(reg) do
          if u == unit then self:_runScript(frame, "OnEvent", event, ...); break end
        end
      end
    end
  end
  for frame in pairs(self.allEventFrames) do
    if self.widgetState[frame].allEvents and self.widgetState[frame].events[event] == nil then
      self:_runScript(frame, "OnEvent", event, ...)
    end
  end
  local cbs = self.eventCallbacks[event]
  if cbs then
    for _, cb in ipairs({ unpack(cbs) }) do self:_pcall(cb.fn, cb.owner, ...) end
  end
  if self.stateDrivers and next(self.stateDrivers) and event ~= "ADDON_ACTION_BLOCKED" and event ~= "ADDON_ACTION_FORBIDDEN" then
    require("wowapi.secure").evaluateDrivers(self)
  end
end

-- Was `event` fired (optionally: how many times)?
function Sim:EventCount(event)
  local n = 0
  for _, e in ipairs(self.firedEvents) do if e.event == event then n = n + 1 end end
  return n
end

function Sim:IsEventRegistered(event)
  return self.eventFrames[event] ~= nil and #self.eventFrames[event] > 0
end

-------------------------------------------------------------------- time

function Sim:_trackOnUpdate(frame, on)
  for i, f in ipairs(self.onUpdateFrames) do
    if f == frame then
      if not on then table.remove(self.onUpdateFrames, i) end
      return
    end
  end
  if on then table.insert(self.onUpdateFrames, frame) end
end

function Sim:_addTimer(delay, fn, interval, iterations)
  self.timerSeq = self.timerSeq + 1
  local t = { at = self.time + math.max(delay or 0, 0), seq = self.timerSeq, fn = fn,
    interval = interval, remaining = iterations }
  table.insert(self.timers, t)
  return t
end

-- Advance the game clock, running OnUpdate scripts and timers as frames tick.
-- The default frame time is 1/32s: exactly representable in binary, so
-- elapsed times add up without float drift and tests stay deterministic.
function Sim:Advance(seconds, step)
  step = step or self.opts.frameTime or (1 / 32)
  local target = self.time + (seconds or 0)
  local ticked = false
  repeat
    local dt = math.min(step, target - self.time)
    if target - (self.time + dt) < 1e-9 then dt = target - self.time end
    self.time = (dt == target - self.time) and target or (self.time + dt)
    for _, frame in ipairs({ unpack(self.onUpdateFrames) }) do
      if self._isVisible(frame) then self:_runScript(frame, "OnUpdate", dt) end
    end
    require("wowapi.animation").tick(self, dt)
    local due = {}
    for _, t in ipairs(self.timers) do
      if not t.cancelled and t.at <= self.time + 1e-9 then due[#due + 1] = t end
    end
    table.sort(due, function(a, b) if a.at ~= b.at then return a.at < b.at end return a.seq < b.seq end)
    for _, t in ipairs(due) do
      if not t.cancelled then
        if t.interval then
          if t.remaining then t.remaining = t.remaining - 1 end
          if t.remaining and t.remaining <= 0 then t.cancelled = true else t.at = t.at + t.interval end
        else
          t.cancelled = true
        end
        self:_pcall(t.fn, t.handle)
      end
    end
    for i = #self.timers, 1, -1 do if self.timers[i].cancelled then table.remove(self.timers, i) end end
    ticked = true
  until self.time >= target - 1e-12 and ticked
end

function Sim:Now() return self.time end

-------------------------------------------------------------------- addons

function Sim:AddAddonPath(dir) table.insert(self.addonPaths, dir) end

function Sim:_resolveAddon(spec, hintDir)
  if toc.findToc(spec) then return spec end
  local dirs = {}
  if hintDir then dirs[#dirs + 1] = hintDir end
  for _, d in ipairs(self.addonPaths) do dirs[#dirs + 1] = d end
  for _, d in ipairs(dirs) do
    local cand = toc.join(d, spec)
    if toc.findToc(cand) then return cand end
  end
end

-- Every addon the client would list (C_AddOns.GetNumAddOns): the folders in
-- the AddOns directories plus anything loaded from elsewhere, sorted by name.
-- Entries are loaded addons or { name, dir, toc } for ones not loaded yet.
function Sim:_installedAddons()
  local seen, list = {}, {}
  local function add(name, entry)
    if seen[name] then return end
    seen[name] = true
    list[#list + 1] = entry
  end
  for _, name in ipairs(self.addonOrder) do add(name, self.addons[name]) end
  self._scanned = self._scanned or {}
  for _, d in ipairs(self.addonPaths) do
    if not self._scanned[d] then
      local found = {}
      local p = io.popen('ls -1 "' .. d .. '" 2>/dev/null')
      if p then
        for n in p:lines() do
          local dir = toc.join(d, n)
          if toc.findToc(dir) then found[#found + 1] = { name = n, dir = dir } end
        end
        p:close()
      end
      self._scanned[d] = found
    end
    for _, e in ipairs(self._scanned[d]) do
      if self.addons[e.name] then add(e.name, self.addons[e.name])
      elseif not seen[e.name] then
        if e.toc == nil then toc.LOCALE = self.locale; e.toc = toc.load(e.dir) or false end
        if e.toc then add(e.name, e) end
      end
    end
  end
  table.sort(list, function(a, b) return a.name:lower() < b.name:lower() end)
  return list
end

-- A loaded or installed addon by name or index.
function Sim:_addonInfo(ref)
  if type(ref) == "number" then return self:_installedAddons()[ref] end
  if self.addons[ref] then return self.addons[ref] end
  if type(ref) ~= "string" then return nil end
  for _, e in ipairs(self:_installedAddons()) do if e.name == ref then return e end end
end

function Sim:_runFile(addon, path, ns)
  if path:lower():match("%.xml$") then
    require("wowapi.xml").loadFile(self, path, addon, function(p)
      p = toc.resolve(p) or p
      if not toc.exists(p) then
        self:_error(string.format("%s: file referenced in %s not found: %s", addon.name, path, p))
      else
        self:_runFile(addon, p, ns)
      end
    end)
    return
  end
  local chunk, err = compat.loadfile(path, self.env)
  if not chunk then
    self:_error(err)
    return
  end
  self:_pcall(chunk, addon.name, ns)
end

function Sim:_error(msg)
  table.insert(self.errors, { message = msg, traceback = msg })
  if not self.quiet then io.stderr:write("|cffff0000Lua error|r: " .. msg .. "\n") end
end

local function charKey(p) return p.name .. " - " .. p.realm end

function Sim:_loadSavedFromDisk(addon)
  if not self.savedVariablesDir then return end
  local function read(path, into)
    local chunk = compat.loadfile(path, into)
    if chunk then pcall(chunk) end
  end
  if not self.store.account[addon.name] then
    local vars = {}
    read(toc.join(self.savedVariablesDir, addon.name .. ".lua"), vars)
    self.store.account[addon.name] = vars
  end
  local ck = charKey(self.player)
  self.store.character[ck] = self.store.character[ck] or {}
  if not self.store.character[ck][addon.name] then
    local vars = {}
    read(toc.join(toc.join(self.savedVariablesDir, ck), addon.name .. ".lua"), vars)
    self.store.character[ck][addon.name] = vars
  end
end

-- Load an addon by directory or by name (searched in addon paths).
-- Returns true, addonNamespace or false, reason.
function Sim:LoadAddon(spec, _hintDir)
  local dir = self:_resolveAddon(spec, _hintDir)
  if not dir then
    local other = toc.exists(spec) and io.popen('ls "' .. spec .. '"/*.toc 2>/dev/null'):read("*a") or ""
    if other ~= "" then
      local flavors = {}
      for f in other:gmatch("_(%a+)%.toc") do flavors[#flavors + 1] = f end
      local why = "no .toc for this client (only for: " .. table.concat(flavors, ", ") .. ")"
      self:_warn(tostring(spec) .. ": " .. why)
      return false, "INCOMPATIBLE: " .. why
    end
    return false, "MISSING"
  end
  toc.LOCALE = self.locale
  local t, err = toc.load(dir)
  if not t then return false, err end
  local existing = self.addons[t.name]
  if existing then return existing.loaded, existing.ns end
  if not t.gameTypeOk then
    self:_warn(string.format("%s: not loaded, its .toc excludes the '%s' game type", t.name, toc.GAME_TYPE))
    return false, "INCOMPATIBLE"
  end
  local addon = { name = t.name, toc = t, dir = dir, ns = {}, loaded = false, loading = true }
  self.addons[t.name] = addon
  table.insert(self.addonOrder, t.name)

  local parent = toc.dirname(dir)
  for _, dep in ipairs(t.deps) do
    local ok = self.addons[dep] and self.addons[dep].loaded or self:LoadAddon(dep, parent)
    if not ok then
      addon.loading = false
      addon.reason = "DEP_MISSING"
      self:_error(string.format("%s: required dependency '%s' is missing or failed to load", t.name, dep))
      return false, "DEP_MISSING"
    end
  end
  for _, dep in ipairs(t.optionalDeps) do
    if not self.addons[dep] and self:_resolveAddon(dep, parent) then self:LoadAddon(dep, parent) end
  end

  local iface = tonumber(t.interface[1] or "")
  if iface and iface ~= self.build.interface then
    local match = false
    for _, i in ipairs(t.interface) do if tonumber(i) == self.build.interface then match = true end end
    if not match then
      self:_warn(string.format("%s: ## Interface: %s does not include %d (would load as out of date)",
        t.name, table.concat(t.interface, ", "), self.build.interface))
    end
  end

  local prev = self.currentAddon
  self.currentAddon = t.name
  for _, f in ipairs(t.files) do
    local path = toc.resolve(toc.join(dir, f)) or toc.join(dir, f)
    if not toc.exists(path) then
      self:_error(string.format("%s: file listed in .toc not found: %s", t.name, f))
    else
      self:_runFile(addon, path, addon.ns)
    end
  end
  local bindings = toc.join(dir, "Bindings.xml")
  if toc.exists(bindings) then require("wowapi.xml").loadBindings(self, bindings, addon) end
  self.currentAddon = prev

  -- SavedVariables are applied after the addon's files ran, before ADDON_LOADED.
  self:_loadSavedFromDisk(addon)
  local acct = self.store.account[t.name] or {}
  for _, var in ipairs(t.savedVariables) do
    if acct[var] ~= nil then rawset(self.env, var, serialize.copy(acct[var])) end
  end
  local ck = charKey(self.player)
  local chr = (self.store.character[ck] or {})[t.name] or {}
  for _, var in ipairs(t.savedVariablesPerCharacter) do
    if chr[var] ~= nil then rawset(self.env, var, serialize.copy(chr[var])) end
  end

  addon.loading = false
  addon.loaded = true
  self:FireEvent("ADDON_LOADED", t.name, false)
  return true, addon.ns
end

-- Every addon directory inside `dir`.
function Sim:LoadAllAddons(dir)
  self:AddAddonPath(dir)
  local p = io.popen('ls -1 "' .. dir .. '" 2>/dev/null')
  local names = {}
  if p then for n in p:lines() do names[#names + 1] = n end; p:close() end
  table.sort(names)
  for _, n in ipairs(names) do
    local d = toc.join(dir, n)
    if toc.findToc(d) and not self.addons[n] then
      local t = toc.load(d)
      if t and not t.loadOnDemand then self:LoadAddon(d) end
    end
  end
end

function Sim:GetAddon(name) return self.addons[name] end
function Sim:NS(name) return self.addons[name] and self.addons[name].ns end

-------------------------------------------------------------------- session

function Sim:Login(isReload)
  if self.loggedIn then return end
  self.loggedIn = true
  self:FireEvent("SPELLS_CHANGED")
  self:FireEvent("PLAYER_LOGIN")
  self:FireEvent("PLAYER_ENTERING_WORLD", not isReload, isReload and true or false)
  self:FireEvent("VARIABLES_LOADED")
  self:Advance(0)
end

function Sim:_persist()
  local ck = charKey(self.player)
  self.store.character[ck] = self.store.character[ck] or {}
  for _, name in ipairs(self.addonOrder) do
    local addon = self.addons[name]
    if addon.loaded then
      local acct, chr = {}, {}
      for _, var in ipairs(addon.toc.savedVariables) do
        local ok, v = pcall(serialize.copy, rawget(self.env, var))
        if ok then acct[var] = v else self:_error(name .. ": SavedVariable " .. var .. ": " .. v) end
      end
      for _, var in ipairs(addon.toc.savedVariablesPerCharacter) do
        local ok, v = pcall(serialize.copy, rawget(self.env, var))
        if ok then chr[var] = v else self:_error(name .. ": SavedVariable " .. var .. ": " .. v) end
      end
      self.store.account[name] = acct
      self.store.character[ck][name] = chr
      if self.savedVariablesDir then
        os.execute('mkdir -p "' .. toc.join(self.savedVariablesDir, ck) .. '"')
        if #addon.toc.savedVariables > 0 then
          local f = io.open(toc.join(self.savedVariablesDir, name .. ".lua"), "w")
          if f then f:write(serialize.file(acct)); f:close() end
        end
        if #addon.toc.savedVariablesPerCharacter > 0 then
          local f = io.open(toc.join(toc.join(self.savedVariablesDir, ck), name .. ".lua"), "w")
          if f then f:write(serialize.file(chr)); f:close() end
        end
      end
    end
  end
end

-- Log out: fires the logout events and writes SavedVariables.
function Sim:Logout()
  self:FireEvent("PLAYER_LEAVING_WORLD")
  self:FireEvent("PLAYER_LOGOUT")
  self:_persist()
  self.loggedIn = false
end

-- /reload: save variables, wipe the Lua state, load every addon again, log in.
function Sim:Reload()
  local order = {}
  for _, name in ipairs(self.addonOrder) do
    local a = self.addons[name]
    if a.loaded then order[#order + 1] = a.dir end
  end
  local errors = self.errors
  self:Logout()
  self:_reset()
  self.errors = errors -- errors from before the reload still count
  for _, dir in ipairs(order) do self:LoadAddon(dir) end
  self:Login(true)
end

-------------------------------------------------------------------- input

-- Run a slash command exactly as if typed in the chat box.
function Sim:Slash(text)
  local cmd, msg = text:match("^(/%S+)%s*(.-)%s*$")
  if not cmd then error("Slash: expected a command like '/foo args', got " .. tostring(text), 2) end
  local lower = cmd:lower()
  if lower == "/reload" or lower == "/rl" and not self:_findSlash("/rl") then self:Reload(); return true end
  local handler = self:_findSlash(lower)
  if not handler then
    if lower == "/run" or lower == "/script" then self:Exec(msg); return true end
    if lower == "/dump" then
      local ok, v = self:Exec("return " .. msg)
      if ok then self:_chat("Dump: " .. msg); self:_chat("[1]=" .. tostring(v)) end
      return true
    end
    self:_chat("Type '/help' for a listing of a few commands.", "SYSTEM")
    return false
  end
  self:_pcall(handler, msg, rawget(self.env, "DEFAULT_CHAT_FRAME"))
  self:_afterInput()
  return true
end

-- ReloadUI() called from addon code takes effect once the handler returns.
function Sim:_afterInput()
  if self.reloadRequested then
    self.reloadRequested = false
    self:Reload()
  end
end

function Sim:_findSlash(cmd)
  local list = rawget(self.env, "SlashCmdList")
  if not list then return end
  for key, fn in pairs(list) do
    local i = 1
    while true do
      local alias = rawget(self.env, "SLASH_" .. key .. i)
      if not alias then break end
      if alias:lower() == cmd then return fn end
      i = i + 1
    end
  end
end

-- Run Lua in the game environment (like /run). Returns ok, results...
function Sim:Exec(code)
  local fn, err = compat.loadstring(code, "=(exec)", self.env)
  if not fn then self:_error(err); return false, err end
  return self:_pcall(fn)
end

-- Read a global from the game environment (without counting it as an
-- undefined-global read).
function Sim:Get(name)
  local v = rawget(self.env, name)
  if v == nil and self._lazyGlobal then v = self._lazyGlobal(name) end
  return v
end

local function frameArg(self, frame)
  if type(frame) == "string" then
    local f = self:Get(frame)
    if not f then error("no frame named " .. frame, 3) end
    return f
  end
  return frame
end

-- Click a frame directly (like frame:Click(), ignores what's on top of it).
function Sim:Click(frame, button)
  frame = frameArg(self, frame)
  self.hardwareEvent = true
  if frame.Click then frame:Click(button) else self:_runScript(frame, "OnClick", button or "LeftButton", false) end
  self.hardwareEvent = false
  self:_afterInput()
end

-- Move the cursor to screen coordinates (origin bottom-left, 1920x1080),
-- firing OnLeave/OnEnter as the frame under the mouse changes.
function Sim:MoveMouse(x, y)
  self.cursorX, self.cursorY = x, y
  local top = layout.framesAt(self, x, y)[1]
  if top ~= self.mouseFocus then
    local old = self.mouseFocus
    self.mouseFocus = top
    if old then self:_runScript(old, "OnLeave", true) end
    if top then self:_runScript(top, "OnEnter", true) end
  end
  return top
end

-- Frame under the cursor (or under x, y).
function Sim:FrameAt(x, y)
  return layout.framesAt(self, x or self.cursorX or 0, y or self.cursorY or 0)[1]
end

local function center(self, frame)
  local l, b, w, h = layout.rect(self, frame)
  if not l then error("frame has no position (no SetPoint?)", 3) end
  return l + w / 2, b + h / 2
end

-- Move the mouse over a frame (to its center) - fires OnEnter.
function Sim:Hover(frame)
  frame = frameArg(self, frame)
  local l = layout.rect(self, frame)
  if l and frame:IsVisible() then
    self:MoveMouse(center(self, frame))
    if self.mouseFocus ~= frame then
      -- covered by another frame or not mouse-enabled: deliver anyway like a direct call
      self.mouseFocus = frame
      self:_runScript(frame, "OnEnter", true)
    end
  else
    self.mouseFocus = frame
    self:_runScript(frame, "OnEnter", true)
  end
end

function Sim:Leave(frame)
  frame = frameArg(self, frame)
  if self.mouseFocus == frame then self.mouseFocus = nil end
  self:_runScript(frame, "OnLeave", true)
end

local function clickRegistered(self, frame, button, up)
  local s = self.widgetState[frame]
  local list = s.clicks or { "LeftButtonUp" }
  local want = button .. (up and "Up" or "Down")
  for _, c in ipairs(list) do
    if c == want or c == "AnyUp" and up or c == "AnyDown" and not up then return true end
  end
  return false
end

-- A real mouse click at screen coordinates (or on a frame's center): goes to
-- whatever frame is on top there, with OnMouseDown/OnMouseUp/OnClick in order.
function Sim:ClickAt(x, y, button)
  if type(x) == "table" or type(x) == "string" then
    button = y
    x, y = center(self, frameArg(self, x))
  end
  button = button or "LeftButton"
  local target = self:MoveMouse(x, y)
  if not target then return nil end
  self:_runScript(target, "OnMouseDown", button)
  local s = self.widgetState[target]
  if target:IsObjectType("EditBox") then target:SetFocus() end
  if target:IsObjectType("Button") and s.enabled ~= false and clickRegistered(self, target, button, true) then
    self.hardwareEvent = true
    target:Click(button, false)
    self.hardwareEvent = false
  end
  if self.widgetState[target] then self:_runScript(target, "OnMouseUp", button, true) end
  self:_afterInput()
  return target
end

-- Drag a frame by (dx, dy) with the mouse: OnMouseDown, OnDragStart (if
-- registered), movement while StartMoving/StartSizing is active,
-- OnDragStop, OnMouseUp.
function Sim:Drag(frame, dx, dy, button)
  frame = frameArg(self, frame)
  button = button or "LeftButton"
  local s = self.widgetState[frame]
  local x, y = center(self, frame)
  self:MoveMouse(x, y)
  self:_runScript(frame, "OnMouseDown", button)
  local canDrag = false
  for _, b in ipairs(s.dragButtons or {}) do if b == button or b == "Any" then canDrag = true end end
  if canDrag then self:_runScript(frame, "OnDragStart", button) end
  if s.moving or s.sizing then
    local l, b, w, h = layout.rect(self, frame)
    local es = layout.effectiveScale(self.widgetState, frame)
    if s.moving then
      s.points = { { "TOPLEFT", self.env.UIParent, "BOTTOMLEFT", (l + dx) / es, (b + h + dy) / es } }
    else
      local nw, nh = math.max(0, w + dx) / es, math.max(0, h - dy) / es
      local bounds = s.resizeBounds
      if bounds then
        nw = math.max(bounds[1] or 0, bounds[3] and bounds[3] > 0 and math.min(bounds[3], nw) or nw)
        nh = math.max(bounds[2] or 0, bounds[4] and bounds[4] > 0 and math.min(bounds[4], nh) or nh)
      end
      s.points = { { "TOPLEFT", self.env.UIParent, "BOTTOMLEFT", l / es, (b + h) / es } }
      s.width, s.height = nw, nh
      self:_runScript(frame, "OnSizeChanged", nw, nh)
    end
  end
  self.cursorX, self.cursorY = x + dx, y + dy
  if canDrag then self:_runScript(frame, "OnDragStop", button) end
  self:_runScript(frame, "OnMouseUp", button, true)
  self:_afterInput()
end

-- Mouse wheel over a frame (delta 1 = up, -1 = down).
function Sim:Scroll(frame, delta)
  frame = frameArg(self, frame)
  local s = self.widgetState[frame]
  if not (s.mouseWheel or (s.props and s.props.MouseWheelEnabled and s.props.MouseWheelEnabled[1])) then return false end
  self:_runScript(frame, "OnMouseWheel", delta or 1)
  return true
end

-------------------------------------------------------------------- screenshot

-- The shared game-art store (downloads and caches textures on demand).
local artStores = {}
function Sim:ArtStore()
  local o = self.opts
  local key = table.concat({ tostring(o.artDir), tostring(o.artCache), tostring(o.offline) }, "|")
  if not artStores[key] then
    artStores[key] = require("wowapi.assets").store({ artDir = o.artDir, cacheDir = o.artCache,
      offline = o.offline, verbose = o.artVerbose })
  end
  return artStores[key]
end

-- A headless browser for PNG export. chrome-headless-shell renders exactly
-- 1920x1080; regular Chrome/Chromium works too (see _viewportExtra).
local function findBrowser()
  local function ok(cmd) local r = os.execute(cmd .. " >/dev/null 2>&1"); return r == 0 or r == true end
  local list = {}
  if os.getenv("WOWAPI_BROWSER") then list[#list + 1] = os.getenv("WOWAPI_BROWSER") end
  for _, b in ipairs({ "chrome-headless-shell", "headless_shell" }) do list[#list + 1] = b end
  -- Playwright / Puppeteer installs of the headless shell
  local p = io.popen("ls -d /opt/pw-browsers/chromium_headless_shell-*/*/headless_shell "
    .. "$HOME/.cache/ms-playwright/chromium_headless_shell-*/*/headless_shell "
    .. "$HOME/.cache/puppeteer/chrome-headless-shell/*/*/chrome-headless-shell 2>/dev/null")
  if p then for line in p:lines() do list[#list + 1] = line end; p:close() end
  for _, b in ipairs({ "chromium", "chromium-browser", "google-chrome", "google-chrome-stable", "msedge",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" }) do list[#list + 1] = b end
  p = io.popen("ls -d /opt/pw-browsers/chromium-*/chrome-linux/chrome $HOME/.cache/ms-playwright/chromium-*/chrome-linux/chrome 2>/dev/null")
  if p then for line in p:lines() do list[#list + 1] = line end; p:close() end
  for _, b in ipairs(list) do
    if ok("command -v '" .. b .. "'") then return b, b:find("headless") ~= nil end
  end
end

-- Tell the art store where loaded addons live (for their own textures).
function Sim:_registerAddonArt()
  local store = self:ArtStore()
  store.addonDirs = store.addonDirs or {}
  for name, a in pairs(self.addons) do store.addonDirs[name:lower()] = a.dir end
end

-- Render what's on screen. Returns the SVG markup; with a path, writes an
-- .svg file, or a .png (rendered with a headless Chrome/Chromium if found).
--   sim:Screenshot("ui.svg", { outlines = true })
--   sim:Screenshot("ui.png", { art = true })   -- real game textures
-- opts: art (default: WoW.new's `art` option), outlines, background
-- (a texture path), embed (default true), cursor, font.
function Sim:Screenshot(path, opts)
  opts = opts or {}
  if opts.art == nil then opts.art = self.opts.art end
  if opts.art then self:_registerAddonArt() end
  local png = path and path:lower():match("%.png$")
  if png then opts.embed = opts.embed ~= false end
  local svg = render.svg(self, opts)
  if path then
    local target = png and (os.tmpname() .. ".svg") or path
    local f = assert(io.open(target, "w"))
    f:write(svg)
    f:close()
    if png then
      local browser, isShell = findBrowser()
      if not browser then
        os.rename(target, path:gsub("%.[Pp][Nn][Gg]$", ".svg"))
        error("Screenshot: no Chrome/Chromium found to make a PNG (set WOWAPI_BROWSER); wrote an .svg instead", 2)
      end
      local abs = target:sub(1, 1) == "/" and target or (io.popen("pwd"):read("*l") .. "/" .. target)
      local out = path:sub(1, 1) == "/" and path or (io.popen("pwd"):read("*l") .. "/" .. path)
      local flags = (isShell and "" or "--headless ") .. "--disable-gpu --no-sandbox --hide-scrollbars --force-device-scale-factor=1 --default-background-color=1a2418ff"
      -- headless Chrome counts browser chrome inside --window-size; measure the
      -- real viewport once and compensate so the PNG is exactly 1920x1080
      if isShell then Sim._viewportExtra = 0 end
      if not Sim._viewportExtra then
        local calib = os.tmpname() .. ".html"
        local cf = io.open(calib, "w")
        cf:write("<html><body><script>document.body.textContent='VH='+innerHeight</script></body></html>")
        cf:close()
        local p = io.popen(string.format("'%s' %s --window-size=1920,1080 --dump-dom 'file://%s' 2>/dev/null", browser, flags, calib))
        local dom = p and p:read("*a") or ""
        if p then p:close() end
        os.remove(calib)
        local vh = tonumber(dom:match("VH=(%d+)"))
        Sim._viewportExtra = vh and (1080 - vh) or 0
      end
      os.execute(string.format("'%s' %s --window-size=%d,%d --screenshot='%s' 'file://%s' >/dev/null 2>&1",
        browser, flags, 1920, 1080 + Sim._viewportExtra, out, abs))
      os.remove(target)
    end
  end
  return svg
end

function Sim:Type(editBox, text)
  local s = self.widgetState[editBox]
  s.text = s.text .. text
  self:_runScript(editBox, "OnTextChanged", true)
end

function Sim:PressEnter(editBox) self:_runScript(editBox, "OnEnterPressed") end
function Sim:PressEscape(editBox) self:_runScript(editBox, "OnEscapePressed") end

-------------------------------------------------------------------- mocks & call log

local function resolvePath(env, key)
  local ns, fname = key:match("^([^.]+)%.(.+)$")
  if ns then return rawget(env, ns), fname end
  return env, key
end

-- Replace what a game API function returns. `impl` is a function, or the
-- value(s) to return. Works for documented and hand-written functions.
--   sim:Mock("C_Map.GetBestMapForUnit", 2112)
--   sim:Mock("UnitHealth", function(unit) return unit == "target" and 50 or 100 end)
function Sim:Mock(key, impl, ...)
  local fn = impl
  if type(impl) ~= "function" then
    local vals = { n = select("#", ...) + 1, impl, ... }
    fn = function() return unpack(vals, 1, vals.n) end
  end
  if self.apiDocs and self.apiDocs.functions[key] then
    self.mocks[key] = fn
  else
    local holder, name = resolvePath(self.env, key)
    if not holder then holder = {}; rawset(self.env, key:match("^([^.]+)"), holder) end
    self.unmock = self.unmock or {}
    if self.unmock[key] == nil then self.unmock[key] = { holder, name, rawget(holder, name) } end
    rawset(holder, name, fn)
  end
end

function Sim:Unmock(key)
  self.mocks[key] = nil
  local u = self.unmock and self.unmock[key]
  if u then rawset(u[1], u[2], u[3]); self.unmock[key] = nil end
end

-- Calls made to a documented API function: list of argument tables.
function Sim:Calls(key)
  local c = self.apiCalls[key]
  return c and c.log or {}
end

function Sim:CallCount(key)
  local c = self.apiCalls[key]
  return c and c.n or 0
end

-- Signature of a documented function or widget method, for reference:
--   sim:Doc("C_Timer.After") / sim:Doc("Frame:SetPoint")
function Sim:Doc(key)
  local d = self.apiDocs
  local typ, method = key:match("^(%w+):(%w+)$")
  local doc
  if typ then doc = docs.methodsFor(typ)[method] else doc = d.functions[key] end
  if not doc then return nil end
  local function list(t)
    local o = {}
    for _, a in ipairs(t) do o[#o + 1] = a[1] .. ": " .. a[2] .. (a[3] and "?" or "") end
    return table.concat(o, ", ")
  end
  local s = key .. "(" .. list(doc.a) .. ")"
  if #doc.r > 0 then s = s .. " -> " .. list(doc.r) end
  return s, doc
end

-------------------------------------------------------------------- world state

function Sim:SetUnit(unit, data)
  if data == nil then self.units[unit] = nil; return end
  local u = {}
  for k, v in pairs(DEFAULT_PLAYER) do u[k] = v end
  u.guid = "Creature-0-0-0-0-" .. unit
  for k, v in pairs(data) do u[k] = v end
  self.units[unit] = u
  return u
end

function Sim:SetTarget(data)
  self:SetUnit("target", data)
  self:FireEvent("PLAYER_TARGET_CHANGED")
end

-- Enter combat. PLAYER_REGEN_DISABLED fires while InCombatLockdown() is
-- still false (the last chance to touch protected frames); lockdown starts
-- right after, like in game.
function Sim:EnterCombat()
  if self.inCombat then return end
  self.inCombat = true
  self:FireEvent("PLAYER_REGEN_DISABLED")
  self.lockdown = true
  require("wowapi.secrets").update(self)
  require("wowapi.secure").evaluateDrivers(self)
end

function Sim:LeaveCombat()
  if not self.inCombat then return end
  self.lockdown = false
  self.inCombat = false
  require("wowapi.secrets").update(self)
  self:FireEvent("PLAYER_REGEN_ENABLED")
  require("wowapi.secure").evaluateDrivers(self)
end

-- Force secret-value restrictions on/off (nil = follow combat). Needs
-- WoW.new({ secretValues = true }).
function Sim:SetSecretRestrictions(on)
  self.forcedSecretRestrictions = on
  require("wowapi.secrets").update(self)
end

-- Actions addons were stopped from doing (ADDON_ACTION_BLOCKED/FORBIDDEN).
function Sim:BlockedActions() return self.blockedActions end

function Sim:AddItem(id, info)
  info.id = id
  info.name = info.name or ("Item " .. id)
  info.quality = info.quality or 1
  info.link = info.link or string.format("|cffffffff|Hitem:%d::::::::60:::::|h[%s]|h|r", id, info.name)
  self.items[id] = info
  self.items[info.name] = info
end

function Sim:AddSpell(id, info)
  info.id = id
  info.name = info.name or ("Spell " .. id)
  self.spells[id] = info
  self.spells[info.name] = info
end

-------------------------------------------------------------------- fake world

function Sim:_fakeBags()
  self.bags = self.bags or {}
  local r = faker.rng("bags:" .. tostring(self.opts.player and self.opts.player.name or "Tester"))
  -- loot: generated items, registered so they exist
  local function loot()
    local id = r.int(20000, 180000)
    self.items[id] = self.items[id] or faker.item(id, self.player.level)
    return id
  end
  local backpack = { size = 16, { itemID = 6948, stackCount = 1 }, { itemID = 4540, stackCount = 8 }, { itemID = 159, stackCount = 12 } }
  for slot = 4, 7 do backpack[slot] = { itemID = loot(), stackCount = 1 } end
  self.bags[0] = backpack
  for bag = 1, 4 do
    local b = { size = 14 }
    for slot = 1, r.int(2, 6) do b[slot] = { itemID = loot(), stackCount = r.chance(0.3) and r.int(2, 20) or 1 } end
    self.bags[bag] = b
  end
end

-- Fill the party (party1..partyN, max 4) with generated players.
function Sim:SpawnParty(n, opts)
  opts = opts or {}
  for i = 1, 4 do self.units["party" .. i] = nil end
  for i = 1, 40 do self.units["raid" .. i] = nil end
  local out = {}
  for i = 1, math.min(n or 4, 4) do
    local o = {}
    for k, v in pairs(opts) do o[k] = v end
    o.faction = o.faction or self.player.faction
    o.level = o.level or self.player.level
    o.class = (opts.classes or {})[i]
    local u = faker.character("party" .. i .. ":" .. tostring(opts.seed or ""), o)
    self.units["party" .. i] = u
    out[i] = u
  end
  self:FireEvent("GROUP_ROSTER_UPDATE")
  return out
end

-- Fill a raid (raid1 = you, raid2..raidN generated; party1-4 = your group).
function Sim:SpawnRaid(n, opts)
  opts = opts or {}
  for i = 1, 4 do self.units["party" .. i] = nil end
  for i = 1, 40 do self.units["raid" .. i] = nil end
  n = math.min(n or 20, 40)
  self.units.raid1 = self.player
  self.player.subgroup = 1
  for i = 2, n do
    local u = faker.character("raid" .. i .. ":" .. tostring(opts.seed or ""), { faction = self.player.faction, level = self.player.level })
    u.subgroup = math.floor((i - 1) / 5) + 1
    self.units["raid" .. i] = u
    if i <= 5 then self.units["party" .. (i - 1)] = u end
  end
  self.player.leader = true
  self:FireEvent("GROUP_ROSTER_UPDATE")
end

-- Spawn an enemy NPC and target it. opts: name, level, boss, classification, healthMax...
function Sim:SpawnEnemy(opts)
  opts = opts or {}
  self.npcSeq = (self.npcSeq or 0) + 1
  local npc = faker.npc(opts.seed or self.npcSeq, opts)
  self.units.target = npc
  if npc.classification == "worldboss" or opts.boss then
    self.units.boss1 = npc
    self:FireEvent("INSTANCE_ENCOUNTER_ENGAGE_UNIT")
  end
  self:FireEvent("PLAYER_TARGET_CHANGED")
  return npc
end

local function spellData(self, spell)
  if type(spell) == "table" then return spell end
  local s = self.spells[spell]
  if not s and type(spell) == "string" then
    for _, k in ipairs(faker.KNOWN_SPELLS) do if k[2]:lower() == spell:lower() then spell = k[1]; break end end
  end
  s = s or self.spells[spell]
  if not s and type(spell) == "number" then s = faker.spell(spell); self.spells[spell] = s end
  return s
end

-- Put an aura on a unit: sim:AddAura("player", 774, { duration = 12 })
function Sim:AddAura(unit, spell, opts)
  opts = opts or {}
  local u = self.units[unit]
  if not u then error("AddAura: no unit '" .. tostring(unit) .. "'", 2) end
  local s = spellData(self, spell)
  opts.now = self.time
  local a = faker.aura(s, opts)
  self.auraSeq = self.auraSeq + 1
  a.auraInstanceID = self.auraSeq
  u.auras = u.auras or {}
  table.insert(u.auras, a)
  self:FireEvent("UNIT_AURA", unit, { addedAuras = { a }, isFullUpdate = false })
  return a
end

function Sim:RemoveAura(unit, spell)
  local u = self.units[unit]
  local s = spellData(self, spell)
  for i = #(u.auras or {}), 1, -1 do
    local a = u.auras[i]
    if a.spellId == s.id then
      table.remove(u.auras, i)
      self:FireEvent("UNIT_AURA", unit, { removedAuraInstanceIDs = { a.auraInstanceID }, isFullUpdate = false })
    end
  end
end

-- Change a unit's health and fire UNIT_HEALTH.
function Sim:SetHealth(unit, value)
  local u = self.units[unit]
  u.health = math.max(0, math.min(u.healthMax or value, value))
  u.dead = u.health == 0
  self:FireEvent("UNIT_HEALTH", unit)
  if u.dead and unit ~= "player" then
    self:CombatLog("UNIT_DIED", { dest = unit })
  end
end

-- The player casts a spell: UNIT_SPELLCAST_* events over the cast time,
-- a combat-log SPELL_CAST_SUCCESS and the spell's cooldown.
function Sim:Cast(spell, opts)
  opts = opts or {}
  local s = spellData(self, spell)
  local target = opts.target or (self.units.target and "target") or "player"
  self.castSeq = self.castSeq + 1
  local castGUID = string.format("Cast-3-1-2-3-%d-%08d", s.id, self.castSeq)
  self:FireEvent("UNIT_SPELLCAST_SENT", "player", self.units[target] and self.units[target].name or "", castGUID, s.id)
  local castTime = (s.castTime or 0) / 1000
  if castTime > 0 then
    self.player.casting = { spell = s, startTime = self.time, endTime = self.time + castTime, castGUID = castGUID }
    self:FireEvent("UNIT_SPELLCAST_START", "player", castGUID, s.id)
    self:CombatLog("SPELL_CAST_START", { source = "player", dest = target, spellId = s.id })
    self:Advance(castTime)
    self.player.casting = nil
    self:FireEvent("UNIT_SPELLCAST_STOP", "player", castGUID, s.id)
  end
  self:FireEvent("UNIT_SPELLCAST_SUCCEEDED", "player", castGUID, s.id)
  self:CombatLog("SPELL_CAST_SUCCESS", { source = "player", dest = target, spellId = s.id })
  if (s.cooldownSeconds or 0) > 0 then
    self.spellCooldowns[s.id] = { startTime = self.time, duration = s.cooldownSeconds, isEnabled = true, modRate = 1 }
    self:FireEvent("SPELL_UPDATE_COOLDOWN")
  end
  return castGUID
end

-------------------------------------------------------------------- chat

local function stripCodes(s)
  s = s:gsub("|c%x%x%x%x%x%x%x%x", ""):gsub("|r", "")
  s = s:gsub("|H.-|h(.-)|h", "%1"):gsub("|T.-|t", ""):gsub("|A.-|a", ""):gsub("||", "|")
  return s
end
Sim.StripCodes = stripCodes

function Sim:_chat(msg, channel)
  table.insert(self.chat, { raw = msg, text = stripCodes(msg), channel = channel or "SYSTEM" })
  if self.onChat then self.onChat(msg, channel) end
end

-- All chat-frame output, color codes stripped, joined with newlines.
function Sim:ChatText()
  local out = {}
  for _, c in ipairs(self.chat) do out[#out + 1] = c.text end
  return table.concat(out, "\n")
end

-- Does any chat line match `pattern`? plain=true for substring match.
function Sim:ChatContains(pattern, plain)
  for _, c in ipairs(self.chat) do
    if c.text:find(pattern, 1, plain) then return true end
  end
  return false
end

function Sim:LastChat() local c = self.chat[#self.chat]; return c and c.text end
function Sim:ClearChat() self.chat = {} end

-------------------------------------------------------------------- reports

function Sim:ClearErrors() self.errors = {} end

function Sim:AssertNoErrors()
  if #self.errors > 0 then
    local lines = {}
    for _, e in ipairs(self.errors) do lines[#lines + 1] = e.traceback end
    error(#self.errors .. " Lua error(s) in addon code:\n" .. table.concat(lines, "\n\n"), 2)
  end
end

-- Globals written while an addon's files were loading (excludes
-- SavedVariables, slash command aliases and named frames).
function Sim:LeakedGlobals(addonName)
  local out = {}
  local addon = self.addons[addonName]
  local allowed = {}
  if addon then
    for _, v in ipairs(addon.toc.savedVariables) do allowed[v] = true end
    for _, v in ipairs(addon.toc.savedVariablesPerCharacter) do allowed[v] = true end
  end
  for k, owner in pairs(self.createdGlobals) do
    if owner == addonName and not allowed[k] and not k:match("^SLASH_") and not k:match("^BINDING_")
      and not self.widgetState[rawget(self.env, k)] and k ~= addonName then
      out[#out + 1] = k
    end
  end
  table.sort(out)
  return out
end

function Sim:Report()
  local out = {}
  local function add(s) out[#out + 1] = s end
  for _, name in ipairs(self.addonOrder) do
    local a = self.addons[name]
    add(string.format("Addon %-24s %s", name, a.loaded and "loaded" or ("NOT LOADED (" .. tostring(a.reason) .. ")")))
    local leaks = self:LeakedGlobals(name)
    if #leaks > 0 then add("  leaked globals: " .. table.concat(leaks, ", ")) end
  end
  add(string.format("Frames created: %d, events fired: %d", self.frameCount, #self.firedEvents))
  if #self.warnings > 0 then
    add("Warnings:")
    for _, w in ipairs(self.warnings) do add("  - " .. w) end
  end
  local stubs = {}
  for k, n in pairs(self.stubbedCalls) do stubs[#stubs + 1] = string.format("%s (x%d)", k, n) end
  table.sort(stubs)
  if #stubs > 0 then add("Called but only stubbed (no visual effect here): " .. table.concat(stubs, ", ")) end
  local undef = {}
  local saved = {}
  for _, a in pairs(self.addons) do
    for _, v in ipairs(a.toc.savedVariables) do saved[v] = true end
    for _, v in ipairs(a.toc.savedVariablesPerCharacter) do saved[v] = true end
  end
  for k in pairs(self.undefinedGlobals) do
    -- ignore names that are defined by now (read-before-write) and SavedVariables
    if not k:match("^__") and rawget(self.env, k) == nil and not saved[k] then undef[#undef + 1] = k end
  end
  table.sort(undef)
  if #undef > 0 then
    add("Globals read but not defined (not emulated, or a typo?): " .. table.concat(undef, ", "))
  end
  if #(self.blockedActions or {}) > 0 then
    add("Blocked by the client's security (ADDON_ACTION_BLOCKED/FORBIDDEN):")
    for _, b in ipairs(self.blockedActions) do add(string.format("  - [%s] %s: %s", b.kind, b.addon, b.action)) end
  end
  -- missing embedded libraries usually means the addon wasn't packaged
  for _, name in ipairs(self.addonOrder) do
    local a = self.addons[name]
    local missingLib = false
    for _, e in ipairs(self.errors) do
      if e.message:find(a.dir, 1, true) and e.message:lower():find("/libs?/") and
        (e.message:find("cannot open") or e.message:find("not found")) then missingLib = true end
    end
    if missingLib then
      local pkg = toc.exists(toc.join(a.dir, ".pkgmeta"))
      add(string.format("Hint: %s is missing embedded libraries%s. Test the packaged addon (the zip from CurseForge/Wago/GitHub releases) or install them into its Libs folder.",
        name, pkg and " (its .pkgmeta lists them as externals fetched at packaging time)" or ""))
    end
  end
  if #self.errors == 0 then
    add("Lua errors: none")
  else
    add(string.format("Lua errors: %d", #self.errors))
    for i, e in ipairs(self.errors) do add(string.format("  [%d] %s", i, e.traceback)) end
  end
  return table.concat(out, "\n")
end

for k, v in pairs(require("wowapi.input").SimMethods) do Sim[k] = v end
for k, v in pairs(require("wowapi.framexml").SimMethods) do Sim[k] = v end
for k, v in pairs(require("wowapi.fakeapi").SimMethods) do Sim[k] = v end
for k, v in pairs(require("wowapi.nameplates").SimMethods) do Sim[k] = v end

return Sim
