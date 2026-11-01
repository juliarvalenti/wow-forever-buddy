-- addon-live (LIVE1): ForeverBuddy running in WOWAPI, the WoW: Forever client
-- emulator, as one long-lived session you drive from the shell, a command at
-- a time, and look at in PNGs. Shotkit, for the in-game window.
--
--   luajit tools/addon-live/live.lua start [--art]   boot the session (in the background)
--   luajit tools/addon-live/live.lua <command ...>   send it one command, print the answer
--   luajit tools/addon-live/live.lua stop
--   luajit tools/addon-live/live.lua script <file>   run a file of commands in this process
--                                                    (no server; what CI uses)
--
-- Commands (`help` lists them): login, slash, click, set, buttons, advance,
-- reload, combat, state, chat, errors, lua, screenshot, upload. Every answer
-- ends with the chat lines and Lua errors the command caused.
--
-- The session lives in .addon-live/ at the repo root (gitignored, or
-- --dir): the command pipe, the server's log, shots/<name>.svg and .png.
-- PNGs come from macOS's QuickLook (qlmanage), else rsvg-convert, else a
-- headless Chrome; all run without a window. Nothing here opens a window,
-- and nothing touches a real WoW install.

local here = (arg[0]:match("^(.*)[/\\]") or ".")
if here:sub(1, 1) ~= "/" then
    here = io.popen("pwd"):read("*l") .. "/" .. here
end
local ROOT = here:gsub("/tools/addon%-live$", "")
local WOWAPI = ROOT .. "/tools/wowapi"
local ADDON = ROOT .. "/src-tauri/resources/addon/ForeverBuddy"
-- The slots the app's send_to_game writes (src-tauri/src/contract.rs, H5),
-- for Thrandor Vargur: the "filled" profile.
local BRIDGE = ROOT .. "/src-tauri/tests/fixtures/bridge"
local BRIDGE_STAMP = 1790960000
local SLOTS = { "Tooltip1", "Tooltip2", "Plan", "Briefing", "Lists", "Cleanup" }

-- `s` as one POSIX shell word.
local function sq(s)
    return "'" .. tostring(s):gsub("'", "'\\''") .. "'"
end

local function sh(cmd)
    local r = os.execute(cmd)
    return r == 0 or r == true
end

local function readAll(path)
    local f = io.open(path, "rb")
    if not f then
        return nil
    end
    local s = f:read("*a")
    f:close()
    return s
end

local function writeAll(path, text)
    local f = assert(io.open(path, "wb"))
    f:write(text)
    f:close()
end

local function has(cmd)
    return sh("command -v " .. cmd .. " >/dev/null 2>&1")
end

-- Arguments: the command, and the options anywhere among them.
local opts = { dir = ROOT .. "/.addon-live" }
local words = {}
do
    local i = 1
    while arg[i] do
        local a = arg[i]
        if a == "--dir" then
            opts.dir = arg[i + 1]
            i = i + 2
        elseif a == "--art" then
            opts.art = true
            i = i + 1
        else
            words[#words + 1] = a
            i = i + 1
        end
    end
end
local DIR = opts.dir
local PIPE = DIR .. "/commands"
local PIDFILE = DIR .. "/server.pid"

-- The session ---------------------------------------------------------------------

local S = {} -- sim, profile, chatSeen, errorsSeen

local function loadWowapi()
    if not readAll(WOWAPI .. "/wowapi/data/apidocs.lua") then
        error("WOWAPI's data is missing: run tools/wowapi/fetch-data.sh first", 0)
    end
    package.path = WOWAPI .. "/?.lua;" .. WOWAPI .. "/?/init.lua;" .. package.path
    S.WoW = require("wowapi")
    S.layout = require("wowapi.layout")
    S.render = require("wowapi.render")
    S.serialize = require("wowapi.serialize")
end

-- A copy of the addon, with the app's slots for "filled" or as shipped for
-- "empty", so a login always starts from what's on disk now.
local PROFILES = { filled = BRIDGE, empty = false }

local function addonCopy(profile)
    local dir = DIR .. "/addon/ForeverBuddy"
    assert(sh("rm -rf " .. sq(dir) .. " && mkdir -p " .. sq(dir) .. " && cp -R " .. sq(ADDON) .. "/. " .. sq(dir)))
    local from = PROFILES[profile]
    if from then
        for _, slot in ipairs(SLOTS) do
            assert(sh("cp " .. sq(from .. "/" .. slot .. ".lua") .. " " .. sq(dir .. "/Data/" .. slot .. ".lua")))
        end
    end
    return dir
end

-- Thrandor Vargur (Forever characters have a surname), level 12 in Elwynn,
-- an hour after the app wrote the slots. As tools/addon-shots/scene.lua.
local function login(profile)
    profile = profile or "filled"
    if PROFILES[profile] == nil then
        error("no profile " .. profile .. " (filled, empty)", 0)
    end
    local sim = S.WoW.new({
        player = { name = "Thrandor", class = "WARRIOR", race = "Human", level = 12, money = 25000, zone = "Elwynn Forest" },
        now = BRIDGE_STAMP + 3600,
        art = opts.art,
        offline = not opts.art,
        quiet = true,
    })
    sim:Mock("UnitName", function(unit)
        if unit == "player" then
            return "Thrandor", "Vargur"
        end
    end)
    -- An empty quest log: WOWAPI makes one up otherwise, differently from
    -- run to run, and the same session should give the same pictures.
    sim:Mock("C_QuestLog.GetNumQuestLogEntries", 0, 0)
    -- An item's level from the items added below, not a made-up one.
    sim:Mock("C_Item.GetDetailedItemLevelInfo", function(ref)
        local it = sim.items[tonumber(tostring(ref):match("item:(%d+)") or ref)]
        if it and it.itemLevel then
            return it.itemLevel, false, it.itemLevel
        end
    end)
    sim:AddItem(2589, { name = "Linen Cloth", quality = 1 })
    sim:AddItem(6948, { name = "Hearthstone", quality = 1 })
    sim:AddItem(14047, { name = "Runecloth", quality = 1 })
    sim:AddItem(117, { name = "Tough Jerky", quality = 1 })
    sim.bags[0][1] = { itemID = 2589, stackCount = 4 }
    sim.bags[0][2] = { itemID = 6948, stackCount = 1 }
    local ok, why = sim:LoadAddon(addonCopy(profile))
    if not ok then
        error("the addon didn't load: " .. tostring(why), 0)
    end
    sim:Login()
    sim:Advance(6)
    S.sim, S.profile = sim, profile
    S.chatSeen, S.errorsSeen = 0, 0
    return ("logged in as Thrandor Vargur, profile %s"):format(profile)
end

local function need()
    if not S.sim then
        login("filled")
    end
    return S.sim
end

-- Frames you can click: shown, taking the mouse, with their name or text.
local function textOf(obj)
    local t
    if type(obj.GetText) == "function" then
        t = obj:GetText()
    end
    if (not t or t == "") and type(obj.label) == "table" and type(obj.label.GetText) == "function" then
        t = obj.label:GetText()
    end
    return t
end

local function clickable()
    local sim = S.sim
    local out = {}
    for obj, s in pairs(sim.widgetState) do
        if s.isFrame and s.mouse and sim._isVisible(obj) then
            local x, y, w, h = S.layout.rect(sim, obj)
            if x then
                out[#out + 1] = {
                    obj = obj,
                    name = type(obj.GetName) == "function" and obj:GetName() or nil,
                    text = textOf(obj),
                    x = math.floor(x + w / 2), y = math.floor(y + h / 2),
                }
            end
        end
    end
    table.sort(out, function(a, b)
        if a.y ~= b.y then
            return a.y > b.y
        end
        return a.x < b.x
    end)
    return out
end

-- A frame by global name, a path from one ("ForeverBuddyWindow.Tabs.5"), or
-- the text on it ("Lists").
local function find(target)
    local sim = S.sim
    if target:match("^[%a_][%w_]*[%.%w_]*$") then
        local v = sim:Get(target:match("^[^%.]+"))
        for part in target:gmatch("%.([^%.]+)") do
            v = type(v) == "table" and (v[tonumber(part) or part]) or nil
        end
        if type(v) == "table" and sim.widgetState[v] and sim.widgetState[v].isFrame then
            return v
        end
    end
    for _, c in ipairs(clickable()) do
        if c.text and c.text:gsub("|c%x%x%x%x%x%x%x%x", ""):gsub("|r", "") == target then
            return c.obj
        end
    end
    error("nothing to click called " .. target .. " (`buttons` lists them)", 0)
end

-- The switch for a setting by id or label, on the window's Settings page or
-- (W1) a module's page, which is opened to find it. "<module>.enabled" is
-- that page's own switch at the top right.
local function settingBox(key)
    local sim = S.sim
    local w = sim:Get("ForeverBuddyWindow")
    -- (The window is built the first time it opens.)
    if not (w and w:IsShown()) then
        sim:Slash("/fb")
        w = sim:Get("ForeverBuddyWindow")
    end
    if not (w and w.Tabs and w.Tabs[5]) then
        error("no Forever Buddy window", 0)
    end
    local rail = { w.Tabs[5] }
    for _, b in pairs(w.mods or {}) do
        rail[#rail + 1] = b
    end
    local names = {}
    for _, b in ipairs(rail) do
        sim:Click(b)
        sim:Advance(0.2)
        if type(b.key) == "string" and (key == b.key .. ".enabled" or key == b.key) then
            return w.modSwitch
        end
        local page = w.pages and w.pages[b.key]
        for _, cb in ipairs(page and page.checks or {}) do
            local o = cb.option or {}
            if o.key == key or o.label == key then
                return cb
            end
            names[#names + 1] = tostring(o.key)
        end
    end
    error("no setting " .. key .. " (there are: " .. table.concat(names, ", ") .. ")", 0)
end

-- The bags and the bank (P4d's pictures). WOWAPI draws no bag frames, so
-- these are stand-ins with what the addon reads from Blizzard's: a backpack
-- whose item buttons know their bag and slot (ContainerFrameUtil_
-- EnumerateContainerFrames), and the bank's BankPanel, whose buttons know
-- their tab and slot. Blizzard's own quality border is drawn on each, as
-- the game does for uncommon and better.
local BAG_ITEMS = {
    -- id, name, quality, item level, equip slot, stack
    { 6948, "Hearthstone", 1 },
    { 2589, "Linen Cloth", 1, nil, nil, 12 },
    { 4865, "Ruined Pelt", 0, nil, nil, 3 },
    { 10001, "Shadowcraft Cap", 3, 63, "INVTYPE_HEAD" },
    { 10010, "Arcanite Reaper", 4, 63, "INVTYPE_2HWEAPON" },
    { 10003, "Band of the Unicorn", 2, 55, "INVTYPE_FINGER" },
    { 6096, "Apprentice's Shirt", 1, 1, "INVTYPE_BODY" },
    { 4867, "Broken Scorpid Leg", 0, nil, nil, 2 },
}
local BANK_ITEMS = {
    { 10005, "Felcloth Hood", 2, 40, "INVTYPE_HEAD" },
    { 4865, "Ruined Pelt", 0, nil, nil, 5 },
    { 14047, "Runecloth", 1, nil, nil, 20 },
    { 10011, "Dal'Rend's Sacred Charge", 3, 63, "INVTYPE_WEAPON" },
}
local BORDER = { [2] = { 0.12, 1, 0 }, [3] = { 0, 0.44, 0.87 }, [4] = { 0.64, 0.21, 0.93 } }

local function putUpBags()
    local sim = need()
    local CreateFrame = sim:Get("CreateFrame")
    local UIParent = sim:Get("UIParent")
    local function grid(name, title, list, bag, x, y, bank)
        sim.bags[bag] = { size = 16 }
        for slot, it in ipairs(list) do
            sim:AddItem(it[1], { name = it[2], quality = it[3], itemLevel = it[4], equipLoc = it[5] })
            sim.bags[bag][slot] = { itemID = it[1], stackCount = it[6] or 1 }
        end
        local f = sim:Get(name) or CreateFrame("Frame", name, UIParent, "BackdropTemplate")
        f:SetSize(4 * 40 + 16, 4 * 40 + 40)
        f:ClearAllPoints()
        f:SetPoint("CENTER", UIParent, "CENTER", x, y)
        if f.SetBackdrop then
            f:SetBackdrop({ bgFile = "Interface\\Buttons\\WHITE8x8", edgeFile = "Interface\\Buttons\\WHITE8x8", edgeSize = 1 })
            f:SetBackdropColor(0.08, 0.08, 0.1, 0.95)
            f:SetBackdropBorderColor(0.4, 0.4, 0.4, 1)
        end
        if not f.title then
            f.title = f:CreateFontString(nil, "OVERLAY", "GameFontNormal")
            f.title:SetPoint("TOPLEFT", 10, -10)
        end
        f.title:SetText(title)
        f.buttons = f.buttons or {}
        for slot = 1, 16 do
            local b = f.buttons[slot]
            if not b then
                b = CreateFrame("Button", name .. "Item" .. slot, f)
                b:SetSize(37, 37)
                b:SetPoint("TOPLEFT", 8 + ((slot - 1) % 4) * 40, -32 - math.floor((slot - 1) / 4) * 40)
                b.icon = b:CreateTexture(nil, "BORDER")
                b.icon:SetAllPoints()
                b.border = b:CreateTexture(nil, "ARTWORK")
                b.border:SetAllPoints()
                b.border:SetTexture("Interface\\Common\\WhiteIconFrame")
                b.count = b:CreateFontString(nil, "OVERLAY", "NumberFontNormal")
                b.count:SetPoint("BOTTOMRIGHT", -2, 2)
                b:SetID(slot)
                f.buttons[slot] = b
            end
            local s = sim.bags[bag][slot]
            local it = s and sim.items[s.itemID]
            b.icon:SetColorTexture(it and 0.25 or 0.12, it and 0.22 or 0.12, it and 0.18 or 0.12, 1)
            local c = it and BORDER[it.quality]
            b.border:SetShown(c ~= nil)
            if c then
                b.border:SetVertexColor(c[1], c[2], c[3])
            end
            b.count:SetText(s and (s.stackCount or 1) > 1 and s.stackCount or "")
            if bank then
                b.GetBankTabID = function()
                    return bag
                end
                b.GetContainerSlotID = function()
                    return slot
                end
            else
                b.GetBagID = function()
                    return bag
                end
            end
        end
        function f:EnumerateValidItems()
            return ipairs(self.buttons)
        end
        f:Show()
        return f
    end
    -- (The window would sit on top of them.)
    local w = sim:Get("ForeverBuddyWindow")
    if w and w:IsShown() then
        w:Hide()
    end
    local backpack = grid("LiveBags", "Backpack", BAG_ITEMS, 0, 110, 0)
    local bank = grid("BankPanel", "Bank", BANK_ITEMS, 6, -110, 0, true)
    function bank.RefreshAllItemsForSelectedTab() end
    rawset(sim.env, "ContainerFrameUtil_EnumerateContainerFrames", function()
        return ipairs({ backpack })
    end)
    sim:FireEvent("BANKFRAME_OPENED")
    sim:FireEvent("BAG_UPDATE_DELAYED")
    sim:Advance(1)
    return "bags and bank up: junk, gear of each quality, a shirt, stacks (screenshot <name> bags)"
end

-- The SVG of the screen, cropped to the window and its tabs when it's open,
-- or with `bags` to the bags and bank the `bags` command put up.
local function bounds(sim, what)
    local l, b, r, t
    local function add(frame)
        if frame and sim._isVisible(frame) then
            local x, y, w, h = S.layout.rect(sim, frame)
            if x then
                l, b = math.min(l or x, x), math.min(b or y, y)
                r, t = math.max(r or x + w, x + w), math.max(t or y + h, y + h)
            end
        end
    end
    if what == "bags" then
        add(sim:Get("LiveBags"))
        add(sim:Get("BankPanel"))
    else
        local w = sim:Get("ForeverBuddyWindow")
        add(w)
        for _, tab in ipairs(w and w.Tabs or {}) do
            add(tab)
        end
    end
    if not l then
        return nil
    end
    local M = 12
    return math.floor(l - M), math.floor(b - M), math.ceil(r - l + 2 * M), math.ceil(t - b + 2 * M)
end

local function svgOf(sim, what)
    if opts.art then
        sim:_registerAddonArt()
    end
    local svg = S.render.svg(sim, { art = opts.art, embed = true })
    local l, b, w, h
    if what ~= "full" then
        l, b, w, h = bounds(sim, what)
    end
    if not l then
        l, b, w, h = 0, 0, S.layout.SCREEN_W, S.layout.SCREEN_H
    end
    local y = S.layout.SCREEN_H - (b + h)
    local n
    svg, n = svg:gsub('viewBox="0 0 %d+ %d+" width="%d+" height="%d+"',
        ('viewBox="%d %d %d %d" width="%d" height="%d"'):format(l, y, w, h, w, h), 1)
    assert(n == 1, "WOWAPI's SVG header changed")
    return svg, l, y, w, h
end

-- An SVG to a PNG at 2x, without a window. QuickLook's thumbnails are
-- square and filled, so the picture goes in the middle of a square canvas
-- and sips crops the middle back out.
local function png(svgPath, pngPath, svg, vx, vy, w, h)
    local W, H = 2 * w, 2 * h
    if has("qlmanage") and has("sips") then
        local side = math.max(w, h)
        local square = svg:gsub('viewBox="[^"]*" width="%d+" height="%d+"',
            ('viewBox="%g %g %d %d" width="%d" height="%d"'):format(vx - (side - w) / 2, vy - (side - h) / 2,
                side, side, 2 * side, 2 * side), 1)
        local tmp = DIR .. "/.ql"
        sh("rm -rf " .. sq(tmp) .. " && mkdir -p " .. sq(tmp))
        writeAll(tmp .. "/shot.svg", square)
        sh("qlmanage -t -s " .. 2 * side .. " -o " .. sq(tmp) .. " " .. sq(tmp .. "/shot.svg") .. " >/dev/null 2>&1")
        local made = tmp .. "/shot.svg.png"
        if readAll(made) then
            local cropped = sh("sips -c " .. H .. " " .. W .. " " .. sq(made) .. " --out " .. sq(pngPath) .. " >/dev/null 2>&1")
            sh("rm -rf " .. sq(tmp))
            if cropped then
                return true
            end
        end
        sh("rm -rf " .. sq(tmp))
    end
    if has("rsvg-convert") then
        return sh("rsvg-convert -z 2 -o " .. sq(pngPath) .. " " .. sq(svgPath))
    end
    for _, browser in ipairs({ "chromium", "chromium-browser", "google-chrome", "google-chrome-stable" }) do
        if has(browser) then
            local abs = svgPath:sub(1, 1) == "/" and svgPath or (io.popen("pwd"):read("*l") .. "/" .. svgPath)
            local out = pngPath:sub(1, 1) == "/" and pngPath or (io.popen("pwd"):read("*l") .. "/" .. pngPath)
            return sh(("%s --headless=new --disable-gpu --no-sandbox --hide-scrollbars --force-device-scale-factor=2 --window-size=%d,%d --screenshot=%s %s >/dev/null 2>&1")
                :format(browser, w, h, sq(out), sq("file://" .. abs)))
        end
    end
    return false
end

local function screenshot(name, what)
    if not name or not name:match("^[%w_%-]+$") then
        error("screenshot <name> [full|bags]: a name of letters, digits, - and _", 0)
    end
    if what ~= "" and what ~= "full" and what ~= "bags" then
        error("screenshot <name> [full|bags]", 0)
    end
    local sim = need()
    sh("mkdir -p " .. sq(DIR .. "/shots"))
    local svg, vx, vy, w, h = svgOf(sim, what)
    local svgPath, pngPath = DIR .. "/shots/" .. name .. ".svg", DIR .. "/shots/" .. name .. ".png"
    writeAll(svgPath, svg)
    os.remove(pngPath)
    if png(svgPath, pngPath, svg, vx, vy, w, h) and readAll(pngPath) then
        return pngPath .. (" (%dx%d)"):format(2 * w, 2 * h)
    end
    return svgPath .. " (no PNG: needs qlmanage, rsvg-convert or a headless Chrome)"
end

local function dump(v, limit)
    local ok, text = pcall(S.serialize.serialize, v)
    if not ok then
        text = tostring(v)
    end
    if limit and #text > limit then
        return text:sub(1, limit) .. ("\n... (%d more characters)"):format(#text - limit)
    end
    return text
end

-- What a command caused: new chat lines and new Lua errors.
local function news()
    local sim = S.sim
    if not sim then
        return ""
    end
    local out = {}
    for i = S.chatSeen + 1, #sim.chat do
        out[#out + 1] = "chat: " .. tostring(sim.chat[i].text):gsub("|c%x%x%x%x%x%x%x%x", ""):gsub("|r", "")
    end
    for i = S.errorsSeen + 1, #sim.errors do
        local e = sim.errors[i]
        out[#out + 1] = "LUA ERROR: " .. tostring(type(e) == "table" and (e.message or e.msg) or e)
    end
    S.chatSeen, S.errorsSeen = #sim.chat, #sim.errors
    return table.concat(out, "\n")
end

local HELP = [[
login [filled|empty]     a fresh session: the app's slots for Thrandor, or none (as shipped)
slash /fb plan           a slash command, as typed in chat
click <frame>            by global name, a path (ForeverBuddyWindow.Tabs.5), or the text on it
set <setting> on|off     a Settings-tab box, by id (window.coach) or label
buttons                  what's clickable on screen: name, text, centre
advance <seconds>        let time pass (timers, OnUpdate)
reload                   /reload (SavedVariables kept)
combat on|off            enter or leave combat
state [path]             SavedVariables: ForeverBuddyDB, ForeverBuddySettings, or a path into them
chat [n]                 the last n chat lines (10)
errors                   every Lua error this session
lua <code>               run Lua in the game (like /run); prints what it returns
bags                     put up the bags and the bank (stand-ins in Blizzard's shape, with junk and gear)
screenshot <name> [full|bags]  .addon-live/shots/<name>.png, of the window (or the screen, or the bags)
upload <name> ...        those shots to the mycelium room (mycelium file upload); prints the links]]

local COMMANDS = {}

function COMMANDS.help()
    return HELP
end

function COMMANDS.login(rest)
    return login(rest ~= "" and rest or nil)
end

function COMMANDS.slash(rest)
    if not rest:match("^/") then
        rest = "/" .. rest
    end
    need():Slash(rest)
    S.sim:Advance(0.2)
    return "ok"
end

function COMMANDS.click(rest)
    need()
    local f = find(rest)
    S.sim:Click(f)
    S.sim:Advance(0.2)
    return "clicked " .. (f.GetName and f:GetName() or textOf(f) or rest)
end

function COMMANDS.set(rest)
    need()
    local key, value = rest:match("^(.-)%s+(%S+)$")
    local on = ({ on = true, off = false, ["true"] = true, ["false"] = false })[value or ""]
    if not key or on == nil then
        error("set <setting> on|off", 0)
    end
    local cb = settingBox(key)
    if (cb:GetChecked() and true or false) ~= on then
        S.sim:Click(cb)
        S.sim:Advance(0.2)
    end
    return ("%s is %s"):format(cb.option and cb.option.key or key, (cb:GetChecked() and true or false) and "on" or "off")
end

function COMMANDS.bags()
    return putUpBags()
end

function COMMANDS.buttons()
    need()
    local lines = {}
    for _, c in ipairs(clickable()) do
        local text = c.text and c.text:gsub("|c%x%x%x%x%x%x%x%x", ""):gsub("|r", ""):gsub("\n", " ")
        lines[#lines + 1] = ("%-34s %-32s at %d,%d"):format(c.name or "-", text and ('"' .. text .. '"') or "", c.x, c.y)
    end
    return table.concat(lines, "\n")
end

function COMMANDS.advance(rest)
    local s = tonumber(rest)
    if not s or s < 0 or s > 86400 then
        error("advance <seconds>", 0)
    end
    need():Advance(s)
    return ("%g s later"):format(s)
end

function COMMANDS.reload()
    need():Reload()
    S.sim:Advance(6)
    return "reloaded"
end

function COMMANDS.combat(rest)
    if rest == "on" then
        need():EnterCombat()
    elseif rest == "off" then
        need():LeaveCombat()
    else
        error("combat on|off", 0)
    end
    S.sim:Advance(0.2)
    return "combat " .. rest
end

function COMMANDS.state(rest)
    local sim = need()
    local path = rest ~= "" and rest or nil
    if path then
        if not path:match("^[%a_][%w_]*[%.%w_]*$") then
            error("state <Global.path>", 0)
        end
        local v = sim:Get(path:match("^[^%.]+"))
        for part in path:gmatch("%.([^%.]+)") do
            v = type(v) == "table" and v[tonumber(part) or part] or nil
        end
        return dump(v, 6000)
    end
    local out = {}
    for _, name in ipairs({ "ForeverBuddyDB", "ForeverBuddySettings" }) do
        out[#out + 1] = name .. " = " .. dump(sim:Get(name), 3000)
    end
    return table.concat(out, "\n")
end

function COMMANDS.chat(rest)
    local sim = need()
    local n = tonumber(rest) or 10
    local out = {}
    for i = math.max(1, #sim.chat - n + 1), #sim.chat do
        out[#out + 1] = (tostring(sim.chat[i].text):gsub("|c%x%x%x%x%x%x%x%x", ""):gsub("|r", ""))
    end
    S.chatSeen = #sim.chat
    return #out > 0 and table.concat(out, "\n") or "(no chat)"
end

function COMMANDS.errors()
    local sim = need()
    local out = {}
    for _, e in ipairs(sim.errors) do
        out[#out + 1] = tostring(type(e) == "table" and (e.message or e.msg) or e)
    end
    return #out > 0 and table.concat(out, "\n") or "no Lua errors"
end

function COMMANDS.lua(rest)
    local results = { need():Exec(rest) }
    if not results[1] then
        return "error: " .. tostring(results[2])
    end
    local out = {}
    for i = 2, #results do
        out[#out + 1] = dump(results[i], 3000)
    end
    return #out > 0 and table.concat(out, "\n") or "ok"
end

function COMMANDS.screenshot(rest)
    local name, what = rest:match("^(%S+)%s*(%S*)$")
    return screenshot(name, what or "")
end

function COMMANDS.upload(rest)
    local files = {}
    for name in rest:gmatch("%S+") do
        local p = DIR .. "/shots/" .. name .. ".png"
        if not name:match("^[%w_%-]+$") or not readAll(p) then
            error("no shot " .. name .. " (take it with screenshot first)", 0)
        end
        files[#files + 1] = sq(p)
    end
    if #files == 0 then
        error("upload <name> ...", 0)
    end
    local p = io.popen("mycelium file upload " .. table.concat(files, " ") .. " 2>&1")
    local out = p:read("*a")
    p:close()
    return out
end

-- One command line in, its answer out (never throws).
local function run(line)
    local cmd, rest = line:match("^%s*(%S+)%s*(.-)%s*$")
    if not cmd or cmd:match("^#") then
        return nil
    end
    local fn = COMMANDS[cmd]
    if not fn then
        return "unknown command " .. cmd .. " (help lists them)", false
    end
    local ok, out = pcall(fn, rest)
    local extra = news()
    out = ok and tostring(out) or ("error: " .. tostring(out))
    return out .. (extra ~= "" and ("\n" .. extra) or ""), ok
end

-- The server: commands arrive on a named pipe, one per line as
-- "<reply file>\t<command>"; each answer is written to its reply file.
local function serve()
    loadWowapi()
    io.stdout:setvbuf("line")
    print(login("filled"))
    while true do
        local f = io.open(PIPE, "r")
        if not f then
            break
        end
        for line in f:lines() do
            local reply, command = line:match("^([^\t]+)\t(.*)$")
            if reply then
                if command == "stop" then
                    writeAll(reply .. ".tmp", "stopped\n")
                    os.rename(reply .. ".tmp", reply)
                    os.remove(PIPE)
                    os.remove(PIDFILE)
                    return
                end
                local out = run(command) or ""
                print("> " .. command)
                writeAll(reply .. ".tmp", out .. "\n")
                os.rename(reply .. ".tmp", reply)
            end
        end
        f:close()
    end
end

local function serverPid()
    local pid = tonumber(readAll(PIDFILE) or "")
    if pid and sh("kill -0 " .. pid .. " 2>/dev/null") then
        return pid
    end
    return nil
end

-- The client: one command to the server, its answer printed.
local function send(command)
    if not serverPid() then
        io.stderr:write("no session running: luajit tools/addon-live/live.lua start\n")
        os.exit(2)
    end
    local tmp = os.tmpname()
    os.remove(tmp)
    local reply = DIR .. "/reply." .. tmp:match("[^/\\]+$")
    local f = assert(io.open(PIPE, "a"))
    f:write(reply .. "\t" .. command:gsub("[\r\n]", " ") .. "\n")
    f:close()
    for _ = 1, 1200 do -- up to 2 minutes
        local out = readAll(reply)
        if out then
            os.remove(reply)
            io.write(out)
            return out:match("^error:") == nil
        end
        if not serverPid() then
            break
        end
        sh("sleep 0.1")
    end
    io.stderr:write("the session didn't answer; see " .. DIR .. "/server.log\n")
    os.exit(2)
end

-- (No math.randomseed: WOWAPI's fakes draw from math.random, and the same
-- session every time is the point.)
local verb = words[1]
if verb == "serve" then
    serve()
elseif verb == "start" then
    if serverPid() then
        print("already running (pid " .. serverPid() .. "); stop it first to start fresh")
        os.exit(0)
    end
    assert(sh("mkdir -p " .. sq(DIR)))
    os.remove(PIPE)
    assert(sh("mkfifo " .. sq(PIPE)), "mkfifo failed")
    local lua = arg[-1] or "luajit"
    sh(("nohup %s %s serve --dir %s%s > %s 2>&1 < /dev/null & echo $! > %s"):format(
        sq(lua), sq(arg[0]), sq(DIR), opts.art and " --art" or "", sq(DIR .. "/server.log"), sq(PIDFILE)))
    -- Wait for the login line.
    for _ = 1, 300 do
        local log = readAll(DIR .. "/server.log") or ""
        if log:match("logged in") then
            io.write(log)
            os.exit(0)
        end
        if not serverPid() then
            io.write(log)
            os.exit(1)
        end
        sh("sleep 0.1")
    end
    print("started, but no login yet; see " .. DIR .. "/server.log")
elseif verb == "stop" then
    if serverPid() then
        send("stop")
    else
        print("not running")
    end
elseif verb == "script" then
    local file = words[2]
    local text = file and readAll(file)
    if not text then
        io.stderr:write("script <file of commands>\n")
        os.exit(2)
    end
    loadWowapi()
    local failed = 0
    for line in text:gmatch("[^\n]+") do
        local out, ok = run(line)
        if out then
            print("> " .. line)
            print(out)
            if not ok or out:match("LUA ERROR") then
                failed = failed + 1
            end
        end
    end
    os.exit(failed == 0 and 0 or 1)
elseif verb then
    local ok = send(table.concat(words, " ", 1))
    os.exit(ok and 0 or 1)
else
    io.stderr:write("live.lua start | stop | script <file> | <command> (help)\n")
    os.exit(2)
end
