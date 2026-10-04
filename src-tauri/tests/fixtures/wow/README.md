# Fixture WoW install

A fake World of Warcraft root for tests (spec §9). Tests copy it into a temp
dir before touching it; never write here.

- `.build.info`: two products, `wow_classic_beta` (WoW: Forever, 1.60.x) and `wow_classic_era`.
- `_classic_beta_/`: the Forever flavor, with an exe stub (`WowB.exe`), AddOns, and a WTF tree:
  - `ACCOUNT1`: account-level SavedVariables (including a `.lua.bak`) and the
    account cache files, plus characters on two realms (one realm name has a space).
  - `ACCOUNT2`: one character.
  - `Cache/`, `Logs/`, `Screenshots/`, which backups must exclude.
- `_classic_era_/`: a WTF-only flavor with no exe, which must still count as valid.

Everything is plain text. `.gitattributes` marks this tree `-text`, so line
endings are byte-exact on every platform.
