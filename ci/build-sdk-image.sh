#!/bin/bash
# Derive slim pre-provisioned SDK image for rpm.yml from upstream's SDK.
#
#     ci/build-sdk-image.sh <sfos-version> <arch> <output-image>
#
# Upstream image: 3 arches, 5.0 GB. Output: one arch, BuildRequires
# preinstalled, i686 rustlib where sb2's host linker looks. ~2 GB, no installs
# on build's critical path.
#
# Run by sdk-image.yml (SDK version or spec BuildRequires change), by rpm.yml
# for an SDK version ci/sdk-images doesn't pin, and by
# `build-in-container.sh --upstream-sdk` for anyone not trusting our published
# image. Podman or Docker (PIIRIT_ENGINE picks; scripts/container-engine.sh).
#
# Same upstream digest + this script: same build environment. Image digest
# differs (export timestamps); packages built in it don't.
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

prog=build-sdk-image
# shellcheck source=scripts/container-engine.sh
. "$root/scripts/container-engine.sh"
pick_engine

[[ $sfos =~ ^[0-9]+(\.[0-9]+){3}$ ]] ||
    { echo "build-sdk-image: '$sfos' is not a dotted SDK version" >&2; exit 1; }
# aarch64 only: the sole target (Jolla Phone 2026). Other arch: edit list.
case "$arch" in
    aarch64) ;;
    *) echo "build-sdk-image: this project builds aarch64, not '$arch'" >&2; exit 1 ;;
esac

# Pinned by digest (third-party, run privileged with rw checkout, tags
# repointable). To add an SDK: pull tag once, read digest with
# `docker inspect --format '{{index .RepoDigests 0}}'`, add a line.
case "$sfos" in
    5.2.0.15) digest=sha256:e7c4379393b17ee63d7f7443ce9930c315a61fdb2e732016346ccbe8c934435e ;;
    *) echo "build-sdk-image: no pinned digest for coderus/sailfishos-platform-sdk:$sfos;" \
            "add one to $0" >&2
       exit 1 ;;
esac

# Digest pins content, so any mirror is safe (Docker Hub rate-limits).
# Fully qualified: Podman refuses short names without a search registry.
upstream="${SDK_UPSTREAM_REPO:-docker.io/coderus/sailfishos-platform-sdk}@$digest"
target="SailfishOS-$sfos-$arch"

echo ">> deriving $output"
echo "   from $upstream"
echo "   target $target"

# Digest present = right image: skip fetch.
if "$engine" image inspect "$upstream" >/dev/null 2>&1; then
    echo ">> $upstream is already here"
else
    "$engine" pull "$upstream"
fi

cid=$("$engine" run -d --privileged "$upstream" sleep infinity)
cleanup() { "$engine" rm -f "$cid" >/dev/null 2>&1 || true; }
trap cleanup EXIT

# Copied, not bind-mounted: mount point would stay in export.
"$engine" cp "$root/rpm/harbour-piirit.spec" "$cid:/tmp/harbour-piirit.spec"

# Two halves, two users: image has no passwordless sudo. mb2 runs as mersdk
# (sdk-manage refuses root); writes outside its home via
# `exec --user root`.
echo ">> installing what the spec needs, as the build user"
# shellcheck disable=SC2016 # expanded by the container's shell.
"$engine" exec -e TARGET="$target" "$cid" bash -euxo pipefail -c '
    # Dir named for package: mb2 takes package name from it (rpm/<that>.spec).
    mkdir -p ~/harbour-piirit/rpm
    cp /tmp/harbour-piirit.spec ~/harbour-piirit/rpm/
    cd ~/harbour-piirit

    # -X as rpm.yml: else build-init runs `git describe` and stops (no tags).
    mb2 -t "$TARGET" -X build-init
    mb2 -t "$TARGET" -X build-requires

    # Default sb2 target i486 is removed below; missing default fails every
    # `sb2`. Rewrite in place as user (sb2-config -d dies; root would own the
    # file).
    sed -i "s|^DEFAULT_TARGET=.*|DEFAULT_TARGET=$TARGET|" "$HOME/.scratchbox2/config"
    grep "^DEFAULT_TARGET=$TARGET$" "$HOME/.scratchbox2/config"

    rm -rf ~/harbour-piirit
'

echo ">> putting the rustlib where the linker looks, and dropping the rest"
# shellcheck disable=SC2016 # expanded by the container's shell.
"$engine" exec --user root -e TARGET="$target" "$cid" bash -euxo pipefail -c '
    # Build scripts/proc-macros link in sb2 host mode (/usr = SDK fs). ld wants
    # i686 rustlib in /usr/lib/rustlib; SDK has it under /srv/mer. Preference
    # order: a glob would hit the build-requires snapshot copy first.
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

    # Other arches only. "<target>.default" snapshot is what mb2 builds in;
    # deleting it leaves a target with no rust.
    for dir in /srv/mer/targets/*/; do
        name=$(basename "$dir")
        [[ "$name" = "$TARGET" || "$name" = "$TARGET.default" ]] && continue
        rm -rf "$dir" "/home/mersdk/.scratchbox2/$name"
    done

    # Bake leavings and package cache.
    rm -f /tmp/harbour-piirit.spec
    rm -rf "/srv/mer/targets/$TARGET/var/cache/zypp"/*

    # Rootless Podman: 65536 subordinate ids per user by default. Upstream has
    # mersdk at 100000, appsupport-* at 500000+: image unpullable there. mersdk
    # to 1000; appsupport (Android app support, never used to build) dropped.
    # passwd/group by sed: no shadow-utils assumed.
    old_uid=$(id -u mersdk)
    old_gid=$(id -g mersdk)
    if [ "$old_uid" != 1000 ]; then
        ! awk -F: "\$3 == 1000 { found = 1 } END { exit !found }" /etc/passwd ||
            { echo "uid 1000 is taken" >&2; exit 1; }
        ! awk -F: "\$3 == 1000 { found = 1 } END { exit !found }" /etc/group ||
            { echo "gid 1000 is taken" >&2; exit 1; }
        sed -i "s/^mersdk:\([^:]*\):$old_uid:$old_gid:/mersdk:\1:1000:1000:/" /etc/passwd
        sed -i "s/^mersdk:\([^:]*\):$old_gid:/mersdk:\1:1000:/" /etc/group
        find / -xdev -uid "$old_uid" -exec chown -h 1000 {} +
        find / -xdev -gid "$old_gid" -exec chgrp -h 1000 {} +
    fi
    for f in /etc/passwd /etc/group /etc/shadow /etc/gshadow; do
        [ -f "$f" ] && sed -i "/^appsupport-/d" "$f"
    done
    rm -rf /home/appsupport-root /var/spool/mail/appsupport-root
    high=$(find / -xdev \( -uid +65535 -o -gid +65535 \) | head -5)
    [ -z "$high" ] || { echo "still owned by ids above 65535:" >&2; echo "$high" >&2; exit 1; }
'

# export: deletions are real, not whiteouts (hence export/import, not
# `RUN rm`). Config restored by hand.
echo ">> flattening"
"$engine" stop "$cid" >/dev/null

# GHCR links package to repo by this label; else repo token can't pull it.
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

"$engine" export "$cid" |
    "$engine" import "${changes[@]}" \
        --message "piirit: $target, BuildRequires installed, one architecture" \
        - "$output" >/dev/null

# Gate: an image failing these fails minutes into a build, misleadingly.
echo ">> checking what came out"
# shellcheck disable=SC2016 # expanded by the container's shell.
"$engine" run --rm --privileged -e TARGET="$target" "$output" bash -euo pipefail -c '
    # Read into var, not `grep -q` pipe: early exit SIGPIPEs sb2-config;
    # pipefail reads that as a failed check.
    targets=$(sb2-config -l)
    printf "%s\n" "$targets"

    # This arch: pristine target and snapshot only.
    [ "$(printf "%s\n" "$targets" | grep -c .)" = 2 ] ||
        { echo "the wrong number of targets survived" >&2; exit 1; }
    grep -qx "$TARGET" <<< "$targets"
    grep -qx "$TARGET.default" <<< "$targets"

    # Against snapshot (what mb2 builds in). Pristine target hid a broken image.
    sb2 -t "$TARGET.default" rpm -q rust cargo gcc-c++ git \
        desktop-file-utils qt5-qttools-linguist
    # Cross std: absence = "can not find crate for std" late in build. By
    # package name, not the spec virtual provide: independent of who provides.
    sb2 -t "$TARGET.default" rpm -qa | grep "^rust-std-static-" ||
        { echo "no cross std in the snapshot" >&2; exit 1; }
    sb2 -t "$TARGET.default" cargo --version
    ls -d /usr/lib/rustlib/i686-unknown-linux-gnu
    grep "^DEFAULT_TARGET=$TARGET$" "$HOME/.scratchbox2/config"
    [ "$(id -u):$(id -g)" = 1000:1000 ] || { echo "build user is $(id -u):$(id -g), not 1000:1000" >&2; exit 1; }

    # Build dir still inits against a target whose pristine snapshot is
    # removed: next thing rpm.yml does, the one assumption slimming makes.
    mkdir -p ~/verify/rpm
    printf "Name: verify\nSummary: s\nVersion: 0\nRelease: 1\nLicense: GPL-3.0-or-later\n%%description\ns\n%%build\n%%install\n%%files\n" \
        > ~/verify/rpm/verify.spec
    cd ~/verify && mb2 -t "$TARGET" -X build-init
'

size=$("$engine" image inspect --format '{{.Size}}' "$output")
echo ">> $output is $((size / 1000 / 1000)) MB unpacked"
