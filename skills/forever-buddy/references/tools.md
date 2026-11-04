# Forever Buddy MCP tools

All tools read the app's local database, read-only; the macro tools read the game's own macro files, also read-only. Data is as of each character's last logout.

## Read

| Tool | Returns |
|---|---|
| `list_characters` | Every character: name, class, race, level, zone, gold (copper), item level, rested XP, last seen. Start here. |
| `get_character` | One character in full: gear, bags, bank, mail, professions, lockouts, login notes. |
| `find_items` | Where an item is across all characters. Takes filters like `ilvl>60`. |
| `get_quests` | Completed quest ids and recent accepts and turn-ins. |
| `get_recent_play` | Recent sessions: zones, levels, gold change, loot, turn-ins, the player's note. |
| `get_gold_history` | Gold per character per day. |
| `get_lockouts` | Current raid and dungeon lockouts with reset times. |
| `get_prices`, `get_price_history` | Auction prices from the player's own Auctionator scans. |
| `get_bag_marks` | Items marked to sell or send, and suggestions. |
| `get_settings` | Every addon option: id, label, help, type, default, current value, whether it acts in the game, a choice's `values`, and `look: true` on the options that set the HUD's and bars' look. `yields_to` names other addons that already do an option's job, `off_because` says one is loaded (ours stays off), and `other_addons` lists those the game had loaded. |
| `list_ui_drafts` | Your UI drafts: waiting, used, thrown away, or not queued (with the reason), each with its settings. |
| `list_macros` | The account-wide macros and each character's own: name, icon, length against the game's 255. Takes an optional `character`. |
| `get_macro` | A macro's full text by name: a character's own, then its account's; without a character, the account's. |
| `render_ui` | Pictures of a stand-in screen (a level 34 test character with a hostile target; none of the player's data) with a UI document (`doc`) drawn on it: one per moment in `moments` (`standing`, `combat`, `stealth`, `low_health`, or `["all"]`), for a test `class` (rogue by default), at a `scale` from 0.25 to 1. A misspelt moment or an unknown class is refused with the right names. A document with problems draws nothing and returns each problem with its path. With no document, round 4 and its JSON to start from. See [ui](ui.md). |
| `get_ui_spec` | The UI language's whole reference, as this version reads it. Read it before your first document. |
| `get_ui` | The player's UI documents: the one in use (with its `doc`), the ones proposed in the last 30 days and what became of each, and any the app hasn't picked up yet. Revise one instead of starting over. |

## Propose (the player approves in the app)

`propose_note`, `propose_quest_plan`, `propose_list_change`, `propose_bag_marks`, `propose_goal`, `propose_settings`, and `list_proposals` to see what's pending (`waiting_for_the_app` names the ones the app hasn't picked up yet).

`propose_macro` suggests one macro: `new`, `edit` (with `was`, the text as `get_macro` gave it) or `copy` (with `from`, and `body` only to adapt it). The player approves each macro on its own, then applies it in game with one click, out of combat; `list_proposals` shows what the game did with it (`in_game`: waiting, applied, declined, changed, full). It's checked before it's queued: 255 bytes, 16-character names, the 120 account and 18 character slots, and no `/run`, `/script`, `/console` or `/dump` lines.

`propose_ui` sends a whole UI document (`doc`, as `render_ui` takes it) with a `name`, a `why` and the player's `words`. It's checked exactly as `render_ui` checks it; a document with problems isn't queued, and you get each problem with its path. The player sees it drawn in Approvals and picks it with Use this: it's live in the game at their next login, for every character, until they go back to the standard HUD. `list_proposals` shows it with `in_use`.

For `propose_settings`: give a short reason per change. Options marked as acting in the game are never proposed in bulk, and the player approves each one separately. Settings reach the game at the next /reload or login.

## Drafts (the player picks one in the app)

`propose_ui_draft` takes a `name` (a few words), a `why` (one sentence), the player's `words`, and `settings`: option id to value, e.g. `{"hud.enabled": true, "hud.layout": "stacked", "hud.healthColor": "#5fb6e8"}`.

- Only `hud.*` and `actionbars.*` options that don't act are allowed. Anything else is refused with the reason.
- A colour is `"#rrggbb"`, or `""` for the look's own.
- A draft isn't an Approvals row. It waits on the Your UI tab, drawn on the player's own screen, until they pick **Use this** (it goes to the game at their next login) or **Throw away**.
- Up to 10 wait at once. Send two or three that differ clearly; see [SKILL](../SKILL.md#changing-the-look-drafts-not-one-answer).
