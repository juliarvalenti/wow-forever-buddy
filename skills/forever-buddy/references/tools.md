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
| `get_settings` | Every addon option: id, label, help, type, default, current value, whether it acts in the game, a choice's `values`, and `look: true` on the options that set the HUD's and bars' look. |
| `list_ui_drafts` | Your UI drafts: waiting, used, thrown away, or not queued (with the reason), each with its settings. |
| `list_macros` | The account-wide macros and each character's own: name, icon, length against the game's 255. Takes an optional `character`. |
| `get_macro` | A macro's full text by name: a character's own, then its account's; without a character, the account's. |
| `render_ui` | A picture of the player's screen with a UI document (`doc`) drawn on it, in a `moment` (`standing`, `combat`, `stealth`, `low_health`). A document with problems draws nothing and returns each problem with its path. With no document, round 4 and its JSON to start from. See [ui](ui.md). |

## Propose (the player approves in the app)

`propose_note`, `propose_quest_plan`, `propose_list_change`, `propose_bag_marks`, `propose_goal`, `propose_settings`, and `list_proposals` to see what's pending.

For `propose_settings`: give a short reason per change. Options marked as acting in the game are never proposed in bulk, and the player approves each one separately. Settings reach the game at the next /reload or login.

## Drafts (the player picks one in the app)

`propose_ui_draft` takes a `name` (a few words), a `why` (one sentence), the player's `words`, and `settings`: option id to value, e.g. `{"hud.enabled": true, "hud.layout": "stacked", "hud.healthColor": "#5fb6e8"}`.

- Only `hud.*` and `actionbars.*` options that don't act are allowed. Anything else is refused with the reason.
- A colour is `"#rrggbb"`, or `""` for the look's own.
- A draft isn't an Approvals row. It waits on the Your UI tab, drawn on the player's own screen, until they pick **Use this** (it goes to the game at their next login) or **Throw away**.
- Up to 10 wait at once. Send two or three that differ clearly; see [SKILL](../SKILL.md#changing-the-look-drafts-not-one-answer).
