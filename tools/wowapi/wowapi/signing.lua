-- Signed addon releases: authors sign a manifest of their addon's files with
-- an Ed25519 key; `wowtest verify` / `wowtest install` check that every file
-- is exactly what the author signed, and remember each addon's key so a
-- changed key (a hijacked mirror, a fake re-upload) is flagged.
--
-- This protects players from tampered downloads. It is not DRM: the files
-- stay plain, readable Lua, as Blizzard's add-on policy requires.
--
-- Uses the `openssl` command-line tool (1.1.1+), which ships with macOS,
-- Linux and Git for Windows.
local M = {}

M.MANIFEST = "wowtest.manifest"
M.SIGNATURE = "wowtest.sig"
M.FORMAT = "wowtest-manifest 1"

local function q(s) return "'" .. tostring(s):gsub("'", "'\\''") .. "'" end
local function sh(cmd)
  local r = os.execute(cmd .. " >/dev/null 2>&1")
  return r == true or r == 0
end
local function read(cmd)
  local p = io.popen(cmd .. " 2>/dev/null")
  if not p then return "" end
  local s = p:read("*a") or ""
  p:close()
  return s
end
local function readFile(path)
  local f = io.open(path, "rb")
  if not f then return nil end
  local s = f:read("*a")
  f:close()
  return s
end
local function writeFile(path, s)
  local f = assert(io.open(path, "wb"))
  f:write(s)
  f:close()
end
local function tmp() return os.tmpname() end
local function basename(p) return (p:gsub("/+$", ""):match("([^/]+)$")) end

function M.available() return sh("openssl version") end

local function requireOpenssl()
  if not M.available() then error("signing needs the `openssl` command-line tool", 0) end
end

-- Short, readable fingerprint of a public key (SHA-256 of its DER form).
function M.fingerprint(pubPem)
  local f = tmp()
  writeFile(f, pubPem)
  local hex = read("openssl pkey -pubin -in " .. q(f) .. " -outform DER | openssl dgst -sha256 -r"):match("^(%x+)")
  os.remove(f)
  if not hex then return nil end
  local groups = {}
  for i = 1, 32, 4 do groups[#groups + 1] = hex:sub(i, i + 3) end
  return table.concat(groups, ":")
end

-- Create a key pair: <dir>/<name>.key (private, keep it secret) and .pub.
function M.keygen(dir, name)
  requireOpenssl()
  sh("mkdir -p " .. q(dir))
  local key, pub = dir .. "/" .. name .. ".key", dir .. "/" .. name .. ".pub"
  if readFile(key) then error(key .. " already exists; not overwriting it", 0) end
  if not sh("umask 077 && openssl genpkey -algorithm ed25519 -out " .. q(key)) then error("openssl genpkey failed", 0) end
  sh("openssl pkey -in " .. q(key) .. " -pubout -out " .. q(pub))
  return key, pub, M.fingerprint(readFile(pub))
end

-- Files under `root`, relative and sorted. For signing (`tracked`), a git
-- checkout contributes only its committed files, so build junk never
-- matters; verifying always looks at every file actually on disk, so
-- nothing can be slipped in beside the signed ones.
function M.listFiles(root, tracked)
  local out = {}
  local isGit = tracked and sh("git -C " .. q(root) .. " rev-parse --is-inside-work-tree")
  local listing
  if isGit then
    listing = read("git -C " .. q(root) .. " ls-files -z --cached .")
    for f in listing:gmatch("([^%z]+)") do out[#out + 1] = f end
  else
    listing = read("cd " .. q(root) .. " && find . -type f -not -path './.git/*' -print0")
    for f in listing:gmatch("([^%z]+)") do out[#out + 1] = (f:gsub("^%./", "")) end
  end
  local keep = {}
  for _, f in ipairs(out) do
    if f ~= M.MANIFEST and f ~= M.SIGNATURE and readFile(root .. "/" .. f) then keep[#keep + 1] = f end
  end
  table.sort(keep)
  return keep
end

-- Files in a git checkout that aren't committed (a fresh clone won't have
-- them, so signing them would make every player's copy look tampered).
function M.untracked(root)
  local out = {}
  local listing = read("git -C " .. q(root) .. " ls-files -z --others --exclude-standard .")
  for f in listing:gmatch("([^%z]+)") do
    if f ~= M.MANIFEST and f ~= M.SIGNATURE then out[#out + 1] = f end
  end
  table.sort(out)
  return out
end

local function sha256(path)
  return (read("openssl dgst -sha256 -r " .. q(path)):match("^(%x+)"))
end

-- The addon's name: its root .toc (minus flavor suffix), else the folder.
function M.addonName(root)
  local p = io.popen("ls " .. q(root) .. "/*.toc 2>/dev/null")
  local first = p and p:read("*l")
  if p then p:close() end
  if first then
    local n = basename(first):gsub("%.toc$", "")
    return (n:gsub("_(%a+)$", function(suf)
      local l = suf:lower()
      if l == "mainline" or l == "classic" or l == "vanilla" or l == "tbc" or l == "wrath" or l == "cata"
        or l == "mists" or l == "camelot" or l == "forever" or l == "standard" then return "" end
    end))
  end
  return basename(root)
end

local function tocField(root, field)
  local p = io.popen("ls " .. q(root) .. "/*.toc 2>/dev/null")
  local tocPath = p and p:read("*l")
  if p then p:close() end
  local text = tocPath and readFile(tocPath) or ""
  return text:match("##%s*" .. field .. "%s*:%s*([^\r\n]+)")
end

-- The manifest text for `root` (exact bytes that get signed).
function M.buildManifest(root, fingerprint)
  local files = M.listFiles(root, true)
  local lines = { M.FORMAT,
    "addon: " .. M.addonName(root),
    "version: " .. (tocField(root, "Version") or "unknown"),
    "key: " .. (fingerprint or "?"),
    "files: " .. #files }
  for _, f in ipairs(files) do
    local h = sha256(root .. "/" .. f)
    if not h then error("could not hash " .. f, 0) end
    lines[#lines + 1] = h .. "  " .. f
  end
  return table.concat(lines, "\n") .. "\n", #files
end

local function b64(path) return (read("openssl base64 -A -in " .. q(path))) end
local function unb64(s, path)
  local f = tmp()
  writeFile(f, s)
  local ok = sh("openssl base64 -d -A -in " .. q(f) .. " -out " .. q(path))
  os.remove(f)
  return ok
end

-- Sign `root` with a private key: writes wowtest.manifest and wowtest.sig.
function M.sign(root, keyPath)
  requireOpenssl()
  if not readFile(keyPath) then error("no private key at " .. keyPath, 0) end
  local pubPem = read("openssl pkey -in " .. q(keyPath) .. " -pubout")
  if not pubPem:find("BEGIN PUBLIC KEY") then error(keyPath .. " is not a usable private key", 0) end
  local fp = M.fingerprint(pubPem)
  local manifest, n = M.buildManifest(root, fp)
  local mpath = root .. "/" .. M.MANIFEST
  writeFile(mpath, manifest)
  local sigBin = tmp()
  if not sh("openssl pkeyutl -sign -inkey " .. q(keyPath) .. " -rawin -in " .. q(mpath) .. " -out " .. q(sigBin)) then
    os.remove(sigBin)
    error("openssl could not sign (needs OpenSSL 1.1.1+ with Ed25519)", 0)
  end
  writeFile(root .. "/" .. M.SIGNATURE, "signature: " .. b64(sigBin) .. "\n" .. pubPem)
  os.remove(sigBin)
  return { files = n, fingerprint = fp }
end

-- Check `root` against its signature.
-- Returns { status = "valid" | "invalid" | "unsigned", fingerprint, addon,
--           version, modified = {}, missing = {}, extra = {}, reason }.
-- opts.pubkey pins the expected public key (PEM text).
function M.verify(root, opts)
  opts = opts or {}
  local res = { modified = {}, missing = {}, extra = {} }
  local manifest = readFile(root .. "/" .. M.MANIFEST)
  local sig = readFile(root .. "/" .. M.SIGNATURE)
  if not manifest or not sig then res.status = "unsigned"; return res end
  requireOpenssl()
  local sigB64 = sig:match("signature:%s*([%w%+/=]+)")
  local pubPem = sig:match("(%-%-%-%-%-BEGIN PUBLIC KEY%-%-%-%-%-.-%-%-%-%-%-END PUBLIC KEY%-%-%-%-%-)")
  local function bad(why) res.status = "invalid"; res.reason = why; return res end
  if not sigB64 or not pubPem then return bad("signature file is malformed") end
  pubPem = pubPem .. "\n"
  res.fingerprint = M.fingerprint(pubPem)
  if opts.pubkey and M.fingerprint(opts.pubkey) ~= res.fingerprint then
    return bad("signed with a different key than the one you trust")
  end
  if not manifest:find("^" .. M.FORMAT:gsub("%-", "%%-") .. "\n") then return bad("unknown manifest format") end
  -- the signature must cover these exact manifest bytes
  local pubF, sigF, manF = tmp(), tmp(), root .. "/" .. M.MANIFEST
  writeFile(pubF, pubPem)
  local ok = unb64(sigB64, sigF) and
    sh("openssl pkeyutl -verify -pubin -inkey " .. q(pubF) .. " -rawin -in " .. q(manF) .. " -sigfile " .. q(sigF))
  os.remove(pubF); os.remove(sigF)
  if not ok then return bad("signature does not match the manifest (manifest edited or wrong key)") end
  res.addon = manifest:match("\naddon: ([^\n]*)")
  res.version = manifest:match("\nversion: ([^\n]*)")
  -- every listed file must hash to what was signed
  local listed = {}
  for hash, file in manifest:gmatch("\n(%x+)  ([^\n]+)") do
    listed[file] = true
    local path = root .. "/" .. file
    if not readFile(path) then res.missing[#res.missing + 1] = file
    elseif sha256(path) ~= hash then res.modified[#res.modified + 1] = file end
  end
  for _, f in ipairs(M.listFiles(root)) do
    if not listed[f] then res.extra[#res.extra + 1] = f end
  end
  if #res.modified > 0 or #res.missing > 0 then
    return bad(string.format("%d file(s) changed, %d missing since the author signed it", #res.modified, #res.missing))
  end
  -- files the author didn't sign could be injected code: only Lua/XML/toc matter
  for _, f in ipairs(res.extra) do
    if f:lower():match("%.lua$") or f:lower():match("%.xml$") or f:lower():match("%.toc$") then
      return bad("unsigned code file added: " .. f)
    end
  end
  res.status = "valid"
  return res
end

------------------------------------------------------------------ keyring
-- Trust on first use: the first valid signature for an addon records its
-- key; later ones must use the same key.

function M.keyringPath(dir) return (dir or ".wowtest") .. "/trusted-keys.txt" end

function M.loadKeyring(path)
  local ring = {}
  local text = readFile(path) or ""
  for name, fp in text:gmatch("([^\t\n]+)\t([%x:]+)") do ring[name] = fp end
  return ring
end

function M.saveKeyring(path, ring)
  local names = {}
  for n in pairs(ring) do names[#names + 1] = n end
  table.sort(names)
  local lines = { "# addon<TAB>trusted key fingerprint (wowtest). Delete a line to trust a new key." }
  for _, n in ipairs(names) do lines[#lines + 1] = n .. "\t" .. ring[n] end
  sh("mkdir -p " .. q(path:match("^(.*)/[^/]+$") or "."))
  writeFile(path, table.concat(lines, "\n") .. "\n")
end

-- Verify and apply the keyring. Adds `trust` = "new" | "known" | "changed".
function M.verifyTrusted(root, name, keyringPath, opts)
  local res = M.verify(root, opts)
  if res.status ~= "valid" then return res end
  -- keys are trusted per addon name, whatever mirror it came from
  name = res.addon or name
  local ring = M.loadKeyring(keyringPath)
  local known = ring[name]
  if not known then
    ring[name] = res.fingerprint
    M.saveKeyring(keyringPath, ring)
    res.trust = "new"
  elseif known == res.fingerprint then
    res.trust = "known"
  else
    res.trust = "changed"
    res.status = "invalid"
    res.reason = "KEY CHANGED: " .. name .. " was signed by " .. known .. " before, now by " .. res.fingerprint
  end
  return res
end

function M.describe(res)
  if res.status == "unsigned" then return "unsigned" end
  if res.status == "invalid" then return "INVALID: " .. tostring(res.reason) end
  local s = "valid signature, key " .. res.fingerprint
  if res.trust == "new" then s = s .. " (first seen, now trusted)" end
  if #res.extra > 0 then s = s .. string.format(" (%d unsigned non-code file(s))", #res.extra) end
  return s
end

return M
