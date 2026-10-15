-- Game art: resolves textures (file paths, fileDataIDs, atlases) to real
-- image files, downloading them on demand into a local cache.
--
-- Nothing from the game is stored in this repository. Sources, in order:
--   1. opts.artDir / $WOWAPI_ART: a folder of extracted UI textures you
--      already have (e.g. a checkout of Gethe/wow-ui-textures, or your own
--      BLP->PNG export). Matched case-insensitively.
--   2. Gethe/wow-ui-textures on GitHub (the game's UI textures as PNG),
--      indexed with a shallow partial git clone, files fetched one by one.
--   3. wago.tools: the game's own .blp file by fileDataID, converted to PNG
--      locally (wowapi/blp.lua). Covers every texture, including new art.
--   4. For icons only: Wowhead's icon CDN (zamimg), as a last fallback.
-- Numeric fileDataIDs are mapped to paths with the icon index shipped in
-- wowapi/data/icons.txt and, for other textures, the community listfile
-- (wowdev/wow-listfile), downloaded and filtered once.
local M = {}

local SEP = package.config:sub(1, 1)
local GETHE_REPO = "https://github.com/Gethe/wow-ui-textures.git"
local GETHE_RAW = "https://raw.githubusercontent.com/Gethe/wow-ui-textures/live/"
local LISTFILE_URL = "https://github.com/wowdev/wow-listfile/releases/latest/download/community-listfile.csv"
local ICON_CDN = "https://wow.zamimg.com/images/wow/icons/large/%s.jpg"
local WAGO_CASC = "https://wago.tools/api/casc/%d?download"

local moduleDir = debug.getinfo(1, "S").source:sub(2):gsub("[^/\\]*$", "")

------------------------------------------------------------------ helpers

local function exists(p)
  local f = io.open(p, "rb")
  if f then f:close(); return true end
  return false
end

local function readAll(p)
  local f = io.open(p, "rb")
  if not f then return nil end
  local s = f:read("*a")
  f:close()
  return s
end

local function sh(cmd)
  local ok = os.execute(cmd)
  return ok == true or ok == 0
end

local function q(s) return "'" .. tostring(s):gsub("'", "'\\''") .. "'" end

local function mkdirp(p) sh("mkdir -p " .. q(p)) end

local function have(tool) return sh("command -v " .. tool .. " >/dev/null 2>&1") end

-- Normalize a game texture path: "Interface\\Buttons\\UI-Panel-Button-Up.blp"
-- -> "buttons/ui-panel-button-up"
function M.normalize(path)
  path = tostring(path):gsub("\\", "/"):lower():gsub("^/+", "")
  path = path:gsub("^interface/", ""):gsub("%.[%a%d]+$", "")
  return path
end

local function urlencode(p)
  return (p:gsub("[^%w%-%._~/]", function(c) return string.format("%%%02X", string.byte(c)) end))
end

------------------------------------------------------------------ image info

-- width, height of a PNG or JPEG
function M.imageSize(data)
  if not data then return nil end
  if data:sub(1, 8) == "\137PNG\r\n\26\n" then
    local function u32(i) local a, b, c, d = data:byte(i, i + 3); return ((a * 256 + b) * 256 + c) * 256 + d end
    return u32(17), u32(21), "image/png"
  end
  if data:sub(1, 2) == "\255\216" then
    local i = 3
    while i < #data do
      if data:byte(i) ~= 255 then return nil end
      local marker = data:byte(i + 1)
      local len = data:byte(i + 2) * 256 + data:byte(i + 3)
      if marker >= 0xC0 and marker <= 0xCF and marker ~= 0xC4 and marker ~= 0xC8 and marker ~= 0xCC then
        local h = data:byte(i + 5) * 256 + data:byte(i + 6)
        local w = data:byte(i + 7) * 256 + data:byte(i + 8)
        return w, h, "image/jpeg"
      end
      i = i + 2 + len
    end
  end
  return nil
end

local B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
function M.base64(data)
  local out = {}
  for i = 1, #data, 3 do
    local a, b, c = data:byte(i, i + 2)
    local n = a * 65536 + (b or 0) * 256 + (c or 0)
    local c1 = math.floor(n / 262144) % 64
    local c2 = math.floor(n / 4096) % 64
    local c3 = math.floor(n / 64) % 64
    local c4 = n % 64
    out[#out + 1] = B64:sub(c1 + 1, c1 + 1) .. B64:sub(c2 + 1, c2 + 1)
      .. (b and B64:sub(c3 + 1, c3 + 1) or "=") .. (c and B64:sub(c4 + 1, c4 + 1) or "=")
  end
  return table.concat(out)
end

------------------------------------------------------------------ shipped indexes

local iconsById, iconsByName, iconList
function M.icons()
  if iconsById then return iconsById, iconsByName, iconList end
  iconsById, iconsByName, iconList = {}, {}, {}
  local f = io.open(moduleDir .. "data" .. SEP .. "icons.txt", "r")
  if f then
    for line in f:lines() do
      local id, name = line:match("^(%d+) (.+)$")
      if id then
        id = tonumber(id)
        iconsById[id] = name
        if not iconsByName[name] then iconsByName[name] = id end
        iconList[#iconList + 1] = { id, name }
      end
    end
    f:close()
  end
  return iconsById, iconsByName, iconList
end

local atlases
function M.atlases()
  if atlases then return atlases end
  atlases = {}
  local f = io.open(moduleDir .. "data" .. SEP .. "atlases.txt", "r")
  if f then
    for line in f:lines() do
      if line:sub(1, 1) ~= "#" then
        local p = {}
        for x in (line .. "\t"):gmatch("(.-)\t") do p[#p + 1] = x end
        local name = p[1]
        local e = { name = name, fileID = tonumber(p[2]), path = p[3] ~= "" and p[3] or nil,
          width = tonumber(p[4]), height = tonumber(p[5]), left = tonumber(p[6]), right = tonumber(p[7]),
          top = tonumber(p[8]), bottom = tonumber(p[9]), tilesH = p[10] == "1", tilesV = p[11] == "1" }
        local key = name:lower()
        atlases[key] = atlases[key] or {}
        table.insert(atlases[key], e)
      end
    end
    f:close()
  end
  return atlases
end

-- Logical atlas info (the smallest variant is the 1x size the UI uses).
function M.atlasInfo(name)
  if type(name) ~= "string" then return nil end
  local variants = M.atlases()[name:lower()]
  if not variants then return nil end
  local best = variants[1]
  for _, v in ipairs(variants) do if v.width < best.width then best = v end end
  return best, variants
end

------------------------------------------------------------------ store

local Store = {}
Store.__index = Store

function M.store(opts)
  opts = opts or {}
  local self = setmetatable({}, Store)
  self.cacheDir = opts.cacheDir or os.getenv("WOWAPI_CACHE") or (moduleDir .. ".." .. SEP .. ".wowtest" .. SEP .. "cache")
  self.artDir = opts.artDir or os.getenv("WOWAPI_ART")
  self.offline = opts.offline or os.getenv("WOWAPI_OFFLINE") == "1"
  self.verbose = opts.verbose
  self.missing = {}
  self.memo = {}
  return self
end

function Store:log(msg) if self.verbose then io.stderr:write("[art] " .. msg .. "\n") end end

-- Case-insensitive index of a local art folder.
function Store:localIndex()
  if self._local ~= nil then return self._local end
  self._local = false
  if not self.artDir then return false end
  local idx = {}
  local p = io.popen("cd " .. q(self.artDir) .. " && find . -type f \\( -iname '*.png' -o -iname '*.jpg' -o -iname '*.tga' \\) 2>/dev/null")
  if p then
    for line in p:lines() do
      local rel = line:gsub("^%./", "")
      idx[M.normalize(rel)] = self.artDir .. SEP .. rel
    end
    p:close()
  end
  self._local = idx
  return idx
end

-- Index of the Gethe/wow-ui-textures repository: normalized path -> repo path.
function Store:getheIndex()
  if self._gethe ~= nil then return self._gethe end
  self._gethe = false
  local indexFile = self.cacheDir .. SEP .. "wow-ui-textures.index"
  if not exists(indexFile) then
    if self.offline or not have("git") then return false end
    mkdirp(self.cacheDir)
    local repo = self.cacheDir .. SEP .. "wow-ui-textures"
    if not exists(repo .. SEP .. ".git" .. SEP .. "HEAD") then
      self:log("indexing Gethe/wow-ui-textures (one-time, metadata only)...")
      if not sh("git clone --quiet --depth 1 --filter=blob:none --no-checkout " .. GETHE_REPO .. " " .. q(repo) .. " >/dev/null 2>&1") then
        return false
      end
    end
    if not sh("git -C " .. q(repo) .. " ls-tree -r --name-only HEAD > " .. q(indexFile .. ".tmp") .. " 2>/dev/null") then return false end
    os.rename(indexFile .. ".tmp", indexFile)
  end
  local idx = {}
  local f = io.open(indexFile, "r")
  if not f then return false end
  for line in f:lines() do
    if line:lower():match("%.png$") then idx[M.normalize(line)] = line end
  end
  f:close()
  self._gethe = idx
  return idx
end

-- fileDataID -> normalized path, from the community listfile (downloaded once).
function Store:listfile()
  if self._listfile ~= nil then return self._listfile end
  self._listfile = false
  local filtered = self.cacheDir .. SEP .. "listfile-interface.txt"
  if not exists(filtered) then
    if self.offline or not have("curl") then return false end
    mkdirp(self.cacheDir)
    self:log("downloading the community listfile (one-time, ~150 MB, filtered to UI textures)...")
    local raw = self.cacheDir .. SEP .. "listfile.csv"
    if not sh("curl -sfL --max-time 900 -o " .. q(raw) .. " " .. LISTFILE_URL) then return false end
    local out = io.open(filtered .. ".tmp", "w")
    for line in io.lines(raw) do
      local l = line:gsub("\r$", "")
      local id, path = l:match("^(%d+);(interface/.+%.blp)$")
      if not id then id, path = l:match("^(%d+);(fonts/.+%.ttf)$") end
      if id then out:write(id, ";", path, "\n") end
    end
    out:close()
    os.remove(raw)
    os.rename(filtered .. ".tmp", filtered)
  end
  local map, rev = {}, {}
  for line in io.lines(filtered) do
    local id, path = line:match("^(%d+);(.+)$")
    if id then
      local n = M.normalize(path)
      id = tonumber(id)
      map[id] = n
      if not rev[n] then rev[n] = id end
    end
  end
  self._listfile = map
  self._reverse = rev
  return map
end

-- fileDataID for a normalized path (icons offline, others via the listfile).
function Store:fileIDFor(norm)
  local icon = norm:match("^icons/(.+)$")
  if icon then
    local _, byName = M.icons()
    if byName[icon] then return byName[icon] end
  end
  if self:listfile() then return self._reverse[norm] end
end

-- Normalized path for a texture given as path or fileDataID.
function Store:pathFor(tex)
  if type(tex) == "number" then
    local name = (M.icons())[tex]
    if name then return "icons/" .. name end
    local lf = self:listfile()
    return lf and lf[tex] or nil
  end
  if type(tex) == "string" then
    if tex:match("^%d+$") then return self:pathFor(tonumber(tex)) end
    return M.normalize(tex)
  end
end

local function download(url, dest)
  return sh("curl -sfL --max-time 60 -o " .. q(dest .. ".part") .. " " .. q(url)) and os.rename(dest .. ".part", dest)
end

-- Addons' own textures (Interface\AddOns\<Addon>\...): .png used as is,
-- .tga/.blp converted to PNG in the cache.
function Store:addonFile(norm)
  local addon, rest = norm:match("^addons/([^/]+)/(.+)$")
  if not addon or not self.addonDirs then return nil end
  local dir = self.addonDirs[addon]
  if not dir then return nil end
  self._addonIndex = self._addonIndex or {}
  local idx = self._addonIndex[addon]
  if not idx then
    idx = {}
    local p = io.popen("cd " .. q(dir) .. " && find . -type f \\( -iname '*.tga' -o -iname '*.blp' -o -iname '*.png' -o -iname '*.jpg' \\) 2>/dev/null")
    if p then
      for line in p:lines() do
        local rel = line:gsub("^%./", "")
        local key = rel:lower():gsub("\\", "/"):gsub("%.[%a%d]+$", "")
        -- prefer png > blp > tga when several exist
        local ext = rel:lower():match("%.(%a+)$")
        local rank = ({ png = 3, jpg = 3, blp = 2, tga = 1 })[ext] or 0
        if not idx[key] or idx[key].rank < rank then idx[key] = { path = dir .. SEP .. rel, ext = ext, rank = rank } end
      end
      p:close()
    end
    self._addonIndex[addon] = idx
  end
  local e = idx[rest]
  if not e then return nil end
  if e.ext == "png" or e.ext == "jpg" then return e.path end
  local cached = self.cacheDir .. SEP .. "addonart" .. SEP .. addon .. SEP .. rest:gsub("/", SEP) .. ".png"
  if exists(cached) then return cached end
  local data = readAll(e.path)
  local png = data and require(e.ext == "blp" and "wowapi.blp" or "wowapi.tga").toPNG(data)
  if not png then return nil end
  mkdirp((cached:gsub("[/\\][^/\\]*$", "")))
  local f = io.open(cached, "wb")
  f:write(png)
  f:close()
  return cached
end

-- Local file for a normalized texture path (downloading it if needed).
function Store:fileFor(norm)
  if not norm then return nil end
  if self.memo[norm] ~= nil then return self.memo[norm] or nil end
  local found = self:addonFile(norm)
  if found then self.memo[norm] = found; return found end
  local li = self:localIndex()
  if li and li[norm] then found = li[norm] end
  if not found then
    local cached = self.cacheDir .. SEP .. "files" .. SEP .. norm:gsub("/", SEP)
    for _, ext in ipairs({ ".png", ".jpg" }) do
      if exists(cached .. ext) then found = cached .. ext end
    end
    if not found and not self.offline then
      local gi = self:getheIndex()
      local repoPath = gi and gi[norm]
      mkdirp((cached:gsub("[/\\][^/\\]*$", "")))
      if repoPath then
        if have("curl") and download(GETHE_RAW .. urlencode(repoPath), cached .. ".png") then
          found = cached .. ".png"
        else
          local repo = self.cacheDir .. SEP .. "wow-ui-textures"
          if sh("git -C " .. q(repo) .. " show " .. q("HEAD:" .. repoPath) .. " > " .. q(cached .. ".png") .. " 2>/dev/null") then
            found = cached .. ".png"
          else
            os.remove(cached .. ".png")
          end
        end
      end
      if not found and have("curl") then
        local id = self:fileIDFor(norm)
        if id and download(WAGO_CASC:format(id), cached .. ".blp") then
          local blp = readAll(cached .. ".blp")
          os.remove(cached .. ".blp")
          local png = blp and require("wowapi.blp").toPNG(blp)
          if png then
            local f = io.open(cached .. ".png", "wb")
            f:write(png)
            f:close()
            found = cached .. ".png"
          end
        end
      end
      local icon = not found and norm:match("^icons/(.+)$")
      if icon and have("curl") and download(ICON_CDN:format(icon), cached .. ".jpg") then
        found = cached .. ".jpg"
      end
    end
  end
  self.memo[norm] = found or false
  if not found then self:log("not available: " .. norm) end
  return found
end

-- Image for a texture: { file, width, height, mime, data } or nil.
function Store:image(tex)
  local norm = self:pathFor(tex)
  local key = "img:" .. tostring(norm)
  if self.memo[key] ~= nil then return self.memo[key] or nil end
  local file = self:fileFor(norm)
  local img = false
  if file then
    local data = readAll(file)
    local w, h, mime = M.imageSize(data)
    if w then img = { file = file, width = w, height = h, mime = mime, data = data, path = norm } end
  end
  self.memo[key] = img
  return img or nil
end

-- A game font file: { file, data, mime } or nil.
function Store:font(path)
  local norm = M.normalize(path)
  local key = "font:" .. norm
  if self.memo[key] ~= nil then return self.memo[key] or nil end
  local result = false
  local li = self:localIndex()
  local file = li and li[norm]
  local cached = self.cacheDir .. SEP .. "files" .. SEP .. norm:gsub("/", SEP) .. ".ttf"
  if not file and exists(cached) then file = cached end
  if not file and not self.offline and have("curl") then
    local id = self:fileIDFor(norm)
    mkdirp((cached:gsub("[/\\][^/\\]*$", "")))
    if id and download(WAGO_CASC:format(id), cached) then file = cached end
  end
  if file then result = { file = file, data = readAll(file), mime = "font/ttf" } end
  self.memo[key] = result
  return result or nil
end

-- Resolve an atlas to { image, left, right, top, bottom, width, height }.
function Store:atlas(name)
  local logical, variants = M.atlasInfo(name)
  if not logical then return nil end
  -- sharpest art first
  local sorted = {}
  for _, v in ipairs(variants) do sorted[#sorted + 1] = v end
  table.sort(sorted, function(x, y) return x.width > y.width end)
  for _, a in ipairs(sorted) do
    local img = (a.path and self:image(a.path)) or (a.fileID and self:image(a.fileID))
    if img then
      return { image = img, left = a.left, right = a.right, top = a.top, bottom = a.bottom,
        width = logical.width, height = logical.height, tilesH = a.tilesH, tilesV = a.tilesV }
    end
  end
  return { width = logical.width, height = logical.height }
end

function Store:dataURI(img)
  if not img.uri then img.uri = "data:" .. img.mime .. ";base64," .. M.base64(img.data) end
  return img.uri
end

-- Download everything a list of textures needs ahead of time.
function Store:prefetch(list)
  local n = 0
  for _, t in ipairs(list) do if self:image(t) then n = n + 1 end end
  return n
end

return M
