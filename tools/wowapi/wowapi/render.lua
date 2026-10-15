-- Renders the simulated UI to SVG: frames in strata/level order, their
-- regions by draw layer, backdrops, textures, status bars, sliders, edit
-- boxes, buttons, tooltips and text with |c color codes.
--
-- With art enabled (sim:Screenshot(path, { art = true }), `wowtest ...
-- --art`), textures are drawn with the real game art, fetched on demand by
-- wowapi/assets.lua: file textures, fileDataIDs and atlases, cropped by
-- texture coordinates, tinted by vertex color, tiled, rotated and blended
-- like the client. Without art (or offline), textures render as labelled
-- placeholders.
local layout = require("wowapi.layout")

local M = {}

local H = layout.SCREEN_H

local function esc(s)
  return (tostring(s):gsub("&", "&amp;"):gsub("<", "&lt;"):gsub(">", "&gt;"):gsub('"', "&quot;"))
end

local function rgb(r, g, b)
  local function c(v) return math.max(0, math.min(255, math.floor((v or 1) * 255 + 0.5))) end
  return string.format("#%02x%02x%02x", c(r), c(g), c(b))
end

local function num(n) return string.format("%.1f", n) end

------------------------------------------------------------------ art helpers

local Ctx = {}
Ctx.__index = Ctx

local function newCtx(sim, opts)
  local c = setmetatable({ sim = sim, opts = opts, out = {}, defs = {}, ids = {}, seq = 0 }, Ctx)
  if opts.art then c.store = sim:ArtStore() end
  return c
end

function Ctx:add(s) self.out[#self.out + 1] = s end
function Ctx:id(prefix) self.seq = self.seq + 1; return prefix .. self.seq end

local cwd
local function absolute(p)
  if p:sub(1, 1) == "/" then return p end
  if not cwd then local h = io.popen("pwd"); cwd = h:read("*l"); h:close() end
  return (cwd .. "/" .. p):gsub("/%./", "/")
end

function Ctx:href(img)
  if self.opts.embed == false then return "file://" .. absolute(img.file) end
  return self.store:dataURI(img)
end

-- Color-multiply (vertex color) / desaturate filter; returns attribute text.
function Ctx:filter(r, g, b, desat)
  r, g, b = r or 1, g or 1, b or 1
  if r >= 0.999 and g >= 0.999 and b >= 0.999 and not desat then return "" end
  local key = string.format("%.3f,%.3f,%.3f,%s", r, g, b, tostring(desat))
  local id = self.ids[key]
  if not id then
    id = self:id("f")
    self.ids[key] = id
    local d = '<filter id="' .. id .. '" color-interpolation-filters="sRGB">'
    if desat then d = d .. '<feColorMatrix type="saturate" values="0"/>' end
    d = d .. string.format('<feColorMatrix type="matrix" values="%.3f 0 0 0 0 0 %.3f 0 0 0 0 0 %.3f 0 0 0 0 0 1 0"/></filter>', r, g, b)
    self.defs[#self.defs + 1] = d
  end
  return ' filter="url(#' .. id .. ')"'
end

-- Draw part of an image (texcoords l,r,t,b in 0..1) into a screen rect.
function Ctx:image(img, x, y, w, h, tc, tint, alpha, extra)
  if w <= 0 or h <= 0 then return end
  extra = extra or {}
  tc = tc or { 0, 1, 0, 1 }
  local W, Hh = img.width, img.height
  local l, r, t, b = tc[1], tc[2], tc[3], tc[4]
  local flipX, flipY = l > r, t > b
  if flipX then l, r = r, l end
  if flipY then t, b = b, t end
  local vx, vy, vw, vh = l * W, t * Hh, math.max((r - l) * W, 0.01), math.max((b - t) * Hh, 0.01)
  local transform = ""
  if flipX or flipY then
    transform = string.format(' transform="translate(%s %s) scale(%d %d)"',
      num(flipX and (2 * vx + vw) or 0), num(flipY and (2 * vy + vh) or 0), flipX and -1 or 1, flipY and -1 or 1)
  end
  local c = tint or { 1, 1, 1, 1 }
  local style = ""
  if extra.blend == "ADD" then style = ' style="mix-blend-mode:screen"' end
  local rot = ""
  if extra.rotation and extra.rotation ~= 0 then
    rot = string.format(' transform="rotate(%s %s %s)"', num(-math.deg(extra.rotation)), num(x + w / 2), num(y + h / 2))
  end
  self:add(string.format('<g opacity="%.3f"%s%s><svg x="%s" y="%s" width="%s" height="%s" viewBox="%s %s %s %s" preserveAspectRatio="none" overflow="hidden"><image href="%s" x="0" y="0" width="%d" height="%d" preserveAspectRatio="none"%s%s/></svg></g>',
    (c[4] or 1) * (alpha or 1), style, rot, num(x), num(y), num(w), num(h), num(vx), num(vy), num(vw), num(vh),
    self:href(img), W, Hh, self:filter(c[1], c[2], c[3], extra.desaturate), transform))
end

-- Tile an image (whole texture) across a rect, tileW x tileH per copy.
function Ctx:tile(img, x, y, w, h, tileW, tileH, tint, alpha)
  if w <= 0 or h <= 0 then return end
  local id = self:id("p")
  self.defs[#self.defs + 1] = string.format('<pattern id="%s" patternUnits="userSpaceOnUse" x="%s" y="%s" width="%s" height="%s"><image href="%s" width="%s" height="%s" preserveAspectRatio="none"/></pattern>',
    id, num(x), num(y), num(tileW), num(tileH), self:href(img), num(tileW), num(tileH))
  local c = tint or { 1, 1, 1, 1 }
  self:add(string.format('<rect x="%s" y="%s" width="%s" height="%s" fill="url(#%s)" opacity="%.3f"%s/>',
    num(x), num(y), num(w), num(h), id, (c[4] or 1) * (alpha or 1), self:filter(c[1], c[2], c[3])))
end

-- Three-slice horizontal art (left cap, stretched middle, right cap).
function Ctx:threeSlice(img, x, y, w, h, tc, capFrac, tint, alpha)
  tc = tc or { 0, 1, 0, 1 }
  local texW = tc[2] - tc[1]
  local capTex = texW * capFrac
  -- keep the caps' aspect ratio
  local capW = math.min(w / 2, h * (capTex * img.width) / ((tc[4] - tc[3]) * img.height))
  self:image(img, x, y, capW, h, { tc[1], tc[1] + capTex, tc[3], tc[4] }, tint, alpha)
  self:image(img, x + capW, y, w - 2 * capW, h, { tc[1] + capTex, tc[2] - capTex, tc[3], tc[4] }, tint, alpha)
  self:image(img, x + w - capW, y, capW, h, { tc[2] - capTex, tc[2], tc[3], tc[4] }, tint, alpha)
end

function Ctx:art(tex)
  if not self.store or tex == nil then return nil end
  return self.store:image(tex)
end

------------------------------------------------------------------ text

-- "|cffff0000red|r plain" -> list of { text, color }
local function colorRuns(text, default)
  local runs = {}
  local color = default
  local i = 1
  text = text:gsub("|T.-|t", ""):gsub("|A.-|a", ""):gsub("|H.-|h(.-)|h", "%1"):gsub("||", "|")
  while i <= #text do
    local s, e, hex = text:find("|c(%x%x%x%x%x%x%x%x)", i)
    local rs, re = text:find("|r", i, true)
    local nextCode = math.min(s or math.huge, rs or math.huge)
    if nextCode == math.huge then
      runs[#runs + 1] = { text:sub(i), color }
      break
    end
    if nextCode > i then runs[#runs + 1] = { text:sub(i, nextCode - 1), color } end
    if s and nextCode == s then
      color = "#" .. hex:sub(3):lower()
      i = e + 1
    else
      color = default
      i = re + 1
    end
  end
  return runs
end

local function textSvg(ctx, x, y, text, size, color, anchor, opacity, outline)
  local lines = {}
  for line in (text .. "\n"):gmatch("(.-)\n") do lines[#lines + 1] = line end
  local family = ctx.fontFamily or "Friz Quadrata TT, Georgia, serif"
  for i, line in ipairs(lines) do
    local parts = {}
    for _, run in ipairs(colorRuns(line, color)) do
      parts[#parts + 1] = string.format('<tspan fill="%s">%s</tspan>', run[2], esc(run[1]))
    end
    ctx:add(string.format(
      '<text x="%s" y="%s" font-size="%s" text-anchor="%s" opacity="%.2f" font-family="%s" stroke="#000" stroke-width="%.1f" stroke-opacity="%s" paint-order="stroke">%s</text>',
      num(x), num(y + (i - 1) * size * 1.15), num(size), anchor, opacity, esc(family),
      outline and size / 6 or size / 10, outline and "1" or "0.6", table.concat(parts)))
  end
end

------------------------------------------------------------------ regions

local function textureTexCoord(s)
  local tc = s.texCoord
  if not tc then return nil end
  if #tc >= 8 then return { tc[1], tc[7], tc[2], tc[8] } end -- ULx, LRx, ULy, LRy
  return { tc[1], tc[2], tc[3], tc[4] }
end

local function drawTexture(ctx, obj, s, x, y, w, h, alpha, clipFrac)
  if w <= 0 or h <= 0 then return end
  local c = s.color or { 1, 1, 1, 1 }
  local a = (c[4] or 1) * alpha
  if s.gradient and s.gradient[1] and s.gradient[2] then
    -- colors multiply the texture (a white color texture shows the gradient)
    local g = s.gradient
    local base = s.texture == "color" and c or { 1, 1, 1, 1 }
    local id = ctx:id("g")
    local vertical = g.orientation == "VERTICAL"
    ctx.defs[#ctx.defs + 1] = string.format('<linearGradient id="%s" x1="0" y1="%d" x2="%d" y2="0"><stop offset="0" stop-color="%s" stop-opacity="%.3f"/><stop offset="1" stop-color="%s" stop-opacity="%.3f"/></linearGradient>',
      id, vertical and 1 or 0, vertical and 0 or 1,
      rgb(g[1][1] * base[1], g[1][2] * base[2], g[1][3] * base[3]), (g[1][4] or 1) * (base[4] or 1),
      rgb(g[2][1] * base[1], g[2][2] * base[2], g[2][3] * base[3]), (g[2][4] or 1) * (base[4] or 1))
    if s.texture == "color" or not ctx.store then
      ctx:add(string.format('<rect x="%s" y="%s" width="%s" height="%s" fill="url(#%s)" opacity="%.3f"/>',
        num(x), num(y), num(w * (clipFrac or 1)), num(h), id, alpha))
      return
    end
  end
  if s.texture == "color" then
    ctx:add(string.format('<rect x="%s" y="%s" width="%s" height="%s" fill="%s" opacity="%.3f"/>',
      num(x), num(y), num(w * (clipFrac or 1)), num(h), rgb(c[1], c[2], c[3]), a))
    return
  end
  if not s.texture and not s.atlas then return end
  local props = s.props or {}
  local extra = { blend = s.blend, desaturate = s.desaturated or (props.Desaturation and (props.Desaturation[1] or 0) > 0.5),
    rotation = s.animRotation and math.rad(s.animRotation) or (props.Rotation and props.Rotation[1]) }
  if ctx.store then
    local img, tc
    if s.atlas then
      local at = ctx.store:atlas(s.atlas)
      if at and at.image then img, tc = at.image, { at.left, at.right, at.top, at.bottom } end
    else
      img = ctx:art(s.texture)
      tc = textureTexCoord(s)
    end
    if img then
      tc = tc or { 0, 1, 0, 1 }
      if clipFrac then
        tc = { tc[1], tc[1] + (tc[2] - tc[1]) * clipFrac, tc[3], tc[4] }
        w = w * clipFrac
      end
      local tileH = props.HorizTile and props.HorizTile[1]
      local tileV = props.VertTile and props.VertTile[1]
      if (tileH or tileV) and not s.atlas then
        ctx:tile(img, x, y, w, h, tileH and img.width or w, tileV and img.height or h, c, alpha)
      else
        ctx:image(img, x, y, w, h, tc, c, alpha, extra)
      end
      return
    end
  end
  -- placeholder
  local label = tostring(s.atlas or s.texture):gsub("^.*[/\\]", "")
  ctx:add(string.format(
    '<g opacity="%.2f"><rect x="%s" y="%s" width="%s" height="%s" fill="%s" fill-opacity="0.55" stroke="%s" stroke-opacity="0.6"/>%s<title>%s</title></g>',
    a, num(x), num(y), num(w * (clipFrac or 1)), num(h), rgb(c[1] * 0.6, c[2] * 0.6, c[3] * 0.6), rgb(c[1], c[2], c[3]),
    (w > 40 and h > 10) and string.format('<text x="%s" y="%s" font-size="9" fill="#ddd" font-family="monospace">%s</text>',
      num(x + 2), num(y + 10), esc(label:sub(1, math.floor(w / 5.5)))) or "",
    esc(tostring(s.atlas or s.texture))))
end

local function region(ctx, obj)
  local sim = ctx.sim
  local st = sim.widgetState
  local s = st[obj]
  local l, b, w, h = layout.rect(sim, obj)
  if not l then return end
  local x, y = l, H - b - h
  local alpha = obj.GetEffectiveAlpha and obj:GetEffectiveAlpha() or 1
  if alpha <= 0 then return end

  if s.type == "Texture" or s.type == "Line" then
    drawTexture(ctx, obj, s, x, y, w, h, alpha)
  elseif s.type == "FontString" then
    if not s.text or s.text == "" then return end
    local _, size, flags = layout.fontOf(sim, obj)
    local tc = s.textColor
    if not tc then
      local fo = s.fontObject
      if type(fo) == "string" then fo = sim:Get(fo) end
      tc = fo and st[fo] and st[fo].textColor
    end
    tc = tc or { 1, 0.82, 0, 1 }
    local jh = s.justifyH or "CENTER"
    local anchor, tx = "middle", x + w / 2
    if jh == "LEFT" then anchor, tx = "start", x elseif jh == "RIGHT" then anchor, tx = "end", x + w end
    local text = s.text
    local ww = layout.wrapWidth(sim, obj, ((s.width or 0) > 0 or #s.points >= 2) and w or nil)
    if ww then text = table.concat(layout.wrap(text, size, ww), "\n") end
    local lines = layout.textLines(text)
    local ty
    local jv = s.justifyV or "MIDDLE"
    if jv == "TOP" then ty = y + size * 0.85
    elseif jv == "BOTTOM" then ty = y + h - (lines - 1) * size * 1.15 - size * 0.2
    else ty = y + h / 2 - (lines - 1) * size * 0.575 + size * 0.35 end
    textSvg(ctx, tx, ty, text, size, rgb(tc[1], tc[2], tc[3]), anchor, alpha * (tc[4] or 1),
      flags and tostring(flags):find("OUTLINE"))
  end
end

------------------------------------------------------------------ backdrops

-- Classic backdrop edge files hold 8 square pieces side by side:
-- left, right, top, bottom, top-left, top-right, bottom-left, bottom-right.
local function drawBackdrop(ctx, s, x, y, w, h, alpha)
  local bd = s.backdrop
  local c = s.backdropColor or { 1, 1, 1, 1 }
  local bc = s.backdropBorderColor or { 1, 1, 1, 1 }
  local ins = bd.insets or {}
  local il, ir, it, ib = ins.left or 0, ins.right or 0, ins.top or 0, ins.bottom or 0
  local bg = bd.bgFile and ctx:art(bd.bgFile)
  local edge = bd.edgeFile and ctx:art(bd.edgeFile)
  if bd.bgFile then
    if bg then
      if bd.tile then
        local ts = bd.tileSize and bd.tileSize > 0 and bd.tileSize or bg.width
        ctx:tile(bg, x + il, y + it, w - il - ir, h - it - ib, ts, ts, c, alpha)
      else
        ctx:image(bg, x + il, y + it, w - il - ir, h - it - ib, nil, c, alpha)
      end
    else
      ctx:add(string.format('<rect x="%s" y="%s" width="%s" height="%s" fill="%s" opacity="%.2f"/>',
        num(x + il), num(y + it), num(w - il - ir), num(h - it - ib), rgb(c[1] * 0.15, c[2] * 0.15, c[3] * 0.2), (c[4] or 1) * alpha * 0.9))
    end
  end
  if bd.edgeFile then
    local e = bd.edgeSize or 16
    if edge then
      local piece = function(i) return { i / 8, (i + 1) / 8, 0, 1 } end
      local midW, midH = math.max(0, w - 2 * e), math.max(0, h - 2 * e)
      ctx:image(edge, x, y + e, e, midH, piece(0), bc, alpha)                 -- left
      ctx:image(edge, x + w - e, y + e, e, midH, piece(1), bc, alpha)         -- right
      -- top/bottom pieces are stored rotated; draw them rotated back
      local function rotated(i, rx, ry, deg)
        local g = string.format('<g transform="translate(%s %s) rotate(%d)">', num(rx), num(ry), deg)
        ctx:add(g)
        ctx:image(edge, 0, 0, e, midW, piece(i), bc, alpha)
        ctx:add("</g>")
      end
      rotated(2, x + e + midW, y, 90)   -- top: outer side faces up
      rotated(3, x + e + midW, y + h - e, 90) -- bottom: its outer side is stored on the right
      ctx:image(edge, x, y, e, e, piece(4), bc, alpha)
      ctx:image(edge, x + w - e, y, e, e, piece(5), bc, alpha)
      ctx:image(edge, x, y + h - e, e, e, piece(6), bc, alpha)
      ctx:image(edge, x + w - e, y + h - e, e, e, piece(7), bc, alpha)
    else
      ctx:add(string.format('<rect x="%s" y="%s" width="%s" height="%s" rx="4" fill="none" stroke="%s" stroke-width="%s" opacity="%.2f"/>',
        num(x + 1), num(y + 1), num(w - 2), num(h - 2), rgb(bc[1] * 0.7, bc[2] * 0.7, bc[3] * 0.7), num(math.max(1, e / 5)), alpha * (bc[4] or 1)))
    end
  end
end

------------------------------------------------------------------ frames

local LAYERS = { "BACKGROUND", "BORDER", "ARTWORK", "OVERLAY", "HIGHLIGHT" }
local LAYER_INDEX = {}
for i, n in ipairs(LAYERS) do LAYER_INDEX[n] = i end

local TOOLTIP_BACKDROP = { bgFile = "Interface\\Tooltips\\UI-Tooltip-Background", edgeFile = "Interface\\Tooltips\\UI-Tooltip-Border",
  tile = true, tileSize = 16, edgeSize = 16, insets = { left = 4, right = 4, top = 4, bottom = 4 } }

local function frame(ctx, obj)
  local sim = ctx.sim
  local st = sim.widgetState
  local s = st[obj]
  local l, b, w, h = layout.rect(sim, obj)
  if not l then return end
  local x, y = l, H - b - h
  local alpha = obj:GetEffectiveAlpha()
  local store = ctx.store

  if s.backdrop then drawBackdrop(ctx, s, x, y, w, h, alpha) end

  if s.type == "GameTooltip" then
    local tipState = { backdrop = TOOLTIP_BACKDROP, backdropColor = { 0.09, 0.09, 0.19, 0.9 }, backdropBorderColor = { 1, 1, 1, 1 } }
    if store and ctx:art(TOOLTIP_BACKDROP.bgFile) then
      drawBackdrop(ctx, tipState, x, y, w, h, alpha)
    else
      ctx:add(string.format('<rect x="%s" y="%s" width="%s" height="%s" rx="4" fill="#090a1a" fill-opacity="0.92" stroke="#b0b0b0" opacity="%.2f"/>',
        num(x), num(y), num(w), num(h), alpha))
    end
    for i, line in ipairs(s.lines or {}) do
      local size = i == 1 and 14 or 12
      local ly = y + 8 + (i - 1) * 14 + size * 0.85
      local c = (line.r and rgb(line.r, line.g, line.b)) or (i == 1 and "#ffffff" or "#ffd100")
      textSvg(ctx, x + 10, ly, line.left or "", size, c, "start", alpha)
      if line.right then textSvg(ctx, x + w - 10, ly, line.right, size, "#ffffff", "end", alpha) end
    end
  elseif s.type == "StatusBar" then
    local lo, hi = s.min or 0, s.max or 0
    local frac = hi > lo and ((s.value or 0) - lo) / (hi - lo) or 0
    frac = math.max(0, math.min(1, frac))
    local bar = s.barTexture and st[s.barTexture]
    if bar and (bar.texture or bar.atlas) and bar.texture ~= "color" then
      local tint = s.color or bar.color
      local saved = bar.color
      bar.color = tint
      if frac > 0 then drawTexture(ctx, s.barTexture, bar, x, y, w, h, alpha, frac) end
      bar.color = saved
    else
      local c = s.color or { 1, 1, 1, 1 }
      ctx:add(string.format('<rect x="%s" y="%s" width="%s" height="%s" fill="%s" opacity="%.2f"/>',
        num(x), num(y), num(w * frac), num(h), rgb(c[1], c[2], c[3]), alpha * (c[4] or 1)))
    end
  elseif s.type == "EditBox" then
    local border = store and ctx:art("Interface\\Common\\Common-Input-Border")
    if border then
      ctx:threeSlice(border, x - 5, y, w + 5, h, { 0, 0.9375, 0, 0.625 }, 0.0625, nil, alpha)
    else
      ctx:add(string.format('<rect x="%s" y="%s" width="%s" height="%s" fill="#000" fill-opacity="0.5" stroke="#777" opacity="%.2f"/>',
        num(x), num(y), num(w), num(h), alpha))
    end
    local text = s.password and string.rep("*", #(s.text or "")) or (s.text or "")
    local tc = s.textColor or { 1, 1, 1, 1 }
    textSvg(ctx, x + 4, y + h / 2 + 4, text .. (sim.keyboardFocus == obj and "|" or ""), 12, rgb(tc[1], tc[2], tc[3]), "start", alpha)
  elseif s.type == "Slider" then
    local lo, hi = s.min or 0, s.max or 0
    local frac = hi > lo and ((s.value or 0) - lo) / (hi - lo) or 0
    local track = store and ctx:art("Interface\\Buttons\\UI-SliderBar-Background")
    local thumb = store and ctx:art("Interface\\Buttons\\UI-SliderBar-Button-Horizontal")
    if track then
      ctx:threeSlice(track, x, y + h / 2 - 8, w, 17, { 0, 1, 0, 1 }, 0.2, nil, alpha)
    else
      ctx:add(string.format('<rect x="%s" y="%s" width="%s" height="6" rx="3" fill="#222" stroke="#666" opacity="%.2f"/>',
        num(x), num(y + h / 2 - 3), num(w), alpha))
    end
    if thumb then
      ctx:image(thumb, x + (w - 32) * frac, y + h / 2 - 16, 32, 32, nil, nil, alpha)
    else
      ctx:add(string.format('<rect x="%s" y="%s" width="10" height="%s" rx="2" fill="#ccc" opacity="%.2f"/>',
        num(x + (w - 10) * frac), num(y), num(h), alpha))
    end
  elseif s.type == "Button" or s.type == "CheckButton" then
    if s.type == "CheckButton" and not s.textures.Normal then
      local d = math.min(w, h)
      local box = store and ctx:art("Interface\\Buttons\\UI-CheckBox-Up")
      local check = store and ctx:art("Interface\\Buttons\\UI-CheckBox-Check")
      if box then
        ctx:image(box, x, y, d, d, nil, nil, alpha)
        if s.checked and check then ctx:image(check, x, y, d, d, nil, nil, alpha) end
      else
        ctx:add(string.format('<rect x="%s" y="%s" width="%s" height="%s" rx="3" fill="#111" stroke="#aaa" opacity="%.2f"/>',
          num(x), num(y), num(d), num(d), alpha))
        if s.checked then
          ctx:add(string.format('<path d="M%s %s L%s %s L%s %s" stroke="#ffd100" stroke-width="3" fill="none" opacity="%.2f"/>',
            num(x + d * 0.2), num(y + d * 0.5), num(x + d * 0.42), num(y + d * 0.75), num(x + d * 0.82), num(y + d * 0.25), alpha))
        end
      end
    elseif s.chrome == "panel" then
      local state = s.enabled == false and "Disabled" or "Up"
      local art = store and ctx:art("Interface\\Buttons\\UI-Panel-Button-" .. state)
      if art then
        ctx:threeSlice(art, x, y, w, h, { 0, 0.625, 0, 0.6875 }, 0.15, nil, alpha)
        if sim.mouseFocus == obj and s.enabled ~= false then
          local hl = ctx:art("Interface\\Buttons\\UI-Panel-Button-Highlight")
          if hl then ctx:threeSlice(hl, x, y, w, h, { 0, 0.625, 0, 0.6875 }, 0.15, nil, alpha * 0.7) end
        end
      else
        local fill = s.enabled == false and "#444" or "#7a1010"
        if sim.mouseFocus == obj and s.enabled ~= false then fill = "#a01818" end
        ctx:add(string.format('<rect x="%s" y="%s" width="%s" height="%s" rx="4" fill="%s" stroke="#d4a017" stroke-width="1.5" opacity="%.2f"/>',
          num(x), num(y), num(w), num(h), fill, alpha))
      end
    elseif s.chrome == "close" and store and ctx:art("Interface\\Buttons\\UI-Panel-MinimizeButton-Up")
      and not (s.textures.Normal and (ctx.store:atlas(st[s.textures.Normal].atlas or "") or {}).image) then
      local state = (sim.mouseFocus == obj) and "Up" or "Up"
      ctx:image(ctx:art("Interface\\Buttons\\UI-Panel-MinimizeButton-" .. state), x, y, w, h, nil, nil, alpha)
      if sim.mouseFocus == obj then
        local hl = ctx:art("Interface\\Buttons\\UI-Panel-MinimizeButton-Highlight")
        if hl then ctx:image(hl, x, y, w, h, nil, nil, alpha, { blend = "ADD" }) end
      end
    elseif s.chrome == "close" and not (store and s.textures.Normal and (ctx.store:atlas(st[s.textures.Normal].atlas or "") or {}).image) then
      ctx:add(string.format('<g opacity="%.2f"><rect x="%s" y="%s" width="%s" height="%s" rx="4" fill="#7a1010" stroke="#d4a017"/><path d="M%s %s L%s %s M%s %s L%s %s" stroke="#ffd100" stroke-width="2.5"/></g>',
        alpha, num(x + 3), num(y + 3), num(w - 6), num(h - 6), num(x + 8), num(y + 8), num(x + w - 8), num(y + h - 8),
        num(x + w - 8), num(y + 8), num(x + 8), num(y + h - 8)))
    end
    -- a close button whose atlas art isn't available keeps its drawn chrome
    local skipNormal = s.chrome == "close" and not (store and s.textures.Normal and (ctx.store:atlas(st[s.textures.Normal].atlas or "") or {}).image)
    for _, slot in ipairs({ "Normal", "Pushed", "Disabled", "Checked", "DisabledChecked", "Highlight" }) do
      local t = s.textures and s.textures[slot]
      local show = (slot == "Normal" and s.enabled ~= false and not skipNormal)
        or (slot == "Disabled" and s.enabled == false)
        or (slot == "Checked" and s.checked)
        or (slot == "Highlight" and sim.mouseFocus == obj)
      if show and t and st[t] and st[t].shown then region(ctx, t) end
    end
  end

  -- regions by draw layer, then sublevel, then creation order
  local regions = {}
  for _, c in ipairs(s.children) do
    local cs = st[c]
    if cs and not cs.isFrame and cs.shown and not cs.buttonSlot then
      if cs.layer ~= "HIGHLIGHT" or sim.mouseFocus == obj then regions[#regions + 1] = c end
    end
  end
  table.sort(regions, function(a, c)
    local sa, sc = st[a], st[c]
    local la, lc = LAYER_INDEX[sa.layer or "ARTWORK"] or 3, LAYER_INDEX[sc.layer or "ARTWORK"] or 3
    if la ~= lc then return la < lc end
    if (sa.sublevel or 0) ~= (sc.sublevel or 0) then return (sa.sublevel or 0) < (sc.sublevel or 0) end
    return (sa.seq or 0) < (sc.seq or 0)
  end)
  for _, r in ipairs(regions) do region(ctx, r) end

  if ctx.opts.outlines then
    ctx:add(string.format('<rect x="%s" y="%s" width="%s" height="%s" fill="none" stroke="#00e0ff" stroke-opacity="0.5" stroke-dasharray="4 3"/>',
      num(x), num(y), num(w), num(h)))
    if s.name then
      ctx:add(string.format('<text x="%s" y="%s" font-size="10" fill="#00e0ff" font-family="monospace">%s</text>',
        num(x + 2), num(y - 3), esc(s.name)))
    end
  end
end

------------------------------------------------------------------ page

-- Returns SVG markup of everything currently visible.
function M.svg(sim, opts)
  opts = opts or {}
  local ctx = newCtx(sim, opts)
  local st = sim.widgetState
  local frames = {}
  for obj, s in pairs(st) do
    if s.isFrame and not s.fixedRect and sim._isVisible(obj) then frames[#frames + 1] = obj end
  end
  table.sort(frames, function(a, c)
    local s1, l1, q1 = layout.frameOrder(sim, a)
    local s2, l2, q2 = layout.frameOrder(sim, c)
    if s1 ~= s2 then return s1 < s2 end
    if l1 ~= l2 then return l1 < l2 end
    return q1 < q2
  end)

  -- the game font, when available
  if ctx.store and opts.font ~= false then
    local font = ctx.store:font("Fonts\\FRIZQT__.TTF")
    if font then
      ctx.fontFamily = "WoWFriz, Friz Quadrata TT, Georgia, serif"
      ctx.defs[#ctx.defs + 1] = '<style>@font-face{font-family:WoWFriz;src:url(' .. ctx.store:dataURI(font) .. ')}</style>'
    end
  end

  -- background: a real game backdrop if requested, else a dusk gradient
  if opts.background and ctx.store then
    local img = ctx:art(opts.background)
    if img then ctx:image(img, 0, 0, layout.SCREEN_W, H) end
  end
  if #ctx.out == 0 then
    ctx:add(string.format('<rect width="%d" height="%d" fill="url(#bg)"/>', layout.SCREEN_W, H))
  end
  for _, c in ipairs(st[sim.env.UIParent].children) do
    if not st[c].isFrame and st[c].shown then region(ctx, c) end
  end
  for _, f in ipairs(frames) do frame(ctx, f) end
  if sim.cursorX and opts.cursor ~= false then
    local cx, cy = sim.cursorX, H - sim.cursorY
    local cur = ctx:art("Interface\\Cursor\\Point")
    if cur then ctx:image(cur, cx, cy, 32, 32)
    else ctx:add(string.format('<path d="M%s %s l0 18 l5 -5 l7 0 z" fill="#fff" stroke="#000"/>', num(cx), num(cy))) end
  end

  local head = {}
  head[#head + 1] = string.format('<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 %d %d" width="%d" height="%d">',
    layout.SCREEN_W, H, opts.width or layout.SCREEN_W, opts.height or H)
  head[#head + 1] = '<defs><linearGradient id="bg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#2b3a4a"/><stop offset="1" stop-color="#1a2418"/></linearGradient>'
    .. table.concat(ctx.defs) .. '</defs>'
  return table.concat(head, "\n") .. "\n" .. table.concat(ctx.out, "\n") .. "\n</svg>"
end

return M
