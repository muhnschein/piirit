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
# archive.ubuntu.com, once.
#
# Usage:  ci/apt-install.sh <package>...
#         ci/apt-install.sh --prune-only     (drop lists, install nothing)
#
# APT_SOURCES_DIR, APT_SOURCES_LIST override pruned dir and sources.list;
# APT_GET stands in for `sudo apt-get`: ci/apt-install-selftest.sh tests its own
# dir and a failing apt.
set -euo pipefail

sources_dir=$(printenv APT_SOURCES_DIR || true)
if [[ -z "$sources_dir" ]]; then
    sources_dir=/etc/apt/sources.list.d
fi
sources_list=$(printenv APT_SOURCES_LIST || true)
if [[ -z "$sources_list" ]]; then
    sources_list=/etc/apt/sources.list
fi
apt_stub=$(printenv APT_GET || true)

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

# Per file: 3 retries, 30 s without data = stalled.
apt_get() {
    local options=(-o Acquire::Retries=3 -o Acquire::http::Timeout=30
        -o Acquire::https::Timeout=30)
    if [[ -n "$apt_stub" ]]; then
        "$apt_stub" "${options[@]}" "$@"
    else
        sudo apt-get "${options[@]}" "$@"
    fi
}

# Country or cloud mirror (azure.archive.ubuntu.com) to archive.ubuntu.com, in
# every list left. security.ubuntu.com, ports.ubuntu.com: not mirrors, kept.
use_main_archive() {
    local list mirror='://[a-z0-9.-]+\.archive\.ubuntu\.com'
    local rewrite="s#$mirror#://archive.ubuntu.com#g"
    for list in "$sources_list" "$sources_dir"/*.list "$sources_dir"/*.sources; do
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

if apt_get update && apt_get install -y "$@"; then
    exit 0
fi
echo "apt-install: mirror failed; once more from archive.ubuntu.com" >&2
use_main_archive
apt_get update
apt_get install -y "$@"
