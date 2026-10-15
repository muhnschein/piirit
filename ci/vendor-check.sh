#!/bin/sh
# Prove third_party/qmetaobject = upstream 0.2.10 + one patch;
# vendor/calls-webapp/index.html = upstream release asset; qml/ emoji = pinned
# Twemoji and emojibase-data.
#
# Fetch crates.io tarball, apply qmetaobject.patch, require byte-for-byte match
# with vendored tree. Direct edits or stale patch fail here. Tarball is
# immutable.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
vendored="$root/third_party/qmetaobject"
patch_file="$root/third_party/qmetaobject.patch"

# Compare against the version cargo is told to replace.
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$vendored/Cargo.toml" | head -1)
[ -n "$version" ] || { echo "vendor-check: FAIL no version in the vendored Cargo.toml" >&2; exit 1; }

if ! command -v curl >/dev/null 2>&1; then
    # Skip locally; fail on runner (CI set). A gate without its tool isn't a
    # gate.
    if [ -n "${CI:-}" ]; then
        echo "vendor-check: FAIL curl not found; this check proved nothing" >&2
        exit 1
    fi
    echo "vendor-check: SKIP curl not found"
    exit 0
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

url="https://static.crates.io/crates/qmetaobject/qmetaobject-$version.crate"
# -L follows redirects; the two --proto flags keep every hop on HTTPS.
if ! curl -sSfL --proto '=https' --proto-redir '=https' "$url" -o "$work/crate.tar.gz"; then
    echo "vendor-check: FAIL could not fetch $url" >&2
    exit 1
fi
tar -C "$work" -xzf "$work/crate.tar.gz"
upstream="$work/qmetaobject-$version"
[ -d "$upstream" ] || { echo "vendor-check: FAIL unexpected tarball layout" >&2; exit 1; }

# cargo's .cargo-ok marker is not in tarball; excluded from comparison.

# Crate's own tests not vendored: never built, 1,200 lines; CodeQL flagged seven
# highs in code not compiled here. Dropped both sides.
rm -rf "$upstream/tests"

if ! patch -s -p1 -d "$upstream" < "$patch_file"; then
    echo "vendor-check: FAIL third_party/qmetaobject.patch does not apply to upstream $version" >&2
    exit 1
fi

if diff -r -q -x .cargo-ok "$upstream" "$vendored" >/dev/null 2>&1; then
    echo "vendor-check: ok (qmetaobject $version + qmetaobject.patch)"
    # Upstream page the call runs in: refetched, compared byte for byte.
    "$root/scripts/fetch-calls-webapp.sh" --check
    # Emoji pictures and list: regenerated from pinned Twemoji tag and
    # emojibase-data.
    "$root/scripts/fetch-emoji.py" --check
    exit 0
fi

echo "vendor-check: FAIL third_party/qmetaobject is not upstream $version plus qmetaobject.patch:" >&2
diff -r -q -x .cargo-ok "$upstream" "$vendored" >&2 || true
echo "vendor-check: either revert the stray edit, or fold it into third_party/qmetaobject.patch" >&2
exit 1
