#!/bin/sh
# Builds Piirit's RPM for a Sailfish device with the Sailfish SDK.
#
#     scripts/build-rpm.sh [<arch>] [<sfos-target-version>]
#
# Defaults: aarch64, its SDK target. Pass a version to pin:
#
#     scripts/build-rpm.sh aarch64 5.0.0.62
#     scripts/build-rpm.sh armv7hl
#
# Needs (none substitutable; docs/BUILDING.md):
#   * Sailfish SDK, Docker engine (VirtualBox engine can't compile Rust).
#   * Installed build target for the arch (`sfdk tools list -a`).
#
# `sfdk build` runs every SPEC section but %prep: builds this tree in place,
# so the rpc-server binaries fetched below (gitignored) are used as is.

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
# Plain `find`: -newermt is GNU-only; this is /bin/sh.
find RPMS -name '*.rpm' 2>/dev/null || echo "  (none under RPMS/)"
