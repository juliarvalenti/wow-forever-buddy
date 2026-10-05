-- Writes a Lua value the way the WoW client writes SavedVariables: one
-- `Name = {...}` statement, tab-indented, positional entries marked
-- `-- [i]`, keyed entries as `["key"] = value,`. Keys are sorted (the client
-- writes them in hash order) so fixtures diff cleanly.

local M = {}

local function quote(s)
    local escaped = string.gsub(s, '[%c"\\]', function(c)
        if c == "\n" then
            return "\\n"
        elseif c == "\r" then
            return "\\r"
        elseif c == "\t" then
            return "\\t"
        elseif c == '"' or c == "\\" then
            return "\\" .. c
        end
        return string.format("\\%03d", string.byte(c))
    end)
    return '"' .. escaped .. '"'
end

local function number(n)
    if n ~= n or n == math.huge or n == -math.huge then
        error("can't save " .. tostring(n), 0)
    end
    if n == math.floor(n) and math.abs(n) < 2 ^ 53 then
        return string.format("%d", n)
    end
    return string.format("%.15g", n)
end

local function keyOrder(a, b)
    local ta, tb = type(a), type(b)
    if ta ~= tb then
        return ta == "number"
    end
    return a < b
end

local write

local function writeTable(out, t, depth, seen)
    if seen[t] then
        error("can't save a table that contains itself", 0)
    end
    seen[t] = true
    out[#out + 1] = "{\n"
    local pad = string.rep("\t", depth + 1)

    local n = 0
    while t[n + 1] ~= nil do
        n = n + 1
    end
    for i = 1, n do
        out[#out + 1] = pad
        write(out, t[i], depth + 1, seen)
        out[#out + 1] = ", -- [" .. i .. "]\n"
    end

    local keys = {}
    for k in pairs(t) do
        local tk = type(k)
        if tk == "string" or (tk == "number" and not (k == math.floor(k) and k >= 1 and k <= n)) then
            keys[#keys + 1] = k
        elseif tk ~= "number" then
            error("can't save a " .. tk .. " key", 0)
        end
    end
    table.sort(keys, keyOrder)
    for _, k in ipairs(keys) do
        local key = type(k) == "string" and quote(k) or number(k)
        out[#out + 1] = pad .. "[" .. key .. "] = "
        write(out, t[k], depth + 1, seen)
        out[#out + 1] = ",\n"
    end

    out[#out + 1] = string.rep("\t", depth) .. "}"
    seen[t] = nil
end

write = function(out, v, depth, seen)
    local t = type(v)
    if t == "string" then
        out[#out + 1] = quote(v)
    elseif t == "number" then
        out[#out + 1] = number(v)
    elseif t == "boolean" then
        out[#out + 1] = tostring(v)
    elseif t == "table" then
        writeTable(out, v, depth, seen)
    else
        error("can't save a " .. t, 0)
    end
end

-- The text of a SavedVariables file holding `name = value`.
function M.serialize(name, value)
    local out = { name, " = " }
    write(out, value, 0, {})
    out[#out + 1] = "\n"
    return table.concat(out)
end

return M
