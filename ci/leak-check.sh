#!/bin/sh
# Every test in the workspace, under AddressSanitizer with leak detection.
#
# What it finds that the normal suite cannot: memory a test's process
# allocated and lost track of by the time it exits, and the out-of-bounds
# reads, use-after-frees and double frees ASan stops on. Not only in our
# Rust -- FFmpeg, x264 and piirit-video's own C glue are built with
# `-fsanitize=address` too, which is where the first leak this found was
# (recode.c's encode_video, a buffer lost per video frame read). The shim's and
# qmetaobject's cpp! glue is instrumented the same way.
#
# A test that asserts on timing can fail here and nowhere else: ASan makes
# everything two to three times slower. The log tells the two apart -- a
# leak ends in a LeakSanitizer report with the allocation's stack.
#
# What it cannot find: memory still reachable at exit, such as a cache that
# only grows, and anything in a library that was not rebuilt (Qt, glib,
# the real core, which runs in a process of its own). Leaks Qt makes on
# purpose are suppressed by name in ci/lsan.supp; read the reasons there
# before adding to it.
#
# Nightly only (`-Zsanitizer`, and `-Zbuild-std` so std is instrumented and
# agrees with everything else about the sanitizer). The nightly is pinned so
# a weekly run fails on our code, not on whatever nightly did that week;
# move it on purpose, like rust-toolchain.toml.
#
# scheduled.yml runs it weekly. It rebuilds FFmpeg and x264 and links every
# test binary with ASan, so it is far slower than `make test` and is not
# part of `make check`. Locally:
#   rustup toolchain install nightly-2026-10-12 --profile minimal -c rust-src
#   cargo install --locked cargo-nextest
#   ./ci/leak-check.sh            (or: make leak-check)
# Arguments go to nextest, so one test can be run on its own:
#   ./ci/leak-check.sh -p piirit-shim --test smoke
# Point DELTACHAT_RPC_SERVER at the fetched server (`make fetch-server`)
# to include the two tests that talk to the real core.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
toolchain=${LEAK_CHECK_TOOLCHAIN:-nightly-2026-10-12}
# Spelled out, so build scripts and proc macros (built for the host) stay
# uninstrumented, and everything that ends up in a test binary is.
target=x86_64-unknown-linux-gnu

# The C and C++ compilers, with the sanitizer. Wrappers rather than flags in
# CC/CXX, because piirit-video's build hands its compiler to FFmpeg's and
# x264's configure scripts as a single path.
wrappers=$(mktemp -d)
trap 'rm -rf "$wrappers"' EXIT
for pair in cc:gcc c++:g++; do
    name=${pair%%:*}
    compiler=${pair#*:}
    printf '#!/bin/sh\nexec %s -fsanitize=address -fno-omit-frame-pointer "$@"\n' \
        "$compiler" >"$wrappers/$name"
    chmod +x "$wrappers/$name"
done
# Plain CC and CXX rather than the cc crate's per-target CC_<triple>: the
# host is the target here, so those would reach exactly the same compiles.
export CC="$wrappers/cc"
export CXX="$wrappers/c++"

# --cap-lints=warn: the workspace denies warnings, and a lint this nightly
# has that the pinned stable does not is scheduled.yml's next-stable-clippy
# job's to report, not a reason to skip the leak check.
export RUSTFLAGS="-Zsanitizer=address --cap-lints=warn"
export CARGO_UNSTABLE_BUILD_STD=std
# Its own directory: nothing built here can be reused by a normal build.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/rust/target/leak-check}"
# Enough for file:line in a report, at a fraction of full debuginfo's disk:
# with ASan, each of the hundred-odd test binaries is large.
export CARGO_PROFILE_DEV_DEBUG=line-tables-only
export CARGO_INCREMENTAL=0
export QT_QPA_PLATFORM=offscreen

# detect_odr_violation=1 rather than the default 2: FFmpeg compiles some
# sources twice from one template (aom_film_grain_template.c, once per bit
# depth), so the same `static` local is registered twice from the same
# line. Those are two variables, not one defined twice, and 1 still reports
# a global defined twice at different sizes, the kind that is a real bug.
export ASAN_OPTIONS=detect_leaks=1:detect_odr_violation=1
export LSAN_OPTIONS="suppressions=$root/ci/lsan.supp:print_suppressions=0"

cd "$root/rust"
rustc "+$toolchain" --version
# The whole workspace unless told which part: cargo takes `--workspace`
# over `-p`, so the two cannot both be passed.
if [ $# -eq 0 ]; then
    set -- --workspace
fi
# Only the tests: nextest does not run doctests, and a doctest has nothing
# to leak that the unit tests beside it would not show.
exec cargo "+$toolchain" nextest run --locked \
    --target "$target" --profile leak-check "$@"
