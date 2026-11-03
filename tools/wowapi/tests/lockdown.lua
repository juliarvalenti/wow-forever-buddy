-- Combat lockdown in WOWAPI (HUD3's blizzardCall, security review on #243):
-- in combat, our code touching a protected frame is refused; Blizzard's own
-- code alone is allowed; Blizzard's code calling into ours is refused (taint).
-- TH3's secure action buttons rely on the first and last.
--
--   lua5.1 tools/wowapi/tests/lockdown.lua      (or luajit; exits 1 on a failure)

local here = (arg[0]:match("^(.*)[/\\]") or ".")
package.path = here .. "/../?.lua;" .. here .. "/../?/init.lua;" .. package.path
local WoW = require("wowapi")

local function quote(s)
    return "'" .. s:gsub("'", "'\\''") .. "'"
end

-- Two throwaway addons: ours, and one named like Blizzard's UI addons.
local root = os.tmpname()
os.remove(root)
assert(os.execute("mkdir -p " .. quote(root .. "/LockTest") .. " " .. quote(root .. "/Blizzard_LockTest")))
local function write(path, text)
    local f = assert(io.open(root .. "/" .. path, "w"))
    f:write(text)
    f:close()
end
write("LockTest/LockTest.toc", "## Interface: 120000\n## Title: LockTest\nLockTest.lua\n")
write("LockTest/LockTest.lua", [[
LockTestButton = CreateFrame("Button", "LockTestButton", UIParent, "SecureActionButtonTemplate")
function LockTest_Hide() LockTestButton:Hide() end
function LockTest_SetAttribute() LockTestButton:SetAttribute("type", "spell") end
]])
write("Blizzard_LockTest/Blizzard_LockTest.toc", "## Interface: 120000\n## Title: Blizzard_LockTest\nBlizzard_LockTest.lua\n")
write("Blizzard_LockTest/Blizzard_LockTest.lua", [[
function Blizzard_LockTest_Hide(frame) frame:Hide() end
function Blizzard_LockTest_Show(frame) frame:Show() end
function Blizzard_LockTest_Call(fn) fn() end
]])

local sim = WoW.new({ quiet = true, offline = true })
assert(sim:LoadAddon(root .. "/LockTest"))
assert(sim:LoadAddon(root .. "/Blizzard_LockTest"))
sim:Login()
local env = sim.env
local button = env.LockTestButton

local failed = 0
local function check(what, ok)
    print((ok and "ok   " or "FAIL ") .. what)
    if not ok then
        failed = failed + 1
    end
end
local function blocked()
    return #sim:BlockedActions()
end

sim:EnterCombat()

local before = blocked()
env.LockTest_Hide()
check("in combat, our addon can't hide a protected frame", button:IsShown() and blocked() == before + 1)

before = blocked()
env.LockTest_SetAttribute()
check("in combat, our addon can't set a protected frame's attribute",
    button:GetAttribute("type") == nil and blocked() == before + 1)

before = blocked()
env.Blizzard_LockTest_Hide(button)
check("in combat, Blizzard's own code can hide it", not button:IsShown() and blocked() == before)
env.Blizzard_LockTest_Show(button)
check("in combat, Blizzard's own code can show it again", button:IsShown())

before = blocked()
env.Blizzard_LockTest_Call(env.LockTest_Hide)
check("in combat, Blizzard's code calling ours is still refused (taint)", button:IsShown() and blocked() == before + 1)

sim:LeaveCombat()
env.LockTest_Hide()
check("out of combat, our addon can hide it", not button:IsShown())

os.execute("rm -rf " .. quote(root))
print(failed == 0 and "lockdown: all passed" or ("lockdown: " .. failed .. " failed"))
os.exit(failed == 0 and 0 or 1)
