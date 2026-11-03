#!/bin/sh
# Fills tools/wowapi/wowapi/data/ from WOWAPI at the vendored commit. That
# folder is Blizzard API docs, global strings and atlas and icon lists, so it
# isn't committed here (security review on #199). Does nothing if it's there.
set -eu

COMMIT=aea2b451a9075a5a5591ac3d56cf6e344c0557d0 # ORIGIN.md
here=$(cd "$(dirname "$0")" && pwd)
dest="$here/wowapi/data"
[ -f "$dest/apidocs.lua" ] && exit 0

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
git -C "$tmp" init -q
# Byte for byte as WOWAPI has them: the app pins these files by SHA-256
# (PV1), and Windows git would otherwise check them out with CRLF.
git -C "$tmp" config core.autocrlf false
git -C "$tmp" fetch -q --depth 1 --filter=blob:none https://github.com/Reece01-Dock/WOWAPI.git "$COMMIT"
# No leading slash: Git Bash on Windows would rewrite "/wowapi/..." into a
# Windows path.
git -C "$tmp" sparse-checkout set --no-cone wowapi/data/
git -C "$tmp" checkout -q FETCH_HEAD
rm -rf "$dest"
cp -R "$tmp/wowapi/data" "$dest"
echo "fetched WOWAPI data/ at $COMMIT"
