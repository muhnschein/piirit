#!/bin/bash
# Derive the slim, pre-provisioned SDK image .github/workflows/rpm.yml
# builds with, from upstream's platform SDK.
#
#     ci/build-sdk-image.sh <sfos-version> <arch> <output-image>
#
# Upstream's image carries three architectures' target rootfs -- 5.0 GB to
# pull, of which one build uses a third -- and leaves this package's
# BuildRequires to be zypper-installed into the target on every build that
# uses it. What comes out of here carries one architecture with those
# packages already in its target and the i686 rustlib where sb2's host-mode
# linker looks for it: about 2 GB, and no package installs on the build's
# critical path. docs/BUILDING.md says what that is worth.
#
# Run by .github/workflows/sdk-image.yml when an SDK version or the spec's
# BuildRequires change, and by rpm.yml itself when the image it wants is
# not published yet.
set -euo pipefail

usage() {
    echo "usage: $0 <sfos-version> <arch> <output-image>" >&2
    exit 2
}

[[ $# -eq 3 ]] || usage
sfos=$1
arch=$2
output=$3

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)

[[ $sfos =~ ^[0-9]+(\.[0-9]+){3}$ ]] ||
    { echo "build-sdk-image: '$sfos' is not a dotted SDK version" >&2; exit 1; }
# aarch64 and nothing else: this project targets the Jolla Phone 2026 and
# no other device (see the README), and the workflows have never built a
# second architecture. Adding one is this list and a dispatch.
case "$arch" in
    aarch64) ;;
    *) echo "build-sdk-image: this project builds aarch64, not '$arch'" >&2; exit 1 ;;
esac

# By digest, not tag. The SDK image is a third party's, it is run
# privileged with a checkout mounted read-write, and a tag can be repointed
# at any content by whoever owns it. To add an SDK version: pull the tag
# once, read the digest with
# `docker inspect --format '{{index .RepoDigests 0}}'`, add a line.
case "$sfos" in
    5.2.0.15) digest=sha256:e7c4379393b17ee63d7f7443ce9930c315a61fdb2e732016346ccbe8c934435e ;;
    *) echo "build-sdk-image: no pinned digest for coderus/sailfishos-platform-sdk:$sfos;" \
            "add one to $0" >&2
       exit 1 ;;
esac

# The digest is what pins the content, so any registry serving that digest
# serves the same bytes -- which is what makes a mirror safe here, and is
# why one is settable at all: Docker Hub rate-limits anonymous pulls.
upstream="${SDK_UPSTREAM_REPO:-coderus/sailfishos-platform-sdk}@$digest"
target="SailfishOS-$sfos-$arch"

echo ">> deriving $output"
echo "   from $upstream"
echo "   target $target"

# Already-present counts: the reference is a digest, so an image that is
# there is the image that was asked for, and a runner that has just built
# this has no reason to fetch five gigabytes to prove it.
if docker image inspect "$upstream" >/dev/null 2>&1; then
    echo ">> $upstream is already here"
else
    docker pull "$upstream"
fi

cid=$(docker run -d --privileged "$upstream" sleep infinity)
cleanup() { docker rm -f "$cid" >/dev/null 2>&1 || true; }
trap cleanup EXIT

# Copied in rather than bind-mounted: a mount leaves its mount point behind
# in the exported filesystem, and the spec is the only thing from this tree
# the bake reads.
docker cp "$root/rpm/harbour-piiri.spec" "$cid:/tmp/harbour-piiri.spec"

# The bake, in two halves, because the two need different users and this
# image grants no passwordless sudo -- inside it `sudo` answers "PAM
# account management error: Authentication service cannot retrieve
# authentication info". mb2 has to run as the image's own mersdk, since
# sdk-manage refuses root saying it cannot determine the Mer SDK user; and
# everything that writes outside that user's home has to be root, which
# `docker exec --user root` grants without asking the image for anything.
echo ">> installing what the spec needs, as the build user"
docker exec -e TARGET="$target" "$cid" bash -euxo pipefail -c '
    # A build directory named for the package: mb2 derives the package it
    # is building from the directory it runs in, and then looks for
    # rpm/<that>.spec.
    mkdir -p ~/harbour-piiri/rpm
    cp /tmp/harbour-piiri.spec ~/harbour-piiri/rpm/
    cd ~/harbour-piiri

    # -X (--no-fix-version) for the reason rpm.yml passes it: without it
    # build-init asks `git describe` for a version, finds no tags, and
    # stops before writing .mb2/spec.
    mb2 -t "$TARGET" -X build-init
    mb2 -t "$TARGET" -X build-requires

    # The image ships with i486 as sb2 s default target, which is one of
    # the ones the root half removes -- and a default naming a target that
    # is not there fails every bare `sb2` call for a reason that has
    # nothing to do with the build. Rewritten in place rather than through
    # `sb2-config -d`, which without a target of its own resolves its log
    # path to / and dies on the way; and rewritten here rather than as
    # root, because sed -i renames a new file into place and root would
    # leave it owned by root.
    sed -i "s|^DEFAULT_TARGET=.*|DEFAULT_TARGET=$TARGET|" "$HOME/.scratchbox2/config"
    grep "^DEFAULT_TARGET=$TARGET$" "$HOME/.scratchbox2/config"

    rm -rf ~/harbour-piiri
'

echo ">> putting the rustlib where the linker looks, and dropping the rest"
docker exec --user root -e TARGET="$target" "$cid" bash -euxo pipefail -c '
    # Build scripts and proc-macros are compiled for the tooling s own
    # i686 and linked in sb2 s host mode, where /usr is the SDK filesystem
    # rather than the target -- so ld looks for the i686 rustlib under
    # /usr/lib/rustlib, and the SDK ships it under /srv/mer. Put it where
    # the linker looks, once, here.
    #
    # Named in preference order rather than globbed: build-requires puts a
    # copy in the snapshot too, and a glob hands back whichever sorts
    # first, which is the snapshot.
    host=i686-unknown-linux-gnu
    src=""
    for candidate in /srv/mer/toolings/*/usr/lib/rustlib/$host \
                     "/srv/mer/targets/$TARGET.default/usr/lib/rustlib/$host" \
                     "/srv/mer/targets/$TARGET/usr/lib/rustlib/$host"; do
        if [ -d "$candidate" ]; then src=$candidate; break; fi
    done
    [ -n "$src" ] || { echo "no host rustlib in this image" >&2; exit 1; }
    mkdir -p /usr/lib/rustlib
    cp -a "$src" /usr/lib/rustlib/

    # Every other architecture, and only those. Each target here comes in
    # two rootfs: the pristine one, and the "<target>.default" snapshot
    # that mb2 actually builds in -- which is where build-requires put
    # everything above, and which build-init does not reset (a file
    # planted in it survives one). Deleting it threw the whole install
    # away and left a target with no rust in it.
    for dir in /srv/mer/targets/*/; do
        name=$(basename "$dir")
        [[ "$name" = "$TARGET" || "$name" = "$TARGET.default" ]] && continue
        rm -rf "$dir" "/home/mersdk/.scratchbox2/$name"
    done

    # The bake s own leavings, and the package cache it filled.
    rm -f /tmp/harbour-piiri.spec
    rm -rf "/srv/mer/targets/$TARGET/var/cache/zypp"/*
'

# `docker export` writes the container's filesystem as it stands, so the
# deletions above are deletions rather than whiteouts over layers that
# still weigh what they weighed. That is the whole reason for the
# export/import rather than a Dockerfile `RUN rm`.
#
# The config a Dockerfile would have carried has to be restored by hand;
# these are upstream's own, read back with `docker image inspect`.
echo ">> flattening"
docker stop "$cid" >/dev/null

# GHCR links a published package to a repository by this label, and a
# package it has not linked is one that repository's own token is not
# granted to pull back -- which would be found out one workflow run later.
source_url=""
if [[ -n "${GITHUB_REPOSITORY:-}" ]]; then
    source_url="${GITHUB_SERVER_URL:-https://github.com}/$GITHUB_REPOSITORY"
elif origin=$(git -C "$root" remote get-url origin 2>/dev/null); then
    source_url=${origin%.git}
fi

changes=(
    --change 'ENV PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin'
    --change 'USER mersdk'
    --change 'WORKDIR /home/mersdk'
)
if [[ -n "$source_url" ]]; then
    changes+=(--change "LABEL org.opencontainers.image.source=$source_url")
fi

docker export "$cid" |
    docker import "${changes[@]}" \
        --message "piiri: $target, BuildRequires installed, one architecture" \
        - "$output" >/dev/null

# A gate, not a report: an image that cannot answer these is one that fails
# minutes into a build instead, with an error about something else.
echo ">> checking what came out"
docker run --rm --privileged -e TARGET="$target" "$output" bash -euo pipefail -c '
    # Read once into a variable and matched from there, rather than piped
    # into `grep -q`: grep stops at its first match, sb2-config dies of
    # SIGPIPE writing the rest, and under `pipefail` that reads as a
    # failed check. The snapshot is the first line listed, so only the
    # check for it ever tripped.
    targets=$(sb2-config -l)
    printf "%s\n" "$targets"

    # This architecture, as its pristine target and its snapshot, and
    # nothing else.
    [ "$(printf "%s\n" "$targets" | grep -c .)" = 2 ] ||
        { echo "the wrong number of targets survived" >&2; exit 1; }
    grep -qx "$TARGET" <<< "$targets"
    grep -qx "$TARGET.default" <<< "$targets"

    # Against the snapshot, which is the rootfs mb2 builds in and the one
    # build-requires installed into: the pristine target beside it has
    # none of this, and asking it was how a broken image first looked
    # fine.
    sb2 -t "$TARGET.default" rpm -q rust cargo gcc-c++ git \
        desktop-file-utils qt5-qttools-linguist
    # The cross std, whose absence is the "can not find crate for std"
    # that arrives a long way into a build. Asked for by package name
    # rather than by the virtual provide the spec names, so the check does
    # not depend on which package carries that provide.
    sb2 -t "$TARGET.default" rpm -qa | grep "^rust-std-static-" ||
        { echo "no cross std in the snapshot" >&2; exit 1; }
    sb2 -t "$TARGET.default" cargo --version
    ls -d /usr/lib/rustlib/i686-unknown-linux-gnu
    grep "^DEFAULT_TARGET=$TARGET$" "$HOME/.scratchbox2/config"

    # And that a build directory still initialises against a target whose
    # pristine snapshot has been removed, which is the next thing rpm.yml
    # does and the one assumption the slimming makes.
    mkdir -p ~/verify/rpm
    printf "Name: verify\nSummary: s\nVersion: 0\nRelease: 1\nLicense: GPL-3.0-or-later\n%%description\ns\n%%build\n%%install\n%%files\n" \
        > ~/verify/rpm/verify.spec
    cd ~/verify && mb2 -t "$TARGET" -X build-init
'

size=$(docker image inspect --format '{{.Size}}' "$output")
echo ">> $output is $((size / 1000 / 1000)) MB unpacked"
