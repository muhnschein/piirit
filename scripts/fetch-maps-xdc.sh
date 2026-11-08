#!/bin/sh
# Fetches the map app: Delta Chat's maps.xdc (deltachat/maps), byte for byte as
# deltachat-desktop bundles it. See vendor/maps/SOURCE.md.
#
# Committed, unlike core binaries: 52 KB, compiled into app (src/maps.rs);
# builds must not need network. This script changes it and proves the copy
# matches upstream.
#
#     scripts/fetch-maps-xdc.sh           fetch, verify, install
#     scripts/fetch-maps-xdc.sh --check   fetch, verify, and require
#                                         the committed copy to match
#
# Pinned: bump COMMIT and SHA256 together, then SOURCE.md and `MAPS_VERSION`
# in src/maps.rs (a test holds them equal). deltachat/maps has no releases;
# Desktop's committed zip is the build every Desktop user runs.

set -eu

COMMIT="bb52bd1d61e8458749745ae93509cd380432c719"
SHA256="38881287752faf943d4c9e6b66bb2be353e169d523ca2ac1c31e0a08d3f41476"

repo_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
target="$repo_root/vendor/maps/maps.xdc"
url="https://raw.githubusercontent.com/deltachat/deltachat-desktop/$COMMIT/static/xdcs/maps.xdc"

check=0
if [ "${1:-}" = "--check" ]; then
    check=1
fi

if ! command -v curl >/dev/null 2>&1; then
    # A check passing without its tool is no check: skip locally, fail on CI
    # (GitHub sets CI).
    if [ "$check" = 1 ] && [ -z "${CI:-}" ]; then
        echo "fetch-maps-xdc: SKIP curl not found"
        exit 0
    fi
    echo "fetch-maps-xdc: FAIL curl not found" >&2
    exit 1
fi

workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT

# Both --proto flags keep every hop on HTTPS.
if ! curl -sSfL --proto '=https' --proto-redir '=https' "$url" -o "$workdir/maps.xdc"; then
    echo "fetch-maps-xdc: FAIL could not fetch $url" >&2
    exit 1
fi

echo "$SHA256  $workdir/maps.xdc" | sha256sum -c - >/dev/null || {
    echo "fetch-maps-xdc: FAIL checksum mismatch for maps.xdc at Desktop $COMMIT; refusing it" >&2
    exit 1
}

if [ "$check" = 1 ]; then
    if ! cmp -s "$workdir/maps.xdc" "$target"; then
        echo "fetch-maps-xdc: FAIL vendor/maps/maps.xdc is not Desktop $COMMIT's" >&2
        exit 1
    fi
    echo "fetch-maps-xdc: OK vendor/maps/maps.xdc is Desktop $COMMIT's"
    exit 0
fi

install -Dm 644 "$workdir/maps.xdc" "$target"
echo "Done. vendor/maps/maps.xdc is Desktop $COMMIT's."
echo "Remember: vendor/maps/SOURCE.md and MAPS_VERSION in src/maps.rs must name this commit."
