#!/bin/sh
# Render cover quick action icons/cover/*.svg to
# qml/art/cover/<name>-<size>-<ink>.png. Home screen draws them from file, so
# each must come at shown size and colour: one per icon size
# (Theme.iconSizeSmall, 32 at 1.0 to 64 at 2.0), white for dark ambience, black
# for light. Cover picks file: qml/js/QuickActions.js.
#
# Sources white; black = recoloured copy. Needs rsvg-convert (librsvg2-bin).
# Output committed: build and CI don't run this.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
out=$root/qml/art/cover

rsvg=$(command -v rsvg-convert || true)
if [ -z "$rsvg" ]; then
    echo "render-cover-icons: rsvg-convert not found (install librsvg2-bin)" >&2
    exit 1
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

mkdir -p "$out"
rm -f "$out"/*.png

for source in "$root"/icons/cover/*.svg; do
    name=$(basename "$source" .svg)
    sed 's/#ffffff/#000000/g' "$source" > "$work/$name-black.svg"
    for size in 32 40 48 56 64; do
        "$rsvg" -w "$size" -h "$size" "$source" -o "$out/$name-$size-white.png"
        "$rsvg" -w "$size" -h "$size" "$work/$name-black.svg" -o "$out/$name-$size-black.png"
    done
done
