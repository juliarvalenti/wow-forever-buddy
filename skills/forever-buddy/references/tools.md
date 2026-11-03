# Forever Buddy MCP tools

All tools read the app's local database, read-only. Data is as of each character's last logout.

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
| `get_settings` | Every addon option: id, label, help, type, default, current value, whether it acts in the game. |

## Propose (the player approves in the app)

`propose_note`, `propose_quest_plan`, `propose_list_change`, `propose_bag_marks`, `propose_goal`, `propose_settings`, and `list_proposals` to see what's pending.

For `propose_settings`: give a short reason per change. Options marked as acting in the game are never proposed in bulk, and the player approves each one separately. Settings reach the game at the next /reload or login.
