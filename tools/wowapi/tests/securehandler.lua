-- A secure header from Blizzard's XML template (HUD4, for TH3's bars): when
-- the extract's SecureHandlerTemplates.xml replaces WOWAPI's built-in
-- template, CreateFrame(..., "SecureHandlerStateTemplate") must still get
-- SetFrameRef, Execute and WrapScript from the template's OnLoad
-- (SecureHandler_OnLoad), and its state snippets must run.
--
--   lua5.1 tools/wowapi/tests/securehandler.lua      (or luajit; exits 1 on a failure)

local here = (arg[0]:match("^(.*)[/\\]") or ".")
package.path = here .. "/../?.lua;" .. here .. "/../?/init.lua;" .. package.path
local WoW = require("wowapi")

local function quote(s)
    return "'" .. s:gsub("'", "'\\''") .. "'"
end

-- The template as Blizzard_RestrictedAddOnEnvironment defines it, in an
-- addon of that name, and an addon of ours making a header from it.
local root = os.tmpname()
os.remove(root)
local blizz, ours = root .. "/Blizzard_SecureHandlerTest", root .. "/HeaderTest"
assert(os.execute("mkdir -p " .. quote(blizz) .. " " .. quote(ours)))
local function write(path, text)
    local f = assert(io.open(path, "w"))
    f:write(text)
    f:close()
end
write(blizz .. "/Blizzard_SecureHandlerTest.toc", "## Interface: 120000\n## Title: Blizzard_SecureHandlerTest\nTemplates.xml\n")
write(blizz .. "/Templates.xml", [[
<Ui xmlns="http://www.blizzard.com/wow/ui/">
	<Frame name="SecureHandlerStateTemplate" inherits="SecureFrameTemplate" virtual="true">
		<Size x="2" y="2"/>
		<Scripts>
			<OnLoad function="SecureHandler_OnLoad"/>
		</Scripts>
	</Frame>
</Ui>
]])
write(ours .. "/HeaderTest.toc", "## Interface: 120000\n## Title: HeaderTest\nHeaderTest.lua\n")
write(ours .. "/HeaderTest.lua", [[
HeaderTestHeader = CreateFrame("Frame", "HeaderTestHeader", UIParent, "SecureHandlerStateTemplate")
HeaderTestButton = CreateFrame("Button", "HeaderTestButton", HeaderTestHeader, "SecureActionButtonTemplate")
]])

local sim = WoW.new({ quiet = true, offline = true })
assert(sim:LoadAddon(blizz))
assert(sim:LoadAddon(ours))
sim:Login()
local env = sim.env
local header, button = env.HeaderTestHeader, env.HeaderTestButton

local failed = 0
local function check(what, ok)
    print((ok and "ok   " or "FAIL ") .. what)
    if not ok then
        failed = failed + 1
    end
end

check("the template's OnLoad gave the header SetFrameRef, Execute and WrapScript",
    type(header.SetFrameRef) == "function" and type(header.Execute) == "function"
        and type(header.WrapScript) == "function")
if type(header.SetFrameRef) == "function" then
    header:SetFrameRef("button", button)
    header:Execute([[ self:GetFrameRef("button"):SetAttribute("type", "spell") ]])
    check("Execute reaches a frame ref", button:GetAttribute("type") == "spell")
end
header:SetAttribute("_onstate-page", [[ self:SetAttribute("page", newstate) ]])
header:SetAttribute("state-page", "7")
check("a state change runs the header's _onstate snippet", header:GetAttribute("page") == "7")

os.execute("rm -rf " .. quote(root))
print(failed == 0 and "securehandler: all passed" or ("securehandler: " .. failed .. " failed"))
os.exit(failed == 0 and 0 or 1)
