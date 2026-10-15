-- Layout engine: resolves anchors (SetPoint/SetAllPoints) and sizes to
-- screen rectangles, the way the client does. Screen space is UIParent's
-- 1920x1080 with the origin at the bottom-left, y growing upwards.
local M = {}

M.SCREEN_W, M.SCREEN_H = 1920, 1080

M.STRATA = { WORLD = 0, BACKGROUND = 1, LOW = 2, MEDIUM = 3, HIGH = 4, DIALOG = 5, FULLSCREEN = 6,
  FULLSCREEN_DIALOG = 7, TOOLTIP = 8, BLIZZARD = 9 }
M.LAYERS = { BACKGROUND = 1, BORDER = 2, ARTWORK = 3, OVERLAY = 4, HIGHLIGHT = 5 }

local POINTS = { TOPLEFT = true, TOP = true, TOPRIGHT = true, LEFT = true, CENTER = true, RIGHT = true,
  BOTTOMLEFT = true, BOTTOM = true, BOTTOMRIGHT = true }
M.POINTS = POINTS

function M.anchor(l, b, w, h, point)
  local x, y
  if point:find("LEFT") then x = l elseif point:find("RIGHT") then x = l + w else x = l + w / 2 end
  if point:find("TOP") then y = b + h elseif point:find("BOTTOM") then y = b else y = b + h / 2 end
  return x, y
end

function M.effectiveScale(state, obj)
  local sc, o = 1, obj
  while o and state[o] do
    sc = sc * (state[o].scale or 1)
    o = state[o].parent
  end
  return sc
end

-- Text metrics: an approximation of FRIZQT at the given height.
function M.textWidth(text, size)
  if not text or text == "" then return 0 end
  local plain = text:gsub("|c%x%x%x%x%x%x%x%x", ""):gsub("|r", ""):gsub("|T.-|t", "  "):gsub("|H.-|h(.-)|h", "%1")
  local longest = 0
  for line in (plain .. "\n"):gmatch("(.-)\n") do
    local _, n = line:gsub("[^\128-\191]", "")
    if n > longest then longest = n end
  end
  return longest * size * 0.52
end

-- Word-wrap `text` to `width` pixels at font `size`. Returns the lines; a
-- colour left open at a line break carries on at the start of the next.
function M.wrap(text, size, width)
  local out = {}
  if not text or text == "" then return out end
  -- the width estimate is approximate: allow a little overflow before
  -- breaking, so text sized from its own GetStringWidth never wraps
  if width then width = width * 1.08 + 2 end
  for para in (text .. "\n"):gmatch("(.-)\n") do
    if not width or width <= 0 or M.textWidth(para, size) <= width then
      out[#out + 1] = para
    else
      local line = ""
      for word in para:gmatch("%S+") do
        local try = line == "" and word or (line .. " " .. word)
        if line ~= "" and M.textWidth(try, size) > width then
          out[#out + 1] = line
          line = word
        else
          line = try
        end
      end
      out[#out + 1] = line
    end
  end
  -- carry open colour codes over line breaks
  local open
  for i, l in ipairs(out) do
    if open then out[i] = open .. l end
    for code in out[i]:gmatch("|c%x%x%x%x%x%x%x%x") do open = code end
    local lastC = out[i]:match(".*()|c%x%x%x%x%x%x%x%x") or 0
    local lastR = out[i]:match(".*()|r") or 0
    if lastR > lastC then open = nil end
  end
  return out
end

-- Should this FontString wrap (it has a width and word wrap is on)?
function M.wrapWidth(sim, obj, width)
  local s = sim.widgetState[obj]
  local ww = s.props and s.props.WordWrap
  if ww and ww[1] == false then return nil end
  return width
end

-- Line height and total height of `n` lines at font `size`.
function M.linesHeight(n, size, spacing)
  if n <= 0 then return 0 end
  return n * size + (n - 1) * (spacing or 2)
end

function M.textLines(text)
  if not text or text == "" then return 0 end
  local _, n = text:gsub("\n", "")
  return n + 1
end

function M.fontOf(sim, obj)
  local s = sim.widgetState[obj]
  if s.font then return s.font[1], s.font[2] or 12, s.font[3] end
  local fo = s.fontObject
  if type(fo) == "string" then fo = sim:Get(fo) end
  local fs = fo and sim.widgetState[fo]
  if fs and fs.font then return fs.font[1], fs.font[2] or 12, fs.font[3] end
  return "Fonts\\FRIZQT__.TTF", 12, ""
end

-- Natural size for regions that size themselves.
local function intrinsic(sim, obj, s)
  if s.type == "FontString" then
    local _, size = M.fontOf(sim, obj)
    return M.textWidth(s.text, size), math.max(M.textLines(s.text), s.text and 1 or 0) * size
  end
  if s.type == "GameTooltip" and s.lines and #s.lines > 0 then
    local w = 0
    for i, l in ipairs(s.lines) do
      local size = i == 1 and 14 or 12
      local lw = M.textWidth(l.left, size) + (l.right and (M.textWidth(l.right, size) + 20) or 0)
      if lw > w then w = lw end
    end
    return w + 20, #s.lines * 14 + 16
  end
  if s.type == "Button" and s.fontString and (s.width or 0) == 0 then
    return nil
  end
end

-- Screen rect of a region: left, bottom, width, height (or nil if it has
-- no valid anchors).
function M.rect(sim, obj, visiting)
  local state = sim.widgetState
  local s = state[obj]
  if not s then return nil end
  if s.fixedRect then return s.fixedRect[1], s.fixedRect[2], s.fixedRect[3], s.fixedRect[4] end
  if #s.points == 0 then return nil end
  visiting = visiting or {}
  if visiting[obj] then return nil end
  visiting[obj] = true

  local es = M.effectiveScale(state, obj)
  local L, R, CX, T, B, CY
  for _, p in ipairs(s.points) do
    local point, rel, relPoint, x, y = p[1], p[2], p[3], p[4], p[5]
    local rl, rb, rw, rh
    if rel then
      rl, rb, rw, rh = M.rect(sim, rel, visiting)
      if not rl then visiting[obj] = nil; return nil end
    else
      rl, rb, rw, rh = 0, 0, M.SCREEN_W, M.SCREEN_H
    end
    local ax, ay = M.anchor(rl, rb, rw, rh, relPoint)
    ax, ay = ax + (x or 0) * es, ay + (y or 0) * es
    if point:find("LEFT") then L = ax elseif point:find("RIGHT") then R = ax else CX = ax end
    if point:find("TOP") then T = ay elseif point:find("BOTTOM") then B = ay else CY = ay end
  end
  visiting[obj] = nil

  local w, h = (s.width or 0) * es, (s.height or 0) * es
  local iw, ih = intrinsic(sim, obj, s)
  if w == 0 and iw then w = iw * es end

  -- an edge anchor plus a centre anchor on the same axis only sizes the
  -- region when it has no size of its own (AceGUI's TOPLEFT + RIGHT rows)
  local l, b
  if L and R then l, w = L, R - L
  elseif L and CX and w == 0 then l, w = L, 2 * (CX - L)
  elseif R and CX and w == 0 then w = 2 * (R - CX); l = R - w
  elseif L then l = L
  elseif R then l = R - w
  else l = CX - w / 2 end
  -- a FontString with a set width wraps, so it grows taller
  if s.type == "FontString" and h == 0 and s.text and s.text ~= "" and ((s.width or 0) > 0 or (L and R)) then
    local _, size = M.fontOf(sim, obj)
    local ww = M.wrapWidth(sim, obj, w / es)
    if ww then ih = M.linesHeight(#M.wrap(s.text, size, ww), size) end
  end
  if h == 0 and ih then h = ih * es end
  if T and B then b, h = B, T - B
  elseif B and CY and h == 0 then b, h = B, 2 * (CY - B)
  elseif T and CY and h == 0 then h = 2 * (T - CY); b = T - h
  elseif B then b = B
  elseif T then b = T - h
  else b = CY - h / 2 end
  if s.animOffsets then
    for _, o in pairs(s.animOffsets) do l, b = l + o[1] * es, b + o[2] * es end
  end
  return l, b, w, h
end

-- Would anchoring `obj` to `rel` create a dependency loop?
function M.dependsOn(sim, rel, obj, seen)
  if rel == obj then return true end
  local s = sim.widgetState[rel]
  if not s then return false end
  seen = seen or {}
  if seen[rel] then return false end
  seen[rel] = true
  for _, p in ipairs(s.points) do
    if p[2] and M.dependsOn(sim, p[2], obj, seen) then return true end
  end
  return false
end

-- Draw order key for frames.
function M.frameOrder(sim, obj)
  local s = sim.widgetState[obj]
  return (M.STRATA[s.strata or "MEDIUM"] or 3), (s.level or 0), (s.seq or 0)
end

-- All visible frames that accept the mouse at screen point (x, y),
-- topmost first.
function M.framesAt(sim, x, y)
  local hits = {}
  for obj, s in pairs(sim.widgetState) do
    if s.isFrame and s.mouse and sim._isVisible(obj) and not s.fixedRect then
      local l, b, w, h = M.rect(sim, obj)
      if l and x >= l and x <= l + w and y >= b and y <= b + h then hits[#hits + 1] = obj end
    end
  end
  table.sort(hits, function(a, c)
    local s1, l1, q1 = M.frameOrder(sim, a)
    local s2, l2, q2 = M.frameOrder(sim, c)
    if s1 ~= s2 then return s1 > s2 end
    if l1 ~= l2 then return l1 > l2 end
    return q1 > q2
  end)
  return hits
end

return M
