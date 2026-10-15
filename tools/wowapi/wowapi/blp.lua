-- BLP2 texture decoder (the game's native texture format): palettized,
-- DXT1/DXT3/DXT5 and uncompressed BGRA. Returns width, height and RGBA8
-- pixels (first mip level). Pure Lua 5.1.
local M = {}

local byte, char, floor, concat = string.byte, string.char, math.floor, table.concat

local function u32(s, i)
  local a, b, c, d = byte(s, i, i + 3)
  return a + b * 256 + c * 65536 + d * 16777216
end

local function rgb565(v)
  local r = floor(v / 2048) % 32
  local g = floor(v / 32) % 64
  local b = v % 32
  return floor(r * 255 / 31 + 0.5), floor(g * 255 / 63 + 0.5), floor(b * 255 / 31 + 0.5)
end

-- Decode one DXT colour block into px (16 entries of {r,g,b,a}).
local function colorBlock(s, i, px, dxt1)
  local c0 = byte(s, i) + byte(s, i + 1) * 256
  local c1 = byte(s, i + 2) + byte(s, i + 3) * 256
  local r0, g0, b0 = rgb565(c0)
  local r1, g1, b1 = rgb565(c1)
  local pal
  if c0 > c1 or not dxt1 then
    pal = { { r0, g0, b0, 255 }, { r1, g1, b1, 255 },
      { floor((2 * r0 + r1) / 3), floor((2 * g0 + g1) / 3), floor((2 * b0 + b1) / 3), 255 },
      { floor((r0 + 2 * r1) / 3), floor((g0 + 2 * g1) / 3), floor((b0 + 2 * b1) / 3), 255 } }
  else
    pal = { { r0, g0, b0, 255 }, { r1, g1, b1, 255 },
      { floor((r0 + r1) / 2), floor((g0 + g1) / 2), floor((b0 + b1) / 2), 255 }, { 0, 0, 0, 0 } }
  end
  for row = 0, 3 do
    local bits = byte(s, i + 4 + row)
    for col = 0, 3 do
      local idx = floor(bits / 4 ^ col) % 4
      local p = pal[idx + 1]
      local o = px[row * 4 + col + 1]
      o[1], o[2], o[3], o[4] = p[1], p[2], p[3], p[4]
    end
  end
end

local function decodeDXT(s, off, w, h, kind)
  local out = {}
  local bw, bh = math.max(1, floor((w + 3) / 4)), math.max(1, floor((h + 3) / 4))
  local blockSize = kind == 1 and 8 or 16
  local px = {}
  for k = 1, 16 do px[k] = { 0, 0, 0, 0 } end
  local rows = {}
  for y = 0, h - 1 do rows[y] = {} end
  local pos = off
  for by = 0, bh - 1 do
    for bx = 0, bw - 1 do
      local alpha
      if kind == 3 then
        alpha = {}
        for k = 0, 7 do
          local v = byte(s, pos + k)
          alpha[k * 2 + 1] = (v % 16) * 17
          alpha[k * 2 + 2] = floor(v / 16) * 17
        end
        colorBlock(s, pos + 8, px, false)
      elseif kind == 5 then
        local a0, a1 = byte(s, pos), byte(s, pos + 1)
        local ap = { a0, a1 }
        if a0 > a1 then
          for k = 1, 6 do ap[k + 2] = floor(((6 - k + 1) * a0 + k * a1) / 7) end
        else
          for k = 1, 4 do ap[k + 2] = floor(((4 - k + 1) * a0 + k * a1) / 5) end
          ap[7], ap[8] = 0, 255
        end
        -- 48 bits of 3-bit indices, little endian
        local lo = byte(s, pos + 2) + byte(s, pos + 3) * 256 + byte(s, pos + 4) * 65536
        local hi = byte(s, pos + 5) + byte(s, pos + 6) * 256 + byte(s, pos + 7) * 65536
        alpha = {}
        for k = 0, 7 do alpha[k + 1] = ap[floor(lo / 8 ^ k) % 8 + 1] end
        for k = 0, 7 do alpha[k + 9] = ap[floor(hi / 8 ^ k) % 8 + 1] end
        colorBlock(s, pos + 8, px, false)
      else
        colorBlock(s, pos, px, true)
      end
      for py = 0, 3 do
        local y = by * 4 + py
        if y < h then
          local row = rows[y]
          for pxx = 0, 3 do
            local x = bx * 4 + pxx
            if x < w then
              local p = px[py * 4 + pxx + 1]
              local a = alpha and alpha[py * 4 + pxx + 1] or p[4]
              row[x + 1] = char(p[1], p[2], p[3], a)
            end
          end
        end
      end
      pos = pos + blockSize
    end
  end
  for y = 0, h - 1 do out[y + 1] = concat(rows[y]) end
  return concat(out)
end

-- Returns width, height, rgba or nil, error.
function M.decode(s)
  if not s or #s < 1172 then return nil, "not a BLP file" end
  local magic = s:sub(1, 4)
  if magic ~= "BLP2" then return nil, "unsupported BLP version " .. magic end
  local compression, alphaDepth, alphaType = byte(s, 9), byte(s, 10), byte(s, 11)
  local w, h = u32(s, 13), u32(s, 17)
  local off = u32(s, 21) + 1
  local size = u32(s, 85)
  if compression == 2 then
    local kind = 1
    if alphaDepth > 1 then kind = (alphaType == 7) and 5 or 3 end
    return w, h, decodeDXT(s, off, w, h, kind)
  elseif compression == 3 then
    local out = {}
    for i = 0, w * h - 1 do
      local b, g, r, a = byte(s, off + i * 4, off + i * 4 + 3)
      out[#out + 1] = char(r, g, b, a)
    end
    return w, h, concat(out)
  elseif compression == 1 then
    local pal = {}
    for i = 0, 255 do
      local b, g, r = byte(s, 149 + i * 4, 151 + i * 4)
      pal[i] = { r, g, b }
    end
    local n = w * h
    local out = {}
    for i = 0, n - 1 do
      local p = pal[byte(s, off + i)]
      local a = 255
      if alphaDepth == 8 then a = byte(s, off + n + i)
      elseif alphaDepth == 1 then a = (floor(byte(s, off + n + floor(i / 8)) / 2 ^ (i % 8)) % 2) * 255
      elseif alphaDepth == 4 then
        local v = byte(s, off + n + floor(i / 2))
        a = ((i % 2 == 0) and (v % 16) or floor(v / 16)) * 17
      end
      out[#out + 1] = char(p[1], p[2], p[3], a)
    end
    return w, h, concat(out)
  end
  return nil, "unsupported BLP compression " .. compression
end

-- BLP bytes -> PNG bytes
function M.toPNG(s)
  local w, h, rgba = M.decode(s)
  if not w then return nil, h end
  return require("wowapi.png").encode(w, h, rgba)
end

return M
