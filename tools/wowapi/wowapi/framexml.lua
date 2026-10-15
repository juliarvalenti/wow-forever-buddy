-- Lua-side utilities from Blizzard's FrameXML that addons use every day:
-- pools, CallbackRegistryMixin, Item/Spell mixins, MenuUtil and dropdowns,
-- time formatting, UI panels, and the combat log.
local compat = require("wowapi.compat")
local unpack = compat.unpack

local M = {}

function M.install(sim, env)
  local Mixin = env.Mixin

  ------------------------------------------------------------ pools
  local ObjectPoolMixin = {}
  function ObjectPoolMixin:Init(create, reset)
    self.createFunc, self.resetFunc = create, reset
    self.activeObjects, self.inactiveObjects, self.numActiveObjects = {}, {}, 0
  end
  function ObjectPoolMixin:Acquire()
    local obj = table.remove(self.inactiveObjects)
    local new = obj == nil
    if new then obj = self.createFunc(self) end
    self.activeObjects[obj] = true
    self.numActiveObjects = self.numActiveObjects + 1
    return obj, new
  end
  function ObjectPoolMixin:Release(obj)
    if not self.activeObjects[obj] then return false end
    if self.resetFunc then self.resetFunc(self, obj, false) end
    self.activeObjects[obj] = nil
    self.numActiveObjects = self.numActiveObjects - 1
    table.insert(self.inactiveObjects, obj)
    return true
  end
  function ObjectPoolMixin:ReleaseAll()
    for obj in pairs(self.activeObjects) do self:Release(obj) end
  end
  function ObjectPoolMixin:EnumerateActive() return pairs(self.activeObjects) end
  function ObjectPoolMixin:GetNextActive(current) return (next(self.activeObjects, current)) end
  function ObjectPoolMixin:IsActive(obj) return self.activeObjects[obj] == true end
  function ObjectPoolMixin:GetNumActive() return self.numActiveObjects end
  function ObjectPoolMixin:EnumerateInactive() return ipairs(self.inactiveObjects) end
  rawset(env, "ObjectPoolMixin", ObjectPoolMixin)

  local function hideReset(_, obj) obj:Hide(); obj:ClearAllPoints() end
  local function CreateObjectPool(create, reset)
    local p = Mixin({}, ObjectPoolMixin)
    p:Init(create, reset)
    return p
  end
  rawset(env, "CreateObjectPool", CreateObjectPool)
  rawset(env, "CreateFramePool", function(frameType, parent, template, reset, forbidden, init)
    local p = CreateObjectPool(function()
      local f = env.CreateFrame(frameType, nil, parent, template)
      if init then init(f) end
      return f
    end, reset or hideReset)
    p.frameType, p.parent, p.frameTemplate = frameType, parent, template
    return p
  end)
  rawset(env, "CreateTexturePool", function(parent, layer, sublevel, template, reset)
    return CreateObjectPool(function() return parent:CreateTexture(nil, layer, template, sublevel) end, reset or hideReset)
  end)
  rawset(env, "CreateFontStringPool", function(parent, layer, sublevel, template, reset)
    return CreateObjectPool(function() return parent:CreateFontString(nil, layer, template, sublevel) end, reset or hideReset)
  end)
  rawset(env, "CreateFramePoolCollection", function()
    local c = { pools = {} }
    function c:GetOrCreatePool(frameType, parent, template, reset)
      local key = tostring(frameType) .. tostring(parent) .. tostring(template)
      if not self.pools[key] then self.pools[key] = env.CreateFramePool(frameType, parent, template, reset) end
      return self.pools[key]
    end
    c.CreatePool = c.GetOrCreatePool
    function c:Acquire(template) for _, p in pairs(self.pools) do if p.frameTemplate == template then return p:Acquire() end end end
    function c:ReleaseAll() for _, p in pairs(self.pools) do p:ReleaseAll() end end
    function c:EnumerateActive()
      local all = {}
      for _, p in pairs(self.pools) do for o in p:EnumerateActive() do all[o] = true end end
      return pairs(all)
    end
    function c:GetNumActive() local n = 0; for _, p in pairs(self.pools) do n = n + p:GetNumActive() end return n end
    return c
  end)
  rawset(env, "Pool_HideAndClearAnchors", hideReset)

  ------------------------------------------------------------ callback registry
  local CallbackRegistryMixin = {}
  function CallbackRegistryMixin:OnLoad()
    self.callbackTables = {}
    self.Event = self.Event or {}
  end
  function CallbackRegistryMixin:SetUndefinedEventsAllowed(v) self.undefinedEventsAllowed = v end
  function CallbackRegistryMixin:GenerateCallbackEvents(events)
    self.Event = self.Event or {}
    for _, e in ipairs(events) do self.Event[e] = e end
  end
  local ownerSeq = 0
  function CallbackRegistryMixin:RegisterCallback(event, func, owner, ...)
    if not self.callbackTables then self:OnLoad() end
    if not self.undefinedEventsAllowed and self.Event and next(self.Event) and not self.Event[event] then
      error("CallbackRegistryMixin:RegisterCallback event '" .. tostring(event) .. "' doesn't exist.", 2)
    end
    if owner == nil then ownerSeq = ownerSeq + 1; owner = ownerSeq end
    self.callbackTables[event] = self.callbackTables[event] or {}
    local extra = select("#", ...) > 0 and { n = select("#", ...), ... } or nil
    self.callbackTables[event][owner] = { func = func, extra = extra }
    return owner
  end
  function CallbackRegistryMixin:RegisterCallbackWithHandle(event, func, owner, ...)
    local o = self:RegisterCallback(event, func, owner, ...)
    local reg = self
    return { Unregister = function() reg:UnregisterCallback(event, o) end }
  end
  function CallbackRegistryMixin:UnregisterCallback(event, owner)
    if self.callbackTables and self.callbackTables[event] then self.callbackTables[event][owner] = nil end
  end
  function CallbackRegistryMixin:UnregisterEvents(events)
    for _, e in ipairs(events or {}) do if self.callbackTables then self.callbackTables[e] = nil end end
  end
  function CallbackRegistryMixin:TriggerEvent(event, ...)
    if not self.callbackTables then return end
    local list = self.callbackTables[event]
    if not list then return end
    local copy = {}
    for owner, cb in pairs(list) do copy[#copy + 1] = { owner, cb } end
    for _, e in ipairs(copy) do
      local owner, cb = e[1], e[2]
      if cb.extra then
        local args = { unpack(cb.extra, 1, cb.extra.n) }
        for i = 1, select("#", ...) do args[#args + 1] = (select(i, ...)) end
        if type(owner) == "number" then cb.func(unpack(args)) else cb.func(owner, unpack(args)) end
      elseif type(owner) == "number" then cb.func(...)
      else cb.func(owner, ...) end
    end
  end
  function CallbackRegistryMixin:HasRegistrantsForEvent(event)
    return self.callbackTables ~= nil and self.callbackTables[event] ~= nil and next(self.callbackTables[event]) ~= nil
  end
  rawset(env, "CallbackRegistryMixin", CallbackRegistryMixin)
  rawset(env, "CallbackRegistry", CallbackRegistryMixin)

  ------------------------------------------------------------ time & text formatting
  local function plural(n, word) return n .. " " .. word .. (n == 1 and "" or "s") end
  rawset(env, "SecondsToTime", function(seconds, noSeconds, notAbbreviated, maxCount, roundUp)
    seconds = math.max(0, math.floor(seconds))
    maxCount = maxCount or 2
    local units = notAbbreviated
      and { { 86400, "Day" }, { 3600, "Hour" }, { 60, "Minute" }, { 1, "Second" } }
      or { { 86400, "Day" }, { 3600, "Hr" }, { 60, "Min" }, { 1, "Sec" } }
    local out, count = {}, 0
    for i, u in ipairs(units) do
      if count >= maxCount then break end
      if not (noSeconds and i == 4) then
        local n = math.floor(seconds / u[1])
        if n > 0 then
          out[#out + 1] = notAbbreviated and plural(n, u[2]) or (n .. " " .. u[2])
          seconds = seconds - n * u[1]
          count = count + 1
        end
      end
    end
    if #out == 0 then return noSeconds and ("0 " .. (notAbbreviated and "Minutes" or "Min")) or ("0 " .. (notAbbreviated and "Seconds" or "Sec")) end
    return table.concat(out, " ")
  end)
  rawset(env, "SecondsToClock", function(seconds, displayZeroHours)
    seconds = math.max(0, math.floor(seconds))
    local h, m, s = math.floor(seconds / 3600), math.floor(seconds / 60) % 60, seconds % 60
    if h > 0 or displayZeroHours then return string.format("%d:%02d:%02d", h, m, s) end
    return string.format("%d:%02d", m, s)
  end)
  rawset(env, "SecondsToTimeAbbrev", function(seconds)
    if seconds >= 86400 then return "%d d", math.ceil(seconds / 86400) end
    if seconds >= 3600 then return "%d h", math.ceil(seconds / 3600) end
    if seconds >= 60 then return "%d m", math.ceil(seconds / 60) end
    return "%d s", math.floor(seconds)
  end)
  rawset(env, "FormatLargeNumber", env.BreakUpLargeNumbers)
  rawset(env, "FormatPercentage", function(p, round)
    return (round and string.format("%d%%", math.floor(p * 100 + 0.5)) or string.format("%.2f%%", p * 100))
  end)
  rawset(env, "CreateTextureMarkup", function(file, fw, fh, w, h, l, r, t, b)
    return string.format("|T%s:%d:%d:0:0:%d:%d:%d:%d:%d:%d|t", file, h or 0, w or 0, fw, fh, l * fw, r * fw, t * fh, b * fh)
  end)
  rawset(env, "CreateAtlasMarkup", function(atlas, w, h) return string.format("|A:%s:%d:%d|a", atlas, h or 0, w or 0) end)
  rawset(env, "StripHyperlinks", function(text) return (text:gsub("|H.-|h(.-)|h", "%1")) end)
  rawset(env, "GetClassColoredTextForUnit", function(unit, text)
    local _, cls = env.UnitClass(unit)
    local c = env.RAID_CLASS_COLORS[cls]
    return c and c:WrapTextInColorCode(text) or text
  end)
  rawset(env, "Round", function(v) return math.floor(v + 0.5) end)
  rawset(env, "GenerateClosure", function(fn, ...)
    local a = { n = select("#", ...), ... }
    return function(...)
      local b = { unpack(a, 1, a.n) }
      local n = a.n
      for i = 1, select("#", ...) do b[n + i] = (select(i, ...)) end
      return fn(unpack(b, 1, n + select("#", ...)))
    end
  end)
  rawset(env, "RunNextFrame", function(fn) env.C_Timer.After(0, fn) end)
  rawset(env, "tUnorderedRemove", function(t, i) t[i] = t[#t]; t[#t] = nil end)
  rawset(env, "SafePack", function(...) return { n = select("#", ...), ... } end)
  rawset(env, "SafeUnpack", function(t) return unpack(t, 1, t.n) end)
  rawset(env, "ExecuteFrameScript", function(frame, script, ...) sim:_runScript(frame, script, ...) end)

  ------------------------------------------------------------ Item / Spell mixins
  local ItemMixin = {}
  local function item(self) return self.itemID and sim.items[self.itemID] end
  function ItemMixin:GetItemID() return self.itemID end
  function ItemMixin:IsItemEmpty() return self.itemID == nil end
  function ItemMixin:IsItemDataCached() return item(self) ~= nil end
  function ItemMixin:GetItemName() local i = item(self); return i and i.name end
  function ItemMixin:GetItemLink() local i = item(self); return i and i.link end
  function ItemMixin:GetItemQuality() local i = item(self); return i and i.quality end
  function ItemMixin:GetItemIcon() local i = item(self); return i and (i.icon or 134400) end
  function ItemMixin:GetCurrentItemLevel() local i = item(self); return i and (i.itemLevel or 1) end
  function ItemMixin:GetItemQualityColor() local i = item(self); return i and env.ITEM_QUALITY_COLORS[i.quality] end
  function ItemMixin:ContinueOnItemLoad(fn)
    -- data for items registered with sim:AddItem is "cached"; others load next frame
    if item(self) then fn() else env.C_Timer.After(0, fn) end
  end
  function ItemMixin:ContinueWithCancelOnItemLoad(fn) self:ContinueOnItemLoad(fn); return function() end end
  local Item = {}
  function Item:CreateFromItemID(id) local o = Mixin({}, ItemMixin); o.itemID = id; return o end
  function Item:CreateFromItemLink(link) return Item:CreateFromItemID(tonumber(link:match("item:(%d+)"))) end
  function Item:CreateFromBagAndSlot(bag, slot)
    local b = sim.bags[bag]; local s = b and b[slot]
    return Item:CreateFromItemID(s and s.itemID)
  end
  rawset(env, "ItemMixin", ItemMixin)
  rawset(env, "Item", Item)

  local SpellMixin = {}
  local function spell(self) return self.spellID and sim.spells[self.spellID] end
  function SpellMixin:GetSpellID() return self.spellID end
  function SpellMixin:IsSpellEmpty() return self.spellID == nil end
  function SpellMixin:IsSpellDataCached() return spell(self) ~= nil end
  function SpellMixin:GetSpellName() local s = spell(self); return s and s.name end
  function SpellMixin:GetSpellTexture() local s = spell(self); return s and (s.icon or 136243) end
  function SpellMixin:GetSpellDescription() local s = spell(self); return s and s.description or "" end
  function SpellMixin:ContinueOnSpellLoad(fn) if spell(self) then fn() else env.C_Timer.After(0, fn) end end
  function SpellMixin:ContinueWithCancelOnSpellLoad(fn) self:ContinueOnSpellLoad(fn); return function() end end
  local Spell = {}
  function Spell:CreateFromSpellID(id) local o = Mixin({}, SpellMixin); o.spellID = id; return o end
  rawset(env, "SpellMixin", SpellMixin)
  rawset(env, "Spell", Spell)

  ------------------------------------------------------------ curves (C_CurveUtil)
  local function newCurve(color)
    local c = { points = {}, curveType = 0 }
    function c:GetType() return self.curveType end
    function c:SetType(t) self.curveType = t end
    function c:HasSecretValues() return false end
    function c:AddPoint(x, y)
      table.insert(self.points, { x = x, y = y })
      table.sort(self.points, function(a, b) return a.x < b.x end)
    end
    function c:ClearPoints() self.points = {} end
    function c:SetPoints(list) self.points = {}; for _, p in ipairs(list or {}) do self:AddPoint(p.x or p[1], p.y or p[2]) end end
    function c:GetPoints() return self.points end
    function c:GetPointCount() return #self.points end
    function c:GetPoint(i) return self.points[i] end
    function c:RemovePoint(i) table.remove(self.points, i) end
    function c:SetToDefaults() self.points = {} end
    function c:Copy() local n = newCurve(color); n:SetPoints(self.points); n.curveType = self.curveType; return n end
    local function lerp(a, b, t)
      if type(a) == "number" then return a + (b - a) * t end
      local r = env.CreateColor(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, (a.a or 1) + ((b.a or 1) - (a.a or 1)) * t)
      return r
    end
    function c:Evaluate(x)
      local pts = self.points
      if #pts == 0 then return color and env.CreateColor(1, 1, 1, 1) or 0 end
      if x <= pts[1].x then return pts[1].y end
      for i = 2, #pts do
        if x <= pts[i].x then
          local a, b = pts[i - 1], pts[i]
          if self.curveType == 1 then return a.y end -- step
          return lerp(a.y, b.y, (x - a.x) / ((b.x - a.x) ~= 0 and (b.x - a.x) or 1))
        end
      end
      return pts[#pts].y
    end
    function c:EvaluateUnpacked(x)
      local v = self:Evaluate(x)
      if type(v) == "table" then return v.r, v.g, v.b, v.a or 1 end
      return v
    end
    return c
  end
  local cu = rawget(env, "C_CurveUtil") or {}
  cu.CreateCurve = function() return newCurve(false) end
  cu.CreateColorCurve = function() return newCurve(true) end
  cu.EvaluateColorValueFromBoolean = function(state, a, b) if state then return a else return b end end
  cu.EvaluateColorFromBoolean = function(state, a, b) if state then return a else return b end end
  rawset(env, "C_CurveUtil", cu)

  -- TextureLoadingGroupMixin (FrameXML)
  local TLG = {}
  function TLG:AddTexture(key, tex) self.textures = self.textures or {}; self.textures[key] = tex end
  function TLG:RemoveTexture(key) if self.textures then self.textures[key] = nil end end
  function TLG:IsEmpty() return not self.textures or next(self.textures) == nil end
  function TLG:Reset() self.textures = {} end
  rawset(env, "TextureLoadingGroupMixin", TLG)

  ------------------------------------------------------------ ItemLocation
  local ItemLocationMixin = {}
  function ItemLocationMixin:Clear() self.bagID, self.slotIndex, self.equipmentSlotIndex, self.isBagAndSlot, self.isEquipmentSlot = nil end
  function ItemLocationMixin:SetBagAndSlot(bag, slot)
    self:Clear(); self.bagID, self.slotIndex, self.isBagAndSlot = bag, slot, true
  end
  function ItemLocationMixin:GetBagAndSlot() return self.bagID, self.slotIndex end
  function ItemLocationMixin:SetEquipmentSlot(slot)
    self:Clear(); self.equipmentSlotIndex, self.isEquipmentSlot = slot, true
  end
  function ItemLocationMixin:GetEquipmentSlot() return self.equipmentSlotIndex end
  function ItemLocationMixin:IsEquipmentSlot() return self.isEquipmentSlot or false end
  function ItemLocationMixin:IsBagAndSlot() return self.isBagAndSlot or false end
  function ItemLocationMixin:HasAnyLocation() return self:IsEquipmentSlot() or self:IsBagAndSlot() end
  function ItemLocationMixin:IsValid() return self:HasAnyLocation() end
  function ItemLocationMixin:IsEqualToBagAndSlot(bag, slot) return self.bagID == bag and self.slotIndex == slot end
  function ItemLocationMixin:IsEqualToEquipmentSlot(slot) return self.equipmentSlotIndex == slot end
  function ItemLocationMixin:IsEqualTo(o)
    return o and self.bagID == o.bagID and self.slotIndex == o.slotIndex and self.equipmentSlotIndex == o.equipmentSlotIndex
  end
  rawset(env, "ItemLocationMixin", ItemLocationMixin)
  rawset(env, "ItemLocation", {
    CreateEmpty = function() return Mixin({}, ItemLocationMixin) end,
    CreateFromBagAndSlot = function(_, bag, slot) local l = Mixin({}, ItemLocationMixin); l:SetBagAndSlot(bag, slot); return l end,
    CreateFromEquipmentSlot = function(_, slot) local l = Mixin({}, ItemLocationMixin); l:SetEquipmentSlot(slot); return l end,
  })

  ------------------------------------------------------------ UI panels
  rawset(env, "UIPanelWindows", {})
  rawset(env, "ShowUIPanel", function(f) if f then f:Show() end end)
  rawset(env, "HideUIPanel", function(f) if f then f:Hide() end end)
  rawset(env, "ToggleFrame", function(f) if f:IsShown() then f:Hide() else f:Show() end end)
  rawset(env, "GetUIPanel", function() return nil end)

  ------------------------------------------------------------ menus (MenuUtil) and dropdowns
  local function newDescription(kind, text, data)
    local d = { kind = kind, text = text, data = data, children = {} }
    local function add(self, child) table.insert(self.children, child); return child end
    function d:CreateButton(t, cb, dt) local c = newDescription("button", t, dt); c.callback = cb; return add(self, c) end
    function d:CreateCheckbox(t, isSel, setSel, dt)
      local c = newDescription("checkbox", t, dt); c.isSelected = isSel; c.callback = setSel; return add(self, c)
    end
    function d:CreateRadio(t, isSel, setSel, dt)
      local c = newDescription("radio", t, dt); c.isSelected = isSel; c.callback = setSel; return add(self, c)
    end
    function d:CreateTitle(t) return add(self, newDescription("title", t)) end
    function d:CreateDivider() return add(self, newDescription("divider")) end
    function d:CreateSpacer() return add(self, newDescription("spacer")) end
    function d:CreateTemplate() return add(self, newDescription("template")) end
    function d:SetTag(t) self.tag = t end
    function d:SetTooltip(fn) self.tooltip = fn end
    function d:SetEnabled(v) self.disabled = not v end
    function d:IsEnabled() return not self.disabled end
    function d:SetResponse(r) self.response = r end
    function d:SetScrollMode(h) self.scrollHeight = h end
    function d:SetGridMode() end
    function d:AddInitializer(fn) self.initializers = self.initializers or {}; table.insert(self.initializers, fn) end
    function d:SetSelectionIgnored() end
    function d:SetOnEnter(fn) self.onEnter = fn end
    function d:SetOnLeave(fn) self.onLeave = fn end
    function d:GetData() return self.data end
    function d:SetData(v) self.data = v end
    function d:IsSelected() return self.isSelected and self.isSelected(self.data) or false end
    return d
  end

  local function openMenu(owner, generator, kind)
    local root = newDescription("root")
    generator(owner, root)
    local menu = { owner = owner, root = root, kind = kind }
    sim.openMenu = menu
    table.insert(sim.menus, menu)
    return menu
  end
  rawset(env, "MenuUtil", {
    CreateContextMenu = function(owner, generator) return openMenu(owner, generator, "context") end,
    CreateButtonMenu = function(parent, ...)
      local items = { ... }
      return openMenu(parent, function(_, root)
        for _, e in ipairs(items) do root:CreateButton(e[1], e[2], e[3]) end
      end, "button")
    end,
    CreateRootMenuDescription = function() return newDescription("root") end,
    SetElementText = function(d, t) d.text = t end,
    GetElementText = function(d) return d.text end,
    HookTooltipScripts = function() end,
    ShowTooltip = function() end,
    HideTooltip = function() end,
  })
  rawset(env, "MenuResponse", { Open = 1, Refresh = 2, Close = 3, CloseAll = 4 })
  rawset(env, "Menu", {
    ModifyMenu = function(tag, fn)
      sim.menuModifiers[tag] = sim.menuModifiers[tag] or {}
      table.insert(sim.menuModifiers[tag], fn)
    end,
    GetOpenMenu = function() return sim.openMenu end,
    GetManager = function() return { HandleESC = function() sim.openMenu = nil end } end,
  })

  -- DropdownButton (WowStyle1DropdownTemplate & friends)
  local DropdownMixin = {}
  function DropdownMixin:SetupMenu(generator) self.menuGenerator = generator; self:GenerateMenu() end
  function DropdownMixin:GenerateMenu()
    if not self.menuGenerator then return end
    local root = newDescription("root")
    self.menuGenerator(self, root)
    self.menuDescription = root
    -- selected text for radio/checkbox menus
    local selected
    local function walk(d) for _, c in ipairs(d.children) do
      if (c.kind == "radio") and c:IsSelected() then selected = c.text end
      walk(c)
    end end
    walk(root)
    if selected then self:OverrideText(selected) elseif self.defaultText then self:OverrideText(self.defaultText) end
  end
  function DropdownMixin:SetDefaultText(t) self.defaultText = t; if not self.selectionText then self:OverrideText(t) end end
  function DropdownMixin:OverrideText(t) self.selectionText = t; if self.Text then self.Text:SetText(t) end end
  function DropdownMixin:GetText() return self.selectionText end
  function DropdownMixin:SetSelectionText(fn) self.selectionTextFunc = fn end
  function DropdownMixin:OpenMenu()
    if not self.menuGenerator then return end
    local menu = openMenu(self, self.menuGenerator, "dropdown")
    self.menuDescription = menu.root
    return menu
  end
  function DropdownMixin:CloseMenu() if sim.openMenu and sim.openMenu.owner == self then sim.openMenu = nil end end
  function DropdownMixin:IsMenuOpen() return sim.openMenu ~= nil and sim.openMenu.owner == self end
  function DropdownMixin:SetMenuOpen(v) if v then self:OpenMenu() else self:CloseMenu() end end
  function DropdownMixin:Update() self:GenerateMenu() end
  function DropdownMixin:SignalUpdate() self:GenerateMenu() end
  function DropdownMixin:SetMenuAnchor() end
  function DropdownMixin:SetDefaultCallback(fn) self.defaultCallback = fn end
  rawset(env, "DropdownButtonMixin", DropdownMixin)
  rawset(env, "WowStyle1DropdownMixin", DropdownMixin)
  for _, n in ipairs({ "WowStyle1DropdownTemplate", "WowStyle2DropdownTemplate", "WowStyle1FilterDropdownTemplate",
    "WowStyle1ArrowDropdownTemplate", "WowStyleDropdownTemplate", "DropdownButtonTemplate" }) do
    sim.templates[n] = { builtin = function(s, obj, create)
      Mixin(obj, DropdownMixin)
      local st = s.widgetState[obj]
      st.width, st.height = 150, 22
      st.chrome = "panel"
      obj.Text = create(s, "FontString", nil, obj)
      obj.Text:SetFontObject("GameFontHighlightSmall")
      obj.Text:SetPoint("LEFT", 8, 0)
      st.fontString = obj.Text
      obj:SetScript("OnMouseDown", function(self) if self:IsMenuOpen() then self:CloseMenu() else self:OpenMenu() end end)
    end }
  end
  sim.frameTypes.dropdownbutton = "Button"

  ------------------------------------------------------------ misc FrameXML
  local function pixelScale() return 768 / 1080 end
  rawset(env, "PixelUtil", {
    GetPixelToUIUnitFactor = function() return pixelScale() end,
    GetNearestPixelSize = function(size, scale, minPixels)
      if size == 0 and (not minPixels or minPixels == 0) then return 0 end
      local unit = pixelScale() / (scale or 1)
      local n = math.floor(size / unit + 0.5)
      if minPixels and n < minPixels then n = minPixels end
      return n * unit
    end,
    ConvertPixelsToUI = function(px, scale) return px * pixelScale() / (scale or 1) end,
    ConvertPixelsToUIForRegion = function(px, region) return px * pixelScale() / region:GetEffectiveScale() end,
    SetWidth = function(region, w) region:SetWidth(w) end,
    SetHeight = function(region, h) region:SetHeight(h) end,
    SetSize = function(region, w, h) region:SetSize(w, h) end,
    SetPoint = function(region, point, rel, relPoint, x, y) region:SetPoint(point, rel, relPoint, x, y) end,
    SetStatusBarValue = function(bar, v) bar:SetValue(v) end,
  })
  rawset(env, "MouseIsOver", function(region, top, bottom, left, right) return region:IsMouseOver(top, bottom, left, right) end)
  rawset(env, "EditBox_ClearFocus", function(eb) eb:ClearFocus() end)
  rawset(env, "EditBox_HighlightText", function(eb) eb:HighlightText() end)
  rawset(env, "DoesTemplateExist", function(name) return sim.templates[name] ~= nil end)
  rawset(env, "C_XMLUtil", { GetTemplateInfo = function(name)
    local t = sim.templates[name]
    if t then return { type = t.type or "Frame", width = 0, height = 0, keyValues = {}, inherits = nil } end
  end })
  local tc = {}
  local order = { "WARRIOR", "MAGE", "ROGUE", "DRUID", "HUNTER", "SHAMAN", "PRIEST", "WARLOCK", "PALADIN",
    "DEATHKNIGHT", "MONK", "DEMONHUNTER", "EVOKER" }
  for i, c in ipairs(order) do
    local col, row = (i - 1) % 4, math.floor((i - 1) / 4)
    tc[c] = { col * 0.25, (col + 1) * 0.25, row * 0.25, (row + 1) * 0.25 }
  end
  rawset(env, "CLASS_ICON_TCOORDS", tc)
  rawset(env, "PLAYER_FACTION_GROUP", { [0] = "Horde", [1] = "Alliance", Horde = 0, Alliance = 1 })
  rawset(env, "FACTION_LABELS", { [0] = "Horde", [1] = "Alliance" })
  rawset(env, "MAX_PARTY_MEMBERS", 4)
  rawset(env, "MAX_RAID_MEMBERS", 40)
  rawset(env, "MAX_PLAYER_LEVEL", 80)
  rawset(env, "NUM_BAG_FRAMES", 4)
  rawset(env, "NUM_CHAT_WINDOWS", 10)
  rawset(env, "FCF_GetCurrentChatFrame", function() return env.DEFAULT_CHAT_FRAME end)
  rawset(env, "FCF_SetWindowName", function(frame, name) sim.widgetState[frame].chatName = name end)
  rawset(env, "FCF_Close", function(frame) frame:Hide() end)
  rawset(env, "GetChatWindowInfo", function(i)
    if i == 1 then return "General", 14, 1, 1, 1, 1, true, false, 1, false end
  end)
  rawset(env, "ChatFrame_AddMessageGroup", function() end)
  rawset(env, "ChatFrame_RemoveAllMessageGroups", function() end)

  -- number abbreviation (C_StringUtil helpers from FrameXML)
  local sutil = rawget(env, "C_StringUtil") or {}
  sutil.GetDefaultAbbreviationBreakpoints = function()
    local out = {}
    for _, b in ipairs({ { 1e12, "T" }, { 1e9, "B" }, { 1e6, "M" }, { 1e3, "K" } }) do
      out[#out + 1] = { breakpoint = b[1], abbreviation = b[2], significandDivisor = b[1] / 10, fractionDivisor = 10, abbreviationIsGlobal = false }
    end
    return out
  end
  rawset(env, "C_StringUtil", sutil)

  ------------------------------------------------------------ combat log
  rawset(env, "CombatLogGetCurrentEventInfo", function()
    local e = sim.currentCombatLogEvent
    if not e then return end
    return unpack(e, 1, e.n)
  end)
  rawset(env, "CombatLog_Object_IsA", function(flags, mask) return (require("wowapi.compat").bit.band(flags or 0, mask or 0)) ~= 0 end)
  for k, v in pairs({
    COMBATLOG_OBJECT_AFFILIATION_MINE = 0x1, COMBATLOG_OBJECT_AFFILIATION_PARTY = 0x2,
    COMBATLOG_OBJECT_AFFILIATION_RAID = 0x4, COMBATLOG_OBJECT_AFFILIATION_OUTSIDER = 0x8,
    COMBATLOG_OBJECT_REACTION_FRIENDLY = 0x10, COMBATLOG_OBJECT_REACTION_NEUTRAL = 0x20,
    COMBATLOG_OBJECT_REACTION_HOSTILE = 0x40, COMBATLOG_OBJECT_CONTROL_PLAYER = 0x100,
    COMBATLOG_OBJECT_CONTROL_NPC = 0x200, COMBATLOG_OBJECT_TYPE_PLAYER = 0x400, COMBATLOG_OBJECT_TYPE_NPC = 0x800,
    COMBATLOG_OBJECT_TYPE_PET = 0x1000, COMBATLOG_OBJECT_TYPE_GUARDIAN = 0x2000, COMBATLOG_OBJECT_TARGET = 0x10000,
    COMBATLOG_OBJECT_FOCUS = 0x20000, COMBATLOG_OBJECT_NONE = 0x80000000,
    COMBATLOG_FILTER_ME = 0x511, COMBATLOG_FILTER_MINE = 0x511, COMBATLOG_FILTER_HOSTILE_UNITS = 0x848,
    SCHOOL_MASK_PHYSICAL = 1, SCHOOL_MASK_HOLY = 2, SCHOOL_MASK_FIRE = 4, SCHOOL_MASK_NATURE = 8,
    SCHOOL_MASK_FROST = 16, SCHOOL_MASK_SHADOW = 32, SCHOOL_MASK_ARCANE = 64,
  }) do rawset(env, k, v) end
end

------------------------------------------------------------------ Sim methods

local Sim = {}

-- Inspect / use the open menu (MenuUtil context menus and dropdowns).
-- Returns a flat list of { text, kind, selected }.
function Sim:MenuItems(menuOrOwner)
  local menu = menuOrOwner or self.openMenu
  local root = menu and (menu.root or menu.menuDescription)
  if not root then return {} end
  local out = {}
  local function walk(d, depth)
    for _, c in ipairs(d.children) do
      out[#out + 1] = { text = c.text, kind = c.kind, selected = c.isSelected and c.isSelected(c.data) or false,
        depth = depth, desc = c }
      walk(c, depth + 1)
    end
  end
  walk(root, 0)
  return out
end

-- Pick a menu entry by its text (searches submenus too).
function Sim:ChooseMenuItem(text, menuOrOwner)
  for _, e in ipairs(self:MenuItems(menuOrOwner)) do
    if e.text == text then
      local d = e.desc
      if d.disabled then return false end
      if d.callback then self:_pcall(d.callback, d.data, { buttonName = "LeftButton" }) end
      local owner = (menuOrOwner and menuOrOwner.GenerateMenu and menuOrOwner) or (self.openMenu and self.openMenu.owner)
      if owner and owner.GenerateMenu then owner:GenerateMenu() end
      if d.kind ~= "checkbox" then self.openMenu = nil end
      return true
    end
  end
  error("no menu item '" .. tostring(text) .. "'", 2)
end

-- Fire a combat log event. Either raw args after the subevent, or a table:
--   sim:CombatLog("SPELL_DAMAGE", { source = "player", dest = "target",
--     spellId = 133, spellName = "Fireball", school = 4, amount = 1200, critical = true })
local PREFIX = { SWING = 0, RANGE = 3, SPELL = 3, SPELL_PERIODIC = 3, SPELL_BUILDING = 3, ENVIRONMENTAL = 1 }
function Sim:CombatLog(subevent, info, ...)
  local function unitInfo(u)
    if type(u) == "table" then return u.guid or "", u.name or "", u.flags or 0, u.raidFlags or 0 end
    local x = u and self.units[u]
    if not x then return "", nil, 0x80000000, 0 end
    local flags = (u == "player") and 0x511 or (x.hostile and 0xa48 or 0x514)
    return x.guid or "", x.name, flags, 0
  end
  local args
  if type(info) ~= "table" then
    args = { self.epoch + self.time, subevent, false, info, ... }
  else
    local sg, sn, sf, srf = unitInfo(info.source)
    local dg, dn, df, drf = unitInfo(info.dest)
    args = { self.epoch + self.time, subevent, info.hideCaster or false, sg, sn, sf, srf, dg, dn, df, drf }
    local prefix
    for p in pairs(PREFIX) do
      if subevent:sub(1, #p + 1) == p .. "_" and (not prefix or #p > #prefix) then prefix = p end
    end
    if prefix == "SPELL" or prefix == "SPELL_PERIODIC" or prefix == "RANGE" or prefix == "SPELL_BUILDING" then
      local sp = info.spellId and self.spells[info.spellId]
      table.insert(args, info.spellId or 0)
      table.insert(args, info.spellName or (sp and sp.name) or "")
      table.insert(args, info.school or 1)
    elseif prefix == "ENVIRONMENTAL" then
      table.insert(args, info.environmentalType or "Falling")
    end
    local suffix = prefix and subevent:sub(#prefix + 2) or subevent
    if suffix == "DAMAGE" or subevent == "DAMAGE_SHIELD" or subevent == "DAMAGE_SPLIT" then
      for _, v in ipairs({ info.amount or 0, info.overkill or -1, info.school or 1, info.resisted or 0,
        info.blocked or 0, info.absorbed or 0, info.critical or false, info.glancing or false,
        info.crushing or false, info.isOffHand or false }) do table.insert(args, v) end
    elseif suffix == "HEAL" then
      for _, v in ipairs({ info.amount or 0, info.overhealing or 0, info.absorbed or 0, info.critical or false }) do table.insert(args, v) end
    elseif suffix == "MISSED" then
      for _, v in ipairs({ info.missType or "MISS", info.isOffHand or false, info.amountMissed or 0, info.critical or false }) do table.insert(args, v) end
    elseif suffix:match("^AURA_") then
      table.insert(args, info.auraType or "BUFF")
      if info.amount then table.insert(args, info.amount) end
    elseif suffix == "ENERGIZE" then
      for _, v in ipairs({ info.amount or 0, info.overEnergize or 0, info.powerType or 0, info.maxPower or 0 }) do table.insert(args, v) end
    elseif suffix == "INTERRUPT" or suffix == "DISPEL" or suffix == "STOLEN" then
      for _, v in ipairs({ info.extraSpellId or 0, info.extraSpellName or "", info.extraSchool or 1 }) do table.insert(args, v) end
      if info.auraType then table.insert(args, info.auraType) end
    elseif subevent == "UNIT_DIED" or subevent == "UNIT_DESTROYED" then
      table.insert(args, info.recapID or 0)
      table.insert(args, info.unconsciousOnDeath or false)
    end
  end
  args.n = #args
  for i = 1, 30 do if args[i] ~= nil and i > args.n then args.n = i end end
  self.currentCombatLogEvent = args
  self:FireEvent("COMBAT_LOG_EVENT_UNFILTERED")
  self.currentCombatLogEvent = nil
end

M.SimMethods = Sim
return M
