#!/usr/bin/env bash
# Every app screen next to its round-3 mock, at 1280x800 and 1024x700, for
# design review. Builds the frontend with the fake backend (npm run
# build:mock), serves it under the app's real CSP, shoots each scenario with
# shotkit, and writes .shotkit/compare/<name>-<size>.png = [app | mock].
#
# Needs: shotkit (`shot` on PATH), ImageMagick (`magick`), libwebp (`dwebp`).
# Usage: npm run shots [-- <scenario-name>]
set -euo pipefail
cd "$(dirname "$0")/.."

ONLY="${1:-}"
OUT=.shotkit/compare
MOCKS=design/mocks/round-3/shots
PORT=4173
FONT=$(fc-match -f '%{file}' sans 2>/dev/null || echo /System/Library/Fonts/Supplemental/Arial.ttf)
mkdir -p "$OUT"

NAV='click:css=nav button >> text=Backups'
OPEN='click:text=On game exit >> nth=0'
# (`wait:` takes a selector: each step waits for what the shot needs.)
CONFIRM="click:Thrandor;click:css=.d-main button >> text=Restore >> nth=-1;wait:css=.d-dialog"

# name | ?mock= scenario | shot --do steps (;-separated) | mock shot
SCENARIOS=(
  "dashboard|dashboard||dashboard-noaddon"
  "dashboard-missing|dashboard-missing||dashboard-folder-missing"
  "dashboard-recover|recover||dashboard-recover"
  "dashboard-recover-unreadable|recover-unreadable-no-safety||dashboard-recover-unreadable"
  "backups|backups|$NAV;$OPEN|backups"
  "backups-confirm|backups|$NAV;$OPEN;$CONFIRM|backups-confirm"
  "backups-corrupt|backups-corrupt|$NAV;$OPEN;$CONFIRM;click:css=.d-dialog button >> text=Restore;wait:text=This snapshot is damaged|backups-corrupt"
  "backups-error|backup-failed|$NAV|backups-error"
  "onboarding-notfound|nogame|wait:text=couldn't find World of Warcraft|onboarding-notfound"
  "startup-error|startup-error||startup-error"
)

echo "== build:mock"
npm run build:mock >/dev/null

echo "== serve on :$PORT"
node scripts/serve-csp.mjs dist-mock "$PORT" >/dev/null 2>&1 &
SERVER=$!
trap 'kill $SERVER 2>/dev/null' EXIT
for _ in $(seq 1 50); do curl -s -o /dev/null "localhost:$PORT" && break; sleep 0.1; done

for row in "${SCENARIOS[@]}"; do
  IFS='|' read -r name scenario steps mock <<<"$row"
  [[ -n "$ONLY" && "$ONLY" != "$name" ]] && continue
  for size in 1280x800 1024x700; do
    args=()
    if [[ -n "$steps" ]]; then
      IFS=';' read -ra parts <<<"$steps"
      for p in "${parts[@]}"; do [[ -n "$p" ]] && args+=(--do "$p"); done
    fi
    app=$(shot url "http://localhost:$PORT/?mock=$scenario" --viewport "$size" ${args[@]+"${args[@]}"} \
      --name "compare-app-$name-$size" | tail -1)
    ref="$OUT/.mock-$mock-$size.png"
    dwebp -quiet "$MOCKS/$mock-$size.webp" -o "$ref"
    h=$(magick identify -format '%h' "$app")
    magick -font "$FONT" \
      \( "$app" -resize "x$h" -gravity north -background '#111' -splice 0x56 -fill '#ddd' -pointsize 34 -annotate +0+10 "app · $name · $size" \) \
      \( "$ref" -resize "x$h" -gravity north -background '#111' -splice 0x56 -fill '#ddd' -pointsize 34 -annotate +0+10 "mock · $mock" \) \
      +append "$OUT/$name-$size.png"
    echo "$OUT/$name-$size.png"
  done
done
