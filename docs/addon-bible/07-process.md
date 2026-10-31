## 7. Testing, release and process

This chapter covers how EllesmereUI keeps mistakes off main: one static checker, a few hand-run Lua tests, a rule table for AI agents, and a tag-driven release. ForeverBuddy already tests more deeply than they do (a fake client in CI, chaos seeds, coverage, handler budgets, layout snapshots, rendered pictures), so most of what we can borrow is on the static-check and review side. The one idea to remember: when review catches the same mistake twice, turn it into a check that runs on every change.

### Static checks

**7.1 Put every mechanical check behind one local command, and make CI run exactly that command.**
EllesmereUI keeps all checks in one stdlib-only Python script that contributors run before committing and CI runs with an explicit base. The workflow says why: "Mechanical checks for mistakes that kept landing on main. Same command as local."
Source: `AGENTS.md:5-9`, `.github/workflows/guardrails.yml:3-4,32-41`, `.tools/guardrails.py:20-29`.
For ForeverBuddy: our equivalent is `lua5.1 tools/addon-test/run.lua --check`, which `addon.yml` already runs. Any static check we add should hang off that same entry point (or one sibling script), not a second CI-only path.

**7.2 Judge only new and changed lines since the merge-base, include untracked files, and never touch the developer's real index.**
Except for compile and line-1 checks, their checker looks only at lines changed since the merge-base: "older code is reported only once you edit it." That is what let it land on a large legacy codebase without a cleanup PR first. Working-tree mode diffs against a throwaway copy of the index, with untracked Lua added as intent-to-add, so a new unstaged file is checked and the user's staging is left alone.
Source: `.tools/guardrails.py:6-8,220-223,239-256`, `AGENTS.md:11`, `.tools/tests/test_guardrails.py:284-292`.
For ForeverBuddy: `ForeverBuddy.lua` is about 4,700 lines of existing code, so any new linter for it should be diff-only from the start.

**7.3 Compile every changed Lua file under a real Lua 5.1, and refuse any other version.**
In WoW, one syntax error or one overflow of the 200-local or 60-upvalue limit kills the whole file. Their commit body lists the incidents: "Syntax errors and 200-local overflows landed on main and broke whole files in game." The checker refuses a luac that is not 5.1.
Source: `.tools/guardrails.py:519-544`, commit `11bf985c`.
For ForeverBuddy: the harness already loads `ForeverBuddy.lua` under `lua5.1` in `addon.yml`, so a parse error or local overflow fails CI today, and `cargo test contract` runs the slot files through Lua 5.1 via mlua. Keep CI on lua5.1, not LuaJIT: the file sits at the 200-locals limit and only 5.1 enforces it the way the client does.

**7.4 Keep a denylist of globals the Forever client lacks, and fail CI on any unguarded new use, naming the replacement.**
EllesmereUI's checker carries a map of legacy globals that are nil on WoW Forever, each with its `C_` replacement. Existence tests and Forever gates pass. The commit explains: "Agents kept adding them ... and each needed a later fix."
Source: `.tools/guardrails.py:564-601`, `AGENTS.md:22`, commit `2c447fac`.
For ForeverBuddy: `tools/addon-test/forever.lua` forces a mock and a row for every API we call, which catches new APIs but not wrong ones. An agent can mock a retail-only global and the harness will pass. A small denylist check, plus a review rule that a new row marked `assumed` names where the API was seen, would close that gap.

**7.5 Fail CI on non-ASCII bytes and BOMs in Lua outside locale files, and tell the author the byte-escape form.**
Their contributing guide: "Multi-byte punctuation corrupts in the packaging pipeline." The checker enforces it on every changed line.
Source: `.tools/guardrails.py:2190-2211`, `.github/CONTRIBUTING.md:60-61`.
For ForeverBuddy: 98 lines of `ForeverBuddy.lua` contain non-ASCII bytes today (glyphs like the middle dot in strings and comments). Our release zips the folder byte for byte with 7z, so nothing has broken, and this is low priority. Write new glyphs as byte escapes (`"\194\183"`) and never save with a BOM.

**7.6 Allow a suppression only with a named human approver and a reason, and give stopgaps an expiry date that breaks the build.**
Their inline tag silences one statement and names who approved it and why. A dated tag fails every run, anywhere in the tree, once the date passes. In practice 7 of the 8 tags in the tree cover code that was "moved verbatim" or "moved unchanged by the file split": a diff-only checker sees code moved across files as new.
Source: `AGENTS.md:36-42`, `.tools/guardrails.py:2225`, `.tools/tests/test_guardrails.py:765-772`, `EllesmereUIActionBars/EUI_ActionBars_Setup.lua:547`.
For ForeverBuddy: when we split `ForeverBuddy.lua` into modules (we must, given the locals limit), expect every diff-only check to flag the moved code. Do the split as a pure move in its own PR, so reviewers can see that nothing else changed.

**7.7 Test the checker itself, and label each regression test with the review finding it reproduces.**
`test_guardrails.py` has 136 tests that build scratch git repos. Tests carry comments like `# author P2 1-3` naming the review finding. CI runs these tests before the check itself.
Source: `.tools/tests/test_guardrails.py:130,142,157,270,314`, `AGENTS.md:13`, `.github/workflows/guardrails.yml:32-33`.
For ForeverBuddy: we already do this for the fake: the `strict_fake` scenario proves the fake is strict, and the harness README records that re-breaking BUG-ADV fails 85 of 200 chaos seeds. Do the same for any linter we add.

**7.8 Commit generated files that others consume, and fail CI when regenerating them produces a diff.**
EllesmereUI regenerates its locale key list in CI and fails on any diff, "so translators always see the current key list." It also runs on push to main, because "Direct pushes and release commits skip the PR trigger."
Source: `.github/workflows/locale-check.yml:3-5,13,32-41`, `.tools/extract-locale-keys.sh:24-32`.
For ForeverBuddy: we already do this three ways: harness fixtures (`--check`), the bridge contract fixtures (`cargo test contract`), and `tools/addon-test/snapshots/window.txt`. Keep new generated outputs on the same pattern.

### Tests

**7.9 Load the shipped file in plain Lua with a fake clock and event bus, and run every such test in CI.**
EllesmereUI's Lua tests `loadfile` the real module with `(addonName, ns)` and stub `CreateFrame`, `C_Timer` and `GetTime`. But they run under a local Lua 5.4 ("the local interpreter is 5.4; the game's is 5.1"), and no workflow runs them: the only test step in CI is the checker's own suite. They ship with fixes (`fc7869ab`, `0a89ac83`) and then nothing keeps them alive.
Source: `.tools/tests/test_kick_engine.lua:32,302-307`, `.tools/tests/test_new_internals.lua:1`, `.github/workflows/guardrails.yml:32-33`.
For ForeverBuddy: here we are ahead. Our harness loads the whole addon under Lua 5.1 in CI on every addon change, with 200 chaos seeds, coverage on the job summary, instruction-count budgets per handler and per frame, layout snapshots, and rendered pictures (`addon-shots.yml`). Keep it that way: a test that is not in CI is not a test.

**7.10 Model secret values in tests as poison objects that error on any use.**
Their sentinel's metatable throws on tostring, concat, compare, index and length, and `issecretvalue` is stubbed to recognize it. Arithmetic on the bare sentinel fails only because Lua rejects arithmetic on any table; explicit arithmetic traps exist only on their boxed numbers.
Source: `.tools/tests/test_kick_engine.lua:7-20,30`.
For ForeverBuddy: `wow.lua` already does this, and any API can be made secret per scenario. Lua 5.1 cannot make `==` or `#` error on a table, so those two rules still depend on review; say so in the review list (7.13).

**7.11 Test "off means zero cost" by counting frames, textures, events and hooks created with the feature disabled.**
One of their Python tests slices a single function out of the shipped source, runs it with the feature off, and asserts that nothing was allocated. It turns their first acceptance criterion into an assertion.
Source: `.tools/tests/test_rf_pandemic_border.py:3-4,16-17,34,38`.
For ForeverBuddy: each P4 module (merchant, bags, quest, map) should get a harness scenario that loads with the module off and asserts zero `RegisterEvent`, `CreateFrame` and `hooksecurefunc` calls from it. `wow.lua` sees every one of those calls, so the counters are cheap to add.

```lua
-- sketch: a module off must cost nothing
local before = client.counts()           -- frames, events, hooks so far
settings.modules.bags = false
client.login()
local after = client.counts()
assert(after.hooks_by.bags == 0 and after.events_by.bags == 0)
```

### Agent contributions and review

**7.12 Keep agent rules in the repo as a table of rule and enforcer, and mark the rules no tool can check.**
EllesmereUI's `AGENTS.md` pairs each rule with the check that enforces it. Rules about secret values and protected frames say "Judgment only. No reliable mechanical check." The commit that added it gives no reason.
Source: `AGENTS.md:17,25-26`, commit `5436ec74`.
For ForeverBuddy: our agent rules live in agent memory, specs and ticket threads, not in the repo. A short in-repo table would let any agent, or a new contributor, see which rules CI enforces and which depend on the reviewer.

**7.13 When a reviewer catches the same mistake a second time, write the check, and cite the offending commits in its message.**
Their maintenance rule: "When a reviewer corrects a mistake, fix it and add the rule here. If the rule is already here and nothing enforces it, add a check ... in the same change." The checks' commits name the repeats, for example "New files landed without the EUI_CLIENT_BLOCKED guard three times since it was introduced." The same reasoning produced a ban on hand-rolled combat-end frames once a shared `CombatQueue` existed, because new code "kept hand-rolling them".
Source: `AGENTS.md:46`, commits `87b3cbe1`, `813aabe7`, `2c447fac`.
For ForeverBuddy: our shared helpers are `read()` for API calls and `guarded()` for list code. A check that rejects a bare `C_` or `Get*` call outside `read()` would be the same move (our inference); add it the second time review catches one.

**7.14 Review every change against a short, fixed acceptance list, and make the PR template that same list.**
EllesmereUI has five criteria: zero cost unless enabled, default off, cheap when on, zero taint, and current client only ("Midnight only"). The PR template repeats them as checkboxes, and "'not applicable' is a fine answer, silence is not."
Source: `.github/CONTRIBUTING.md:16-17,20-55,78-79`, `.github/PULL_REQUEST_TEMPLATE.md:20-24`.
For ForeverBuddy: we have no PR template. A first list for addon PRs: off means zero cost (7.11), within handler budgets, no SavedVariables loss (`_meta.counts` integrity), every API through `read()`, TOC `Version` and `VERSION` bumped together.

**7.15 Point agents at Blizzard's UI source and wago.tools for API questions, and have them write original code that reuses our helpers.**
EllesmereUI's AI rules ask contributors to submit original code, keep comments brief, and reuse the existing helpers instead of adding near-copies.
Source: `.github/CONTRIBUTING.md:81-99`.
For ForeverBuddy: this bible describes practices and cites lines, and its sketches are our own. New addon code goes through `read()`, `isSecret()` and the window helpers, not fresh copies of them.

**7.16 Keep the co-author trailer on agent commits, and make the body say what broke, why, and the fix.**
Over 8 weeks, 173 of 2,482 commits carry 175 `Co-Authored-By: Claude` trailers. The good ones explain the mechanism, for example `b924b96e`: the viewport's resize fired inside the sticky-row pass, "where stickyGuard returned before the scroll child got the new width." Their practice is uneven, though: 38 of the 173 have no body beyond the trailer (for example `32ae3aff`), and only 3 link a bug report.
Source: commits `b924b96e`, `7f9f2f3f`, `32ae3aff`.
For ForeverBuddy: hold agent commits to the `b924b96e` standard, not the average: two to five lines of mechanism, plus the ticket or report link.

**7.17 To know what an agent shipped, read the final merged diff, not the commit messages, and keep unrequested changes in their own PR.**
Commit `7ced8ad9` reverted a contributor's reconciliation optimization (from a Codex branch) with the message "Keep the reconciliation optimization out of the PR until the user has reviewed and tested it manually." Thirty minutes later `540eaa93` re-applied the same change, and it merged with PR #2790 (`ab75826c`). The maintainer's release commit `481f55f9` (v9.6.8) later removed it. The revert message promised something the merged PR did not keep, and only a later release caught it.
Source: commits `7ced8ad9`, `540eaa93`, `ab75826c`, `481f55f9`.
For ForeverBuddy: this is why we squash against the merge-base and check that `git diff origin/main --stat` shows only the ticket. A performance change an agent adds unasked goes in a separate PR, so it can be tested in game on its own.

### Release

**7.18 Pin release tooling and every external library to a fixed version, and keep a local dry run that uses the same pin.**
EllesmereUI releases on any pushed tag through the BigWigs packager, pinned by commit SHA, and a local script runs the same packager commit with no upload. All their actions are SHA-pinned with `persist-credentials: false`. Their library externals are another story: only LibStub and LibDataBroker are pinned; LibDeflate uses `tag: latest` and four others track trunk.
Source: `.github/workflows/release.yml:3-6,22`, `.tools/release.sh:17-25`, `.pkgmeta:7-29,52-65`.
For ForeverBuddy: our workflows already pin actions by SHA with `persist-credentials: false`. The addon zip is built only inside `release.yml` (the PowerShell step at lines 62-77), with no local dry run. We ship no libraries; if we add one, vendor it at a tag.

**7.19 Bump every version string in the same commit, check in CI that they match, and compare versions numerically.**
EllesmereUI's release commit bumps 26 TOCs and the Lua `VERSION` constant. Its login popup for mismatched versions compares version strings with `>`, and in Lua `"9.10" < "9.7"` is true. In practice only the host addon registers a version, so the popup rarely fires at all.
Source: commit `0ddf27ac`, `EllesmereUI.lua:3258-3262,3296-3305`.
For ForeverBuddy: `tools/addon-test/run.lua:770` already fails if the TOC `Version` differs from `_meta.addon` (our `VERSION`). Anywhere the app compares addon versions, split on dots and compare the numbers.

**7.20 Keep release commits to version bumps, and run every check on push to main as well as on PRs.**
EllesmereUI's release commits also carry code (22 non-TOC files in `0ddf27ac`), at a pace of about 19 tags in 4 weeks, with hotfix tags sometimes under an hour apart. Their locale check runs on push to main because release commits skip the PR trigger.
Source: commit `0ddf27ac`, `.github/workflows/locale-check.yml:13`.
For ForeverBuddy: `addon.yml` already runs on push to main (path-filtered). `addon-shots.yml` runs on PRs only, which is fine, since its job is pictures for a designer to review.

**7.21 Store patch notes as data, link each entry to its setting, flag "new" only on upgrades, and show at most one announcement per login.**
EllesmereUI keeps per-version notes in a Lua table. Each feature can open its options page and pulse the control, and a reminder dot appears when the stored version changes: "First-ever login (no prior stamp) never raises it." An old intro popup was retired with an early return so upgraders "are never shown several intro popups back to back."
Source: `EllesmereUIOptions/EUI__General_Options.lua:229-243`, `EllesmereUI.lua:3264-3279`, `EllesmereUI_PatchNotesPopup.lua:23-28`.
For ForeverBuddy: if the login briefing ever announces addon changes, key it on the previous `VERSION` in `ForeverBuddySettings`, stay silent on a first install, and keep one announcement slot.

### Runtime safety

**7.22 Report caught errors somewhere a person will see them, and call other addons' code only through a guard.**
EllesmereUI's core runs lifecycle hooks and combat-queue entries under `xpcall` with the user's error handler, so "a failing callee can never leave the window open." Plugin and skin callbacks get the same guard, and registrations are validated with caps and reserved labels. Elsewhere the suite does not follow its own pattern: 1,192 `pcall(` calls against 12 `xpcall(`, many of them discarding the error.
Source: `EllesmereUI_Lite.lua:45-48`, `EllesmereUI.lua:967-970`, `EllesmereUI_Ticker.lua:178-197`, `EllesmereUI_Panel.lua:5629-5630,5641-5646,5691-5698,5708-5713`, `EllesmereUIBlizzardSkin/EllesmereUIBlizzardSkin_SkinAPI.lua:130-135`.
For ForeverBuddy: we `pcall` and count failures into `_meta.errors` and the `*_errors` counters, and the harness fails any scenario that leaves one. That makes errors visible in tests and in the saved file, but never to a player in game. Consider routing them through `geterrorhandler()` when a dev setting is on. TomTom and the tooltip callbacks are already guarded.

**7.23 Stop before any SavedVariables code on a client you don't support, and fail open when the build can't be read.**
EllesmereUI's first file sets a blocked flag on an unsupported client, and every other file returns on line 1 (637 of 638 files outside `.tools/`; the gate file itself is the exception). Otherwise "the migration pass would rewrite settings into shapes the pre-12.1 addon cannot read." If the interface number isn't a number, the suite runs normally.
Source: `EllesmereUI_ClientGate.lua:18-34`, `.tools/guardrails.py:547-560`.
For ForeverBuddy: we target one client (`## Interface: 16001`) and read `GetBuildInfo` only for `_meta`. If a future Forever build changes something our SavedVariables code depends on, gate at the top of `ForeverBuddy.lua` before any `ForeverBuddyDB` migration.

**7.24 Make shared namespace tables strict, so a misspelled name errors where it is read.**
EllesmereUI's split modules share guard tables whose `__index` raises "<owner> has no entry <key>", and a Lua test pins the message. The point is to fail loudly on typos (our inference; no commit states it).
Source: `EllesmereUI_SharedHelpers.lua:955-971`, `.tools/tests/test_new_internals.lua:19-20`.
For ForeverBuddy: when the file is split into modules, the shared table between them should be strict from day one. A nil from a typo would otherwise pass quietly, which is exactly what our strict fake was built to stop.

**7.25 Test each setting's apply path on every lifecycle edge (login, reload, logout and profile changes), with restrictions forced on.**
By non-merge commit over 4 weeks (872 total), their most common fix topics were locale (114), Forever (93), combat (48), unlock (43), reload (30), profile (28) and secret values (23). The cited fixes are mostly state not re-applied on a lifecycle edge (`b924b96e`, `0ba228b4`) and secret values reaching a comparison (`7f9f2f3f`, `32ae3aff`). For the secret-value paths they ship `/euidev`, which forces six restriction CVars on and shows a dev-mode badge, so the paths can be tested out of combat.
Source: commits `b924b96e`, `0ba228b4`, `7f9f2f3f`, `32ae3aff`, `0a89ac83`; `EllesmereUI.lua:3635-3660`.
For ForeverBuddy: chaos seeds already cover login order, `/reload`, the three logout paths and secret APIs. Each new P4 module setting needs the same coverage when it is toggled mid-session. Whether Forever honors those restriction CVars is unverified, so check before building a toggle on them.

### Not for us

- Reading `_Camelot.toc` and `_Mainline.toc` load rules to decide which files run on Forever (`.tools/guardrails.py:2134-2155`): we ship one addon for one client.
- `.pkgmeta` `move-folders` to lift sub-addons out of one repo: we ship one folder.
- The cross-addon version popup: we have no sibling addons.
- Their release pace (four to five tags a week, release commits carrying code): it works for a large suite with a live bug tracker, not as a model.
