# The addon bible: building WoW: Forever addons

How to build an addon for WoW: Forever (Interface 16001, the 12.x API with secret values) that is fast, safe in combat, kind to saved data and easy to ship. It's written for ForeverBuddy's addon and for the people and agents who work on it.

**Where it comes from.** A study of EllesmereUI, a 25-addon UI suite that ships on Forever and retail several times a week. The study was done at commit `06d91a8c`, 16 commits after v9.7.1, on 2026-10-30. It covers EllesmereUI's code, comments, CI, and 6,640 commits of history.

**How it was checked.** Seven research agents each read one area and wrote numbered findings, each tied to a file and line or a commit. A separate checker agent then re-opened every citation and tried to break each claim:

- About 280 findings were checked.
- None were rejected, but about one in four was corrected: a wrong line, a count that didn't hold, or a "never" or "always" the code doesn't keep.
- Every rule here uses the corrected version.

Two lessons came out of the checking itself:

- **Comments are claims, not facts.** Notes written from file-header comments needed the most corrections. Several of EllesmereUI's comments disagree with its own code, or with each other: on what secret values allow, on how saved tables are written, and on reloading. Where they conflict, this bible says so and gives the safe rule.
- **A big suite breaks its own rules.** Most corrections to "never" rules came from EllesmereUI itself doing the thing elsewhere, deliberately. A UI replacement suite takes risks a small addon shouldn't. Rules here are stated at the strength the evidence supports, with a conservative default for us.

**How to read a rule.** Each rule is one imperative sentence and a short why. Then comes a `Source:` line pointing into EllesmereUI at that commit, and, where it changes something, a `For ForeverBuddy:` line. "EllesmereUI does X" is a fact from their code. "Do X" is our advice. "(our inference)" marks reasoning the source doesn't state. Code sketches are our own words.

## The rules that matter most

If you read nothing else:

1. **Secret values:** test `issecretvalue` first. Never compare, branch on, `or`-default, format or join a secret in Lua; hand it to an engine API that accepts it, or show a placeholder (2.1 to 2.4).
2. **Globals Forever lacks:** never call them (bare `GetItemInfo`, the old spec functions and others). Use the `C_` forms (2.17, 2.18).
3. **Off costs nothing:** a feature that's off registers no events, runs no hooks or timers and builds no frames. New options default off (1.19, 4.1).
4. **Events, not polling:** batch bursts into one update a frame, and make throttles apply the last change too (4.5, 4.8, 4.9).
5. **Never act on Blizzard's protected UI in combat:** defer with one keyed queue that re-reads live state when combat ends (3.3, 3.4).
6. **Hook outward, never inward:** hook global functions with `hooksecurefunc`, keep the work cosmetic, write no fields onto Blizzard frames, and guard with `IsForbidden` (3.10 to 3.12, 3.18, 3.19, 8.7).
7. **Fail closed:** anything that sells, deletes or changes what you don't own is opt-in, refuses data it can't read, and asks harder the more it costs (8.1, 8.2).
8. **Undo only what is still yours:** record the before-value, and restore only values that still hold yours (8.13).
9. **Saved data:**
   - Read it at your own `ADDON_LOADED`, and act on game data at `PLAYER_LOGIN`.
   - Keep settings and records apart.
   - Migrate with stable ids, marked done only on success.
   - Validate anything imported, our own app's slots included (2.24, chapter 6).
10. **Lua 5.1's limits are hard:** 200 locals and 60 upvalues. One overflow and the file doesn't load (1.21, 1.22).
11. **Reload only from a click or keypress, first thing in the handler** (5.22; one comment's evidence, confirm in game).
12. **Make every correction a rule or a check,** and read the final merged diff of agent work, not the first one (7.13, 7.17).

## Contents

1. [Structure and loading](01-structure.md)
2. [The Forever API and secret values](02-forever-api.md)
3. [Taint, combat and Blizzard's frames](03-taint-combat.md)
4. [Performance](04-performance.md)
5. [Building the UI](05-ui.md)
6. [Saved data, settings and profiles](06-data.md)
7. [Testing, release and process](07-process.md)
8. [Designing features](08-features.md)

Appendix: [gaps in ForeverBuddy's addon today](09-gaps.md).

**Keeping it current.** EllesmereUI moves fast (about 300 commits a week), so source line numbers drift. Cite the commit when you add a rule. When a review corrects a mistake, add it as a rule here or as a check in the harness (7.13).
