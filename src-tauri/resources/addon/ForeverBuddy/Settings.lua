-- ForeverBuddy's settings, declared once (P4a, docs/specs/p4-claude-settings.md §2).
--
-- Data only: what each option is, not what it does (that's in
-- ForeverBuddy.lua, by id). The window's Settings tab and Esc › Options ›
-- AddOns both read this list, in this order, and the harness generates
-- settings-schema.json from it for the app and its tools.
--
--   id       stable, "<module>.<name>"; never renamed (profiles refer to it)
--   module   which feature it belongs to
--   section  the heading it's listed under
--   type     "bool" (the only type so far)
--   default  what a new player has, and what Options' Defaults goes back to
--   label    its name
--   help     the one line shown on hover
--   parent   (optional) the id it depends on: shown, and in effect, only
--            while that one is on
--   acts     whether turning it on lets the addon do something in the game
--            for you (none do yet; P4 §8)
--   showHelp (optional) the help line also shows under the label

local _, ns = ...

ns.settings = {
    version = 1,
    options = {
        {
            id = "window.briefing",
            module = "window",
            section = "In game",
            type = "bool",
            default = true,
            label = "Login briefing",
            help = "One quiet chat line when you log in: quests to hand in, repairs, mail on your alts, your note.",
            acts = false,
        },
        {
            id = "window.lockouts",
            module = "window",
            section = "In game",
            type = "bool",
            default = true,
            label = "Lockouts at the entrance",
            help = "When you enter a dungeon or raid, says which of your other characters are saved there.",
            acts = false,
        },
        {
            id = "window.coach",
            module = "window",
            section = "In game",
            type = "bool",
            default = false,
            label = "Session coach",
            help = "A small strip with this session's gold, experience and loot.",
            acts = false,
        },
        {
            id = "window.coachCombat",
            module = "window",
            section = "In game",
            type = "bool",
            default = false,
            label = "Hide the coach in combat",
            help = "Steps out of the way while you fight.",
            parent = "window.coach",
            acts = false,
        },
        {
            id = "window.card",
            module = "window",
            section = "In game",
            type = "bool",
            default = true,
            label = "Session card at logout",
            help = "A short summary of the session during the logout countdown.",
            acts = false,
        },
        {
            id = "window.bagMarks",
            module = "window",
            section = "In game",
            type = "bool",
            default = true,
            label = "Marks in your bags",
            help = "Coin and letter marks on items you marked to sell or send in Forever Buddy.",
            acts = false,
        },
        {
            id = "tooltips.alts",
            module = "tooltips",
            section = "Tooltips",
            type = "bool",
            default = true,
            label = "Your alts on item tooltips",
            help = "Which of your other characters hold an item, and where.",
            acts = false,
        },
        {
            id = "window.minimap",
            module = "window",
            section = "This window",
            type = "bool",
            default = true,
            label = "Minimap button",
            help = "Click to open Forever Buddy. Drag to move.",
            acts = false,
        },
        {
            id = "window.record",
            module = "window",
            section = "Help fix bugs",
            type = "bool",
            default = false,
            label = "Record my next session",
            help = "Numbers only: no names, chat or mail. Turns itself off when you log out.",
            showHelp = true,
            acts = false,
        },
    },
}
