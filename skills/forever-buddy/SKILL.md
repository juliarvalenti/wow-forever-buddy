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
| Their addon set up a certain way | `get_settings`, then `propose_settings` with a reason for each change |
| Their bags cleaned up | `get_bag_marks`, then `propose_bag_marks` |
| Their GUI redesigned | Read [design](references/design.md) first; the look follows its rules |

## Rules that never bend

- **Propose, never act.** Nothing changes in the game until the player approves it in the app. Never ask the player to bypass an approval.
- **No automation.** Nothing plays the game: no botting, no reading game memory, no input automation. Modules that act in the game (Merchant, Quest helper) are off by default, only act on windows the player opened, and are never turned on in bulk.
- **Display only about other players.** Never store or propose anything about characters the player doesn't own.
- **The player's characters only.** Everything is local; there is no network call to make.

See [game facts](references/game.md) for /reload, restarts, Edit Mode and combat.
