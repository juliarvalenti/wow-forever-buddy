#!/bin/sh
# Fills tools/wowapi/wowapi/data/globalstrings-forever.csv: WoW Forever's own
# UI strings (GlobalStrings.db2 at its build, from wago.tools), which WOWAPI
# loads over the bundled retail strings. The retail list lacks Forever's
# (GAMEPAD_LABEL). Blizzard data, so it goes in the gitignored data/ folder
# like the rest. Does nothing if it's there.
set -eu

BUILD=1.60.1.70707 # WoW Forever, as tracked by wago.tools ("wowf")
here=$(cd "$(dirname "$0")" && pwd)
dest="$here/wowapi/data/globalstrings-forever.csv"
[ -f "$dest" ] && exit 0
mkdir -p "$here/wowapi/data"

tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
curl -fsSL "https://wago.tools/db2/GlobalStrings/csv?build=$BUILD" -o "$tmp"
head -n 1 "$tmp" | grep -q '^ID,BaseTag,TagText_lang' || { echo "not the GlobalStrings table" >&2; exit 1; }
mv "$tmp" "$dest"
echo "fetched Forever's global strings"
