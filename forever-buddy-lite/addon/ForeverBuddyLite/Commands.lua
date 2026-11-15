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
