#!/bin/sh
# Fetches the page a call runs in: upstream calls-webapp's built .html from its
# GitHub releases (the page every Delta Chat client runs). See
# vendor/calls-webapp/SOURCE.md.
#
# Page is committed, unlike core binaries: 44 KB, compiled into app
# (src/call_host.rs); builds must not need network. This script changes it and
# proves the copy matches upstream.
#
#     scripts/fetch-calls-webapp.sh           fetch, verify, install
#     scripts/fetch-calls-webapp.sh --check   fetch, verify, and require
#                                             the committed copy to match
#
# Pinned: bump VERSION and SHA256 together, then SOURCE.md. Asset is upstream's
# CI build of the tag, reproducible (`pnpm install --frozen-lockfile && pnpm
# build` at the tag gives same bytes), so anyone can rebuild the checksum.

set -eu

VERSION="v0.12.1"
SHA256="e4a94065a848be0b52f75c9ad53fd5a674e4e66cedc1d75c8a92baf74681e951"

repo_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
target="$repo_root/vendor/calls-webapp/index.html"
url="https://github.com/deltachat/calls-webapp/releases/download/$VERSION/index.html"

check=0
if [ "${1:-}" = "--check" ]; then
    check=1
fi

if ! command -v curl >/dev/null 2>&1; then
    # A check passing without its tool is no check: skip locally, fail on CI
    # (GitHub sets CI).
    if [ "$check" = 1 ] && [ -z "${CI:-}" ]; then
        echo "fetch-calls-webapp: SKIP curl not found"
        exit 0
    fi
    echo "fetch-calls-webapp: FAIL curl not found" >&2
    exit 1
fi

workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT

# -L follows redirect to GitHub's store; both --proto flags keep hops on HTTPS.
if ! curl -sSfL --proto '=https' --proto-redir '=https' "$url" -o "$workdir/index.html"; then
    echo "fetch-calls-webapp: FAIL could not fetch $url" >&2
    exit 1
fi

echo "$SHA256  $workdir/index.html" | sha256sum -c - >/dev/null || {
    echo "fetch-calls-webapp: FAIL checksum mismatch for calls-webapp $VERSION; refusing it" >&2
    exit 1
}

if [ "$check" = 1 ]; then
    if ! cmp -s "$workdir/index.html" "$target"; then
        echo "fetch-calls-webapp: FAIL vendor/calls-webapp/index.html is not calls-webapp $VERSION" >&2
        exit 1
    fi
    echo "fetch-calls-webapp: OK vendor/calls-webapp/index.html is calls-webapp $VERSION"
    exit 0
fi

install -Dm 644 "$workdir/index.html" "$target"
echo "Done. vendor/calls-webapp/index.html is calls-webapp $VERSION."
echo "Remember: vendor/calls-webapp/SOURCE.md must describe this exact version."
