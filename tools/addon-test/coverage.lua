-- Line and function coverage of ForeverBuddy.lua under the harness (H3),
-- with no dependency (luacov needs luarocks; CI has plain Lua 5.1).
--
-- A debug hook notes every line run and every function entered in the
-- addon's chunk ("@ForeverBuddy.lua"). Lua 5.1 can't list a chunk's
-- executable lines, so the line figure is against an estimate: every line
-- that isn't blank, a comment, or only a closing keyword (end, else, },
-- ...). The function list is exact: each `function` in the source, and
-- whether it was ever entered. That list is the useful part: an event
-- handler no scenario reaches is a gap a real client will find first.

local M = {}

local CHUNK = "@ForeverBuddy.lua"
local hits, entered = {}, {}

local function hook(event, line)
    local info = debug.getinfo(2, "S")
    if not info or info.source ~= CHUNK then
        return
    end
    if event == "line" then
        hits[line] = true
    elseif event == "call" then
        entered[info.linedefined] = true
    end
end

function M.start()
    debug.sethook(hook, "cl")
end

function M.stop()
    debug.sethook()
end

-- Lines that hold code, by the estimate above.
local function codeLines(src)
    local lines, inComment = {}, false
    local n = 0
    for text in (src .. "\n"):gmatch("([^\n]*)\n") do
        n = n + 1
        local t = text:match("^%s*(.-)%s*$")
        if inComment then
            if t:find("%]%]") then
                inComment = false
            end
        elseif t:match("^%-%-%[%[") then
            inComment = not t:find("%]%]")
        else
            -- Code without its trailing comment.
            local c = t:gsub("%s*%-%-.*$", "")
            -- Not run as lines of their own: a bare `local x` (no value),
            -- and the entries of a table constructor (a line ending in `,`
            -- or `{`, or a lone `{`), which run as part of one statement.
            local skip = c == "" or c:match("^end[%s%),]*$") or c == "else"
                or c:match("^[%}%)]+[,%)]*$") or c:match("^local [%w_, ]+$")
                or (c:match("[,{]$") and not c:match("function")) or c == "{"
            if not skip then
                lines[n] = text
            end
        end
    end
    return lines, n
end

-- Each function definition: its line and a readable name.
local function functions(src)
    local out, n = {}, 0
    for text in (src .. "\n"):gmatch("([^\n]*)\n") do
        n = n + 1
        if not text:match("^%s*%-%-") then
            local name = text:match("function%s+([%w_%.:]+)%s*%(")
                or text:match("([%w_%.%[%]\"]+)%s*=%s*function%s*%(")
            if text:find("function%s*[%w_%.:]*%s*%(") then
                out[#out + 1] = { line = n, name = name or "(anonymous)" }
            end
        end
    end
    return out
end

-- The report: totals, then the functions never entered, handlers first.
function M.report(src)
    local code = codeLines(src)
    local total, hit = 0, 0
    for line in pairs(code) do
        total = total + 1
        if hits[line] then
            hit = hit + 1
        end
    end
    local defs = functions(src)
    local missed, handlers = {}, {}
    for _, f in ipairs(defs) do
        if not entered[f.line] then
            if f.name:match("^handlers%.") then
                handlers[#handlers + 1] = f
            else
                missed[#missed + 1] = f
            end
        end
    end
    local out = {}
    out[#out + 1] = string.format("ForeverBuddy.lua coverage: lines %d of %d (%.1f%%, estimate), functions %d of %d",
        hit, total, total > 0 and hit * 100 / total or 0, #defs - #missed - #handlers, #defs)
    out[#out + 1] = string.format("event handlers never run (%d):", #handlers)
    for _, f in ipairs(handlers) do
        out[#out + 1] = string.format("  %5d  %s", f.line, f.name)
    end
    out[#out + 1] = string.format("other functions never entered (%d):", #missed)
    for _, f in ipairs(missed) do
        out[#out + 1] = string.format("  %5d  %s", f.line, f.name)
    end
    -- Branches, roughly: runs of 3 or more code lines never run, inside
    -- functions that were (a whole missed function is listed above).
    local runs, start, last, count = {}, nil, nil, 0
    local maxLine = 0
    for line in pairs(code) do
        if line > maxLine then
            maxLine = line
        end
    end
    local function close()
        if start and count >= 3 then
            runs[#runs + 1] = string.format("  %5d-%-5d %d lines: %s", start, last, count,
                (code[start]:match("^%s*(.-)%s*$")):sub(1, 70))
        end
        start, count = nil, 0
    end
    for line = 1, maxLine do
        if code[line] then
            if hits[line] then
                close()
            else
                start = start or line
                last = line
                count = count + 1
            end
        end
    end
    close()
    out[#out + 1] = string.format("unrun stretches of 3+ code lines (%d):", #runs)
    for _, r in ipairs(runs) do
        out[#out + 1] = r
    end
    return table.concat(out, "\n") .. "\n"
end

return M
