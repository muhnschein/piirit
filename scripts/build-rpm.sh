#!/bin/sh
# Build the Piiri RPM for a Sailfish device, using the Sailfish SDK.
#
#     scripts/build-rpm.sh [<arch>] [<sfos-target-version>]
#
# Defaults to aarch64 and whatever SDK target that arch resolves to; pass a
# version to pin one, e.g.:
#
#     scripts/build-rpm.sh aarch64 5.0.0.62
#     scripts/build-rpm.sh armv7hl
#
# Requirements (none of which can be substituted -- see docs/BUILDING.md):
#   * Sailfish SDK with the *Docker* build engine. The VirtualBox build
#     engine cannot compile Rust.
#   * An installed build target for the arch. `sfdk tools list -a` lists
#     what is available.
#
# `sfdk build` runs every SPEC section except %prep, i.e. it builds from
# this working tree in place -- which is why the deltachat-rpc-server
# binaries fetched below (gitignored, not committed) are picked up without
# any tarball/vendoring step.

set -eu

arch=${1:-aarch64}
version=${2:-}

repo_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"

rpc_server="vendor/deltachat-rpc-server/$arch/deltachat-rpc-server"
if [ ! -f "$rpc_server" ]; then
    echo ">> $rpc_server missing; fetching upstream binaries"
    sh scripts/fetch-rpc-server.sh
fi
[ -f "$rpc_server" ] || {
    echo "error: no deltachat-rpc-server for arch '$arch'." >&2
    echo "       Upstream publishes aarch64, armv7hl and x86_64 only." >&2
    exit 1
}

if ! command -v sfdk >/dev/null 2>&1; then
    cat >&2 <<EOF
error: 'sfdk' not found.

The RPM cannot be produced without the Sailfish SDK: the app links against
the target's Qt 5.6 and glibc, so it has to be compiled inside a Sailfish
build target. Install the SDK (Docker build engine), then re-run this
script. See https://docs.sailfishos.org/Tools/Sailfish_SDK/
EOF
    exit 1
fi

if [ -n "$version" ]; then
    target="SailfishOS-$version-$arch"
else
    target=$(sfdk tools list --targets 2>/dev/null \
        | tr -d '\t ' | grep -- "-$arch\$" | tail -1)
    [ -n "$target" ] || {
        echo "error: no installed build target for $arch; run 'sfdk tools list -a'." >&2
        exit 1
    }
fi

echo ">> building for $target"
sfdk -c target="$target" build

echo
echo "RPMs:"
# Plain `find`: -newermt is a GNU extension and this is /bin/sh.
find RPMS -name '*.rpm' 2>/dev/null || echo "  (none under RPMS/)"
