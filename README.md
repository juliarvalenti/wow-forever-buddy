# WoW Forever Buddy

A desktop companion for **World of Warcraft: Forever** — addons, macros, and
whatever else is easier to manage outside the game. Right now it's just the
shell: no features yet.

Built with [Tauri 2](https://tauri.app), React, Tailwind CSS 4,
[shadcn/ui](https://ui.shadcn.com), and the Warcraft-styled components from
[warcraftcn/ui](https://github.com/TheOrcDev/warcraftcn-ui) (MIT).

## Offline by design

Everything the UI needs ships inside the app — no CDN, no Google Fonts:

- The warcraftcn components and their art live in
  `src/components/ui/warcraftcn/`. Their stylesheet originally hot-linked
  images from warcraftcn.com; it now points at the local copies.
- Fonts (Cinzel, Geist) come from `@fontsource` packages and are bundled at
  build time.

If you add more warcraftcn components, check the new files for `https://`
URLs, and fetch binary assets straight from the GitHub repo — the shadcn CLI
has corrupted the `.webp` files it installed from the warcraftcn registry.

## Develop

Needs Node 20+ and Rust (plus the
[Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS).

```sh
npm install
npm run tauri dev     # the desktop app
npm run dev           # just the UI in a browser, on http://localhost:1420
```

## Releases

Publishing a GitHub release runs `.github/workflows/release.yml`, which builds
on Windows and attaches to the release:

- `WoW Forever Buddy_<version>_x64-setup.exe` — installer
- `WoW Forever Buddy_<version>_x64_en-US.msi` — MSI installer
- a plain `.exe` — portable, runs without installing (needs WebView2, which
  ships with Windows 10 and 11)

Bump `version` in `src-tauri/tauri.conf.json` (and `package.json`) before
tagging. The builds are unsigned, so Windows SmartScreen will warn the first
time you run one: **More info → Run anyway**.
