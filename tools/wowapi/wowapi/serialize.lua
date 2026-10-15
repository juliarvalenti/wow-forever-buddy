-- Serializes SavedVariables the same way the WoW client writes WTF files:
-- plain Lua assignments of strings, numbers, booleans and nested tables.
local M = {}

local function sortedKeys(t)
  local keys = {}
  for k in pairs(t) do keys[#keys + 1] = k end
  table.sort(keys, function(a, b)
    local ta, tb = type(a), type(b)
    if ta ~= tb then return ta < tb end
    if ta == "number" or ta == "string" then return a < b end
    return tostring(a) < tostring(b)
  end)
  return keys
end

local function fmtNumber(n)
  if n ~= n then return "0/0" end
  if n == math.huge then return "math.huge" end
  if n == -math.huge then return "-math.huge" end
  if n == math.floor(n) and math.abs(n) < 2 ^ 53 then return string.format("%d", n) end
  return string.format("%.17g", n)
end

local function value(v, indent, seen)
  local t = type(v)
  if t == "string" then return string.format("%q", v)
  elseif t == "number" then return fmtNumber(v)
  elseif t == "boolean" then return tostring(v)
  elseif t == "table" then
    if seen[v] then error("cannot serialize a table that contains itself") end
    seen[v] = true
    local pad = string.rep("\t", indent + 1)
    local out = { "{\n" }
    for _, k in ipairs(sortedKeys(v)) do
      local kv, vv = k, v[k]
      local okKey = type(kv) == "string" or type(kv) == "number" or type(kv) == "boolean"
      local vt = type(vv)
      local okVal = vt == "string" or vt == "number" or vt == "boolean" or vt == "table"
      if okKey and okVal then
        out[#out + 1] = string.format("%s[%s] = %s,\n", pad, value(kv, 0, {}), value(vv, indent + 1, seen))
      end
    end
    out[#out + 1] = string.rep("\t", indent) .. "}"
    seen[v] = nil
    return table.concat(out)
  end
  return "nil" -- functions, userdata, threads are dropped like the real client
end

function M.serialize(v) return value(v, 0, {}) end

-- Build a WTF-style file from a { name = value } map.
function M.file(vars)
  local out = {}
  for _, name in ipairs(sortedKeys(vars)) do
    out[#out + 1] = string.format("\n%s = %s\n", name, value(vars[name], 0, {}))
  end
  return table.concat(out)
end

-- Deep copy only serializable data (what survives a /reload).
function M.copy(v, seen)
  if type(v) ~= "table" then
    local t = type(v)
    if t == "string" or t == "number" or t == "boolean" then return v end
    return nil
  end
  seen = seen or {}
  if seen[v] then error("cannot save a table that contains itself") end
  seen[v] = true
  local c = {}
  for k, val in pairs(v) do
    local kt = type(k)
    if kt == "string" or kt == "number" or kt == "boolean" then
      c[k] = M.copy(val, seen)
    end
  end
  seen[v] = nil
  return c
end

return M
