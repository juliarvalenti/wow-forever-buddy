# Spec: P2, connecting an AI agent (MCP)

Status: draft, 2026-10-07 (@coder). Spec only; nothing here is built yet.

**What it's for.** Julia's idea: plan outside the game with an agent ("set up a questing plan for Kaelor tonight"), approve it in Forever Buddy, then see it in game. The agent can read what the app knows about your characters and can **propose** a plan, a note or a shopping list. Nothing it proposes takes effect until you approve it in the app.

**Ground rules** (bridge spec §7, security's ToS pass):

1. **Read-only, plus proposals.** Every tool either reads, or stages a proposal. No tool writes a file, a game setting or the app's data directly.
2. **No automation.** Nothing an agent writes ever becomes an action in the game: no macro, keybind, slash command, chat, secure attribute or click. A plan step is display text, shown through `plain()` like every other slot string.
3. **Off by default.** A Settings switch turns the connection on; while it's off, every tool refuses.
4. **Your characters only.** No other players' words (mail text, guild rosters), and nothing about other players.
5. **No network.** The connection is a local process on stdin/stdout. There's no listening port.

---

## 1. How an agent connects

**A sidecar binary, `forever-buddy-mcp`,** ships with the app (a Tauri `externalBin`). The agent client starts it as a local MCP server over stdio. Claude Desktop, Claude Code and most MCP clients support this out of the box.

Why a separate binary, not the app itself:

- **Single instance.** The app runs `tauri-plugin-single-instance`: a second launch only focuses the running window and exits, so `wow-forever-buddy.exe --mcp` can't serve a session.
- **No port.** A localhost HTTP server would be a listening socket any local process, or a web page through DNS rebinding, could reach. Stdio is a private pipe to the client that started it.
- **Least privilege.** The sidecar opens the app's database read-only and can write in exactly one folder (§4). It has no game paths, no write gate and no Tauri.

It's built with `rmcp` (the official Rust MCP SDK) and reuses the app's own query code (`characters`, `quests`, `ah`, `ledger`) from the library crate, so answers match the screens.

**Setup, in Settings › Agents:**

- A switch: "Let AI agents read my characters and suggest plans", off by default.
- Under it, the config to paste, with a Copy button:
  ```json
  { "mcpServers": { "forever-buddy": { "command": "C:\\…\\forever-buddy-mcp.exe" } } }
  ```
  and the Claude Code one-liner `claude mcp add forever-buddy -- "C:\…\forever-buddy-mcp.exe"`.
- "Recent agent activity": the last 20 tool calls (client name, tool, time; never the arguments' text), and the pending proposals count with a link to Approvals.

**Turning it off** takes effect on the next tool call. The sidecar re-reads `settings.json` on every call and refuses with "Agent access is off in Forever Buddy". There's nothing to revoke beyond that: no token or key exists.

**Threat model.** Anyone who can start the sidecar as Julia can already read `buddy.db` directly, so a local secret would add nothing. The real risk is the agent: a model that's wrong, or that a web page or document has prompt-injected. That's why it gets read-only data, can only propose, and every proposal waits for a click.

---

## 2. Read tools

All are annotated `readOnlyHint: true`. Each answers from the app's database opened with `SQLITE_OPEN_READ_ONLY` and `PRAGMA query_only = ON` (the app runs WAL, so reading alongside it is safe). Results are capped (see "Limits") and say how old they are ("as of last logout").

| Tool | Arguments | Returns |
|---|---|---|
| `list_characters` | none | Each character: a stable `character` key, name, class, race, level, zone, gold, item level, last seen. |
| `get_character` | `character` | Gear by slot (item, ilvl, quality), bags and bank summary (free slots, as of), professions, lockouts, rested state. |
| `find_items` | `query` (text, `ilvl>60` style filters as in the app's search) | Where each match is: character, bags/bank/mail, count, as of. |
| `get_quests` | `character`, optional `since` | Completed quest ids (Q1b's `char_quests_done`), plus recent accepts and turn-ins with titles and zones from our own events. |
| `get_prices` | `items` (ids) | Last scan price, 30-day median, scan age, from Auctionator data. |
| `get_recent_play` | optional `character`, `days` (max 30) | Sessions: time played, zones, gold change, levels, notable loot (the Adventures data). |
| `list_proposals` | optional `status` | The agent's own proposals and whether each was approved, declined or is still pending. |

**What's left out on purpose:**

- **Mail message text and senders.** These are other players' words; only items and gold amounts are given.
- **File paths, settings, API keys and backup contents.**
- **Anything about other players.**

**Text from the game is data.** Item names, zone names and quest titles come from Blizzard's client, and character names are Julia's own. Every result wraps them in plain JSON fields and never in instructions. The tool descriptions say so, to keep a confused model from treating a field as a command.

---

## 3. Proposal tools

All are annotated `readOnlyHint: false, destructiveHint: false`. Each one **stages** a proposal and returns at once with its id and "Waiting for your approval in Forever Buddy". Nothing else happens until Julia acts.

| Tool | Arguments | On approval |
|---|---|---|
| `propose_quest_plan` | `character`, `title`, `steps`: up to 50 × { `text` (≤ 200 chars), optional `quest_id`, `zone` } | It becomes that character's quest plan (P1's table), and the next slot write sends it to the game as a checklist the player ticks off by hand. |
| `propose_note` | `target`: { `adventure` id } or { `character` }, `text` (≤ 2,000 chars) | Sets that note, as the Adventure screen's note field does. An existing note is shown side by side and replaced only on approval. |
| `propose_shopping_list` | `name`, `items`: up to 100 × { `item_id`, `count` } | It becomes a shopping list (B2), tracked across alts and shown on tooltips (TIP2 (c)). |

**What they never do:** edit an addon's SavedVariables, ElvUI or any other game file. Those are §7's `SvEdit` and `Profile` kinds, and they stay out of P2. When they come, they'll use the same queue with the same approval, plus the write gate (WoW closed, safety snapshot).

---

## 4. How a proposal reaches the app

The app stays the only writer of its database. The sidecar never writes to `buddy.db`.

1. **The inbox.** The sidecar writes the proposal as one JSON file, `<local data>/agent-inbox/<ulid>.json`, via `atomic_replace`. The folder path comes from the app's own data directory, never from the agent. The file is the producer (the MCP client's `clientInfo.name`), the kind, the body and the time.
2. **Ingest.** The app picks up new inbox files on start, on window focus and every 10 seconds while open. For each file it:
   - parses it with `deny_unknown_fields` against the kind's schema;
   - applies the limits;
   - resolves every `character`, `adventure` and `item_id` against its own data, refusing unknown ones;
   - stores the result in the **staged-changes queue** (migration **010**; bridge §7 named 009, which Q1b's quests table took), with status `staged`;
   - deletes the inbox file.

   A file that fails any check is stored as `rejected` with the reason, so `list_proposals` can tell the agent why, and is deleted too.
3. **Approvals.** A new **Approvals** panel lists pending proposals, newest first. Each shows who proposed it, when, and a preview:
   - a plan's steps;
   - a note's new text next to the current one;
   - a list's items with counts and icons.

   It has **Approve** and **Decline** buttons, and Approve all / Decline all for a batch. It reuses F6's stage-and-apply bar. A badge on the sidebar shows the count.
4. **Apply.** Approving runs the kind's normal app code path (the same functions the UI calls), records `applied` with the time, and the change shows up wherever that data lives. Declining records `discarded`. The agent sees either outcome through `list_proposals`.

**The queue is the one from bridge §7**, with three new kinds: `QuestPlan`, `Note`, `ShoppingList`. The columns stay as specced there: id, kind, JSON body, producer (`app` or `agent:<client name>`), created_at, status (`staged`, `applied`, `discarded`, `conflict`), plus `rejected` and its reason.

**Conflicts:**

- A note proposal carries the note text the agent saw. If the note changed since, Approve shows `conflict` and both versions, and replaces nothing until Julia picks.
- A plan for a character who already has one replaces it only on approval, with the old plan shown alongside.

---

## 5. Limits

These are enforced in the sidecar, and again at ingest, since the inbox is a folder any local process could write.

| What | Limit |
|---|---|
| Pending proposals | 50. Past that, proposal tools refuse until some are approved or declined. |
| Inbox file | 64 KB; the sidecar won't write past 200 unprocessed files. |
| Text fields | As in §3, UTF-8, no control characters. |
| Read results | 500 rows per call, with a `more` flag; `get_recent_play` up to 30 days. |
| Call rate | 10 calls a second per sidecar, as a guard against a looping agent. |

---

## 6. Build order

1. **P2a, read side:** the sidecar with the read tools, the Settings › Agents section (switch, config snippet, activity), and bundling it as `externalBin`. Tests:
   - each tool against a fixture db;
   - "off refuses everything";
   - the db is opened read-only (a write attempt fails);
   - mail text never appears in any result.
2. **P2b, proposals:** the inbox, ingest with validation, migration 010, and the Approvals panel, with **notes** first since they're the simplest and already exist in the app. Tests:
   - malformed, oversized and unknown-target files are rejected with a reason;
   - nothing is applied without Approve;
   - the conflict path.
3. **P2c:** `propose_quest_plan`, when P1's plan table and slot land (@coder2), and `propose_shopping_list`, when B2's lists land. Each is a schema plus its apply function on the same queue.

---

## 7. Open questions

- **Which clients to document first?** Claude Desktop and Claude Code are assumed. Others work the same over stdio.
- **Does a plan go to the game on approval, or only when Julia also presses "Send to game"?** Recommended: on approval, since approving is already the explicit step, and the slot is display-only.
- **Name in the agent's tool list:** `forever-buddy`. It's kept generic until the rename decision (backlog: away from "WoW").

Sources: MCP specification (stdio transport, tool annotations `readOnlyHint`/`destructiveHint`), modelcontextprotocol.io; the official Rust SDK `rmcp`; Tauri sidecars (`bundle.externalBin`); this repo's bridge-v0.4.md §7 and security's ToS pass (2026-10-06).
