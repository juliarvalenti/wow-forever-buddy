-- Minimal PNG encoder (RGBA8, stored deflate blocks). Pure Lua 5.1, no
-- bit library needed.
local M = {}

local char, byte, floor, concat = string.char, string.byte, math.floor, table.concat

-- 8-bit XOR lookup table
local XOR8
local function xor8()
  if XOR8 then return XOR8 end
  XOR8 = {}
  for a = 0, 255 do
    for b = 0, 255 do
      local r, p, x, y = 0, 1, a, b
      for _ = 1, 8 do
        local ba, bb = x % 2, y % 2
        if ba ~= bb then r = r + p end
        x, y, p = (x - ba) / 2, (y - bb) / 2, p * 2
      end
      XOR8[a * 256 + b] = r
    end
  end
  return XOR8
end

-- CRC32 table split into bytes: T[i] = {b0, b1, b2, b3}
local CRC
local function crcTable()
  if CRC then return CRC end
  CRC = {}
  for n = 0, 255 do
    local c = n
    for _ = 1, 8 do
      if c % 2 == 1 then
        -- c = 0xEDB88320 xor (c >> 1)
        local h = floor(c / 2)
        local r, p, x, y = 0, 1, h, 0xEDB88320
        for _ = 1, 32 do
          local bx, by = x % 2, y % 2
          if bx ~= by then r = r + p end
          x, y, p = (x - bx) / 2, (y - by) / 2, p * 2
        end
        c = r
      else
        c = floor(c / 2)
      end
    end
    CRC[n] = { c % 256, floor(c / 256) % 256, floor(c / 65536) % 256, floor(c / 16777216) % 256 }
  end
  return CRC
end

local function crc32(...)
  local X, T = xor8(), crcTable()
  local c0, c1, c2, c3 = 255, 255, 255, 255
  for i = 1, select("#", ...) do
    local s = select(i, ...)
    for j = 1, #s do
      local t = T[X[c0 * 256 + byte(s, j)]]
      c0, c1, c2, c3 = X[c1 * 256 + t[1]], X[c2 * 256 + t[2]], X[c3 * 256 + t[3]], t[4]
    end
  end
  return char(255 - c3, 255 - c2, 255 - c1, 255 - c0)
end

local function u32(n)
  return char(floor(n / 16777216) % 256, floor(n / 65536) % 256, floor(n / 256) % 256, n % 256)
end

local function chunk(kind, data)
  return u32(#data) .. kind .. data .. crc32(kind, data)
end

local function adler32(s)
  local a, b = 1, 0
  for i = 1, #s do
    a = (a + byte(s, i)) % 65521
    b = (b + a) % 65521
  end
  return u32(b * 65536 + a)
end

-- rgba: string of width*height*4 bytes, row-major, top row first.
function M.encode(width, height, rgba)
  local rows = {}
  local stride = width * 4
  for y = 0, height - 1 do rows[#rows + 1] = "\0" .. rgba:sub(y * stride + 1, (y + 1) * stride) end
  local raw = concat(rows)
  local blocks = { "\120\1" }
  local pos, len = 1, #raw
  repeat
    local n = math.min(65535, len - pos + 1)
    local final = (pos + n > len) and 1 or 0
    blocks[#blocks + 1] = char(final, n % 256, floor(n / 256), 255 - n % 256, 255 - floor(n / 256))
    blocks[#blocks + 1] = raw:sub(pos, pos + n - 1)
    pos = pos + n
  until pos > len
  blocks[#blocks + 1] = adler32(raw)
  local ihdr = u32(width) .. u32(height) .. "\8\6\0\0\0"
  return "\137PNG\r\n\26\n" .. chunk("IHDR", ihdr) .. chunk("IDAT", concat(blocks)) .. chunk("IEND", "")
end

return M
