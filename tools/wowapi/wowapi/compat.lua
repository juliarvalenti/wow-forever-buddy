-- Lua version shims. WoW runs Lua 5.1, so 5.1 / LuaJIT is the reference
-- runtime, but the harness also works on 5.2+ for convenience.
local compat = {}

compat.is51 = (_VERSION == "Lua 5.1")
compat.isJIT = type(rawget(_G, "jit")) == "table"

local unpack = rawget(_G, "unpack") or table.unpack
compat.unpack = unpack

-- Load a chunk from a string with a custom environment.
function compat.loadstring(src, chunkname, env)
  if compat.is51 then
    local fn, err = loadstring(src, chunkname)
    if not fn then return nil, err end
    if env then setfenv(fn, env) end
    return fn
  end
  return load(src, chunkname, "t", env)
end

function compat.loadfile(path, env)
  local f, err = io.open(path, "rb")
  if not f then return nil, err end
  local src = f:read("*a")
  f:close()
  -- strip UTF-8 BOM, which WoW tolerates
  src = src:gsub("^\239\187\191", "")
  return compat.loadstring(src, "@" .. path, env)
end

-- Pure Lua 32-bit bit library (WoW ships `bit`; LuaJIT already has it).
local bitlib = rawget(_G, "bit")
if not bitlib then
  local MOD = 2 ^ 32
  local function norm(x) x = x % MOD; if x >= 2 ^ 31 then x = x - MOD end; return x end
  local function u(x) return x % MOD end
  local function op(a, b, fn)
    a, b = u(a), u(b)
    local r, p = 0, 1
    for _ = 1, 32 do
      local ra, rb = a % 2, b % 2
      if fn(ra, rb) then r = r + p end
      a, b, p = (a - ra) / 2, (b - rb) / 2, p * 2
    end
    return norm(r)
  end
  local function reduce(fn)
    return function(a, ...)
      local r = a
      for i = 1, select("#", ...) do r = op(r, select(i, ...), fn) end
      return norm(r)
    end
  end
  bitlib = {
    band = reduce(function(x, y) return x == 1 and y == 1 end),
    bor = reduce(function(x, y) return x == 1 or y == 1 end),
    bxor = reduce(function(x, y) return x ~= y end),
    bnot = function(a) return norm(MOD - 1 - u(a)) end,
    lshift = function(a, n) return norm(u(a) * 2 ^ (n % 32)) end,
    rshift = function(a, n) return norm(math.floor(u(a) / 2 ^ (n % 32))) end,
    arshift = function(a, n) return norm(math.floor(norm(a) / 2 ^ (n % 32))) end,
    tobit = norm,
  }
  bitlib.mod = function(a, b) return a % b end
end
compat.bit = bitlib

return compat
