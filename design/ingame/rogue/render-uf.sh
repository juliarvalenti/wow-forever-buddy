#!/bin/sh
# render-uf.sh <spec> [layout]: renders the rogue scene (tools/addon-shots/hud.lua,
# --class rogue) with FBUnitProto drawing specs/<spec>.lua, to out/<spec>.svg.
# Needs the CASC extract (set EXTRACT) and WOWAPI's data (tools/wowapi/fetch-*.sh).
# Open the SVG in a browser, or rasterise it with any SVG tool.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
X=${EXTRACT:-"$HOME/Library/Application Support/com.juliarvalenti.wowforeverbuddy/cache/casc-extract/wowf/7feb61f3c994bcb168526b8db521514a"}
spec=$1 lay=${2:-centered}
mkdir -p "$here/out"
cp "$here/specs/$spec.lua" "$here/FBUnitProto/Spec.lua"
cd "$root"
WOWAPI_ART="$X" WOWAPI_OFFLINE=1 luajit tools/addon-shots/hud.lua "$here/out/$spec.svg" --blizzard "$X/interface/addons" \
  --class rogue --layout "$lay" --addon "$here/FBUnitProto" 2>&1 | head -3
rm -f "$here/FBUnitProto/Spec.lua"
