#!/usr/bin/env bash
# Install Ubuntu packages on CI runner, without others' apt repo breaking the
# build.
#
# `apt-get update` exits 100 if ANY repo fails. Runner image ships extra repos
# this project never uses: one bad index kills every job.
#
# So third-party lists dropped before update. Ubuntu's kept wherever image puts
# them (sources.list, or .list/.sources under sources.list.d). List dropped only
# if no ubuntu.com host in it, so the archive we install from is never deleted.
#
# Mirror slow or down (runner's azure.archive.ubuntu.com): apt retries each
# file, gives up on a stalled one; still failing = Ubuntu's own
# archive.ubuntu.com, once. Trickling mirror never stalls (seen: 120 kB/s, 124
# MB, 9.5 min of 20): first try's index and download each capped at
# APT_FIRST_TRY_SECONDS (240), then archive. Download only under cap: killed
# apt-get mid-dpkg leaves system half-installed. Debs fetched stay, reused.
#
# Usage:  ci/apt-install.sh <package>...
#         ci/apt-install.sh --prune-only     (drop lists, install nothing)
#
# APT_SOURCES_DIR, APT_SOURCES_LIST, APT_MIRROR_LIST override pruned dir,
# sources.list and apt-mirrors.txt;
# APT_GET stands in for `sudo apt-get`: ci/apt-install-selftest.sh tests its own
# dir and a failing or slow apt.
set -euo pipefail

sources_dir=$(printenv APT_SOURCES_DIR || true)
if [[ -z "$sources_dir" ]]; then
    sources_dir=/etc/apt/sources.list.d
fi
sources_list=$(printenv APT_SOURCES_LIST || true)
if [[ -z "$sources_list" ]]; then
    sources_list=/etc/apt/sources.list
fi
# Current image: ubuntu.sources says `mirror+file:` this list; hosts live here
# (azure first). apt moves on only when a request errors; trickle never does.
mirror_list=$(printenv APT_MIRROR_LIST || true)
if [[ -z "$mirror_list" ]]; then
    mirror_list=/etc/apt/apt-mirrors.txt
fi
apt_stub=$(printenv APT_GET || true)
first_try=$(printenv APT_FIRST_TRY_SECONDS || true)
if [[ -z "$first_try" ]]; then
    first_try=240
fi

prune_only=0
if [[ "$#" -ge 1 ]] && [[ "$1" = "--prune-only" ]]; then
    prune_only=1
    shift
fi

# rm as owner if writable, else sudo. Selftest dir must not prompt for password.
drop() {
    local list=$1
    if [[ -w "$(dirname -- "$list")" ]]; then
        rm -f "$list"
    else
        sudo rm -f "$list"
    fi
}

if [[ -d "$sources_dir" ]]; then
    for list in "$sources_dir"/*; do
        [[ -f "$list" ]] || continue
        case "$list" in
            *.list|*.sources) ;;
            *) continue ;;
        esac
        if grep -qE '://[^[:space:]]*\.ubuntu\.com' "$list"; then
            echo "apt-install: keeping $(basename -- "$list")"
            continue
        fi
        echo "apt-install: dropping $(basename -- "$list")"
        drop "$list"
    done
fi

if [[ "$prune_only" -eq 1 ]]; then
    exit 0
fi

if [[ "$#" -eq 0 ]]; then
    echo "apt-install: no packages named" >&2
    exit 1
fi

# Per file: 3 retries, 30 s without data = stalled. `--within <s>`: killed
# after s seconds (timeout under sudo: must reach root apt-get).
apt_get() {
    local options=(-o Acquire::Retries=3 -o Acquire::http::Timeout=30
        -o Acquire::https::Timeout=30)
    local cap=()
    if [[ "$1" = --within ]]; then
        cap=(timeout "$2")
        shift 2
    fi
    if [[ -n "$apt_stub" ]]; then
        "${cap[@]}" "$apt_stub" "${options[@]}" "$@"
    else
        sudo "${cap[@]}" apt-get "${options[@]}" "$@"
    fi
    return $?
}

# Country or cloud mirror (azure.archive.ubuntu.com) to archive.ubuntu.com, in
# every list left and mirror list. security.ubuntu.com, ports.ubuntu.com: not
# mirrors, kept.
use_main_archive() {
    local list mirror='://[a-z0-9.-]+\.archive\.ubuntu\.com'
    local rewrite="s#$mirror#://archive.ubuntu.com#g"
    for list in "$sources_list" "$mirror_list" "$sources_dir"/*.list "$sources_dir"/*.sources; do
        [[ -f "$list" ]] || continue
        grep -qE "$mirror" "$list" || continue
        echo "apt-install: $(basename -- "$list") now uses archive.ubuntu.com"
        if [[ -w "$list" ]]; then
            sed -i -E "$rewrite" "$list"
        else
            sudo sed -i -E "$rewrite" "$list"
        fi
    done
}

if apt_get --within "$first_try" update &&
    apt_get --within "$first_try" install -y --download-only "$@"; then
    apt_get install -y "$@"
    exit 0
fi
echo "apt-install: mirror failed or slower than ${first_try} s; once more from archive.ubuntu.com" >&2
use_main_archive
apt_get update
apt_get install -y "$@"
