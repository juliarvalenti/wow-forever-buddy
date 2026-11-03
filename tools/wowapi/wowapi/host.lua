-- WFB patch (PV1): the one place WOWAPI touches the file system.
--
-- Run from the command line (luajit, lua5.1), these use `io` and the POSIX
-- shell as WOWAPI always did. Embedded in the app (PV1's live preview), the
-- Lua state has no io, os or shell at all, and the app puts a table of its
-- own functions in the global WOWAPI_HOST instead: read and list confined to
-- the bundled tree and the preview's scratch folder, write to that folder
-- only. That keeps the sandbox in Rust, and works on Windows, where ls,
-- find, pwd and mkdir -p don't.
--
--   host.read(path)         -> contents, or nil and an error
--   host.list(dir)          -> { names } (files and folders), or nil
--   host.walk(dir, exts)    -> { "./sub/file.ext", ... } with one of the
--                              lowercase extensions in `exts` ({ png = true })
--   host.exists(path)       -> boolean
--   host.write(path, text)  -> true, or nil and an error
--   host.mkdir(path)        -> makes it and its parents
--   host.cwd()              -> the current folder

local M = {}

local injected = rawget(_G, "WOWAPI_HOST")

local function q(s)
  return "'" .. tostring(s):gsub("'", "'\\''") .. "'"
end

if injected then
  M.read = injected.read
  M.list = injected.list
  M.walk = injected.walk
  M.exists = injected.exists
  M.write = injected.write
  M.mkdir = injected.mkdir
  M.cwd = injected.cwd
  M.embedded = true
  return M
end

function M.read(path)
  local f, err = io.open(path, "rb")
  if not f then return nil, err end
  local s = f:read("*a")
  f:close()
  return s
end

function M.exists(path)
  local f = io.open(path, "rb")
  if f then f:close(); return true end
  return false
end

function M.list(dir)
  local p = io.popen("ls -1a " .. q(dir == "" and "/" or dir) .. " 2>/dev/null")
  if not p then return nil end
  local out = {}
  for n in p:lines() do
    if n ~= "." and n ~= ".." then out[#out + 1] = n end
  end
  p:close()
  return out
end

function M.walk(dir, exts)
  local names = {}
  for e in pairs(exts or {}) do names[#names + 1] = "-iname '*." .. e .. "'" end
  local filter = #names > 0 and ("\\( " .. table.concat(names, " -o ") .. " \\) ") or ""
  local p = io.popen("cd " .. q(dir) .. " && find . -type f " .. filter .. "2>/dev/null")
  local out = {}
  if p then
    for line in p:lines() do out[#out + 1] = line end
    p:close()
  end
  return out
end

function M.write(path, text)
  local f, err = io.open(path, "wb")
  if not f then return nil, err end
  f:write(text)
  f:close()
  return true
end

function M.mkdir(path)
  local r = os.execute("mkdir -p " .. q(path))
  return r == 0 or r == true
end

local cwd
function M.cwd()
  if not cwd then
    local h = io.popen("pwd")
    cwd = h and h:read("*l") or "."
    if h then h:close() end
  end
  return cwd
end

return M
