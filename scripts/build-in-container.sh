#!/bin/sh
# Builds device RPM in the Sailfish SDK image releases use, via Podman or
# Docker (no installed SDK).
#
#     scripts/build-in-container.sh [--upstream-sdk] [<sfos-version>]
#
# Version default: 5.2.0.15, the Jolla Phone baseline. Image: the one
# ci/sdk-images pins for that version, by digest (2.3 GB, public), derived by
# ci/build-sdk-image.sh. rpm.yml runs this too: same steps as a release.
#
# --upstream-sdk: don't pull ours; derive it here with ci/build-sdk-image.sh
# from the upstream SDK image (coderus/sailfishos-platform-sdk, digest pinned
# there). Trusts upstream, not us. ~5 GB pull, ~30 min more.
#
# Reproducible: SOURCE_DATE_EPOCH (default: commit time) sets package build
# time and file dates; host name fixed. Same commit, same image: same RPM,
# byte for byte (docs/VERIFYING.md).
#
# Environment, all optional:
#   PIIRIT_ENGINE      docker | podman; default: Docker if running, else Podman
#   PIIRIT_SDK_IMAGE   image to use instead of the pinned one
#   PIIRIT_CARGO_HOME  host dir for crate downloads
#   PIIRIT_CARGO_JOBS  cargo jobs in scratchbox2; 1, see rpm/harbour-piirit.spec
#
# Rootless Podman: no root anywhere, tree stays yours. Docker: tree chowned to
# the image's user for the build, back after (root in container, no host sudo).
#
# aarch64 only: project target; only arch the image carries.

set -eu

prog=build-in-container
upstream_sdk=0
if [ "${1:-}" = --upstream-sdk ]; then
    upstream_sdk=1
    shift
fi
sfos=${1:-5.2.0.15}
arch=aarch64
cargo_home=${PIIRIT_CARGO_HOME:-}
jobs=${PIIRIT_CARGO_JOBS:-1}

case "$sfos" in
    *[!0-9.]* | '') echo "$prog: '$sfos' is not a dotted SDK version" >&2; exit 2 ;;
    *) ;;
esac

repo_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"

# shellcheck source=scripts/container-engine.sh
. "$repo_root/scripts/container-engine.sh"
pick_engine
echo ">> using $engine$([ "$rootless" = 1 ] && echo ' (rootless)')"

if [ "$upstream_sdk" = 1 ]; then
    [ -z "${PIIRIT_SDK_IMAGE:-}" ] || {
        echo "$prog: --upstream-sdk derives the image; PIIRIT_SDK_IMAGE names one. Pick one." >&2
        exit 2
    }
    image="localhost/piirit-sdk:$sfos-$arch-upstream"
else
    image=${PIIRIT_SDK_IMAGE:-$(awk -v v="$sfos" '$1 == v { print $2; exit }' ci/sdk-images)}
    [ -n "$image" ] || {
        echo "$prog: no image pinned for $sfos in ci/sdk-images;" \
             "use --upstream-sdk, or set PIIRIT_SDK_IMAGE" >&2
        exit 1
    }
fi

# Checked before the pull, not minutes in (build.rs says the same).
if [ ! -f third_party/ffmpeg/configure ] || [ ! -f third_party/x264/x264.h ]; then
    echo ">> fetching the FFmpeg and x264 submodules"
    git submodule update --init --depth 1
fi
if [ ! -f "vendor/deltachat-rpc-server/$arch/deltachat-rpc-server" ]; then
    echo ">> fetching deltachat-rpc-server"
    sh scripts/fetch-rpc-server.sh
fi
# Every build, not just the fetch: a binary swapped since would ship.
sh scripts/fetch-rpc-server.sh --check

# Whatever git doesn't have still gets packaged (%install globs qml/ and
# translations/), and the result won't match a release. Warn, don't refuse.
if [ -z "${SOURCE_DATE_EPOCH:-}" ]; then
    SOURCE_DATE_EPOCH=$(git log -1 --format=%ct 2>/dev/null || true)
fi
if [ -z "$SOURCE_DATE_EPOCH" ]; then
    echo "$prog: warning: not a git checkout and no SOURCE_DATE_EPOCH;" \
         "package dates will be today's, so it won't match a release" >&2
else
    dirty=$(git status --porcelain --ignore-submodules=dirty 2>/dev/null || true)
    if [ -n "$dirty" ]; then
        echo "$prog: warning: these changes aren't committed; they go into the" \
             "package, so it won't match a release:" >&2
        echo "$dirty" | head -10 >&2
    fi
fi

if [ "$upstream_sdk" = 1 ]; then
    if "$engine" image inspect "$image" >/dev/null 2>&1; then
        echo ">> $image already derived here"
    else
        PIIRIT_ENGINE=$engine bash ci/build-sdk-image.sh "$sfos" "$arch" "$image"
    fi
else
    ctr_pull "$image"
fi

# Image's one target, named by SDK convention. Asked for, not assumed: a
# differently named image fails here, listing what it has.
targets=$(ctr_run "$image" sb2-config -l 2>/dev/null || true)
target=$(echo "$targets" | grep -- "-$arch\$" | tail -1 || true)
[ -n "$target" ] || {
    echo "$prog: no $arch target in $image; it has:" >&2
    echo "$targets" >&2
    exit 1
}

# Runs as image's user: sdk-manage refuses root ("cannot determine the Mer SDK
# user"). uid read from image, not assumed.
uid=$(ctr_run "$image" id -u)
gid=$(ctr_run "$image" id -g)
# shellcheck disable=SC2016 # the image's $HOME, not ours.
home=$(ctr_run "$image" sh -c 'echo $HOME')

# In SDK user's home, named for the spec. scratchbox2 redirects unknown absolute
# paths into the target rootfs, so at /build mb2's .mb2/spec is unopenable
# (exists, yet "cannot open": two roots). mb2 takes the package name from the
# dir: needs rpm/<name>.spec; ci/packaging-lint.sh checks it.
builddir="$home/harbour-piirit"

if [ -n "$cargo_home" ]; then
    mkdir -p "$cargo_home"
fi

# Rootless Podman: keep-id maps us onto the image's user, so the mounted tree
# is already theirs. Else chown there and back.
keep_id=0
[ "$engine" = podman ] && [ "$rootless" = 1 ] && keep_id=1

# Every run mounts the tree, plus crate cache if set.
in_image() {
    if [ "$keep_id" = 1 ]; then
        set -- --userns=keep-id:uid="$uid",gid="$gid" "$@"
    fi
    if [ -n "$cargo_home" ]; then
        ctr_run -v "$repo_root:$builddir" -v "$cargo_home:$home/.cargo" "$@"
    else
        ctr_run -v "$repo_root:$builddir" "$@"
    fi
}
hand_to() {
    # Plain assignment: POSIX sh has no `local`.
    owner=$1
    in_image --user root "$image" \
        chown -R "$owner" "$builddir" ${cargo_home:+"$home/.cargo"}
}
# Also on failure, or the host user can't read or delete build output. Rootless
# Docker: container root is us.
give_back() {
    if [ "$rootless" = 1 ]; then hand_to 0:0 || true
    else hand_to "$(id -u):$(id -g)" || true
    fi
}
if [ "$keep_id" = 0 ]; then
    trap give_back EXIT
    hand_to "$uid:$gid"
fi

# build-init before build; -X (no-fix-version) on both: version from spec, not
# git. Without it build-init may not write .mb2/spec. build-requires is a no-op
# on the derived image; kept so new BuildRequires fail here. --privileged: as
# SDK's own instructions. Fixed host name: rpm records it (spec pins
# %_buildhost too).
echo ">> building in $image, target $target"
# shellcheck disable=SC2016 # expanded by the container's shell.
in_image --privileged --hostname piirit-build -w "$builddir" \
    -e TARGET="$target" -e JOBS="$jobs" \
    ${SOURCE_DATE_EPOCH:+-e SOURCE_DATE_EPOCH="$SOURCE_DATE_EPOCH"} "$image" \
    /bin/bash -c 'set -e
        mb2 -t "$TARGET" -X build-init
        mb2 -t "$TARGET" -X build-requires ||
            echo "build-in-container: mb2 build-requires failed; the SDK image may be behind the spec" >&2
        mb2 -t "$TARGET" -X build --no-check --define "jobs $JOBS"'

if [ "$keep_id" = 0 ]; then
    give_back
    trap - EXIT
fi

echo
echo "RPMs:"
find RPMS -name '*.rpm' 2>/dev/null || echo "  (none under RPMS/)"
