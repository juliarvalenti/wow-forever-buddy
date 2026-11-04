-- A small JSON reader: objects, arrays, strings, numbers, true, false, null.
-- For the UI documents in ui-examples/, read by ui.lua and by the addon
-- harness (tools/addon-test/run.lua, which draws each one live).

local M = {}

function M.decode(s)
    local i = 1
    local function ws()
        i = s:find("[^ \t\r\n]", i) or #s + 1
    end
    local value
    local function str()
        local out, j = {}, i + 1
        while true do
            local c = s:sub(j, j)
            if c == "" then
                error("unterminated string")
            elseif c == '"' then
                i = j + 1
                return table.concat(out)
            elseif c == "\\" then
                local e = s:sub(j + 1, j + 1)
                local map = { n = "\n", t = "\t", r = "\r", b = "\b", f = "\f", ['"'] = '"', ["\\"] = "\\", ["/"] = "/" }
                if e == "u" then
                    local code = tonumber(s:sub(j + 2, j + 5), 16)
                    out[#out + 1] = code < 128 and string.char(code) or "?"
                    j = j + 6
                else
                    out[#out + 1] = map[e] or e
                    j = j + 2
                end
            else
                out[#out + 1] = c
                j = j + 1
            end
        end
    end
    function value()
        ws()
        local c = s:sub(i, i)
        if c == "{" then
            local t = {}
            i = i + 1
            ws()
            if s:sub(i, i) == "}" then
                i = i + 1
                return t
            end
            while true do
                ws()
                local k = str()
                ws()
                assert(s:sub(i, i) == ":", "expected : at " .. i)
                i = i + 1
                t[k] = value()
                ws()
                local d = s:sub(i, i)
                i = i + 1
                if d == "}" then
                    return t
                end
                assert(d == ",", "expected , or } at " .. i)
            end
        elseif c == "[" then
            local t = {}
            i = i + 1
            ws()
            if s:sub(i, i) == "]" then
                i = i + 1
                return t
            end
            while true do
                t[#t + 1] = value()
                ws()
                local d = s:sub(i, i)
                i = i + 1
                if d == "]" then
                    return t
                end
                assert(d == ",", "expected , or ] at " .. i)
            end
        elseif c == '"' then
            return str()
        elseif s:sub(i, i + 3) == "true" then
            i = i + 4
            return true
        elseif s:sub(i, i + 4) == "false" then
            i = i + 5
            return false
        elseif s:sub(i, i + 3) == "null" then
            i = i + 4
            return nil
        end
        local num = s:match("^-?%d+%.?%d*[eE]?[-+]?%d*", i)
        assert(num and #num > 0, "unexpected " .. c .. " at " .. i)
        i = i + #num
        return tonumber(num)
    end
    return value()
end

return M
