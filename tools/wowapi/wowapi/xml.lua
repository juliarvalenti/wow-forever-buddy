-- FrameXML support: parses addon .xml files and builds the frames they
-- declare, like the client does.
--
-- Supported: <Script>/<Include>, every frame type, virtual templates and
-- inherits (usable from CreateFrame too), intrinsic frames, mixin/secureMixin,
-- parentKey/parentArray, $parent names, <Size>, <Anchors> (relativeTo,
-- relativeKey), <Layers> with Texture/FontString/MaskTexture/Line,
-- <Frames>, <Scripts> (inline, function=, method=, inherit=prepend/append),
-- <KeyValues>, <Attributes>, <Animations>, <Font> objects, button textures
-- and fonts, StatusBar/Slider/EditBox/ScrollFrame specifics, and Bindings.xml.
local M = {}

------------------------------------------------------------------ parser

local ENT = { lt = "<", gt = ">", amp = "&", quot = '"', apos = "'" }
local function unescape(s)
  return (s:gsub("&(#?x?)(%w+);", function(pre, v)
    if pre == "#" then return string.char(tonumber(v)) end
    if pre == "#x" then return string.char(tonumber(v, 16)) end
    return ENT[v] or ("&" .. v .. ";")
  end))
end

-- Returns the root node: { tag, attrs = {}, children = {}, text, line }
function M.parse(text, filename)
  text = text:gsub("^\239\187\191", "")
  local root = { tag = "#document", attrs = {}, children = {} }
  local stack = { root }
  local pos = 1
  local len = #text
  local function lineAt(p)
    local _, n = text:sub(1, p):gsub("\n", "")
    return n + 1
  end
  local function fail(msg, p)
    error(string.format("%s:%d: XML error: %s", filename or "?", lineAt(p or pos), msg), 0)
  end
  while pos <= len do
    local lt = text:find("<", pos, true)
    local chunk = text:sub(pos, (lt or len + 1) - 1)
    if chunk:find("%S") then
      local top = stack[#stack]
      top.text = (top.text or "") .. unescape(chunk)
    end
    if not lt then break end
    if text:sub(lt, lt + 3) == "<!--" then
      local e = text:find("-->", lt + 4, true) or fail("unterminated comment", lt)
      pos = e + 3
    elseif text:sub(lt, lt + 8) == "<![CDATA[" then
      local e = text:find("]]>", lt + 9, true) or fail("unterminated CDATA", lt)
      local top = stack[#stack]
      top.text = (top.text or "") .. text:sub(lt + 9, e - 1)
      pos = e + 3
    elseif text:sub(lt, lt + 1) == "<?" then
      local e = text:find("?>", lt + 2, true) or fail("unterminated declaration", lt)
      pos = e + 2
    elseif text:sub(lt, lt + 1) == "<!" then
      local e = text:find(">", lt + 2, true) or fail("unterminated declaration", lt)
      pos = e + 1
    elseif text:sub(lt, lt + 1) == "</" then
      local name, e = text:match("^</%s*([%w_:%.%-]+)%s*>()", lt)
      if not name then fail("malformed closing tag", lt) end
      local top = stack[#stack]
      if top.tag ~= name then fail("expected </" .. tostring(top.tag) .. "> but found </" .. name .. ">", lt) end
      stack[#stack] = nil
      pos = e
    else
      local name, p = text:match("^<([%w_:%.%-]+)()", lt)
      if not name then fail("malformed tag", lt) end
      local node = { tag = name:gsub("^.*:", ""), attrs = {}, children = {}, line = lineAt(lt) }
      while true do
        local ws = text:match("^%s*()", p)
        p = ws
        if text:sub(p, p + 1) == "/>" then p = p + 2; break end
        if text:sub(p, p) == ">" then p = p + 1; stack[#stack + 1] = node; break end
        local key, q, val, e2 = text:match('^([%w_:%.%-]+)%s*=%s*(["\'])(.-)%2()', p)
        if not key then fail("malformed attribute in <" .. name .. ">", p) end
        node.attrs[key:gsub("^.*:", "")] = unescape(val)
        p = e2
      end
      local parent = stack[#stack] == node and stack[#stack - 1] or stack[#stack]
      table.insert(parent.children, node)
      pos = p
    end
  end
  if #stack > 1 then fail("unclosed <" .. stack[#stack].tag .. ">", len) end
  return root
end

local function kids(node, tag)
  local out = {}
  for _, c in ipairs(node.children) do if not tag or c.tag == tag then out[#out + 1] = c end end
  return out
end
local function kid(node, tag) return kids(node, tag)[1] end
M.kids, M.kid = kids, kid

local function bool(v) return v == "true" or v == "1" end
local function numattr(v) return v and tonumber(v) end

------------------------------------------------------------------ builder

local FRAME_TAGS -- filled per sim from widget classes
local REGION_TAGS = { Texture = true, FontString = true, MaskTexture = true, Line = true }

-- Standard argument names for inline script bodies.
local SCRIPT_ARGS = {
  OnEvent = "self, event, ...", OnUpdate = "self, elapsed", OnClick = "self, button, down",
  PreClick = "self, button, down", PostClick = "self, button, down", OnDoubleClick = "self, button",
  OnEnter = "self, motion", OnLeave = "self, motion", OnMouseDown = "self, button", OnMouseUp = "self, button, upInside",
  OnMouseWheel = "self, delta", OnDragStart = "self, button", OnDragStop = "self",
  OnKeyDown = "self, key", OnKeyUp = "self, key", OnChar = "self, text",
  OnValueChanged = "self, value, userInput", OnMinMaxChanged = "self, min, max",
  OnTextChanged = "self, userInput", OnSizeChanged = "self, width, height",
  OnAttributeChanged = "self, name, value", OnTooltipSetItem = "self", OnHyperlinkClick = "self, link, text, button",
  OnHyperlinkEnter = "self, link, text", OnHyperlinkLeave = "self", OnVerticalScroll = "self, offset",
  OnHorizontalScroll = "self, offset", OnScrollRangeChanged = "self, xrange, yrange",
  OnLoop = "self, loopState", OnFinished = "self, requested", OnStop = "self, requested",
  OnCooldownDone = "self", OnEditFocusGained = "self", OnEditFocusLost = "self",
  OnEnterPressed = "self", OnEscapePressed = "self", OnTabPressed = "self", OnSpacePressed = "self",
  OnArrowPressed = "self, key", OnCursorChanged = "self, x, y, width, height",
}

local function newCtx(sim, file)
  file = file or "?"
  return { sim = sim, file = file, dir = (file:match("^(.*)[/\\]") or "."), anchors = {}, onloads = {} }
end

local function resolveName(name, parent, sim)
  if not name then return nil end
  if name:find("%$[Pp]arent") then
    local pname = parent and sim.widgetState[parent] and sim.widgetState[parent].name or ""
    name = name:gsub("%$[Pp]arent", pname)
  end
  return name
end

local function compileScript(ctx, node, script, body)
  local args = SCRIPT_ARGS[script] or "self, ..."
  local src = "return function(" .. args .. ")\n" .. body .. "\nend"
  local chunkname = "@" .. ctx.file .. ":" .. (node.line or 0) .. " (" .. script .. ")"
  local compat = require("wowapi.compat")
  -- pad with newlines so error line numbers match the XML file
  local fn, err = compat.loadstring(string.rep("\n", math.max(0, (node.line or 1) - 2)) .. src, "@" .. ctx.file, ctx.sim.env)
  if not fn then ctx.sim:_error(err); return nil end
  local ok, f = pcall(fn)
  if not ok then ctx.sim:_error(tostring(f)); return nil end
  return f
end

local function setScripts(ctx, obj, scriptsNode)
  local sim = ctx.sim
  for _, sn in ipairs(scriptsNode.children) do
    local script = sn.tag
    local fn
    if sn.attrs["function"] then
      local fname = sn.attrs["function"]
      fn = sim:Get(fname)
      if type(fn) ~= "function" then
        -- may be defined later by a Lua file; look it up when the script runs
        fn = function(...)
          local f = sim:Get(fname)
          if type(f) ~= "function" then error("XML script function '" .. fname .. "' is not defined", 2) end
          return f(...)
        end
      end
    elseif sn.attrs.method then
      local m = sn.attrs.method
      fn = function(self, ...)
        local f = self[m]
        if type(f) ~= "function" then error("XML script method '" .. m .. "' not found on " .. tostring(self:GetDebugName()), 2) end
        return f(self, ...)
      end
    elseif sn.text and sn.text:find("%S") then
      fn = compileScript(ctx, sn, script, sn.text)
    end
    if fn then
      local inherit = sn.attrs.inherit
      local existing = obj:GetScript(script)
      if existing and inherit == "prepend" then
        local old = existing
        obj:SetScript(script, function(...) fn(...); return old(...) end)
      elseif existing and inherit == "append" then
        local old = existing
        obj:SetScript(script, function(...) old(...); return fn(...) end)
      else
        obj:SetScript(script, fn)
      end
      if script == "OnLoad" then sim.widgetState[obj].hasOnLoad = true end
    end
  end
end

local function parseDim(node)
  if not node then return nil end
  local abs = kid(node, "AbsDimension")
  local x = numattr(node.attrs.x) or (abs and numattr(abs.attrs.x))
  local y = numattr(node.attrs.y) or (abs and numattr(abs.attrs.y))
  return x, y
end

local function colorOf(node)
  if not node then return nil end
  if node.attrs.color then
    local c = M._sim:Get(node.attrs.color)
    if c and c.r then return c.r, c.g, c.b, c.a or 1 end
  end
  return tonumber(node.attrs.r) or 0, tonumber(node.attrs.g) or 0, tonumber(node.attrs.b) or 0, tonumber(node.attrs.a) or 1
end

local function queueAnchors(ctx, obj, node, parent)
  local anchors = kid(node, "Anchors")
  if anchors then
    for _, a in ipairs(kids(anchors, "Anchor")) do
      table.insert(ctx.anchors, { obj = obj, node = a, parent = parent })
    end
  end
  if bool(node.attrs.setAllPoints) then
    table.insert(ctx.anchors, { obj = obj, all = true, parent = parent })
  end
end

local function resolveRelative(ctx, entry)
  local sim, a = ctx.sim, entry.node.attrs
  local parent = entry.obj:GetParent() or entry.parent
  if a.relativeKey then
    local t = entry.obj
    for part in a.relativeKey:gmatch("[^.]+") do
      if part == "$parent" then t = t and (t.GetParent and t:GetParent() or nil)
      else t = t and t[part] end
    end
    if not t then sim:_error(string.format("%s: Couldn't find relativeKey '%s'", ctx.file, a.relativeKey)) end
    return t
  end
  if a.relativeTo then
    if a.relativeTo == "$parent" then return parent end
    local name = resolveName(a.relativeTo, parent, sim)
    local t = sim:Get(name)
    if not t then sim:_error(string.format("%s: Couldn't find relativeTo '%s'", ctx.file, name)) end
    return t
  end
  return nil
end

local function applyAnchors(ctx)
  local sim = ctx.sim
  for _, entry in ipairs(ctx.anchors) do
    if entry.all then
      entry.obj:SetAllPoints()
    else
      local a = entry.node.attrs
      local rel = resolveRelative(ctx, entry)
      local offset = kid(entry.node, "Offset")
      local ox, oy = parseDim(offset)
      local x = numattr(a.x) or ox or 0
      local y = numattr(a.y) or oy or 0
      local ok, err = pcall(entry.obj.SetPoint, entry.obj, a.point or "TOPLEFT", rel, a.relativePoint or a.point or "TOPLEFT", x, y)
      if not ok then sim:_error(ctx.file .. ": " .. tostring(err)) end
    end
  end
  ctx.anchors = {}
end

local buildFrameContents

local function applyKeyValues(ctx, obj, node)
  local kv = kid(node, "KeyValues")
  if not kv then return end
  for _, k in ipairs(kids(kv, "KeyValue")) do
    local key, value, typ = k.attrs.key, k.attrs.value, k.attrs.type or "string"
    local v = value
    if typ == "number" then v = tonumber(value)
    elseif typ == "boolean" then v = value == "true"
    elseif typ == "global" then v = ctx.sim:Get(value)
    elseif typ == "nil" then v = nil end
    if k.attrs.keyType == "number" then key = tonumber(key) end
    obj[key] = v
  end
end

local function applyAttributes(ctx, obj, node)
  local at = kid(node, "Attributes")
  if not at then return end
  for _, a in ipairs(kids(at, "Attribute")) do
    local v, typ = a.attrs.value, a.attrs.type or "string"
    if typ == "number" then v = tonumber(v) elseif typ == "boolean" then v = v == "true"
    elseif typ == "global" then v = ctx.sim:Get(v) end
    ctx.sim.widgetState[obj].attributes[a.attrs.name] = v
  end
end

local function attachKey(obj, parent, node)
  if not parent then return end
  if node.attrs.parentKey then
    parent[node.attrs.parentKey] = obj
    local s = M._sim.widgetState[obj]
    if s then s.parentKey = node.attrs.parentKey end
  end
  if node.attrs.parentArray then
    local k = node.attrs.parentArray
    parent[k] = parent[k] or {}
    table.insert(parent[k], obj)
  end
end

-- Textures, FontStrings and friends in <Layers> or button slots.
local function buildRegion(ctx, node, parent, layer, sublevel, existing)
  local sim = ctx.sim
  local tag = node.tag
  local name = resolveName(node.attrs.name, parent, sim)
  local obj = existing
  if not obj then
    local create = require("wowapi.widgets").create
    obj = create(sim, tag, name, parent)
  end
  local s = sim.widgetState[obj]
  s.layer = layer or s.layer or "ARTWORK"
  s.sublevel = sublevel or s.sublevel or 0
  attachKey(obj, parent, node)
  -- templates for regions: font objects for FontStrings, XML virtual regions otherwise
  if node.attrs.inherits then
    for _, t in ipairs(require("wowapi.templates").split(node.attrs.inherits)) do
      local tmpl = sim.templates[t]
      if tag == "FontString" and not tmpl then
        local fo = sim:Get(t)
        if fo then obj:SetFontObject(fo) else sim:_error(ctx.file .. ': Couldn\'t find inherited node "' .. t .. '"') end
      elseif tmpl and tmpl.xml then
        M.applyTemplate(sim, obj, tmpl, ctx)
      elseif not tmpl then
        sim:_error(ctx.file .. ': Couldn\'t find inherited node "' .. t .. '"')
      end
    end
  end
  local a = node.attrs
  if a.hidden then obj:SetShown(not bool(a.hidden)) end
  if a.alpha then obj:SetAlpha(tonumber(a.alpha)) end
  if tag == "Texture" or tag == "MaskTexture" or tag == "Line" then
    if a.file then obj:SetTexture(tonumber(a.file) or a.file) end
    if a.atlas then
      obj:SetAtlas(a.atlas, bool(a.useAtlasSize))
    end
    if a.alphaMode then obj:SetBlendMode(a.alphaMode) end
    local c = kid(node, "Color")
    if c then obj:SetColorTexture(colorOf(c)) end
    local tc = kid(node, "TexCoords")
    if tc then obj:SetTexCoord(tonumber(tc.attrs.left) or 0, tonumber(tc.attrs.right) or 1, tonumber(tc.attrs.top) or 0, tonumber(tc.attrs.bottom) or 1) end
  elseif tag == "FontString" then
    if a.font then s.font = { a.font, tonumber((kid(node, "FontHeight") or { attrs = {} }).attrs.val) or 12, a.outline or "" } end
    if a.text then
      local t = a.text
      local g = sim:Get(t)
      obj:SetText(type(g) == "string" and g or t)
    end
    if a.justifyH then obj:SetJustifyH(a.justifyH) end
    if a.justifyV then obj:SetJustifyV(a.justifyV) end
    if a.maxLines then s.props = s.props or {}; s.props.MaxLines = { n = 1, tonumber(a.maxLines) } end
    local c = kid(node, "Color")
    if c then obj:SetTextColor(colorOf(c)) end
  end
  local w, h = parseDim(kid(node, "Size"))
  if w then s.width = w end
  if h then s.height = h end
  queueAnchors(ctx, obj, node, parent)
  applyKeyValues(ctx, obj, node)
  local anims = kid(node, "Animations")
  if anims then M.buildAnimations(ctx, obj, anims) end
  local scripts = kid(node, "Scripts")
  if scripts then setScripts(ctx, obj, scripts) end
  return obj
end

function M.buildAnimations(ctx, region, node)
  local sim = ctx.sim
  for _, gn in ipairs(kids(node, "AnimationGroup")) do
    local g = region:CreateAnimationGroup(resolveName(gn.attrs.name, region, sim), gn.attrs.inherits)
    attachKey(g, region, gn)
    if gn.attrs.looping then g:SetLooping(gn.attrs.looping) end
    if gn.attrs.setToFinalAlpha then g:SetToFinalAlpha(bool(gn.attrs.setToFinalAlpha)) end
    for _, an in ipairs(gn.children) do
      if an.tag ~= "Scripts" and an.tag ~= "KeyValues" then
        local ok, anim = pcall(g.CreateAnimation, g, an.tag, resolveName(an.attrs.name, region, sim), an.attrs.inherits)
        if not ok then sim:_error(ctx.file .. ":" .. (an.line or 0) .. ": " .. tostring(anim))
        else
          local a = an.attrs
          attachKey(anim, g, an)
          if a.duration then anim:SetDuration(tonumber(a.duration)) end
          if a.order then anim:SetOrder(tonumber(a.order)) end
          if a.startDelay then anim:SetStartDelay(tonumber(a.startDelay)) end
          if a.endDelay then anim:SetEndDelay(tonumber(a.endDelay)) end
          if a.smoothing then anim:SetSmoothing(a.smoothing) end
          if a.childKey then anim:SetChildKey(a.childKey) end
          if a.targetKey then anim:SetTargetKey(a.targetKey) end
          if a.target then anim:SetTargetName(resolveName(a.target, region, sim)) end
          if an.tag == "Alpha" then
            if a.fromAlpha then anim:SetFromAlpha(tonumber(a.fromAlpha)) end
            if a.toAlpha then anim:SetToAlpha(tonumber(a.toAlpha)) end
            if a.change then anim:SetFromAlpha(region:GetAlpha()); anim:SetToAlpha(region:GetAlpha() + tonumber(a.change)) end
          elseif an.tag == "Translation" or an.tag == "LineTranslation" then
            anim:SetOffset(tonumber(a.offsetX) or 0, tonumber(a.offsetY) or 0)
          elseif an.tag == "Scale" or an.tag == "LineScale" then
            if a.scaleX or a.scaleY then anim:SetScale(tonumber(a.scaleX) or 1, tonumber(a.scaleY) or 1) end
            if a.fromScaleX then anim:SetScaleFrom(tonumber(a.fromScaleX), tonumber(a.fromScaleY) or 1) end
            if a.toScaleX then anim:SetScaleTo(tonumber(a.toScaleX), tonumber(a.toScaleY) or 1) end
          elseif an.tag == "Rotation" then
            if a.degrees then anim:SetDegrees(tonumber(a.degrees)) end
            if a.radians then anim:SetRadians(tonumber(a.radians)) end
          elseif an.tag == "FlipBook" then
            if a.flipBookRows then anim:SetFlipBookRows(tonumber(a.flipBookRows)) end
            if a.flipBookColumns then anim:SetFlipBookColumns(tonumber(a.flipBookColumns)) end
            if a.flipBookFrames then anim:SetFlipBookFrames(tonumber(a.flipBookFrames)) end
          end
          local sc = kid(an, "Scripts")
          if sc then setScripts(ctx, anim, sc) end
        end
      end
    end
    local sc = kid(gn, "Scripts")
    if sc then setScripts(ctx, g, sc) end
    applyKeyValues(ctx, g, gn)
  end
end

local BUTTON_TEXTURES = { NormalTexture = "Normal", PushedTexture = "Pushed", HighlightTexture = "Highlight",
  DisabledTexture = "Disabled", CheckedTexture = "Checked", DisabledCheckedTexture = "DisabledChecked" }

-- Apply an element's attributes and children to an existing frame.
function buildFrameContents(ctx, obj, node, parent)
  local sim = ctx.sim
  local s = sim.widgetState[obj]
  local a = node.attrs

  if a.mixin or a.secureMixin then
    for _, m in ipairs(require("wowapi.templates").split((a.mixin or "") .. "," .. (a.secureMixin or ""))) do
      local mt = sim:Get(m)
      if type(mt) ~= "table" then
        sim:_error(string.format("%s:%d: mixin '%s' is not defined (load the Lua file that defines it first)", ctx.file, node.line or 0, m))
      else
        for k, v in pairs(mt) do obj[k] = v end
      end
    end
  end
  if a.hidden then s.shown = not bool(a.hidden) end
  if a.alpha then s.alpha = tonumber(a.alpha) end
  if a.scale then s.scale = tonumber(a.scale) end
  if a.frameStrata then obj:SetFrameStrata(a.frameStrata) end
  if a.frameLevel then s.level = tonumber(a.frameLevel) end
  if a.toplevel then s.toplevel = bool(a.toplevel) end
  if a.movable then obj:SetMovable(bool(a.movable)) end
  if a.resizable then obj:SetResizable(bool(a.resizable)) end
  if a.enableMouse then obj:EnableMouse(bool(a.enableMouse)) end
  if a.enableMouseWheel then s.mouseWheel = bool(a.enableMouseWheel) end
  if a.enableKeyboard then obj:EnableKeyboard(bool(a.enableKeyboard)) end
  if a.clampedToScreen then obj:SetClampedToScreen(bool(a.clampedToScreen)) end
  if a.id then obj:SetID(tonumber(a.id)) end
  if a.protected and bool(a.protected) then s.protected = true end
  if a.text and obj.SetText then
    local g = sim:Get(a.text)
    obj:SetText(type(g) == "string" and g or a.text)
  end
  -- widget specifics
  if s.type == "Slider" or s.type == "StatusBar" then
    if a.minValue or a.maxValue then obj:SetMinMaxValues(tonumber(a.minValue) or 0, tonumber(a.maxValue) or 0) end
    if a.defaultValue then obj:SetValue(tonumber(a.defaultValue)) end
    if a.valueStep then obj:SetValueStep(tonumber(a.valueStep)) end
    if a.obeyStepOnDrag then obj:SetObeyStepOnDrag(bool(a.obeyStepOnDrag)) end
    if a.orientation then obj:SetOrientation(a.orientation) end
    local bt = kid(node, "BarTexture")
    if bt then obj:SetStatusBarTexture(buildRegion(ctx, { tag = "Texture", attrs = bt.attrs, children = bt.children, line = bt.line }, obj, "ARTWORK")) end
    local bc = kid(node, "BarColor")
    if bc then obj:SetStatusBarColor(colorOf(bc)) end
    local th = kid(node, "ThumbTexture")
    if th then
      local t = buildRegion(ctx, { tag = "Texture", attrs = th.attrs, children = th.children, line = th.line }, obj, "ARTWORK")
      s.thumb = t
    end
  elseif s.type == "EditBox" then
    if a.autoFocus then obj:SetAutoFocus(bool(a.autoFocus)) end
    if a.multiLine then obj:SetMultiLine(bool(a.multiLine)) end
    if a.numeric then obj:SetNumeric(bool(a.numeric)) end
    if a.password then s.password = bool(a.password) end
    if a.letters then obj:SetMaxLetters(tonumber(a.letters)) end
    if a.historyLines then obj:SetHistoryLines(tonumber(a.historyLines)) end
    local fs = kid(node, "FontString")
    if fs and fs.attrs.inherits then s.fontObject = sim:Get(fs.attrs.inherits) end
  elseif s.type == "CheckButton" then
    if a.checked then obj:SetChecked(bool(a.checked)) end
  end
  if s.type == "Button" or s.type == "CheckButton" then
    for tag, slot in pairs(BUTTON_TEXTURES) do
      local tn = kid(node, tag)
      if tn then
        local t = buildRegion(ctx, { tag = "Texture", attrs = tn.attrs, children = tn.children, line = tn.line }, obj,
          slot == "Highlight" and "HIGHLIGHT" or "ARTWORK")
        sim.widgetState[t].buttonSlot = slot
        if #sim.widgetState[t].points == 0 then t:SetAllPoints(obj) end
        s.textures[slot] = t
      end
    end
    local bt = kid(node, "ButtonText")
    if bt then
      local fs = buildRegion(ctx, { tag = "FontString", attrs = bt.attrs, children = bt.children, line = bt.line }, obj, "OVERLAY")
      s.fontString = fs
      if fs:GetNumPoints() == 0 then fs:SetPoint("CENTER") end
    end
    for _, f in ipairs({ "NormalFont", "HighlightFont", "DisabledFont" }) do
      local fn = kid(node, f)
      if fn and fn.attrs.style then
        s.buttonFonts = s.buttonFonts or {}
        s.buttonFonts[f] = sim:Get(fn.attrs.style)
        if f == "NormalFont" then
          if not s.fontString then
            s.fontString = require("wowapi.widgets").create(sim, "FontString", nil, obj)
            s.fontString:SetPoint("CENTER")
          end
          s.fontString:SetFontObject(s.buttonFonts[f])
        end
      end
    end
  end

  local w, h = parseDim(kid(node, "Size"))
  if w then s.width = w end
  if h then s.height = h end
  queueAnchors(ctx, obj, node, parent)

  local layers = kid(node, "Layers")
  if layers then
    for _, ln in ipairs(kids(layers, "Layer")) do
      local level = ln.attrs.level or "ARTWORK"
      local sub = tonumber(ln.attrs.textureSubLevel) or 0
      for _, rn in ipairs(ln.children) do
        if REGION_TAGS[rn.tag] then buildRegion(ctx, rn, obj, level, sub) end
      end
    end
  end

  local frames = kid(node, "Frames")
  if frames then
    for _, fnode in ipairs(frames.children) do M.buildFrame(ctx, fnode, obj) end
  end
  if s.type == "ScrollFrame" then
    local sc = kid(node, "ScrollChild")
    if sc then
      for _, fnode in ipairs(sc.children) do
        local child = M.buildFrame(ctx, fnode, obj)
        if child then obj:SetScrollChild(child) end
      end
    end
  end

  applyKeyValues(ctx, obj, node)
  applyAttributes(ctx, obj, node)
  local anims = kid(node, "Animations")
  if anims then M.buildAnimations(ctx, obj, anims) end
  local scripts = kid(node, "Scripts")
  if scripts then setScripts(ctx, obj, scripts) end
end

-- Template application: inherited templates first, then this template's
-- own attributes and children. Used by XML inherits= and by CreateFrame.
function M.applyTemplate(sim, obj, tmpl, ctx)
  ctx = ctx or newCtx(sim, tmpl.file)
  if tmpl.node.attrs.inherits then
    require("wowapi.templates").apply(sim, obj, tmpl.node.attrs.inherits, require("wowapi.widgets").create, ctx)
  end
  local node = tmpl.node
  if REGION_TAGS[node.tag] then
    buildRegion(ctx, { tag = node.tag, attrs = { hidden = node.attrs.hidden, alpha = node.attrs.alpha, file = node.attrs.file,
      atlas = node.attrs.atlas, text = node.attrs.text, justifyH = node.attrs.justifyH, justifyV = node.attrs.justifyV },
      children = node.children, line = node.line }, obj:GetParent(), nil, nil, obj)
  else
    local fileCtx = ctx
    local saveFile, saveDir = fileCtx.file, fileCtx.dir
    fileCtx.file, fileCtx.dir = tmpl.file, tmpl.file:match("^(.*)[/\\]") or "."
    buildFrameContents(fileCtx, obj, node, obj:GetParent())
    fileCtx.file, fileCtx.dir = saveFile, saveDir
  end
end

function M.runOnLoads(sim, ctx)
  local list = ctx.onloads
  ctx.onloads = {}
  for _, obj in ipairs(list) do
    sim:_runScript(obj, "OnLoad")
  end
end

-- Build a frame element (top level or inside <Frames>).
function M.buildFrame(ctx, node, parent)
  local sim = ctx.sim
  local tag = node.tag
  local a = node.attrs
  local intrinsic = sim.xmlIntrinsics[tag]
  if not FRAME_TAGS[tag] and not intrinsic then
    if tag ~= "Scripts" and tag ~= "KeyValues" then
      sim:_warn(string.format("%s:%d: <%s> is not a frame type", ctx.file, node.line or 0, tag))
    end
    return nil
  end
  if bool(a.virtual) or bool(a.intrinsic) then
    if not a.name then
      sim:_error(string.format("%s:%d: virtual frame needs a name", ctx.file, node.line or 0))
      return nil
    end
    sim.templates[a.name] = { xml = true, node = node, type = intrinsic and intrinsic.type or tag, file = ctx.file }
    if bool(a.intrinsic) then sim.xmlIntrinsics[a.name] = sim.templates[a.name] end
    return nil
  end
  if a.parent then
    parent = sim:Get(a.parent)
    if not parent then sim:_error(string.format("%s:%d: parent '%s' not found", ctx.file, node.line or 0, a.parent)) end
  end
  local create = require("wowapi.widgets").create
  local ftype = intrinsic and intrinsic.type or tag
  local name = resolveName(a.name, parent, sim)
  local obj = create(sim, ftype, name, parent)
  attachKey(obj, parent, node)
  -- order matters: intrinsic base, inherited templates, then this node
  if intrinsic then M.applyTemplate(sim, obj, intrinsic, ctx) end
  if a.inherits then
    require("wowapi.templates").apply(sim, obj, a.inherits, create, ctx)
  end
  buildFrameContents(ctx, obj, node, parent)
  -- children were queued first, so their OnLoad runs before the parent's
  table.insert(ctx.onloads, obj)
  return obj
end

-- <Font name inherits font height outline><Color/><Shadow/></Font>
local function buildFont(ctx, node)
  local sim = ctx.sim
  local a = node.attrs
  if not a.name then return end
  local f = require("wowapi.widgets").create(sim, "Font", a.name)
  local s = sim.widgetState[f]
  if a.inherits then
    local base = sim:Get(a.inherits)
    local bs = base and sim.widgetState[base]
    if bs then
      s.font = bs.font and { bs.font[1], bs.font[2], bs.font[3] }
      s.textColor = bs.textColor
      s.justifyH = bs.justifyH
    end
  end
  local h = kid(node, "FontHeight")
  local height = tonumber(a.height) or (h and tonumber((kid(h, "AbsValue") or h).attrs.val))
  s.font = { a.font or (s.font and s.font[1]) or "Fonts\\FRIZQT__.TTF", height or (s.font and s.font[2]) or 12, a.outline or (s.font and s.font[3]) or "" }
  local c = kid(node, "Color")
  if c then s.textColor = { colorOf(c) } end
  if a.justifyH then s.justifyH = a.justifyH end
end

-- Load one XML file. `addon` is the addon being loaded (for Script files).
function M.loadFile(sim, path, addon, runLua)
  local toc = require("wowapi.toc")
  local text, err = toc.readFile(path)
  if not text then sim:_error((addon and addon.name .. ": " or "") .. "cannot open " .. path); return end
  M._env, M._sim = sim.env, sim
  local ok, root = pcall(M.parse, text, path)
  if not ok then sim:_error(root); return end
  local ui = root.children[1]
  if not ui then return end
  local ctx = newCtx(sim, path)
  for _, node in ipairs(ui.children) do
    if node.tag == "Script" then
      if node.attrs.file then
        runLua(toc.join(ctx.dir, (node.attrs.file:gsub("\\", "/"))))
      elseif node.text then
        local compat = require("wowapi.compat")
        local fn, e = compat.loadstring(string.rep("\n", (node.line or 1) - 1) .. node.text, "@" .. path, sim.env)
        if fn then sim:_pcall(fn, addon and addon.name, addon and addon.ns) else sim:_error(e) end
      end
    elseif node.tag == "Include" then
      local f = node.attrs.file and toc.join(ctx.dir, (node.attrs.file:gsub("\\", "/")))
      f = f and (toc.resolve(f) or f)
      if f and f:lower():match("%.lua$") then runLua(f)
      elseif f then M.loadFile(sim, f, addon, runLua) end
    elseif node.tag == "Font" then
      buildFont(ctx, node)
    elseif REGION_TAGS[node.tag] then
      if bool(node.attrs.virtual) and node.attrs.name then
        sim.templates[node.attrs.name] = { xml = true, node = node, type = node.tag, file = path }
      end
    elseif node.tag == "Binding" or node.tag == "Bindings" then
      -- handled by bindings loader
    else
      local obj = M.buildFrame(ctx, node, nil)
      if obj then
        applyAnchors(ctx)
        M.runOnLoads(sim, ctx)
      end
    end
  end
  applyAnchors(ctx)
  M.runOnLoads(sim, ctx)
end

-- Bindings.xml: <Bindings><Binding name="X" header="Y" category="Z">lua</Binding></Bindings>
function M.loadBindings(sim, path, addon)
  local toc = require("wowapi.toc")
  local text = toc.readFile(path)
  if not text then return end
  local ok, root = pcall(M.parse, text, path)
  if not ok then sim:_error(root); return end
  local compat = require("wowapi.compat")
  for _, b in ipairs(kids(root.children[1] or { children = {} }, "Binding")) do
    local name = b.attrs.name
    if name then
      local body = b.text or ""
      local fn, e = compat.loadstring(string.rep("\n", (b.line or 1) - 1) .. "return function(keystate) " .. body .. "\nend", "@" .. path, sim.env)
      if not fn then sim:_error(e)
      else
        sim.bindingActions[name] = { fn = fn(), header = b.attrs.header, category = b.attrs.category, addon = addon and addon.name,
          runOnUp = bool(b.attrs.runOnUp) }
        if b.attrs.default then sim:SetBinding(b.attrs.default, name) end
      end
    end
  end
end

M.newCtx = newCtx
M.applyAnchors = function(ctx) applyAnchors(ctx) end

function M.install(sim)
  sim.xml = M
  sim.xmlIntrinsics = {}
  sim.bindingActions = sim.bindingActions or {}
  FRAME_TAGS = {}
  for name, cls in pairs(sim.widgetClasses) do
    local c = cls
    while c do
      if c.name == "Frame" then FRAME_TAGS[name] = true; break end
      c = c.parent
    end
  end
end

return M
