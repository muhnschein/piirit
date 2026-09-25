#!/bin/sh
# Prove third_party/qmetaobject is upstream 0.2.10 plus one known patch,
# and vendor/calls-webapp/index.html is upstream's release asset.
#
# Carrying 7,000 lines of someone else's crate is only safe while everyone
# can see exactly how it differs from the published one. So: fetch the
# crates.io tarball, apply third_party/qmetaobject.patch to it, and require
# the result to match the vendored tree byte for byte. An edit made
# directly to the copy, or a patch that stops describing it, fails here.
#
# The .crate tarball is immutable once published, so this is the same
# comparison every time.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
vendored="$root/third_party/qmetaobject"
patch_file="$root/third_party/qmetaobject.patch"

# The version to compare against is the one cargo is told to replace.
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$vendored/Cargo.toml" | head -1)
[ -n "$version" ] || { echo "vendor-check: FAIL no version in the vendored Cargo.toml" >&2; exit 1; }

if ! command -v curl >/dev/null 2>&1; then
    # A gate that passes without its tool is not a gate. Locally that is
    # a skip; on a runner, where GitHub sets CI, it is a failure.
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

# cargo drops a .cargo-ok marker into its own extraction; it is not in the
# tarball and is left out of the comparison below rather than deleted from
# a tree this script is only meant to read.

# The crate's own tests are not vendored. Cargo never builds a dependency's
# tests, so they are 1,200 lines that cannot run -- and CodeQL scanned them
# and reported seven high-severity findings in code this repository does not
# compile. Dropped from both sides so the comparison stays exact.
rm -rf "$upstream/tests"

if ! patch -s -p1 -d "$upstream" < "$patch_file"; then
    echo "vendor-check: FAIL third_party/qmetaobject.patch does not apply to upstream $version" >&2
    exit 1
fi

if diff -r -q -x .cargo-ok "$upstream" "$vendored" >/dev/null 2>&1; then
    echo "vendor-check: ok (qmetaobject $version + qmetaobject.patch)"
    # The page a call runs in is the other thing committed from upstream:
    # fetched again from the release and compared byte for byte.
    "$root/scripts/fetch-calls-webapp.sh" --check
    exit 0
fi

echo "vendor-check: FAIL third_party/qmetaobject is not upstream $version plus qmetaobject.patch:" >&2
diff -r -q -x .cargo-ok "$upstream" "$vendored" >&2 || true
echo "vendor-check: either revert the stray edit, or fold it into third_party/qmetaobject.patch" >&2
exit 1
