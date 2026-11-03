-- .toc parsing and addon discovery.
local M = {}

local function trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end

local function splitList(s)
  local out = {}
  for item in (s or ""):gmatch("[^,]+") do
    item = trim(item)
    if item ~= "" then out[#out + 1] = item end
  end
  return out
end
M.splitList = splitList

function M.exists(path)
  local f = io.open(path, "rb")
  if f then f:close(); return true end
  return false
end

-- WoW reads addon files case-insensitively (Windows); find `path` on a
-- case-sensitive disk by matching each missing component ignoring case.
function M.resolve(path)
  if M.exists(path) then return path end
  local abs = path:sub(1, 1) == "/"
  local cur = abs and "" or "."
  local parts = {}
  for part in path:gmatch("[^/\\]+") do parts[#parts + 1] = part end
  for i, part in ipairs(parts) do
    local cand = (cur == "" and "/" or cur .. "/") .. part
    if part == "." or part == ".." or M.exists(cand) then
      cur = cand
    else
      local found
      local p = io.popen('ls -1a "' .. (cur == "" and "/" or cur) .. '" 2>/dev/null')
      if p then
        for n in p:lines() do if n:lower() == part:lower() then found = n; break end end
        p:close()
      end
      if not found then return nil end
      cur = (cur == "" and "/" or cur .. "/") .. found
    end
    if i == #parts then return abs and cur or cur:gsub("^%./", "") end
  end
end

function M.readFile(path)
  local f, err = io.open(path, "rb")
  if not f then return nil, err end
  local s = f:read("*a")
  f:close()
  return s
end

function M.basename(path) return (path:gsub("[/\\]+$", ""):match("([^/\\]+)$")) end
function M.dirname(path)
  local d = path:gsub("[/\\]+$", ""):match("^(.*)[/\\][^/\\]+$")
  return d or "."
end
function M.join(a, b)
  if b:match("^/") or b:match("^%a:[/\\]") then return b end
  if a == "" or a == "." then return b end
  return (a:gsub("[/\\]+$", "")) .. "/" .. b
end

-- Find the .toc for an addon directory. Prefers Name_Forever.toc /
-- Name_Mainline.toc flavor files, then Name.toc.
function M.findToc(dir)
  local name = M.basename(dir)
  if not name or name == "." or name == ".." then return nil, name end
  for _, suffix in ipairs({ "_Camelot", "-Camelot", "_Forever", "-Forever", "_Mainline", "-Mainline", "" }) do
    local p = M.join(dir, name .. suffix .. ".toc")
    if M.exists(p) then return p, name end
  end
  return nil, name
end

-- Parse toc text into { name, metadata = {}, files = {} }.
-- WoW: Forever's client game type is "camelot" (addons target it with
-- [AllowLoadGameType camelot]); it is not "standard" (retail).
M.GAME_TYPE = "camelot"
M.FAMILY = "Mainline"
M.GAME = "Camelot"

-- Evaluate [AllowLoadGameType a, b] / [ExcludeLoadGameType a, b] on a line.
-- WFB patch (HUD2): a tag names a game type or the family: "mainline" takes
-- in Forever's camelot (Blizzard_ChatFrame lists a Mainline file as
-- [AllowLoadGameType mainline] [ExcludeLoadGameType camelot], and Forever's
-- target frame needs Shared\TargetFrameAuraShared.lua, tagged mainline).
function M.gameTypeAllowed(line, gameType)
  gameType = (gameType or M.GAME_TYPE):lower()
  local family = M.FAMILY:lower()
  local function names(g) g = g:lower(); return g == gameType or g == family end
  local ok = true
  for list in line:gmatch("%[AllowLoadGameType%s+([^%]]+)%]") do
    local found = false
    for g in list:gmatch("[^,%s]+") do if names(g) then found = true end end
    if not found then ok = false end
  end
  for list in line:gmatch("%[ExcludeLoadGameType%s+([^%]]+)%]") do
    for g in list:gmatch("[^,%s]+") do if names(g) then ok = false end end
  end
  return ok
end

-- WFB patch (HUD1): Blizzard's TOCs list one dependency per "## Dep:" line,
-- each with its own game-type tag, so these keys collect every line whose
-- tag allows this client instead of keeping the last.
local LIST_KEYS = { dep = "deps", deps = "deps", dependencies = "deps", requireddeps = "deps",
  requireddep = "deps", dependancies = "deps", optionaldeps = "optionaldeps", optionaldep = "optionaldeps" }

function M.parse(text, name)
  local toc = { name = name, metadata = {}, files = {} }
  local lists = { deps = {}, optionaldeps = {} }
  text = text:gsub("^\239\187\191", "")
  for line in (text .. "\n"):gmatch("([^\r\n]*)\r?\n") do
    local key, val = line:match("^##%s*([^:]-)%s*:%s*(.-)%s*$")
    if key then
      local list = LIST_KEYS[key:lower()]
      if list then
        if M.gameTypeAllowed(val) then
          for _, d in ipairs(splitList((val:gsub("%[[^%]]*%]", "")))) do table.insert(lists[list], d) end
        end
      else
        toc.metadata[key] = val
        toc.metadata[key:lower()] = val
      end
    elseif not line:match("^%s*#") then
      local file = trim(line)
      -- WFB patch (HUD1): [AllowLoad Glue] files are for the login screens,
      -- not the game; and once the path tokens ([Family], [Game]) are
      -- filled in, any other tag ([LoadIntoEnvironment secure]) is not part
      -- of the file name.
      local allowed = M.gameTypeAllowed(file) and not file:match("%[AllowLoad%s+[Gg]lue%]")
      file = file:gsub("%[Family%]", M.FAMILY):gsub("%[Game%]", M.GAME):gsub("%[TextLocale%]", M.LOCALE or "enUS")
      file = trim(file:gsub("%[[^%]]*%]", ""))
      if file ~= "" and allowed then
        file = file:gsub("\\", "/")
        toc.files[#toc.files + 1] = file
      end
    end
  end
  local md = toc.metadata
  toc.title = md.title or name
  toc.interface = splitList(md.interface)
  toc.deps = lists.deps
  toc.optionalDeps = lists.optionaldeps
  toc.savedVariables = splitList(md.savedvariables)
  toc.savedVariablesPerCharacter = splitList(md.savedvariablespercharacter)
  toc.loadOnDemand = md.loadondemand == "1"
  -- addon-level game type restrictions
  local allow, exclude = md.allowloadgametype, md.excludeloadgametype
  toc.gameTypeOk = M.gameTypeAllowed((allow and ("[AllowLoadGameType " .. allow .. "]") or "")
    .. (exclude and ("[ExcludeLoadGameType " .. exclude .. "]") or ""))
  return toc
end

function M.load(dir)
  local path, name = M.findToc(dir)
  if not path then return nil, "no .toc file found in " .. dir .. " (expected " .. name .. ".toc)" end
  local toc = M.parse(assert(M.readFile(path)), name)
  toc.dir = dir
  toc.path = path
  return toc
end

-- Minimal XML support: only <Script file=""/> and <Include file=""/> are
-- followed. Frames declared in XML are reported, not built.
function M.xmlFiles(path)
  local text, err = M.readFile(path)
  if not text then return nil, err end
  text = text:gsub("<!%-%-.-%-%->", "")
  local out, unsupported = {}, {}
  for tag, attrs in text:gmatch("<%s*([%w_]+)(.-)/?>") do
    local file = attrs:match('file%s*=%s*"([^"]+)"') or attrs:match("file%s*=%s*'([^']+)'")
    if (tag == "Script" or tag == "Include") and file then
      out[#out + 1] = file:gsub("\\", "/")
    elseif tag == "Frame" or tag == "Button" or tag == "CheckButton" or tag == "StatusBar" then
      unsupported[#unsupported + 1] = tag
    end
  end
  return out, unsupported
end

return M
