#!/bin/sh
# Builds device RPM in the Sailfish SDK image releases use, via Docker (no
# installed SDK). fetch-rpc-server.sh (runs first if core missing) needs pip, unzip.
#
#     scripts/build-in-docker.sh [<sfos-version>]
#
# Version default: 5.2.0.15, the Jolla Phone baseline. Image: from
# ci/build-sdk-image.sh, published by rpm.yml (2.3 GB, public). rpm.yml runs
# this too: same steps as a release.
#
# Environment, all optional:
#   PIIRIT_SDK_IMAGE   image to use, not the published one
#   PIIRIT_CARGO_HOME  host dir for crate downloads
#   PIIRIT_CARGO_JOBS  cargo jobs in scratchbox2; 1, see rpm/harbour-piirit.spec
#
# aarch64 only: project target; only arch the image carries.

set -eu

sfos=${1:-5.2.0.15}
arch=aarch64
image=${PIIRIT_SDK_IMAGE:-ghcr.io/muhnschein/piirit-sdk:$sfos-$arch}
cargo_home=${PIIRIT_CARGO_HOME:-}
jobs=${PIIRIT_CARGO_JOBS:-1}

case "$sfos" in
    *[!0-9.]* | '') echo "build-in-docker: '$sfos' is not a dotted SDK version" >&2; exit 2 ;;
    *) ;;
esac

repo_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"

command -v docker >/dev/null 2>&1 || {
    echo "build-in-docker: needs Docker; with the Sailfish SDK, use scripts/build-rpm.sh" >&2
    exit 1
}

# Checked before the pull, not minutes in (build.rs says the same).
if [ ! -f third_party/ffmpeg/configure ] || [ ! -f third_party/x264/x264.h ]; then
    echo ">> fetching the FFmpeg and x264 submodules"
    git submodule update --init --depth 1
fi
if [ ! -f "vendor/deltachat-rpc-server/$arch/deltachat-rpc-server" ]; then
    echo ">> fetching deltachat-rpc-server"
    sh scripts/fetch-rpc-server.sh
fi

docker image inspect "$image" >/dev/null 2>&1 || docker pull "$image"

# Image's one target, named by SDK convention. Asked for, not assumed: a
# differently named image fails here, listing what it has.
targets=$(docker run --rm "$image" sb2-config -l 2>/dev/null || true)
target=$(echo "$targets" | grep -- "-$arch\$" | tail -1 || true)
[ -n "$target" ] || {
    echo "build-in-docker: no $arch target in $image; it has:" >&2
    echo "$targets" >&2
    exit 1
}

# Runs as image's user: sdk-manage refuses root ("cannot determine the Mer SDK
# user"). Tree owned by that user while building, handed back after (root in
# container; no host sudo). uid read from image, not assumed.
uid=$(docker run --rm "$image" id -u)
gid=$(docker run --rm "$image" id -g)
# shellcheck disable=SC2016 # the image's $HOME, not ours.
home=$(docker run --rm "$image" sh -c 'echo $HOME')

# In SDK user's home, named for the spec. scratchbox2 redirects unknown absolute
# paths into the target rootfs, so at /build mb2's .mb2/spec is unopenable
# (exists, yet "cannot open": two roots). mb2 takes the package name from the
# dir: needs rpm/<name>.spec; ci/packaging-lint.sh checks it.
builddir="$home/harbour-piirit"

if [ -n "$cargo_home" ]; then
    mkdir -p "$cargo_home"
fi

# Every run mounts the tree, plus crate cache if set.
in_image() {
    if [ -n "$cargo_home" ]; then
        docker run --rm -v "$repo_root:$builddir" -v "$cargo_home:$home/.cargo" "$@"
    else
        docker run --rm -v "$repo_root:$builddir" "$@"
    fi
}
hand_to() {
    # Plain assignment: POSIX sh has no `local`.
    owner=$1
    in_image --user root "$image" \
        chown -R "$owner" "$builddir" ${cargo_home:+"$home/.cargo"}
}
# Also on failure, or the host user can't read or delete build output.
give_back() { hand_to "$(id -u):$(id -g)" || true; }
trap give_back EXIT
hand_to "$uid:$gid"

# build-init before build; -X (no-fix-version) on both: version from spec, not
# git. Without it build-init may not write .mb2/spec. build-requires is a no-op
# on the derived image; kept so new BuildRequires fail here. --privileged: as
# SDK's own instructions.
echo ">> building in $image, target $target"
# shellcheck disable=SC2016 # expanded by the container's shell.
in_image --privileged -w "$builddir" \
    -e TARGET="$target" -e JOBS="$jobs" "$image" \
    /bin/bash -c 'set -e
        mb2 -t "$TARGET" -X build-init
        mb2 -t "$TARGET" -X build-requires ||
            echo "build-in-docker: mb2 build-requires failed; the SDK image may be behind the spec" >&2
        mb2 -t "$TARGET" -X build --no-check --define "jobs $JOBS"'

give_back
trap - EXIT

echo
echo "RPMs:"
find RPMS -name '*.rpm' 2>/dev/null || echo "  (none under RPMS/)"
