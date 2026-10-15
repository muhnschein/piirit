#!/bin/sh
# Offline crate sources for `--with vendor` build of rpm/harbour-piirit.spec:
# rpm/vendor.tar.xz (`cargo vendor`, packed as rust/vendor/) and rpm/vendor.toml
# ([source] replacement stanza).
#
# OBS (so Chum) builds lack network: crates can't fetch in %build. Plain
# `sfdk build` in SDK has network, doesn't need these.
#
# Not committed (.gitignore): regenerated from rust/Cargo.lock, the pin.

set -eu

repo_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root/rust"

echo ">> cargo vendor"
cargo vendor --locked --versioned-dirs > "$repo_root/rpm/vendor.toml"

echo ">> packing rust/vendor -> rpm/vendor.tar.xz"
cd "$repo_root"
tar cJf rpm/vendor.tar.xz rust/vendor

echo "Done:"
ls -lh rpm/vendor.tar.xz rpm/vendor.toml
echo
echo "Build with:  sfdk build -- --with vendor"
