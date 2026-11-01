#!/bin/sh
# Fills tools/wowapi/wowapi/data/atlases-forever.txt: WoW Forever's own UI
# atlases (UiTextureAtlas, -Member and -Element db2 at the Forever build, from
# wago.tools), in atlases.txt's format. The bundled atlases.txt is retail's
# and lacks Forever's (ui-hud-minimap-header-button, the micro menu's
# legacy buttons). Where Forever has a c60 variant of an atlas, that is the
# art its client draws, so only the c60 variants are kept for it. Paths come
# from the community listfile (tools/casc-listfile/fetch-listfile.sh).
# Blizzard data, so it goes in the gitignored data/ folder like the rest.
# Does nothing if it's there.
set -eu

BUILD=1.60.1.70707 # WoW Forever, as tracked by wago.tools ("wowf")
here=$(cd "$(dirname "$0")" && pwd)
dest="$here/wowapi/data/atlases-forever.txt"
[ -f "$dest" ] && exit 0
listfile="$here/../casc-listfile/files.csv"
[ -f "$listfile" ] || "$here/../casc-listfile/fetch-listfile.sh"
mkdir -p "$here/wowapi/data"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
for t in UiTextureAtlas UiTextureAtlasMember UiTextureAtlasElement; do
    curl -fsSL "https://wago.tools/db2/$t/csv?build=$BUILD" -o "$tmp/$t.csv"
done
head -n 1 "$tmp/UiTextureAtlasMember.csv" | grep -q '^CommittedName,ID,UiTextureAtlasID,Width,Height,CommittedLeft' \
    || { echo "not the UiTextureAtlasMember table" >&2; exit 1; }

# The listfile is large: keep only the atlases' files.
cut -d, -f2 "$tmp/UiTextureAtlas.csv" | sed 1d | sort -u > "$tmp/ids"
awk -F';' 'NR == FNR { want[$1] = 1; next } ($1 in want) { print }' "$tmp/ids" "$listfile" > "$tmp/paths"

awk -F, -v OFS='\t' -v OFMT='%.10g' '
    FILENAME ~ /paths$/ { split($0, p, ";"); path = tolower(p[2]); sub(/\.[^.\/]*$/, "", path); paths[p[1]] = path; next }
    FNR == 1 { next }
    FILENAME ~ /UiTextureAtlas\.csv$/ { file[$1] = $2; aw[$1] = $4; ah[$1] = $5; next }
    FILENAME ~ /UiTextureAtlasElement\.csv$/ { name[$2] = $1; next }
    {
        n++; m[n] = $0
        if ($1 ~ /-c60(-2x)?$/) c60[$10] = 1
    }
    END {
        print "# atlas\tfileID\tpath\twidth\theight\tleft\tright\ttop\tbottom\ttileH\ttileV  (WoW Forever, wago.tools)"
        for (i = 1; i <= n; i++) {
            split(m[i], f, ",")
            el = f[10]; at = f[3]
            if (!(el in name) || !(at in file) || aw[at] == 0 || ah[at] == 0) continue
            if ((el in c60) && f[1] !~ /-c60(-2x)?$/) continue
            flags = f[13] + 0
            print name[el], file[at], paths[file[at]], f[4], f[5], f[6] / aw[at], f[7] / aw[at], f[8] / ah[at], f[9] / ah[at],
                (int(flags / 4) % 2), (int(flags / 2) % 2)
        }
    }
' "$tmp/paths" "$tmp/UiTextureAtlas.csv" "$tmp/UiTextureAtlasElement.csv" "$tmp/UiTextureAtlasMember.csv" > "$tmp/out"
mv "$tmp/out" "$dest"
echo "fetched $(($(wc -l < "$dest") - 1)) Forever atlas variants"
