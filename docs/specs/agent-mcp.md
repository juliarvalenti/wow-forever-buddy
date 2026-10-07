# Spec: P2, connecting an AI agent (MCP)

Status: 2026-10-07 (@coder). Built: P2a (the read side), P2b (the queue, login note proposals, Approvals) and P2c (quest plan and list proposals). Still to come: showing list proposals in place on the Lists screen (IMPLEMENTING §15).

**What it's for.** Julia's idea: plan outside the game with an agent ("set up a questing plan for Kaelor tonight"), approve it in Forever Buddy, then see it in game. The agent can read what the app knows about your characters and can **propose** a plan, a note or a shopping list. Nothing it proposes takes effect until you approve it in the app.

**Ground rules** (bridge spec §7, security's ToS pass):

1. **Read-only, plus proposals.** Every tool either reads, or stages a proposal. No tool writes a file, a game setting or the app's data directly.
2. **No automation.** Nothing an agent writes ever becomes an action in the game: no macro, keybind, slash command, chat, secure attribute or click. A plan step is display text, shown through `plain()` like every other slot string.
3. **Off by default.** A Settings switch turns the connection on; while it's off, every tool refuses.
4. **Your characters only.** No other players' words (mail text, guild rosters), and nothing about other players.
5. **No network.** The connection is a local process on stdin/stdout. There's no listening port.

---

## 1. How an agent connects

**The app's own executable, started with `--mcp`.** The agent client starts it as a local MCP server over stdio. Claude Desktop, Claude Code and most MCP clients support this out of the box. Below, "the agent process" means the app started this way.

As built in P2a, this is a flag rather than a separate `externalBin`:

- **Single instance.** `main` checks for `--mcp` before Tauri and `tauri-plugin-single-instance` start. The agent process never becomes a second app instance, and it never focuses the running window.
- **One file to ship and sign.** There's no sidecar to bundle per target triple.
- **No port.** A localhost HTTP server would be a listening socket any local process, or a web page through DNS rebinding, could reach. Stdio is a private pipe to the client that started it.
- **Least privilege, the same as a sidecar would have.** The agent process:
  - opens the app's database read-only;
  - writes only in its own folder, `<local data>/agent/`;
  - has no window, no game paths, no write gate and no Tauri.

It uses a small JSON-RPC loop of its own (`agent/rpc.rs`: `initialize`, `ping`, `tools/list`, `tools/call`) rather than `rmcp`, so there's no async runtime or new dependency to review. It reuses the app's own query code (`characters`, `quests`, `ah`, `adventures`), so answers match the screens.

**Setup, in Settings › Agents:**

- A switch: "Let AI agents read my characters and suggest plans", off by default.
- Under it, the config to paste, with a Copy button:
  ```json
  { "mcpServers": { "forever-buddy": { "command": "C:\\…\\wow-forever-buddy.exe", "args": ["--mcp"] } } }
  ```
  and the Claude Code one-liner `claude mcp add forever-buddy -- "C:\…\wow-forever-buddy.exe" --mcp`.
- "Recent agent activity": the last 20 tool calls (client name, tool, time; never the arguments' text), and the pending proposals count with a link to Approvals.

The activity list is `<local data>/agent/activity.json`, written by the agent process. It holds only the client name, the tool, the time and whether the call worked.

**Turning it off** takes effect on the next tool call. The agent process re-reads `settings.json` on every call and refuses with "Agent access is off in Forever Buddy". There's nothing to revoke beyond that: no token or key exists.

**Threat model.** Anyone who can start the agent process as Julia can already read `buddy.db` directly, so a local secret would add nothing. The real risk is the agent: a model that's wrong, or that a web page or document has prompt-injected. That's why it gets read-only data, can only propose, and every proposal waits for a click.

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
| `get_gold_history` (P3) | optional `character`, `days` (max 90) | Each character's gold at the end of each day, by the Ledger's own rule, with the change from the day before. Only times and amounts are stored, so no counterpart (sender, trade partner, buyer) can leave. |
| `get_price_history` (P3) | `items` (up to 10 ids), `days` (max 90) | Per scanned day: lowest and highest buyout and how many were listed (the AH chart's data). |
| `get_lockouts` (P3) | none | Every character's current lockouts with reset time and as of; reset ones left out. |
| `get_bag_marks` (P3) | `character` | B3's marks (sell, or send to another of the player's characters, with the reason and whether an agent's proposal made it) and the app's open suggestions. |
| `list_proposals` | none | Proposals from the last 30 days: waiting, approved, declined, or not queued with the reason, plus how many files the app hasn't picked up yet. |

**Built in P2a:** the first six tools. `get_quests` returns completed quest ids as a plain number list, up to 10,000, since a planner needs the whole set and ids are tiny. Every other list is capped at 500 rows with `more`. P2b adds `list_proposals`, and `get_character` now lists the character's waiting login notes (id, text, timing), so an agent can propose a replacement.

**What's left out on purpose:**

- **Mail message text and senders.** These are other players' words; only items and gold amounts are given.
- **File paths, settings, API keys and backup contents.**
- **Anything about other players.**

**Text from the game is data.** Item names, zone names and quest titles come from Blizzard's client, and character names are Julia's own. Every result wraps them in plain JSON fields and never in instructions. The tool descriptions say so, to keep a confused model from treating a field as a command.

The same goes for text an agent proposed earlier. An approved note or plan comes back later through `get_recent_play` or `get_quests`, so it's always returned in its own field and never concatenated into a tool description or a prompt (stored injection).

---

## 3. Proposal tools

All are annotated `readOnlyHint: false, destructiveHint: false`. Each one **stages** a proposal and returns at once with its id and "Waiting for your approval in Forever Buddy". Nothing else happens until Julia acts.

| Tool | Arguments | On approval |
|---|---|---|
| `propose_quest_plan` | `character`, `title`, `steps`: up to 50 × { `text` (≤ 200 chars), optional `quest_id`, `zone` } | It becomes that character's quest plan through P1's own `plans::set_plan`, replacing any current one, and the next slot write sends it to the game. P1 has no "proposed" state of its own: the proposal lives in this queue until approved. |
| `propose_note` | `character`, `text` (≤ 300 chars, one line), optional `until` (YYYY-MM-DD; absent means next login only), optional `replaces` { `id`, `text` as read } | It becomes a B1 login note for that character, through `notes::add`, marked `from "<client>", approved`. A replaced note is retired. If that note changed after the agent read it, it's a conflict (below). |

| `propose_list_change` | `list` (one of the player's lists by name, or a new name), optional `for_character` (new lists only), `items`: up to 100 × { `item_id` (one a character has seen), `need` 1 to 9999 } | A new list is made through B2's `lists::create_list`, and each item is added or given its new need through `lists::add_item`, with producer `agent:<client>`. It never removes anything. The preview shows only the items that change, "added" or "changed" with the old need struck through. |

Every proposal tool also takes an optional `reason` (≤ 300 chars). It's shown as `Reason given: "…"` and hidden when empty (IMPLEMENTING §17). The login note replaced the adventure-note target the first draft had, per the design. Built: `propose_note` (P2b), then `propose_quest_plan` and `propose_list_change` (P2c). A plan step's `kind` (`accept`, `turn_in`, `objective`) is optional, but accept and turn-in steps need a `quest_id`.

**What they never do:** edit an addon's SavedVariables, ElvUI or any other game file. Those are §7's `SvEdit` and `Profile` kinds, and they stay out of P2. When they come, they'll use the same queue with the same approval, plus the write gate (WoW closed, safety snapshot).

---

## 4. How a proposal reaches the app

The app stays the only writer of its database. The agent process never writes to `buddy.db`.

1. **The inbox.** The agent process writes the proposal as one JSON file, `<local data>/agent/inbox/<ulid>.json`, via `atomic_replace`. The folder path comes from the app's own data directory, never from the agent. The file is the producer (the MCP client's `clientInfo.name`), the kind, the body and the time.
2. **Ingest.** The app picks up new inbox files on start, on window focus and every 10 seconds while open.

   **While agent access is off, nothing is staged.** The switch is checked here too, not only in the agent process, because any local process can write the inbox. Each file found while off is recorded `rejected: agent access is off` (so the agent can tell why) and deleted unread past its header. Test: switch off, drop a valid file in the inbox, and confirm nothing reaches the Approvals panel.

   For each file, it:
   - parses it with `deny_unknown_fields` against the kind's schema;
   - applies the limits;
   - resolves every `character`, `adventure` and `item_id` against its own data, refusing unknown ones;
   - stores the result in the **staged-changes queue** (the next free migration when P2b rebases, per the room's numbering rule), with status `staged`;
   - deletes the inbox file.

   A file that fails any check is stored as `rejected` with the reason, so `list_proposals` can tell the agent why, and is deleted too.
3. **Approvals.** A new **Approvals** panel lists pending proposals, newest first. Each shows who proposed it, when, and a preview:
   - a plan's steps;
   - a note's new text next to the current one;
   - a list's items with counts and icons.

   It has **Approve** and **Decline** buttons, and Approve all / Decline all for a batch. It reuses F6's stage-and-apply bar. A badge on the sidebar shows the count.

   **What's previewed is exactly what's applied:**
   - Every proposed string (steps, notes, list names, the producer) is rendered as React text, never HTML.
   - No field is applied that the preview doesn't show.
   - Approve applies the stored, validated body that was on screen, not a re-read of anything.
   - The producer is the client's self-reported `clientInfo.name`, so it's labelled as a claim ("from "Claude Desktop""), never shown as a verified identity.
   - Approve all is fine for these display-only kinds. When §7's `SvEdit` and `Profile` kinds arrive, they're approved one by one.
4. **Apply.** Approving runs the kind's normal app code path (the same functions the UI calls), records `applied` with the time, and the change shows up wherever that data lives. Declining records `discarded`. The agent sees either outcome through `list_proposals`.

**The queue (as built in P2b)** is the `proposals` table, with kinds `login_note` now and `quest_plan` and `list` later. Its columns are:
- id;
- flavor;
- the inbox file name (unique, so a file is stored once);
- kind;
- the checked body as JSON;
- the producer (the client's self-reported name);
- reason;
- created_at and received_at;
- status (`staged`, `applied`, `discarded`, `rejected`) with status_reason and decided_at.

A conflict isn't a stored status. It's worked out when Approvals is shown, by comparing the note the proposal replaces with the text the agent read. Approve refuses a conflict, and only "Use proposed" or "Keep mine" (decline) settle it.

**Conflicts:**

- A note proposal carries the note text the agent saw. If the note changed since, Approve shows `conflict` and both versions, and replaces nothing until Julia picks.
- A plan for a character who already has one replaces it only on approval, with the old plan shown alongside.

---

## 5. Limits

These are enforced in the agent process, and again at ingest, since the inbox is a folder any local process could write.

| What | Limit |
|---|---|
| Pending proposals | 50. Past that, proposal tools refuse until some are approved or declined. |
| Inbox file | 64 KB; the agent process won't write past 200 unprocessed files. |
| Text fields | As in §3, UTF-8, no control characters. |
| Read results | 500 rows per call, with a `more` flag; `get_recent_play` up to 30 days; gold and price history up to 90 days, price history 10 items a call. |
| Call rate | 10 calls a second per agent process, as a guard against a looping agent. |

---

## 6. Build order

1. **P2a, read side:** the `--mcp` agent process with the read tools, and the Settings › Agents section (switch, config snippet, activity). Tests:
   - each tool against a fixture db;
   - "off refuses everything";
   - the db is opened read-only (a write attempt fails);
   - mail text never appears in any result.
2. **P2b, proposals:** the inbox, ingest with validation, the queue's migration, and the Approvals panel, with **notes** first since they're the simplest and already exist in the app. Tests:
   - malformed, oversized and unknown-target files are rejected with a reason;
   - with agent access off, a valid inbox file never reaches Approvals;
   - the preview shows every applied field, as text;
   - nothing is applied without Approve;
   - the conflict path.
3. **P2c:** `propose_quest_plan` and `propose_list_change`. Each is a schema plus its apply function on the same queue (`proposals/plan.rs`, `proposals/list.rs`).

---

## 7. Open questions

- **Which clients to document first?** Claude Desktop and Claude Code are assumed. Others work the same over stdio.
- **Does a plan go to the game on approval, or only when Julia also presses "Send to game"?** Recommended: on approval, since approving is already the explicit step, and the slot is display-only.
- **Name in the agent's tool list:** `forever-buddy`. It's kept generic until the rename decision (backlog: away from "WoW").

Sources: MCP specification (stdio transport, tool annotations `readOnlyHint`/`destructiveHint`), modelcontextprotocol.io; the official Rust SDK `rmcp`; Tauri sidecars (`bundle.externalBin`); this repo's bridge-v0.4.md §7 and security's ToS pass (2026-10-06).
