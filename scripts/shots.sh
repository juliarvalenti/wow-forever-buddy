#!/usr/bin/env bash
# Every app screen next to its round-3 mock, at 1280x800 and 1024x700, for
# design review. Builds the frontend with the fake backend (npm run
# build:mock), serves it under the app's real CSP, shoots each scenario with
# shotkit, and writes .shotkit/compare/<name>-<size>.png = [app | mock].
#
# Needs: shotkit (`shot` on PATH), ImageMagick (`magick`), libwebp (`dwebp`).
# Usage: npm run shots [-- <scenario-name>]   (SHOTS_PORT=… if 4173 is taken)
set -euo pipefail
cd "$(dirname "$0")/.."

ONLY="${1:-}"
OUT=.shotkit/compare
MOCKS=design/mocks/round-3/shots
PORT="${SHOTS_PORT:-4173}"
FONT=$(fc-match -f '%{file}' sans 2>/dev/null || echo /System/Library/Fonts/Supplemental/Arial.ttf)
mkdir -p "$OUT"

# By title: below 1100px the nav is an icon rail and its labels are hidden.
NAV='click:css=nav button[title="Backups"]'
OPEN='click:text=On game exit >> nth=0'
# (`wait:` takes a selector: each step waits for what the shot needs.)
CONFIRM="click:Thrandor;click:css=.d-main button >> text=Restore >> nth=-1;wait:css=.d-dialog"

# name | ?mock= scenario | shot --do steps (;-separated) | mock shot
SCENARIOS=(
  "dashboard|noaddon||dashboard-noaddon"
  "dashboard-addon|dashboard||dashboard"
  "dashboard-addon-update|addon-update||dashboard"
  "dashboard-missing|dashboard-missing||dashboard-folder-missing"
  "dashboard-recover|recover||dashboard-recover"
  "dashboard-recover-unreadable|recover-unreadable-no-safety||dashboard-recover-unreadable"
  "backups|backups|$NAV;$OPEN|backups"
  "backups-confirm|backups|$NAV;$OPEN;$CONFIRM|backups-confirm"
  "backups-corrupt|backups-corrupt|$NAV;$OPEN;$CONFIRM;click:css=.d-dialog button >> text=Restore;wait:text=This snapshot is damaged|backups-corrupt"
  "backups-error|backup-failed|$NAV|backups-error"
  "onboarding-notfound|nogame|wait:text=couldn't find World of Warcraft|onboarding-notfound"
  "startup-error|startup-error||startup-error"
  "characters|characters|click:css=nav button[title=\"Characters\"];wait:css=.ch-card|characters"
  "character|characters|click:css=nav button[title=\"Characters\"];click:css=.ch-card >> text=Thrandor;wait:css=.ch-sheet|character"
  "character-tooltip|characters|click:css=nav button[title=\"Characters\"];click:css=.ch-card >> text=Thrandor;wait:css=.ch-sheet;hover:text=Truestrike Shoulders;wait:css=.ch-tt|character"
  "character-bank-alt|characters|click:css=nav button[title=\"Characters\"];click:css=.ch-card >> text=Coinpurse;wait:css=.ch-sheet|character"
  "ah|ah|click:css=nav button[title=\"Auction House\"];wait:css=.ah-chart|ah"
  "ah-empty|ah-empty|click:css=nav button[title=\"Auction House\"];wait:text=No auction prices yet|ah"
  "settings|settings|click:css=.d-side-foot button[title=\"Settings\"];wait:css=.st-svc|settings"
  "settings-move-confirm|settings|click:css=.d-side-foot button[title=\"Settings\"];click:css=.st-set.full:has-text(\"Store backups in\") button;wait:css=.st-confirm|settings"
  "settings-moving|settings-moving|click:css=.d-side-foot button[title=\"Settings\"];click:css=.st-set.full:has-text(\"Store backups in\") button;click:Move backups;wait:text=Copying 412|settings"
  "settings-move-refused|settings-pending|click:css=.d-side-foot button[title=\"Settings\"];click:css=.st-set.full:has-text(\"Store backups in\") button;click:Move backups;wait:text=Nothing was moved|settings"
  "characters-noaddon|characters-empty|click:css=nav button[title=\"Characters\"];wait:css=.ch-card.unseen|characters-noaddon"
  "characters-search|characters|click:css=nav button[title=\"Characters\"];wait:css=.ch-card;fill:Search every satchel, bank and mailbox=Runecloth;wait:css=.ch-results|characters-search"
  "addons|characters|click:css=nav button[title=\"Addons\"];wait:css=.ad-table|addons-readonly"
  "addons-empty|addons-empty|click:css=nav button[title=\"Addons\"];wait:text=No addons in|addons-readonly"
  # F6: after turning Questie on for Thrandor (Undo offered), and locked while WoW runs.
  "addons-staged|characters|click:css=nav button[title=\"Addons\"];click:css=.ad-table tr:has-text(\"Questie\");click:css=[aria-label=\"Questie for Thrandor\"];wait:text=1 change|addons-staged"
  "addons-applied|characters|click:css=nav button[title=\"Addons\"];click:css=.ad-table tr:has-text(\"Questie\");click:css=[aria-label=\"Questie for Thrandor\"];click:Apply;wait:text=A safety snapshot was taken first|addons-applied"
  "addons-running|dashboard|click:css=nav button[title=\"Addons\"];click:css=.ad-table tr:has-text(\"Questie\");wait:text=addon changes are locked|addons-running"
  # No shot of the linked state yet: it sits next to the edit state.
  "addons-linked|addons-linked|click:css=nav button[title=\"Addons\"];click:css=.ad-table tr:has-text(\"Questie\");wait:text=linked folder|addons-edit"
)

echo "== build:mock"
npm run build:mock >/dev/null

echo "== serve on :$PORT"
# Something else on the port (an earlier run, a manual serve-csp) would
# answer instead of this build, so refuse rather than shoot the wrong thing.
if curl -s -o /dev/null "localhost:$PORT"; then
  echo "port $PORT is already serving something; stop it (lsof -ti :$PORT | xargs kill) and rerun" >&2
  exit 1
fi
SERVER=
serve() {
  node scripts/serve-csp.mjs dist-mock "$PORT" >"$OUT/.serve.log" 2>&1 &
  SERVER=$!
  for _ in $(seq 1 50); do curl -s -o /dev/null "localhost:$PORT" && return 0; sleep 0.1; done
  echo "serve-csp didn't start; see $OUT/.serve.log" >&2
  exit 1
}
trap '[[ -n "$SERVER" ]] && kill "$SERVER" 2>/dev/null' EXIT
serve

for row in "${SCENARIOS[@]}"; do
  IFS='|' read -r name scenario steps mock <<<"$row"
  [[ -n "$ONLY" && "$ONLY" != "$name" ]] && continue
  for size in 1280x800 1024x700; do
    # Restart the server if it died (reported once: SIGTERM after the first
    # scenario); its log says why.
    kill -0 "$SERVER" 2>/dev/null || { echo "serve-csp exited, restarting" >&2; serve; }
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
