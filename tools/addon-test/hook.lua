-- The harness's one debug hook (Lua allows a single hook per coroutine).
-- It counts VM instructions, every STEP, for the handler budgets (H6), and
-- passes line and call events to coverage (H3) when that's on.
--
-- Instruction counts, not time: a CI runner's clock varies run to run, a
-- count doesn't. Under LuaJIT the JIT is switched off while counting, since
-- compiled code skips hooks; its counts then differ from Lua 5.1's (what CI
-- runs), so the budgets are set for Lua 5.1.

local M = { count = 0, line = nil }

M.STEP = 100

local function hook(event, line)
    if event == "count" then
        M.count = M.count + M.STEP
    elseif M.line then
        M.line(event, line)
    end
end

-- (Re)installs the hook: counting always, line and call events when
-- coverage set M.line.
function M.install()
    if jit and jit.off then
        jit.off()
    end
    debug.sethook(hook, M.line and "cl" or "", M.STEP)
end

return M
