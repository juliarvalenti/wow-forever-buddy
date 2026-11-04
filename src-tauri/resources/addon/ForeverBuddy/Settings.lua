-- ForeverBuddy's settings, declared once (P4a, context/specs/p4-claude-settings §2).
--
-- Data only: what each option is, not what it does (that's in
-- ForeverBuddy.lua, by id). The window's module pages and Settings page and
-- Esc › Options › AddOns all read this list, in this order, and the harness
-- generates settings-schema.json from it for the app and its tools.
--
--   id       stable, "<module>.<name>"; never renamed (profiles refer to it)
--   module   which feature it belongs to
--   section  the heading it's listed under
--   type     "bool" (a switch) or "number" (a slider, with min, max and
--            step); a look option may also be "enum" (one of its values)
--            or "color" ("#rrggbb", or "" for the look's own)
--   default  what a new player has, and what Options' Defaults goes back to
--   label    its name
--   help     the one line shown on hover (and under it in the window)
--   parent   (optional) the id it depends on: shown, and in effect, only
--            while that one is on
--   acts     whether turning it on lets the addon do something in the game
--            for you (none do yet; P4 §8)
--   showHelp (optional) the help line also shows under the label in
--            Esc › Options
--   moduleSwitch (optional) the module's own on/off (W1): the switch at the
--            top of its page and the dot in the window's rail
--   look     (optional, ED1) part of the HUD's or the bars' look: set by
--            Claude and the app, never drawn in the window or Options, and
--            left alone by presets
--   yields   (optional, COMPAT1) addons that already do this job: while one
--            is loaded, ours stays off and its row says so
--
-- A profile's values are checked against these, in the addon and in the
-- app: a number's min and max, and (ready for later types) an "enum"'s
-- values = { ... } and a "string"'s maxLength (200 if unset).
--
-- modules: the window's Modules (W1), in rail order. Each has its label,
-- the line under its page title, optionally a note shown on its page (for
-- one with no options yet), and its options: those whose module is its
-- id, plus any it lists (the Coach's were declared before modules were).
-- Its switch is "<id>.enabled", on by default so turning modules into
-- modules switched nobody's choices off (a new module may default off, as
-- the HUD does); each option at the top of a module hangs off it.
--
-- lints: known poor combinations (P4 §1, layer 4), each with the values it
-- fires on ({ min, max } for a number, else the value itself) and the line
-- the app shows beside a proposal that would make it fire. A lint with an
-- `addon` fires only while the game last had that addon loaded (COMPAT1).
--
-- presets: named starting points; each module adds its row of values,
-- except options that act, which are only ever turned on one at a time.

local _, ns = ...

ns.settings = {
    version = 1,
    modules = {
        { id = "bags", label = "Bags", sub = "Junk and item levels on what you carry. Display only." },
        { id = "map", label = "Map", sub = "Coordinates, and a map you can see through. Display only." },
        { id = "quest", label = "Quests", sub = "Levels, new quests and progress where you look. Display only." },
        { id = "tooltips", label = "Tooltips", sub = "Who has an item, on its tooltip." },
        {
            id = "coach",
            label = "Coach",
            sub = "This session's gold, experience and loot, on screen.",
            options = { "window.coach", "window.coachCombat" },
        },
        {
            id = "merchant",
            label = "Merchant",
            sub = "Repairs and selling, at a vendor you open. Each option is off until you turn it on.",
        },
        {
            id = "questhelp",
            label = "Quest helper",
            sub = "Accepts and hands in quests you open with an NPC. Each option starts off.",
        },
        {
            id = "hud",
            label = "HUD",
            sub = "Health, power and combo points under your character. Display only.",
            note = "Turn it on to replace your player and target frames. Blizzard's come back when it's off.",
        },
        {
            id = "actionbars",
            label = "Action bars",
            sub = "Your action bars in a clean grid, in place of Blizzard's. Off until you turn it on.",
        },
    },
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
        -- W1: the Bags module (P4d), display only. Sorting and selling wait
        -- for the "may the addon act" call (P4 §8.1).
        {
            id = "bags.enabled",
            module = "bags",
            section = "Bags",
            type = "bool",
            default = true,
            label = "Bags",
            help = "Junk and item levels on what you carry. Display only.",
            moduleSwitch = true,
            acts = false,
        },
        {
            id = "bags.junk",
            module = "bags",
            section = "Bags",
            type = "bool",
            default = false,
            label = "Junk in your bags",
            help = "A small coin on grey items, the ones only worth selling, in your bags and bank.",
            parent = "bags.enabled",
            yields = { "Baganator" },
            acts = false,
        },
        {
            id = "bags.itemLevel",
            module = "bags",
            section = "Bags",
            type = "bool",
            default = false,
            label = "Item level on gear",
            help = "The item level on each piece of gear in your bags and bank, in its quality colour.",
            parent = "bags.enabled",
            yields = { "Baganator" },
            acts = false,
        },
        -- P4c: the Map module. Display only.
        {
            id = "map.enabled",
            module = "map",
            section = "Map",
            type = "bool",
            default = true,
            label = "Map",
            help = "Coordinates, and a map you can see through. Display only.",
            moduleSwitch = true,
            acts = false,
        },
        {
            id = "map.coords",
            module = "map",
            section = "Map",
            type = "bool",
            default = false,
            label = "Coordinates on the world map",
            help = "Where you are, and where the cursor points, along the bottom of the world map.",
            parent = "map.enabled",
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
            parent = "map.enabled",
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
            parent = "map.enabled",
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
        -- P4e: the Quest module's display part. Display only: nothing here
        -- accepts or hands in a quest.
        {
            id = "quest.enabled",
            module = "quest",
            section = "Quests",
            type = "bool",
            default = true,
            label = "Quests",
            help = "Levels, new quests and progress where you look. Display only.",
            moduleSwitch = true,
            acts = false,
        },
        {
            id = "quest.levels",
            module = "quest",
            section = "Quests",
            type = "bool",
            default = false,
            label = "Quest levels in the quest log",
            help = "Each quest's level before its title, like [23] Wanted: Hogger.",
            parent = "quest.enabled",
            acts = false,
        },
        {
            id = "quest.newTag",
            module = "quest",
            section = "Quests",
            type = "bool",
            default = false,
            label = "Mark quests that are new in Forever",
            help = "A (New) tag after the quests WoW Forever added, so you can tell them from the original game's.",
            parent = "quest.enabled",
            acts = false,
        },
        {
            id = "quest.tooltips",
            module = "quest",
            section = "Quests",
            type = "bool",
            default = false,
            label = "Quest progress on item tooltips",
            help = "When a quest in your log needs an item, its tooltip says which quest and how many you have.",
            parent = "quest.enabled",
            acts = false,
        },
        {
            id = "quest.trackerFade",
            module = "quest",
            section = "Quests",
            type = "bool",
            default = false,
            label = "See-through quest tracker",
            help = "The quest tracker fades until you move the mouse over it.",
            parent = "quest.enabled",
            yields = { "Questie" },
            acts = false,
        },
        {
            id = "quest.trackerAlpha",
            module = "quest",
            section = "Quests",
            type = "number",
            min = 0.1,
            max = 0.9,
            step = 0.05,
            default = 0.4,
            label = "How much of the tracker shows",
            help = "Lower is more see-through. It's whole while the mouse is over it.",
            parent = "quest.trackerFade",
            acts = false,
        },
        -- W1: the Tooltips module.
        {
            id = "tooltips.enabled",
            module = "tooltips",
            section = "Tooltips",
            type = "bool",
            default = true,
            label = "Tooltips",
            help = "Who has an item, on its tooltip.",
            moduleSwitch = true,
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
            parent = "tooltips.enabled",
            acts = false,
        },
        -- W1: the Coach module (the strip was declared as window.coach).
        {
            id = "coach.enabled",
            module = "coach",
            section = "Coach",
            type = "bool",
            default = true,
            label = "Coach",
            help = "This session's gold, experience and loot, on screen.",
            moduleSwitch = true,
            acts = false,
        },
        {
            id = "window.coach",
            module = "window",
            section = "Coach",
            type = "bool",
            default = false,
            label = "Session coach",
            help = "A small strip with this session's gold, experience and loot.",
            parent = "coach.enabled",
            acts = false,
        },
        {
            id = "window.coachCombat",
            module = "window",
            section = "Coach",
            type = "bool",
            default = false,
            label = "Hide the coach in combat",
            help = "Steps out of the way while you fight.",
            parent = "window.coach",
            acts = false,
        },
        -- M1: the Merchant module, the first that acts (P4 §8.1). Each option
        -- does something only when you open a vendor yourself; all are off
        -- until turned on, one at a time (never by a preset). W1: the module's
        -- own switch stops all of it at once; the options stay the real gate.
        {
            id = "merchant.enabled",
            module = "merchant",
            section = "Merchant",
            type = "bool",
            default = true,
            label = "Merchant",
            help = "Repairs and selling, at a vendor you open. Each option is off until you turn it on.",
            moduleSwitch = true,
            acts = false,
        },
        {
            id = "merchant.repair",
            module = "merchant",
            section = "Merchant",
            type = "bool",
            default = false,
            label = "Repair at vendors",
            help = "Pays for your repairs when you open a vendor who can repair.",
            parent = "merchant.enabled",
            showHelp = true,
            acts = true,
        },
        {
            id = "merchant.repairGuild",
            module = "merchant",
            section = "Merchant",
            type = "bool",
            default = false,
            label = "Guild bank pays first",
            help = "Uses guild funds when your rank allows it, and your own gold when it doesn't.",
            parent = "merchant.repair",
            showHelp = true,
            acts = true,
        },
        {
            id = "merchant.sellJunk",
            module = "merchant",
            section = "Merchant",
            type = "bool",
            default = false,
            label = "Sell grey items",
            help = "When you open a vendor, sells up to 12 grey items. The vendor's buyback keeps them.",
            parent = "merchant.enabled",
            showHelp = true,
            acts = true,
        },
        {
            id = "merchant.sellMarked",
            module = "merchant",
            section = "Merchant",
            type = "bool",
            default = false,
            label = "Sell items you marked",
            help = "Also sells items you marked to sell in Forever Buddy, within the same 12.",
            parent = "merchant.enabled",
            showHelp = true,
            acts = true,
        },
        -- M3: the Quest helper, acting on quest windows you open with an NPC.
        -- Off until turned on, one at a time (never by a preset). W1: the
        -- module's switch stops all of it at once.
        {
            id = "questhelp.enabled",
            module = "questhelp",
            section = "Quest helper",
            type = "bool",
            default = true,
            label = "Quest helper",
            help = "Accepts and hands in quests you open with an NPC. Each option is off until you turn it on.",
            moduleSwitch = true,
            acts = false,
        },
        {
            id = "questhelp.accept",
            module = "questhelp",
            section = "Quest helper",
            type = "bool",
            default = false,
            label = "Accept quests",
            help = "Accepts a quest when you open it with an NPC. Hold Shift as you open it to read it first.",
            parent = "questhelp.enabled",
            showHelp = true,
            acts = true,
        },
        {
            id = "questhelp.turnIn",
            module = "questhelp",
            section = "Quest helper",
            type = "bool",
            default = false,
            label = "Hand in quests",
            help = "Hands in a finished quest when you open it. With a reward to choose, or gold to pay, it waits for you.",
            parent = "questhelp.enabled",
            showHelp = true,
            acts = true,
        },
        {
            id = "questhelp.repeatable",
            module = "questhelp",
            section = "Quest helper",
            type = "bool",
            default = false,
            label = "Daily and repeatable quests too",
            help = "Without this, the helper leaves those for you to accept and hand in.",
            parent = "questhelp.enabled",
            showHelp = true,
            acts = true,
        },
        -- M3b: dialog keys. Acts only on your own keypress, but it acts, so
        -- it's off until turned on and in no preset.
        {
            id = "questhelp.dialogKeys",
            module = "questhelp",
            section = "Quest helper",
            type = "bool",
            default = false,
            label = "Dialog keys",
            help = "In an NPC's chat or a quest window, Space takes the first choice and 1 to 9 the others. "
                .. "Not in combat, and never a reward choice.",
            parent = "questhelp.enabled",
            showHelp = true,
            yields = { "DialogueUI", "DialogKey" },
            acts = true,
        },
        -- TH2: the HUD module, the one module that's off until you turn it
        -- on (it takes over where your health shows).
        {
            id = "hud.enabled",
            module = "hud",
            section = "HUD",
            type = "bool",
            default = false,
            label = "HUD",
            help = "Your health and your target's, power, combo points and Slice and Dice, under your character. "
                .. "Blizzard's player and target frames turn see-through while it's on. Display only.",
            moduleSwitch = true,
            acts = false,
        },
        -- TH3: our own action bars, secure buttons in the round 4 grid. Off
        -- until turned on (the launch-week HUD ships behind this), and no
        -- preset turns it on.
        {
            id = "actionbars.enabled",
            module = "actionbars",
            section = "Action bars",
            type = "bool",
            default = true,
            label = "Action bars",
            help = "Your action bars in a clean grid, in place of Blizzard's.",
            moduleSwitch = true,
            acts = false,
        },
        {
            id = "actionbars.ours",
            module = "actionbars",
            section = "Action bars",
            type = "bool",
            default = false,
            label = "Our action bars",
            help = "Your first two bars in our look. Same spells, binds and drag and drop; Blizzard's bars fade out. Changes wait until you leave combat.",
            parent = "actionbars.enabled",
            showHelp = true,
            acts = false,
        },
        -- ED1: the look of the HUD and the action bars, set by Claude (a
        -- draft the player picks, or a proposal they approve) and seen on
        -- the app's Your UI tab. Not drawn in the window or Esc › Options.
        -- The defaults are round 4's (design/ingame/rogue/specs/round4.lua).
        {
            id = "hud.healthColor",
            module = "hud",
            section = "Look",
            type = "color",
            default = "#4fd99a",
            label = "Health colour",
            help = "Your health bar, and a friendly target's, as #rrggbb. Empty for round 4's green.",
            look = true,
            acts = false,
        },
        {
            id = "hud.powerColor",
            module = "hud",
            section = "Look",
            type = "color",
            default = "",
            label = "Power colour",
            help = "Your power bar, as #rrggbb. Empty: each kind of power in its own colour (mana blue, rage red, energy yellow).",
            look = true,
            acts = false,
        },
        {
            id = "hud.accentColor",
            module = "hud",
            section = "Look",
            type = "color",
            default = "#fff569",
            label = "Accent colour",
            help = "Combo points, as #rrggbb. Empty for round 4's yellow.",
            look = true,
            acts = false,
        },
        {
            id = "hud.gradient",
            module = "hud",
            section = "Look",
            type = "bool",
            default = true,
            label = "Gradient on bars",
            help = "Bars lighter on the left and deeper on the right. Off: one flat colour.",
            look = true,
            acts = false,
        },
        {
            id = "hud.width",
            module = "hud",
            section = "Look",
            type = "number",
            default = 300,
            min = 200,
            max = 420,
            step = 2,
            label = "Frame width",
            help = "Your frame's and your target's width, in pixels on a 1080p screen.",
            look = true,
            acts = false,
        },
        {
            id = "hud.height",
            module = "hud",
            section = "Look",
            type = "number",
            default = 48,
            min = 32,
            max = 72,
            step = 1,
            label = "Frame height",
            help = "Their height, in pixels on a 1080p screen. The power line under the health bar stays 5 pixels.",
            look = true,
            acts = false,
        },
        {
            id = "hud.textSize",
            module = "hud",
            section = "Look",
            type = "number",
            default = 22,
            min = 14,
            max = 32,
            step = 1,
            label = "Number size",
            help = "Your health numbers' size; your target's and the names follow it.",
            look = true,
            acts = false,
        },
        {
            id = "hud.outline",
            module = "hud",
            section = "Look",
            type = "enum",
            values = { "thick", "thin", "none" },
            default = "thick",
            label = "Text outline",
            help = "The stroke around the HUD's text and the buttons' numbers: thick, thin or none.",
            look = true,
            acts = false,
        },
        {
            id = "hud.layout",
            module = "hud",
            section = "Look",
            type = "enum",
            values = { "side", "stacked" },
            default = "side",
            label = "Layout",
            help = "side: yours left of the action bars and your target's right. stacked: both above the bars, your target's on top.",
            look = true,
            acts = false,
        },
        {
            id = "actionbars.iconSize",
            module = "actionbars",
            section = "Look",
            type = "number",
            default = 46,
            min = 32,
            max = 64,
            step = 1,
            label = "Button size",
            help = "Each action button's size, in pixels on a 1080p screen. Changes wait until you leave combat.",
            look = true,
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
        {
            id = "quest.trackerNearlyGone",
            when = { ["quest.trackerFade"] = true, ["quest.trackerAlpha"] = { max = 0.15 } },
            say = "The quest tracker almost disappears until you move the mouse over it.",
        },
        {
            id = "bags.nothingToSell",
            when = { ["bags.junk"] = false, ["window.bagMarks"] = false },
            say = "Nothing in your bags will show what's only worth selling.",
        },
        {
            id = "merchant.sellsUnseen",
            when = { ["merchant.sellMarked"] = true, ["window.bagMarks"] = false },
            say = "Marked items will sell without a mark in your bags to show which ones.",
        },
        -- COMPAT1: two addons accepting or handing in the same quest race
        -- each other. Questie has both, in its own options.
        {
            id = "questhelp.acceptsWithQuestie",
            when = { ["questhelp.accept"] = true },
            addon = "Questie",
            say = "Questie can accept quests too. If its auto-accept is on, keep only one of the two.",
        },
        {
            id = "questhelp.handsInWithQuestie",
            when = { ["questhelp.turnIn"] = true },
            addon = "Questie",
            say = "Questie can hand in quests too. If its auto-complete is on, keep only one of the two.",
        },
    },
    presets = {
        -- The designer's three (P4c), a row for every option except "Record
        -- my next session", which no preset touches. Every module stays on
        -- (W1): a preset picks what each one shows. The HUD (TH2) and our
        -- action bars (TH3) stay off: they take over Blizzard's, so they're
        -- only ever your own choice.
        {
            id = "quiet",
            label = "Quiet",
            values = {
                ["window.briefing"] = true, ["tooltips.alts"] = true,
                ["window.lockouts"] = false, ["window.coach"] = false, ["window.coachCombat"] = false,
                ["window.card"] = false, ["window.bagMarks"] = false, ["window.minimap"] = false,
                ["map.coords"] = false, ["map.minimap"] = false, ["map.fade"] = false,
                ["quest.levels"] = false, ["quest.newTag"] = false, ["quest.tooltips"] = false,
                ["quest.trackerFade"] = false,
                ["bags.junk"] = false, ["bags.itemLevel"] = false,
                ["bags.enabled"] = true, ["map.enabled"] = true, ["quest.enabled"] = true,
                ["tooltips.enabled"] = true, ["coach.enabled"] = true, ["merchant.enabled"] = true,
                ["questhelp.enabled"] = true,
                ["hud.enabled"] = false,
                ["actionbars.enabled"] = true, ["actionbars.ours"] = false,
            },
        },
        {
            id = "helpful",
            label = "Helpful",
            values = {
                ["window.briefing"] = true, ["tooltips.alts"] = true,
                ["window.lockouts"] = true, ["window.coach"] = false, ["window.coachCombat"] = false,
                ["window.card"] = true, ["window.bagMarks"] = true, ["window.minimap"] = true,
                ["map.coords"] = true, ["map.minimap"] = false, ["map.fade"] = false,
                ["quest.levels"] = true, ["quest.newTag"] = true, ["quest.tooltips"] = true,
                ["quest.trackerFade"] = false,
                ["bags.junk"] = true, ["bags.itemLevel"] = true,
                ["bags.enabled"] = true, ["map.enabled"] = true, ["quest.enabled"] = true,
                ["tooltips.enabled"] = true, ["coach.enabled"] = true, ["merchant.enabled"] = true,
                ["questhelp.enabled"] = true,
                ["hud.enabled"] = false,
                ["actionbars.enabled"] = true, ["actionbars.ours"] = false,
            },
        },
        {
            id = "everything",
            label = "Everything",
            values = {
                ["window.briefing"] = true, ["tooltips.alts"] = true,
                ["window.lockouts"] = true, ["window.coach"] = true, ["window.coachCombat"] = true,
                ["window.card"] = true, ["window.bagMarks"] = true, ["window.minimap"] = true,
                ["map.coords"] = true, ["map.minimap"] = true, ["map.fade"] = true, ["map.fadeAlpha"] = 0.5,
                ["quest.levels"] = true, ["quest.newTag"] = true, ["quest.tooltips"] = true,
                ["quest.trackerFade"] = true, ["quest.trackerAlpha"] = 0.5,
                ["bags.junk"] = true, ["bags.itemLevel"] = true,
                ["bags.enabled"] = true, ["map.enabled"] = true, ["quest.enabled"] = true,
                ["tooltips.enabled"] = true, ["coach.enabled"] = true, ["merchant.enabled"] = true,
                ["questhelp.enabled"] = true,
                ["hud.enabled"] = false,
                ["actionbars.enabled"] = true, ["actionbars.ours"] = false,
            },
        },
    },
}
