---
name: forever-buddy
description: Work with WoW Forever Buddy, the desktop companion and in-game addon for World of Warcraft Forever. Use when the player asks about their characters, gold, items, quests or lockouts, wants to change their in-game UI or addon settings, wants a plan for tonight, or wants their GUI redesigned (for example "make my rogue UI look clean").
---

# Forever Buddy

Forever Buddy is a desktop app plus an addon (ForeverBuddy) for World of Warcraft: Forever. The app reads what the addon saves at logout, keeps a history, and lets an agent read it and propose changes. The player approves every change in the app before it reaches the game.

## Connect

The app exposes an MCP server. In the app, turn on Settings > Agents, then:

```sh
claude mcp add forever-buddy -- "<path to WoW Forever Buddy>" mcp
```

Every tool refuses while agent access is off. Tools only read; the `propose_*` tools queue a proposal the player approves or declines in the app. See [tools](references/tools.md).

## What you can help with

| The player wants | Do this |
|---|---|
| "Where is my X?", "how much gold?" | `find_items`, `list_characters`, `get_gold_history` |
| A plan for tonight | `get_recent_play`, `get_quests`, `get_lockouts`, then `propose_quest_plan` |
| Their addon set up a certain way | `get_settings`, then `propose_settings` with a reason for each change. Skip an option with `off_because`: another of their addons does that job, and ours stays off while it's loaded |
| Their bags cleaned up | `get_bag_marks`, then `propose_bag_marks` |
| "What macros does my rogue have?" | `list_macros` with the character, then `get_macro` for the text |
| "Make me a macro for this", "copy my mount macro to my alts" | `propose_macro`: one macro per call (a copy to three alts is three calls). For an edit, pass `was` exactly as `get_macro` gave it. Slash commands only: `/run`, `/script`, `/console` and `/dump` are refused |
| Their HUD or action bars to look different | Read [design](references/design.md), then make **two or three drafts** with `propose_ui_draft` and let the player pick one on the Your UI tab |
| A whole new UI, their own layout | Write a UI document and see it with `render_ui` in every moment before showing anyone: [ui](references/ui.md). It can't be sent to the game yet. |

## Rules that never bend

- **Propose, never act.** Nothing changes in the game until the player approves it in the app. Never ask the player to bypass an approval.
- **No automation.** Nothing plays the game: no botting, no reading game memory, no input automation. Modules that act in the game (Merchant, Quest helper) are off by default, only act on windows the player opened, and are never turned on in bulk.
- **Macros are the player's to press.** A proposed macro is approved on its own, then applied in the game by the player's own click; it only ever runs when they press it. Never propose code.
- **Display only about other players.** Never store or propose anything about characters the player doesn't own.
- **The player's characters only.** Everything is local; there is no network call to make.

## Changing the look: drafts, not one answer

The player can't judge a look from a description, so show them choices. For anything about how the HUD or action bars look:

1. `get_settings`, and note the options with `look: true` and the `hud.*` and `actionbars.*` switches. Those are the only keys a draft takes, and none of them act in the game.
2. Make **two or three drafts** with `propose_ui_draft`, each different in one way the player can see at a glance: side by side or stacked, a different health colour, bigger or smaller. Don't send ten small variations, and don't send just one.
3. Give each draft a short `name` the player can pick by ("Stacked, blue"), a `why` that says what it does differently, and the player's own ask in `words`.
4. Tell the player the drafts are on the Your UI tab in Forever Buddy. They see each one drawn on their own screen, standing around, in combat, in stealth and at low health. **Use this** sends one to the game for their next login. **Throw away** drops one.
5. Later, `list_ui_drafts` shows which one they used or threw away. If they threw them all away, ask what was off before making more.

Use `propose_settings` for the rest of the addon, not for the look. The player can turn the HUD and our action bars on or off themselves in game (/fb), so don't make a draft just to flip a switch.

See [game facts](references/game.md) for /reload, restarts, Edit Mode and combat.
