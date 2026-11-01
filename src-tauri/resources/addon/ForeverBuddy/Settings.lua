-- ForeverBuddy's settings, declared once (P4a, context/specs/p4-claude-settings §2).
--
-- Data only: what each option is, not what it does (that's in
-- ForeverBuddy.lua, by id). The window's Settings tab and Esc › Options ›
-- AddOns both read this list, in this order, and the harness generates
-- settings-schema.json from it for the app and its tools.
--
--   id       stable, "<module>.<name>"; never renamed (profiles refer to it)
--   module   which feature it belongs to
--   section  the heading it's listed under
--   type     "bool" (a tick box) or "number" (a slider, with min, max and
--            step)
--   default  what a new player has, and what Options' Defaults goes back to
--   label    its name
--   help     the one line shown on hover
--   parent   (optional) the id it depends on: shown, and in effect, only
--            while that one is on
--   acts     whether turning it on lets the addon do something in the game
--            for you (none do yet; P4 §8)
--   showHelp (optional) the help line also shows under the label
--
-- A profile's values are checked against these, in the addon and in the
-- app: a number's min and max, and (ready for later types) an "enum"'s
-- values = { ... } and a "string"'s maxLength (200 if unset).
--
-- lints: known poor combinations (P4 §1, layer 4), each with the values it
-- fires on ({ min, max } for a number, else the value itself) and the line
-- the app shows beside a proposal that would make it fire.
--
-- presets: named starting points; each module adds its row of values.

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
        -- P4c: the Map module. Display only.
        {
            id = "map.coords",
            module = "map",
            section = "Map",
            type = "bool",
            default = false,
            label = "Coordinates on the world map",
            help = "Where you are, and where the cursor points, along the bottom of the world map.",
            acts = false,
        },
        {
            id = "map.minimap",
            module = "map",
            section = "Map",
            type = "bool",
            default = false,
            label = "Coordinates under the minimap",
            help = "Where you are, in small numbers under the minimap.",
            acts = false,
        },
        {
            id = "map.fade",
            module = "map",
            section = "Map",
            type = "bool",
            default = false,
            label = "Fade the world map while you move",
            help = "The map turns see-through while you walk or ride, so you can see where you're going.",
            acts = false,
        },
        {
            id = "map.fadeAlpha",
            module = "map",
            section = "Map",
            type = "number",
            min = 0.1,
            max = 0.9,
            step = 0.05,
            default = 0.5,
            label = "How much of the map shows while moving",
            help = "Lower is more see-through.",
            parent = "map.fade",
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
    lints = {
        {
            id = "map.fadeNearlyGone",
            when = { ["map.fade"] = true, ["map.fadeAlpha"] = { max = 0.2 } },
            say = "The world map almost disappears while you move.",
        },
    },
    presets = {
        {
            id = "minimal",
            label = "Minimal",
            values = { ["map.coords"] = false, ["map.minimap"] = false, ["map.fade"] = true, ["map.fadeAlpha"] = 0.4 },
        },
        {
            id = "questing",
            label = "Questing",
            values = { ["map.coords"] = true, ["map.minimap"] = true, ["map.fade"] = true, ["map.fadeAlpha"] = 0.6 },
        },
    },
}
