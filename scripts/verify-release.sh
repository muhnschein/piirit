#!/bin/sh
# Rebuilds a release from its source and checks it against the published RPM.
#
#     scripts/verify-release.sh [--upstream-sdk] [--rpm <file>] [--ref <ref>] [<version>]
#
#   <version>       release to check, e.g. 2.2.0; default: latest
#   --rpm <file>    check this file, not the download (e.g. one copied off a
#                   phone); version from its name unless given
#   --ref <ref>     build this tag, branch or commit, not v<version> (rpm.yml
#                   checks a release before it is tagged)
#   --upstream-sdk  derive the SDK image from upstream's, don't pull ours
#                   (scripts/build-in-container.sh)
#
# Needs git, curl, sha256sum, Podman or Docker. First run: ~2.3 GB pull, ~30
# min build. Builds in a fresh clone under $TMPDIR, never in this tree.
#
# Exit: 0 same file, byte for byte; 1 same files inside, different packaging
# (releases before reproducible builds); 2 files differ; 3 couldn't check.
#
# Environment, all optional:
#   PIIRIT_REPO  where to clone from; default GitHub
#   PIIRIT_KEEP  1: keep the clone and build
#   build-in-container.sh's PIIRIT_* pass through.

set -eu

prog=verify-release
github=muhnschein/piirit
repo=${PIIRIT_REPO:-https://github.com/$github.git}
# Built before ci/sdk-images existed: all such releases used this image.
legacy_image=ghcr.io/muhnschein/piirit-sdk@sha256:842ff7b9ffefb95eabbd31fe1e64cf08e3cfddc4c39ab9cce2184d66653579e8

usage() {
    sed -n '4,13p' "$0" | sed 's/^# \{0,1\}//' >&2
    exit 3
}

upstream_sdk=""
rpm=""
ref=""
version=""
while [ $# -gt 0 ]; do
    case "$1" in
        --upstream-sdk) upstream_sdk=--upstream-sdk ;;
        --rpm) [ $# -ge 2 ] || usage; rpm=$2; shift ;;
        --ref) [ $# -ge 2 ] || usage; ref=$2; shift ;;
        -h | --help) usage ;;
        -*) echo "$prog: unknown option $1" >&2; usage ;;
        *) [ -z "$version" ] || usage; version=$1 ;;
    esac
    shift
done

here=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
# shellcheck source=scripts/container-engine.sh
. "$here/scripts/container-engine.sh"
pick_engine

fail() { echo "$prog: $*" >&2; exit 3; }

work=$(mktemp -d "${TMPDIR:-/tmp}/piirit-verify.XXXXXX")
# shellcheck disable=SC2317,SC2329 # run by the trap.
cleanup() {
    if [ "${PIIRIT_KEEP:-0}" = 1 ]; then
        echo "kept: $work"
    else
        rm -rf "$work" 2>/dev/null || echo "$prog: could not remove all of $work" >&2
    fi
}
trap cleanup EXIT
mkdir -p "$work/cmp"

# What to check against.
if [ -n "$rpm" ]; then
    [ -f "$rpm" ] || fail "no such file: $rpm"
    cp "$rpm" "$work/cmp/published.rpm"
    if [ -z "$version" ]; then
        version=$(basename "$rpm" | sed -n 's/^harbour-piirit-\(.*\)-[^-]*\.aarch64\.rpm$/\1/p')
        [ -n "$version" ] || [ -n "$ref" ] ||
            fail "can't read a version from '$(basename "$rpm")'; give one"
    fi
    echo ">> checking $rpm"
else
    if [ -z "$version" ]; then
        version=$(curl -sSf "https://api.github.com/repos/$github/releases/latest" |
            sed -n 's/.*"tag_name": *"v\([^"]*\)".*/\1/p' | head -1) ||
            fail "can't ask GitHub for the latest release"
        [ -n "$version" ] || fail "can't find the latest release"
    fi
    url="https://github.com/$github/releases/download/v$version"
    echo ">> downloading release $version"
    curl -sSfL -o "$work/SHA256SUMS" "$url/SHA256SUMS" || fail "no release v$version"
    name=$(awk '/\.rpm$/ { sub(/^\*/, "", $2); print $2; exit }' "$work/SHA256SUMS")
    [ -n "$name" ] || fail "release v$version's SHA256SUMS names no RPM"
    curl -sSfL -o "$work/$name" "$url/$name" || fail "can't download $name"
    (cd "$work" && grep " \*\{0,1\}$name\$" SHA256SUMS | sha256sum -c -) >/dev/null ||
        fail "$name doesn't match the release's SHA256SUMS; download damaged?"
    cp "$work/$name" "$work/cmp/published.rpm"
fi
ref=${ref:-v$version}

# Fresh clone: nothing uncommitted from anywhere gets in.
echo ">> cloning $ref"
src="$work/harbour-piirit"
if printf '%s\n' "$ref" | grep -Eq '^[0-9a-f]{7,40}$'; then
    git clone --quiet "$repo" "$src"
    git -C "$src" checkout --quiet --detach "$ref"
else
    git clone --quiet --depth 1 --branch "$ref" "$repo" "$src" 2>/dev/null ||
        git clone --quiet --branch "$ref" "$repo" "$src" ||
        fail "can't clone $ref from $repo"
fi

# The source's own recipe. Releases from before it had one: this tree's,
# against the image they were built in.
if [ ! -f "$src/scripts/build-in-container.sh" ]; then
    echo ">> $ref predates reproducible releases: expect same files inside," \
         "different packaging"
    cp "$here/scripts/build-in-container.sh" "$here/scripts/container-engine.sh" "$src/scripts/"
    cp "$here/ci/build-sdk-image.sh" "$src/ci/"
    [ -n "$upstream_sdk" ] || [ -f "$src/ci/sdk-images" ] ||
        PIIRIT_SDK_IMAGE=${PIIRIT_SDK_IMAGE:-$legacy_image}
    export PIIRIT_SDK_IMAGE
fi

# Commit time, from the clone: never the caller's.
unset SOURCE_DATE_EPOCH
# shellcheck disable=SC2086 # $upstream_sdk: one flag or none.
sh "$src/scripts/build-in-container.sh" $upstream_sdk ||
    fail "the build failed"

built=$(find "$src/RPMS" -name 'harbour-piirit-*.rpm' ! -name '*debug*' ! -name '*.src.rpm' | head -1)
[ -n "$built" ] || fail "the build made no device RPM"
cp "$built" "$work/cmp/rebuilt.rpm"

theirs=$(sha256sum < "$work/cmp/published.rpm" | cut -d' ' -f1)
ours=$(sha256sum < "$work/cmp/rebuilt.rpm" | cut -d' ' -f1)
echo
echo "published  $theirs"
echo "rebuilt    $ours"
if [ "$theirs" = "$ours" ]; then
    echo
    echo "VERIFIED: $ref rebuilds to the same file, byte for byte."
    exit 0
fi

# Unpacked in the SDK image (rpm2cpio, cpio there, maybe not here). As root:
# rootless, that's us; reads our files whatever the image's user.
if [ -n "$upstream_sdk" ]; then
    image="localhost/piirit-sdk:5.2.0.15-aarch64-upstream"
else
    image=${PIIRIT_SDK_IMAGE:-$(awk '$1 == "5.2.0.15" { print $2; exit }' "$src/ci/sdk-images" 2>/dev/null)}
    image=${image:-$legacy_image}
fi
chmod 755 "$work/cmp"
chmod 644 "$work/cmp"/*.rpm
echo
echo ">> files differ; comparing what's inside"
rc=0
# shellcheck disable=SC2016 # expanded by the container's shell.
ctr_run --user 0 -v "$work/cmp:/cmp:ro" "$image" sh -c '
    set -e
    for s in published rebuilt; do
        mkdir -p "/tmp/$s"
        (cd "/tmp/$s" && rpm2cpio "/cmp/$s.rpm" | cpio -idm --quiet)
        (cd "/tmp/$s" && find . -type f -exec sha256sum {} + | sort -k2) > "/tmp/$s.sums"
        rpm -qp --qf "[%{FILEMODES:perms} %{FILEUSERNAME}:%{FILEGROUPNAME} %{FILENAMES} %{FILELINKTOS}\n]" \
            "/cmp/$s.rpm" | sort -k3 > "/tmp/$s.meta"
    done
    echo "$(wc -l < /tmp/published.sums) files"
    diff /tmp/published.sums /tmp/rebuilt.sums > /tmp/d1 || true
    diff /tmp/published.meta /tmp/rebuilt.meta > /tmp/d2 || true
    if [ -s /tmp/d1 ] || [ -s /tmp/d2 ]; then
        echo "differing (< published, > rebuilt):"
        cat /tmp/d1 /tmp/d2 | grep "^[<>]" | head -50
        exit 10
    fi' || rc=$?
case "$rc" in
    0) ;;
    10) echo; echo "DIFFERENT: $ref does not rebuild to what was checked. Please report it."; exit 2 ;;
    *) fail "couldn't unpack the two RPMs to compare them" ;;
esac
echo
echo "SAME FILES: every file, mode and owner inside matches; packaging"
echo "differs (build time, file dates, host or signature). Expected for"
echo "releases before reproducible builds; for a later one, a bug: please report."
exit 1
