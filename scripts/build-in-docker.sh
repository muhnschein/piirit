#!/bin/sh
# Build the device RPM in the Sailfish SDK image releases are built in,
# with Docker rather than an installed SDK. fetch-rpc-server.sh, which
# runs first when the bundled core is missing, wants pip and unzip.
#
#     scripts/build-in-docker.sh [<sfos-version>]
#
# The version defaults to 5.2.0.15, the Jolla Phone's baseline. The image
# is the one ci/build-sdk-image.sh derives and .github/workflows/rpm.yml
# publishes, about 2.3 GB to pull, and it is public. rpm.yml builds through
# this script too, so a build here is the same steps as a release.
#
# Environment, all optional:
#   PIIRIT_SDK_IMAGE   the image to build in, rather than the published one
#   PIIRIT_CARGO_HOME  a host directory to keep crate downloads in
#   PIIRIT_CARGO_JOBS  cargo jobs inside scratchbox2; 1, for the reason in
#                      rpm/harbour-piirit.spec
#
# aarch64 only: it is what this project targets and the only architecture
# the image carries.

set -eu

sfos=${1:-5.2.0.15}
arch=aarch64
image=${PIIRIT_SDK_IMAGE:-ghcr.io/muhnschein/piirit-sdk:$sfos-$arch}
cargo_home=${PIIRIT_CARGO_HOME:-}
jobs=${PIIRIT_CARGO_JOBS:-1}

case "$sfos" in
    *[!0-9.]* | '') echo "build-in-docker: '$sfos' is not a dotted SDK version" >&2; exit 2 ;;
esac

repo_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"

command -v docker >/dev/null 2>&1 || {
    echo "build-in-docker: needs Docker; with the Sailfish SDK, use scripts/build-rpm.sh" >&2
    exit 1
}

# Before the pull rather than minutes into the build, where build.rs would
# say the same thing.
if [ ! -f third_party/ffmpeg/configure ] || [ ! -f third_party/x264/x264.h ]; then
    echo ">> fetching the FFmpeg and x264 submodules"
    git submodule update --init --depth 1
fi
if [ ! -f "vendor/deltachat-rpc-server/$arch/deltachat-rpc-server" ]; then
    echo ">> fetching deltachat-rpc-server"
    sh scripts/fetch-rpc-server.sh
fi

docker image inspect "$image" >/dev/null 2>&1 || docker pull "$image"

# The image ships one target, named by the SDK's own convention. Asked for
# rather than assumed, so an image whose naming differs fails here with
# the list rather than deep inside rpmbuild.
targets=$(docker run --rm "$image" sb2-config -l 2>/dev/null || true)
target=$(echo "$targets" | grep -- "-$arch\$" | tail -1 || true)
[ -n "$target" ] || {
    echo "build-in-docker: no $arch target in $image; it has:" >&2
    echo "$targets" >&2
    exit 1
}

# The build runs as the image's own user: sdk-manage refuses root, saying
# it cannot determine the Mer SDK user. So the tree belongs to that user
# while it builds and is handed back afterwards, both by root inside the
# container, which needs no sudo on the host. The uid is read from the
# image rather than assumed.
uid=$(docker run --rm "$image" id -u)
gid=$(docker run --rm "$image" id -g)
# shellcheck disable=SC2016 # the image's $HOME, not ours.
home=$(docker run --rm "$image" sh -c 'echo $HOME')

# Inside the SDK user's home, and named for the spec. rpm runs under
# scratchbox2, which redirects absolute paths it does not recognise into
# the target rootfs: with the tree at /build, mb2 writes .mb2/spec and rpm
# then says it cannot open it -- a file that exists and cannot be opened is
# two different roots. mb2 derives the package from the directory name and
# looks for rpm/<that>.spec. ci/packaging-lint.sh holds the name to the
# spec's.
builddir="$home/harbour-piirit"

if [ -n "$cargo_home" ]; then
    mkdir -p "$cargo_home"
fi

# Every run mounts the tree, and the crate cache when there is one.
in_image() {
    if [ -n "$cargo_home" ]; then
        docker run --rm -v "$repo_root:$builddir" -v "$cargo_home:$home/.cargo" "$@"
    else
        docker run --rm -v "$repo_root:$builddir" "$@"
    fi
}
hand_to() {
    in_image --user root "$image" \
        chown -R "$1" "$builddir" ${cargo_home:+"$home/.cargo"}
}
# Handed back on failure too, or the host user cannot read or delete what
# the build left.
give_back() { hand_to "$(id -u):$(id -g)" || true; }
trap give_back EXIT
hand_to "$uid:$gid"

# `build-init` before `build`, and `-X` (--no-fix-version) on both, so
# the version is the spec's rather than one derived from git; without it
# build-init can stop before writing .mb2/spec at all. `build-requires` has nothing to do
# against the derived image, which carries them already; it stays so that
# a BuildRequires added before the image is rebuilt fails here, clearly.
# `--privileged` is what the SDK image's own instructions run it with.
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
