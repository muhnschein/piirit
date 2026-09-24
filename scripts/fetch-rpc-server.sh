#!/bin/sh
# Fetch upstream deltachat-rpc-server binaries for the architectures
# Piirit packages, and place them where rpm/harbour-piirit.spec expects
#
#     vendor/deltachat-rpc-server/<sailfish-arch>/deltachat-rpc-server
#
# Upstream (chatmail/core) builds these statically against musl libc
# ("to avoid problems with glibc version incompatibility", per their CI
# workflow), so a binary depends only on the kernel -- this is why a
# generic upstream build is expected to run on Sailfish unmodified.
#
# Source: the exact same binaries upstream attaches to its GitHub release
# are also published inside its PyPI wheels (see
# .github/workflows/deltachat-rpc-server.yml upstream: both are the nix
# build's `result/bin/deltachat-rpc-server`). PyPI is used here because
# it's fetchable with plain HTTPS and the wheel is just a zip.
#
# Everything is pinned: bump VERSION *and* refresh all checksums together
# (fetch the new wheels, run `sha256sum`, update below), then update
# vendor/deltachat-rpc-server/SOURCE.md to match -- the MPL-2.0
# source-availability notice must always describe the binaries actually
# bundled (see vendor/deltachat-rpc-server/SOURCE.md).

set -eu

VERSION="2.62.0"

# sailfish-arch  upstream-arch  wheel-tag                                                                     wheel-sha256                                                       binary-sha256
TABLE="
aarch64 aarch64 py3-none-manylinux_2_17_aarch64.manylinux2014_aarch64.musllinux_1_1_aarch64 c008ef4ec3bf4bdb1f84d88939f682c01959ddcb270b1095971c3bbd85d2c48e a81a75e2de356c5b0074100aee655a7a1884dcc747c79e2b57e7b6f4316a70ee
armv7hl armv7l py3-none-linux_armv7l.manylinux_2_17_armv7l.manylinux2014_armv7l.musllinux_1_1_armv7l 804b2e30e515a9d3987e4d8a9065b831bc066367b7e1883c2cf4ddcf572d8d2b ec968b307d0e82ba9012e2d194dcdab123ce4c1289b8c756b715e8b27e176973
x86_64 x86_64 py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64.musllinux_1_1_x86_64 f4c4c2a7f5daa5343785b46a287c9938e6787552e3755302843fa273af6f643a 62aa375904aeea88f356a20cb5d7bb6b90728f5e14d81c2b0f689c6c7341af2d
"

repo_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
vendor_dir="$repo_root/vendor/deltachat-rpc-server"
workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT

fetch_one() {
    sfos_arch="$1"; upstream_arch="$2"; wheel_tag="$3"; wheel_sha="$4"; bin_sha="$5"

    wheel="deltachat_rpc_server-$VERSION-$wheel_tag.whl"

    echo ">> $sfos_arch (upstream $upstream_arch)"
    # pip resolves PyPI's hash-addressed file URL; --no-deps and the exact
    # version pin keep it honest, and the sha256 check below keeps it
    # honest even if the index were tampered with. --isolated and the
    # explicit index: a PIP_INDEX_URL or pip.conf on the machine running
    # this would otherwise point it at a mirror without anyone knowing --
    # the hashes would still catch a substitution, but as a puzzling
    # failure rather than a named one.
    pip download "deltachat-rpc-server==$VERSION" --no-deps \
        --isolated --index-url https://pypi.org/simple --no-cache-dir \
        --platform "musllinux_1_1_$upstream_arch" --only-binary=:all: \
        -d "$workdir" >/dev/null

    echo "$wheel_sha  $workdir/$wheel" | sha256sum -c - >/dev/null || {
        echo "fetch-rpc-server: checksum mismatch for $wheel; refusing to install it" >&2
        exit 1
    }

    unzip -oq "$workdir/$wheel" -d "$workdir/$sfos_arch"
    bin="$workdir/$sfos_arch/deltachat_rpc_server/deltachat-rpc-server"
    echo "$bin_sha  $bin" | sha256sum -c - >/dev/null || {
        echo "fetch-rpc-server: checksum mismatch for the $sfos_arch binary inside $wheel; refusing to install it" >&2
        exit 1
    }

    install -Dm 755 "$bin" "$vendor_dir/$sfos_arch/deltachat-rpc-server"
    echo "   -> vendor/deltachat-rpc-server/$sfos_arch/deltachat-rpc-server"
}

echo "$TABLE" | while read -r sfos_arch upstream_arch wheel_tag wheel_sha bin_sha; do
    [ -n "$sfos_arch" ] || continue
    fetch_one "$sfos_arch" "$upstream_arch" "$wheel_tag" "$wheel_sha" "$bin_sha"
done

echo "Done. Binaries are deltachat-rpc-server v$VERSION, statically linked (musl)."
echo "Remember: vendor/deltachat-rpc-server/SOURCE.md must describe this exact version."
