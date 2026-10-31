# Addon smoke test (SIM5b)

Runs addons headless through WOWAPI (`tools/wowapi`, the WoW: Forever client emulator, Interface 16001 with secret values on), one at a time. Each addon gets a fresh client and a short session: log in, idle 10 s, combat in and out, `/reload`. The report lists, per addon:

- whether it loaded (or why not: a missing dependency, or a TOC that excludes Forever);
- Lua errors;
- actions the client's security blocked;
- globals it read that WOWAPI doesn't define. Usually an API WOWAPI doesn't emulate, sometimes the addon's own optional checks (another addon's global, a data file not written yet), or a real typo;
- calls WOWAPI only stubs.

It's a building block for P4's "configure other addons": before Claude proposes settings for an addon, we know whether it even runs on Forever.

**Third-party code runs only in CI,** on a throwaway runner with a read-only token, never on a developer's machine. Run the **Addon smoke test** workflow by hand (Actions › Addon smoke test › Run workflow, optionally with a comma list of folders). The report goes on the run's summary page and into the `addon-smoke` artifact. `addons.txt` is the list, as GitHub `owner/repo`. Each is fetched and packaged as the CurseForge packager would (`.pkgmeta` externals, move-folders) by WOWAPI's installer.

**Locally,** only on code we wrote or a folder you trust:

```sh
tools/wowapi/fetch-data.sh                     # once
luajit tools/addon-smoke/smoke.lua <AddOns folder> [--only A,B] [--md report.md]
```

**Next:** Julia's real AddOns folder, through the same report. That needs a way to get the folder's addons onto a runner without passing her files through anyone else; to be decided.
