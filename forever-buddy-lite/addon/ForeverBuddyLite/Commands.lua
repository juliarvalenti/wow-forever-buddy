-- /fbl: the window on its own; otherwise the first word picks a command that
-- a module registered with ns.command, and the rest of the line is its argument.
-- Loads last, so every module has registered by now.

local _, ns = ...

ns.command({ name = "help", aliases = { "?" }, help = "this list", fn = function()
    ns.say("build " .. ns.versionString() .. ". /fbl opens the window. Commands:")
    for _, c in ipairs(ns.commands) do
        local aliases = c.aliases and #c.aliases > 0 and (" (" .. table.concat(c.aliases, ", ") .. ")") or ""
        print(string.format("  |cffffd200/fbl %s%s|r%s  %s", c.name, c.args and (" " .. c.args) or "", aliases, c.help or ""))
    end
end })

-- /rl: the same as /fbl r (snapshot, then reload, which writes the file), unless another addon
-- already took /rl.
if not SlashCmdList.FBLRELOAD and not (rawget(_G, "hash_SlashCmdList") or {})["/RL"] then
    SLASH_FBLRELOAD1 = "/rl"
    SlashCmdList.FBLRELOAD = function()
        ns.run("r")
    end
end

SLASH_FOREVERBUDDYLITE1 = "/fbl"
SlashCmdList.FOREVERBUDDYLITE = function(msg)
    msg = (msg or ""):gsub("^%s+", ""):gsub("%s+$", "")
    if msg == "" then
        ns.toggleWindow()
        return
    end
    local word, rest = msg:match("^(%S+)%s*(.*)$")
    local c = ns.findCommand(word:lower())
    if c then
        c.fn(rest or "")
    else
        ns.say("no command \"" .. word .. "\".")
        ns.run("help")
    end
end
