#!/bin/sh
# Fills tools/wowapi/wowapi/data/globalcolor.csv: the game's named UI colours
# (GlobalColor.db2: NORMAL_FONT_COLOR, ACTIONBAR_HOTKEY_FONT_COLOR, ...),
# which the client hands to Blizzard's Color.lua through C_UIColor.GetColors.
# Blizzard data, so it goes in the gitignored data/ folder like the rest.
# Does nothing if it's there.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
dest="$here/wowapi/data/globalcolor.csv"
[ -f "$dest" ] && exit 0
mkdir -p "$here/wowapi/data"

tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
curl -fsSL "https://wago.tools/db2/GlobalColor/csv" -o "$tmp"
head -n 1 "$tmp" | grep -q '^ID,LuaConstantName,Color' || { echo "not the GlobalColor table" >&2; exit 1; }
mv "$tmp" "$dest"
echo "fetched $(($(wc -l < "$dest") - 1)) global colours"
