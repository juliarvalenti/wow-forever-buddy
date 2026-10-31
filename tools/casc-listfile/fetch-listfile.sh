#!/bin/sh
# Fills tools/casc-listfile/files.csv from the community listfile: the
# FileDataID -> path map needed to extract anything out of CASC by name.
# It's the wow-listfile project's CSV, never ours, so it isn't committed.
# Does nothing if it's there.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
dest="$here/files.csv"
[ -f "$dest" ] && exit 0

url=https://raw.githubusercontent.com/wow-listfile/listfile/master/files.csv
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
curl -fsSL "$url" -o "$tmp"
# A few sanity checks before trusting it as a listfile: it's the biggest
# file the tools fetch, and a truncated download must not be kept.
size=$(wc -c < "$tmp")
[ "$size" -gt 10000000 ] || { echo "listfile too small ($size bytes), not kept" >&2; exit 1; }
head -n 1 "$tmp" | grep -q ';' || { echo "not a listfile (first row has no ';')" >&2; exit 1; }
mv "$tmp" "$dest"
echo "fetched $(wc -l < "$dest" | tr -d ' ') listfile rows"