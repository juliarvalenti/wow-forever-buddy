# Spec: P4, Claude configures your UI

Status: draft, 2026-10-31 (@coder). For @project-mgmt, @security-sanity-reviewer and @designer, then Julia.

**What it's for.** You tell Claude what you want ("minimal UI, bars fade out of combat, auto-repair, sell greys"). Claude proposes settings in plain words, you approve each change in Forever Buddy, and the game picks them up. Julia's direction (room, 2026-10-31): ForeverBuddy itself becomes the "everything addon", modules like EnhanceQoL and EllesmereUI, and its settings panel is Claude. Configuring other people's addons is the same machinery pointed at files we don't own, so it comes later (§7).

**Why our own addon first.** We own every option, so each one is declared with its type, range, default and meaning. Claude can't guess a key wrong, the app can check every value, and the harness can run every value. For another addon, we'd be inferring what a key means from code we don't control.

**Ground rules**, carried over from P2 (`agent-mcp.md`):

1. Claude only **proposes**. Nothing changes until you approve it in the app, and settings changes are approved one by one.
2. Claude never makes the game **act**. It sets options you approve. What an option does in game is the addon's normal behaviour, the same as if you'd ticked the box yourself.
3. **Your characters only**, no network, off by default (the existing Agents switch).

---

## 1. Validation: how anyone knows a proposal is good

Julia's question: how can Claude and a person both trust that the settings make a good game, not just a valid file? Four layers, from cheapest to hardest. The first three are mechanical and block a bad proposal. The fourth is judgement, so it's made safe to try.

| Layer | Question | How | When |
|---|---|---|---|
| 1. Valid | Is every value one the addon accepts? | The settings schema (§2): types, ranges, enums, dependencies. The app refuses anything else at ingest, as P2 does for plans. | Every proposal |
| 2. Safe | Can any accepted profile break the addon? | The harness runs every declared value, and chaos seeds randomize the whole profile (§5). The handler budgets (H6) cover performance. | CI, every addon PR |
| 3. Looks right | Does the UI look as intended? | Presets and module states are drawn by the G1 pictures in CI, so a reviewer sees each one before it ships. | CI, every addon PR |
| 4. Plays well | Is it a good experience for *you*? | Curated presets as starting points, a reason on every change, lints for known bad combinations, and "try it, keep it or undo it" (§4). | Every proposal |

**Layer 2 makes any profile safe, not each one checked.** Every combination the schema allows is in the space the harness samples, so a profile Claude invents is one the harness has effectively run. The schema is the boundary. That's why options must be declared, never free-form.

**Layer 4 can't be automated.** No test knows that you hate fading bars. So the design keeps the cost of a wrong guess at one click:

- **Presets.** The designer makes a few named presets ("Minimal", "Classic", "Questing"), reviewed through the G1 pictures. Claude proposes a preset plus a few changes with reasons, rather than inventing 60 values.
- **Lints.** Known bad combinations get a warning in the preview, written in the schema next to the options: "Action bars hidden in combat with no keybind fallback", "Tooltip anchored off screen at this scale". Claude sees the same lints and should avoid them.
- **Try it.** Applying a profile keeps a snapshot. On the next login the app asks once: "Keep the new UI or go back?", and going back is one click (§4).

**What we can't check:** whether the UI looks right on your real screen (resolution, UI scale, other addons). The pictures are drawn at 1920×1080 in the emulator, without your other addons. A future "send me a screenshot" flow could close that gap, but it's out of scope here.

---

## 2. The settings schema

One declaration per option, written once in the addon and used everywhere:

```lua
-- In the addon (ForeverBuddy/Settings.lua), the single source of truth
M.option("bars.fadeOutOfCombat", {
    module = "bars",
    type = "bool", default = false,
    label = "Fade action bars out of combat",
    help = "Bars fade to 20% when you're not fighting, and come back on mouseover.",
})
M.option("bars.fadeAlpha", {
    module = "bars", type = "number", min = 0, max = 1, step = 0.05, default = 0.2,
    requires = "bars.fadeOutOfCombat",
    label = "Faded opacity",
})
M.lint("bars.hiddenNoFallback", { when = { ["bars.fadeAlpha"] = 0 },
    say = "Bars disappear completely out of combat." })
```

- **Types:** `bool`, `number` (min, max, step), `enum` (named values), `color`, `string` (max length, display text only, never a path, macro or command).
- **`requires`** hides and ignores an option while its parent is off, the same convention as U1d's in-game Settings page.
- **Generated, never hand-copied.** The harness runs the addon, reads the registry and writes `settings-schema.json` (like H5's bridge fixtures). `--check` fails CI if it's stale. The app bundles that file, so the app, the MCP tools, the in-game Settings page and the harness all read the same declaration.
- **Versioned.** The schema has a `version`. A profile made for an older schema is migrated with stable-id migrations (from R3: EllesmereUI's `RegisterMigration`), or refused with a reason.

---

## 3. Modules

Each module declares its options and costs nothing while off: no events, no frames, no hooks. That's EllesmereUI's and EnhanceQoL's rule, and our handler budgets enforce it.

Ranked by value for Forever and by how safe each is to build. The list is for @project-mgmt and Julia to cut.

| Rank | Module | What it does | Notes |
|---|---|---|---|
| 1 | **Tooltips** | What we already have (alt counts, prices, bind lines) with its options declared | No new behaviour, so it proves the pipeline end to end |
| 2 | **Window and briefing** | Today's settings: login briefing, coach, session card, bag marks, minimap button | Already built; this is declaring them |
| 3 | **Merchant** | Auto-repair (own gold, or guild first), sell greys, sell marked items | **Acts** (rule change, §8). Runs only on `MERCHANT_SHOW`, which you opened yourself. Never buys. |
| 4 | **Bags** | Sort and marks display, junk highlight | Display only, except sorting (acts, on a click) |
| 5 | **Quest** | Tracker look, auto-accept or turn-in off by default | Auto-accept acts on a dialog you opened |
| 6 | **Bars and frames** | Fade out of combat, scale, hide art | **Secure frames**: positions and scale only out of combat (R3: their combat queue); never protected calls in combat |
| 7 | **Map** | Coordinates, reveal, fade while moving | Display only |
| 8 | **Chat** | Timestamps, class colours, copy | EllesmereUI won't take PRs for chat: it's a taint risk. Last, if at all. |

The biggest risk is in rank 6: anything that skins or moves Blizzard's secure frames can taint. R3a's guardrails (luac, deprecated globals) are the start, and every module that touches secure frames follows EllesmereUI's rules: hook, alpha only, nothing in combat.

---

## 4. The flow

1. **Read.** A new MCP tool `get_settings` returns the schema (options, labels, help, lints, presets) and the current values. Values come from `ForeverBuddySettings` (account) and `ForeverBuddyDB` (per character), which the app already reads at logout.
2. **Propose.** `propose_settings` takes a list of `{ option, value, reason }` (up to 50), or `{ preset, changes }`. The agent process checks it against the schema before writing the inbox file, and ingest checks again (P2's rule: the inbox is a folder any local process can write).
3. **Approve.** In Approvals, each change is one row in plain language, from the schema's labels: "Fade action bars out of combat: off → on. *Reason given: you said minimal.*" Lints show beside the change they apply to. You approve each row, or "Approve all" for a whole preset you picked. The bulk approve is allowed because every value has passed layers 1 and 2.
4. **Deliver.** Approved settings go to the game as a new data slot, `Data/Settings.lua`. It isn't an edit of the addon's SavedVariables. The slot holds a stamped profile. At login, the addon applies it once if the stamp is newer than the last one it applied, then records the stamp. After that, in-game toggles work as usual and win until the next approved profile.
   - **Why a slot and not an SvEdit:** slots are the app's own files, already checked, capped and written atomically (bridge §2). Editing `ForeverBuddySettings` in place would need the write gate and could race the game's own save on logout.
   - **When:** slots are written while WoW is closed (bridge §3). Once B4 lands, a `/reload` picks them up while WoW is running.
5. **Keep or undo.** Each applied profile is kept in the app's database with the one before it. "Undo" writes the previous profile as a new stamped slot. At the next login after a change, the briefing line says "New settings from Forever Buddy" and the app asks "Keep or go back?" once.

---

## 5. Harness and CI

- **Every option, every value.** A `settings_matrix` scenario logs in once per option with that option at each enum value, its min, max and default, and with its `requires` parent on. It runs the usual after-each checks: no escaped, caught or unmodelled errors, and handler budgets.
- **Random profiles.** The H4 chaos seeds also randomize the whole profile from the schema. A failing seed names the profile, so it can be replayed.
- **Presets drawn.** The G1 workflow draws the window and every module's own frames under each preset, and posts the pictures on the PR.
- **Schema check.** `settings-schema.json` is regenerated and diffed under `--check`. A new option without a label, help text or default fails.
- **Lints tested.** Each lint has a scenario where it fires and one where it doesn't.

---

## 6. The MCP tools

| Tool | Arguments | Returns or does |
|---|---|---|
| `get_settings` | optional `module` | The schema slice, current values, presets, lints. Read only. |
| `propose_settings` | `changes`: up to 50 × { `option`, `value`, `reason` ≤ 300 }, or `preset` plus `changes` | Stages one proposal. Unknown options, out-of-range values and options whose `requires` is off are refused with the reason. |

Text in labels, help and lints is ours and fixed, so Claude can't inject words into the preview: the preview shows our label, the value and Claude's reason in its own quoted field (P2's rule on producer text).

---

## 7. Later: other addons' settings (the original P4)

The same pipeline, using bridge §7's `SvEdit` and `Profile` change kinds, which are already reserved in the queue's types. Differences:

- **No schema of our own.** For each addon we support, we write a reviewed **settings map**: a curated key list with types, ranges and labels, for v1's target list. Claude may read an addon's options code to suggest a map entry, but only keys in a reviewed map can be proposed. Inferring meanings at run time is how a wrong guess corrupts someone's UI.
- **Gated writes.** The write gate, as F6's AddOns.txt toggle does: WoW closed, a safety snapshot, an atomic write, a round-trip parse check, and undo from the snapshot. Edits are approved one by one, with no bulk approve.
- **Other players' data stripped on read.** Addon SavedVariables often hold guild rosters, chat history and other players' names. `get_addon_settings` returns only the keys in the map.
- **Profile strings** (EllesmereUI's `!EUI_…`, ElvUI's): where an addon has an import string, Claude can produce one for you to paste in game. That's the safest route of all, since the addon validates its own import. Decoding them needs each addon's codec (EllesmereUI: LibDeflate over its own serializer, R3).
- **Layers 2 and 3 are weaker.** We can't run other addons in the harness without their code, so a map entry is only as good as its review.

v1's target list is Julia's real addon list. @project-mgmt asked her for it (EllesmereUI, EnhanceQoL, Baganator, Auctionator, Details, Questie?).

---

## 8. Decisions needed

1. **The addon starts acting (Julia, security).** Today ForeverBuddy never acts. Merchant (auto-repair, sell greys) and Quest (auto-accept) would act on frames you opened, as every bag and merchant addon does. Claude never triggers anything; it only sets options. The ToS line stays: no automation of play, no reading game memory. Does that hold with these modules on?
2. **Module list and order** (§3): which ones, and is rank 6 (secure frames) in scope at all?
3. **Presets** (@designer): which starting presets, and who signs off on how each looks.
4. **One addon or several?** EllesmereUI splits into child addons so an unused module isn't even loaded. Recommended: one addon with modules that cost nothing while off. It's simpler to ship, and the handler budgets and "no events while off" test keep it honest. Revisit if the file nears the 200-locals limit (R3a's luac check will say).

## 9. Build order

1. **P4a:** the schema registry in the addon, `settings-schema.json` generation and check, today's settings declared (Tooltips and Window, ranks 1 and 2), and the `settings_matrix` scenario. No new behaviour.
2. **P4b:** the `Settings` slot with stamp and apply-once, `get_settings`, `propose_settings`, the Approvals rows, keep or undo.
3. **P4c onward:** one module per PR, in the agreed order, each with its options, lints, harness scenarios and pictures.
4. **Later:** §7, other addons' settings, per reviewed map.

Sources: this repo's `agent-mcp.md` (P2), `bridge-v0.4.md` §2, §3 and §7, the R3 EllesmereUI teardown (room, 2026-10-30), and the room thread where Julia set the direction (2026-10-31).
