-- TGA decoder (the other texture format addons ship): uncompressed and RLE
-- true-color (24/32-bit) and grayscale. Returns width, height, RGBA8.
local M = {}
local byte, char, concat = string.byte, string.char, table.concat

function M.decode(s)
  if not s or #s < 18 then return nil, "not a TGA file" end
  local idLen, cmapType, imgType = byte(s, 1, 3)
  local cmapLen = byte(s, 6) + byte(s, 7) * 256
  local cmapDepth = byte(s, 8)
  local w = byte(s, 13) + byte(s, 14) * 256
  local h = byte(s, 15) + byte(s, 16) * 256
  local depth, desc = byte(s, 17), byte(s, 18)
  if not (imgType == 2 or imgType == 3 or imgType == 10 or imgType == 11) then
    return nil, "unsupported TGA type " .. imgType
  end
  local bpp = math.floor(depth / 8)
  local pos = 19 + idLen + (cmapType == 1 and cmapLen * math.floor((cmapDepth + 7) / 8) or 0)
  local gray = imgType == 3 or imgType == 11
  local function pixel(p)
    if gray then local v = byte(s, p); return char(v, v, v, 255) end
    local b, g, r, a = byte(s, p, p + 3)
    return char(r, g, b, bpp == 4 and a or 255)
  end
  local px, n = {}, w * h
  if imgType == 2 or imgType == 3 then
    for i = 0, n - 1 do px[i + 1] = pixel(pos + i * bpp) end
  else
    local i = 1
    while i <= n do
      local hdr = byte(s, pos); pos = pos + 1
      local count = (hdr % 128) + 1
      if hdr >= 128 then
        local p = pixel(pos); pos = pos + bpp
        for _ = 1, count do px[i] = p; i = i + 1 end
      else
        for _ = 1, count do px[i] = pixel(pos); pos = pos + bpp; i = i + 1 end
      end
    end
  end
  -- rows are bottom-up unless the top-left origin bit is set
  local rows = {}
  local topDown = math.floor(desc / 32) % 2 == 1
  for y = 0, h - 1 do
    local src = topDown and y or (h - 1 - y)
    rows[y + 1] = concat(px, "", src * w + 1, src * w + w)
  end
  return w, h, concat(rows)
end

function M.toPNG(s)
  local w, h, rgba = M.decode(s)
  if not w then return nil, h end
  return require("wowapi.png").encode(w, h, rgba)
end

return M
