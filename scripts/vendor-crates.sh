#!/bin/sh
# Generate the offline crate sources the `--with vendor` build mode of
# rpm/harbour-piiri.spec expects:
#
#     rpm/vendor.tar.xz   -- `cargo vendor` output, packed as rust/vendor/
#     rpm/vendor.toml     -- the [source] replacement stanza cargo prints
#
# OBS (and therefore Chum) builds run without network access, so crates
# cannot be fetched during %build there; these two extra sources are what
# make an offline build possible. A plain `sfdk build` inside the SDK does
# have network and does not need them.
#
# Neither file is committed (see .gitignore): they are regenerated from
# rust/Cargo.lock, which is committed and is the actual pin.

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
