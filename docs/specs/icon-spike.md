# ICON-SPIKE: item icons from the local CASC archive

Status: findings, 2026-10-05 (@coder). The format layers are checked against real Forever data; the local-index layer waits on Julia's `icon_probe` run on a real install.

## Question

Can the app show item icons (the addon records `icon_file_id`) by reading them from the player's own install, with no network and no third-party data?

**Answer: yes.** A pure-Rust reader in `src-tauri/src/casc/` follows the chain from FileDataID to PNG, and every format layer decodes real build **1.60.1.70205** (`wow_classic_beta`) data.

## The chain

| Step | File | Module |
|---|---|---|
| Which build is installed | `<WoW>/.build.info` (row for the product named in `<flavor>/.flavor.info`) | `config` |
| Root and encoding keys | `Data/config/xx/yy/<build key>` | `config` |
| Content key to encoding key | the encoding file (160 MB on this build; one page scanned per lookup, never expanded into a map) | `encoding` |
| FileDataID to content key | the root file (MFST, 10.1.7+ header with v2 blocks; enUS or all-locale blocks, low-violence blocks skipped) | `root` |
| Encoding key to archive offset | `Data/data/BBVVVVVVVV.idx` (v7, newest per bucket, read on first use) | `idx` |
| The bytes | `Data/data/data.NNN`: a 30-byte header (reversed key and size, checked so a stale index is caught), then BLTE (raw and zlib; encrypted chunks fail as `Encrypted`) | `blte`, `mod` |
| The image | BLP2: palettized (0/1/4/8-bit alpha), DXT1/3/5, BGRA; to PNG | `blp` |

## Safety

- **Read-only on `Data/`.** Small files go through `safe_read`; archives are opened with full sharing and only the needed span is read (`seek` + `read_exact`), then the handle is closed. A test snapshots every file under `Data` before and after a read and asserts nothing changed. Safe with the game running.
- **Capped and fuzzed.** Every length and offset goes through a bounds-checked reader (`casc/bytes.rs`). Caps: 512 MB for encoding and root, 64 MB for any other file, 4096 px a side for images, zlib output stopped at the cap (a test feeds a zlib bomb). Each parser has proptest cases on random bytes and on single-byte damage to a valid file.
- **Fails soft.** Every failure is a `CascError` for that one file (`Bad(<structure>)`, `TooBig`, `Encrypted`, `Missing`, `Io`); messages name the structure and never quote the data. `IconCache::fill` returns a result per icon, so one bad icon never blocks the rest, and failures leave nothing in the cache.
- **Cache keyed by build + id.** `<cache>/<build key>/<id>.png`. A patch changes the build key, so a stale icon is never served, and a hit only reads `.build.info`.

## Validation so far

There's no WoW install on my machine, so the format layers were checked against Blizzard's public CDN for the same build (as agreed with @project-mgmt: a dev-time check only, never in the app, nothing committed or redistributed):

- encoding (160 MB) and root (44 MB) for 1.60.1.70205 parse; 4,290 of the 4,300 FileDataIDs in 132000–136299 resolve.
- 20 icons across that range decode to correct-looking PNGs (19 at 64x64, one 1024x1024 texture).
- The same files, packed into a local `Data/` layout (`.build.info`, build config, `.idx`, `data.000`), run through the real `icon_probe`:

| Step | Time (M-series Mac, release build) |
|---|---|
| `.build.info` (enough for a cache hit) | 0.6 ms |
| Open: build config, index list, encoding, root | 125 ms |
| 20 icons, cold (one root scan, read, decode, PNG, write) | 255 ms (12.7 ms each, mostly the 1024x1024 one) |
| 20 icons, cached | under 0.1 ms |

**Not yet proven:** that real `.idx` and `data.NNN` files match the layout the tests build (header sizes, the 5-byte offset packing, whether the `.idx` size includes the 30-byte header). That's what Julia's run checks.

## Julia's run

1. Download `icon_probe.exe` from the `icon-probe` workflow run on this PR (Checks tab, then the run's Artifacts).
2. Double-click it. It finds `_classic_beta_` in the usual places or asks for the folder (drag it into the window). The game can stay open.
3. Send back `icon-probe/timings.txt` (next to the exe) and a glance at whether the 20 PNGs in `icon-probe/` look like icons.

If it fails, the line it prints names the layer (`malformed idx layout`, `malformed archive header`, ...), which is enough to fix it without another round of questions.

## For the real feature (not in this spike)

- Encoding stays in memory while a `Casc` is open (about 160 MB). Open it for a batch of missing icons, then drop it; or keep a ckey→ekey map for just the icons we need.
- A Tauri command to serve cached PNGs to the UI, and pruning cache folders for old builds.
- Icons on encrypted (`E`) chunks show the placeholder; none of the 20 checked were encrypted.
