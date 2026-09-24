#!/bin/sh
# Render the quick action icons, icons/cover/*.svg, into the PNGs the cover
# hands to the home screen: qml/art/cover/<name>-<size>-<ink>.png.
#
# The home screen draws a cover action's icon itself, from a file it reads,
# so the picture has to arrive at the size and in the colour it is shown
# in: one per icon size Sailfish scales to (Theme.iconSizeSmall, 32 at
# 1.0 up to 64 at 2.0), in white for a dark ambience and black for a light
# one. The cover picks the file; see qml/js/QuickActions.js.
#
# The sources draw in white; the black ones are the same file recoloured.
# Needs rsvg-convert (Debian's librsvg2-bin). What it writes is committed,
# so neither the build nor CI runs this.
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
