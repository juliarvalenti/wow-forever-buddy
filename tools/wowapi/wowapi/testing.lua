-- A small busted-style test runner for addon specs.
--
-- Spec files get: describe, it, pending, before_each, after_each, expect,
-- WoW (the wowapi module), ADDON_DIR, NewSim(opts) and Boot(opts).
--
-- Any Lua error raised inside addon code during a test fails that test,
-- unless the test clears it with sim:ClearErrors().
local WoW = require("wowapi")
local toc = require("wowapi.toc")
local compat = require("wowapi.compat")

local M = {}

-- Sims created by the test currently running (checked for addon errors).
local activeSims = {}
WoW.onNew(function(sim) table.insert(activeSims, sim) end)

local color = os.getenv("NO_COLOR") == nil
local function c(code, s) return color and ("\27[" .. code .. "m" .. s .. "\27[0m") or s end

------------------------------------------------------------------ expect

local function fmt(v)
  if type(v) == "string" then return string.format("%q", v) end
  if type(v) == "table" then
    local ok, s = pcall(WoW.serialize.serialize, v)
    if ok and #s < 400 then return s:gsub("\n%s*", " ") end
  end
  return tostring(v)
end

local function deepEqual(a, b, seen)
  if a == b then return true end
  if type(a) ~= "table" or type(b) ~= "table" then return false end
  seen = seen or {}
  if seen[a] == b then return true end
  seen[a] = b
  for k, v in pairs(a) do if not deepEqual(v, b[k], seen) then return false end end
  for k in pairs(b) do if a[k] == nil then return false end end
  return true
end

local function contains(hay, needle)
  if type(hay) == "string" then return hay:find(needle, 1, true) ~= nil end
  if type(hay) == "table" then
    for _, v in pairs(hay) do if deepEqual(v, needle) then return true end end
  end
  return false
end

-- Drop the harness's own frames from tracebacks.
local function cleanTrace(msg)
  local out = {}
  for line in (tostring(msg) .. "\n"):gmatch("(.-)\n") do
    if not (line:find("wowapi/testing.lua", 1, true) or line:find("wowtest:", 1, true)
      or line:find("wowapi/sim.lua", 1, true) or line:find("(tail call)", 1, true)
      or line:find("in function 'xpcall'", 1, true) or line:find("[C]: in function 'error'", 1, true)
      or line:match("^%s*%[C%]: %?$")) then
      out[#out + 1] = line
    end
  end
  local s = table.concat(out, "\n"):gsub("\nstack traceback:%s*$", "")
  return s
end

local function expect(actual)
  local function build(negate)
    local m = {}
    local function check(ok, msg)
      if negate then ok = not ok; msg = "not " .. msg end
      if not ok then error("expected " .. fmt(actual) .. " " .. msg, 3) end
    end
    function m.to_be(x) check(actual == x, "to be " .. fmt(x)) end
    function m.to_equal(x) check(deepEqual(actual, x), "to equal " .. fmt(x)) end
    function m.to_be_truthy() check(actual and true, "to be truthy") end
    function m.to_be_falsy() check(not actual, "to be falsy") end
    function m.to_be_nil() check(actual == nil, "to be nil") end
    function m.to_be_type(t) check(type(actual) == t, "to be of type " .. t) end
    function m.to_contain(x) check(contains(actual, x), "to contain " .. fmt(x)) end
    function m.to_match(p) check(type(actual) == "string" and actual:find(p) ~= nil, "to match " .. fmt(p)) end
    function m.to_have_length(n)
      local len = type(actual) == "table" and #actual or (type(actual) == "string" and #actual or -1)
      check(len == n, "to have length " .. n .. " (has " .. len .. ")")
    end
    function m.to_be_greater_than(n) check(type(actual) == "number" and actual > n, "to be greater than " .. n) end
    function m.to_be_less_than(n) check(type(actual) == "number" and actual < n, "to be less than " .. n) end
    function m.to_be_close_to(n, eps) check(type(actual) == "number" and math.abs(actual - n) <= (eps or 1e-6), "to be close to " .. n) end
    function m.to_error(pattern)
      local ok, err = pcall(actual)
      local matched = not ok and (pattern == nil or tostring(err):find(pattern) ~= nil)
      check(matched, "to raise an error" .. (pattern and (" matching " .. fmt(pattern)) or "")
        .. (ok and " (it did not error)" or (" (got: " .. tostring(err) .. ")")))
    end
    return m
  end
  local e = build(false)
  e.never = build(true)
  e.to_not = e.never
  return e
end

------------------------------------------------------------------ runner

function M.findAddonDir(path)
  local dir = toc.dirname(path)
  for _ = 1, 6 do
    if toc.findToc(dir) then return dir end
    local up = toc.dirname(dir)
    if up == dir then break end
    dir = up
  end
end

-- Run one spec file. Returns passed, failed, pending counts.
function M.runFile(path, opts)
  opts = opts or {}
  local results = { passed = 0, failed = 0, pending = 0, failures = {} }
  local root = { name = "", children = {}, before = {}, after = {} }
  local current = root
  local addonDir = M.findAddonDir(path)

  local sims = activeSims

  local env = setmetatable({}, { __index = _G })
  env.WoW = WoW
  env.ADDON_DIR = addonDir
  env.SPEC_DIR = toc.dirname(path)
  env.expect = expect
  function env.describe(name, fn)
    local node = { name = name, children = {}, before = {}, after = {}, parent = current }
    table.insert(current.children, node)
    local prev = current
    current = node
    fn()
    current = prev
  end
  env.context = env.describe
  function env.it(name, fn) table.insert(current.children, { name = name, fn = fn, parent = current }) end
  env.test = env.it
  function env.pending(name) table.insert(current.children, { name = name, parent = current, isPending = true }) end
  env.xit = env.pending
  function env.before_each(fn) table.insert(current.before, fn) end
  function env.after_each(fn) table.insert(current.after, fn) end
  -- A sim that can find the addon under test and its siblings (dependencies).
  function env.NewSim(o)
    o = o or {}
    o.addonPaths = o.addonPaths or {}
    if addonDir then table.insert(o.addonPaths, toc.dirname(addonDir)) end
    o.quiet = o.quiet ~= false
    return WoW.new(o)
  end
  -- NewSim + load the addon under test + log in.
  function env.Boot(o)
    local sim = env.NewSim(o)
    if not addonDir then error("Boot(): no .toc found above " .. path, 2) end
    local ok, why = sim:LoadAddon(addonDir)
    if not ok then error("Boot(): addon failed to load: " .. tostring(why), 2) end
    sim:Login()
    return sim
  end

  local chunk, err = compat.loadfile(path, env)
  if not chunk then
    results.failed = 1
    table.insert(results.failures, { name = path, err = err })
    print(c("31", "  ✗ could not load spec: " .. err))
    return results
  end
  local ok, loadErr = pcall(chunk)
  if not ok then
    results.failed = 1
    table.insert(results.failures, { name = path, err = loadErr })
    print(c("31", "  ✗ error while defining specs: " .. tostring(loadErr)))
    return results
  end

  local function fullName(node)
    local parts = {}
    while node and node.name ~= "" do table.insert(parts, 1, node.name); node = node.parent end
    return table.concat(parts, " › ")
  end
  local function chain(node, key)
    local list = {}
    local n = node
    while n do
      if key == "before" then for i = #n.before, 1, -1 do table.insert(list, 1, n.before[i]) end
      else for _, f in ipairs(n.after) do table.insert(list, f) end end
      n = n.parent
    end
    return list
  end

  local function runNode(node, depth)
    for _, child in ipairs(node.children) do
      local indent = string.rep("  ", depth)
      if child.children then
        print(indent .. c("1", child.name))
        runNode(child, depth + 1)
      else
        local name = fullName(child)
        if opts.filter and not name:find(opts.filter) then
          -- skipped by filter
        elseif child.isPending then
          results.pending = results.pending + 1
          print(indent .. c("33", "○ " .. child.name .. " (pending)"))
        else
          for i = #sims, 1, -1 do sims[i] = nil end
          local okAll, errMsg = true, nil
          local function step(fn)
            if not okAll then return end
            local ok2, e = xpcall(fn, function(m) return cleanTrace(debug.traceback(tostring(m), 2)) end)
            if not ok2 then okAll, errMsg = false, e end
          end
          for _, b in ipairs(chain(node, "before")) do step(b) end
          step(child.fn)
          if okAll then
            for _, sim in ipairs(sims) do
              if #sim.errors > 0 then
                local lines = {}
                for _, e in ipairs(sim.errors) do lines[#lines + 1] = cleanTrace(e.traceback) end
                okAll = false
                errMsg = "addon raised " .. #sim.errors .. " Lua error(s):\n" .. table.concat(lines, "\n")
                break
              end
            end
          end
          for _, a in ipairs(chain(node, "after")) do
            local ok3, e = pcall(a)
            if not ok3 and okAll then okAll, errMsg = false, e end
          end
          if okAll then
            results.passed = results.passed + 1
            print(indent .. c("32", "✓ ") .. child.name)
          else
            results.failed = results.failed + 1
            table.insert(results.failures, { name = name, err = errMsg })
            print(indent .. c("31", "✗ " .. child.name))
            if opts.verbose then print(c("31", (tostring(errMsg):gsub("\n", "\n" .. indent .. "    ")))) end
          end
        end
      end
    end
  end
  runNode(root, 1)
  return results
end

-- Run many spec files, print a summary, return true if everything passed.
function M.run(files, opts)
  local total = { passed = 0, failed = 0, pending = 0, failures = {} }
  for _, f in ipairs(files) do
    print(c("36", f))
    local r = M.runFile(f, opts)
    total.passed = total.passed + r.passed
    total.failed = total.failed + r.failed
    total.pending = total.pending + r.pending
    for _, x in ipairs(r.failures) do table.insert(total.failures, x) end
  end
  if #total.failures > 0 then
    print("\n" .. c("31;1", "Failures:"))
    for i, fl in ipairs(total.failures) do
      print(c("31", string.format("\n%d) %s", i, fl.name)))
      print("   " .. tostring(fl.err):gsub("\n", "\n   "))
    end
  end
  print(string.format("\n%s, %s, %s",
    c("32", total.passed .. " passed"),
    c(total.failed > 0 and "31" or "32", total.failed .. " failed"),
    c("33", total.pending .. " pending")))
  return total.failed == 0, total
end

return M
