#!/bin/sh
# Fills tools/casc-listfile/WoW.txt from wowdev's public TACT key list: the
# key names Blizzard's encrypted files ask for, mapped to Salsa20 keys. The
# keys are the community's, never ours, so they aren't committed.
# Does nothing if they're there.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
dest="$here/WoW.txt"
[ -f "$dest" ] && exit 0

url=https://raw.githubusercontent.com/wowdev/TACTKeys/master/WoW.txt
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
curl -fsSL "$url" -o "$tmp"
# Sanity checks: the key list is large, and a truncated download must not be
# kept as if it were the real one.
size=$(wc -c < "$tmp")
[ "$size" -gt 100000 ] || { echo "key list too small ($size bytes), not kept" >&2; exit 1; }
head -n 1 "$tmp" | grep -Eq '^[0-9A-F]{16} [0-9A-F]{32}$' || {
    echo "not a key list (first row is malformed)" >&2
    exit 1
}
mv "$tmp" "$dest"
echo "fetched $(wc -l < "$dest" | tr -d ' ') TACT keys"