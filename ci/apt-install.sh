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
# Usage:  ci/apt-install.sh <package>...
#         ci/apt-install.sh --prune-only     (drop lists, install nothing)
#
# APT_SOURCES_DIR overrides pruned dir, so ci/apt-install-selftest.sh can test
# its own dir.
set -euo pipefail

sources_dir=$(printenv APT_SOURCES_DIR || true)
if [[ -z "$sources_dir" ]]; then
    sources_dir=/etc/apt/sources.list.d
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

sudo apt-get update
sudo apt-get install -y "$@"
