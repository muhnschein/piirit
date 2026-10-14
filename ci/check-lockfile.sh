#!/bin/sh
# Sailfish cargo 1.75 can't read v4 lockfile (v4 from 1.78). Modern `cargo
# update` rewrites it.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
lock="$root/rust/Cargo.lock"

version=$(grep -E '^version = [0-9]+$' "$lock" | head -1 | tr -cd '0-9')
if [ "${version:-}" != "3" ]; then
    echo "check-lockfile: FAIL rust/Cargo.lock is v${version:-unknown}, must stay v3 for Sailfish's cargo 1.75 (see docs/BUILDING.md)" >&2
    exit 1
fi
echo "check-lockfile: ok (v3)"
