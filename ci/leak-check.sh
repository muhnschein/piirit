#!/bin/sh
# ASan + LeakSanitizer, whole workspace. Finds leaks lost by exit, OOB, UAF,
# double free -- also in FFmpeg, x264, piirit-video's C glue and shim's /
# qmetaobject's cpp! glue (all built with ASan). Timing tests can fail only here
# (ASan ~2-3x slower); a real leak ends in an LSan report with allocation stack.
# Misses memory still reachable at exit and un-rebuilt libs (Qt, glib, real
# core in own process). Deliberate Qt leaks: ci/lsan.supp (read reasons first).
#
# Nightly only (`-Zsanitizer`; `-Zbuild-std` so std is instrumented too).
# Pinned: weekly fails on our code, not nightly's. Move on purpose, like
# rust-toolchain.toml.
#
# scheduled.yml runs weekly. Local:
#   rustup toolchain install nightly-2026-10-12 --profile minimal -c rust-src
#   cargo install --locked cargo-nextest
#   ./ci/leak-check.sh            (or: make leak-check)
# Args go to nextest, e.g. `./ci/leak-check.sh -p piirit-shim --test smoke`.
# DELTACHAT_RPC_SERVER (`make fetch-server`) adds two real-core tests.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
toolchain=${LEAK_CHECK_TOOLCHAIN:-nightly-2026-10-12}
# Explicit target: host build scripts/proc macros stay uninstrumented,
# everything in test binaries is instrumented.
target=x86_64-unknown-linux-gnu

# lsan.supp matches by function name, which LSan gets only via llvm-symbolizer.
# Without it frames are bare addresses, nothing matches, every Qt test leaks.
if ! command -v llvm-symbolizer >/dev/null 2>&1; then
    echo "leak-check: FAIL llvm-symbolizer is not on PATH (apt install llvm)" >&2
    exit 1
fi

# Wrappers, not flags in CC/CXX: piirit-video hands its compiler to FFmpeg's
# and x264's configure as a single path.
wrappers=$(mktemp -d)
trap 'rm -rf "$wrappers"' EXIT
for pair in cc:gcc c++:g++; do
    name=${pair%%:*}
    compiler=${pair#*:}
    printf '#!/bin/sh\nexec %s -fsanitize=address -fno-omit-frame-pointer "$@"\n' \
        "$compiler" >"$wrappers/$name"
    chmod +x "$wrappers/$name"
done
# Plain CC/CXX, not cc crate's CC_<triple>: host is target here.
export CC="$wrappers/cc"
export CXX="$wrappers/c++"

# --cap-lints=warn: workspace denies warnings; newer nightly lints are
# scheduled.yml's next-stable-clippy job's, not a reason to skip this.
export RUSTFLAGS="-Zsanitizer=address --cap-lints=warn"
export CARGO_UNSTABLE_BUILD_STD=std
# Own dir: normal builds can't reuse it.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/rust/target/leak-check}"
# Line tables: file:line in report, small. ASan makes binaries large.
export CARGO_PROFILE_DEV_DEBUG=line-tables-only
export CARGO_INCREMENTAL=0
export QT_QPA_PLATFORM=offscreen

# detect_odr_violation=1, not 2: FFmpeg compiles one template twice (same
# static registered twice, really two vars). 1 still reports a global defined
# twice at different sizes (real bug).
export ASAN_OPTIONS=detect_leaks=1:detect_odr_violation=1
export LSAN_OPTIONS="suppressions=$root/ci/lsan.supp:print_suppressions=0"

cd "$root/rust"
rustc "+$toolchain" --version
# Whole workspace unless args given: cargo rejects `--workspace` with `-p`.
if [ $# -eq 0 ]; then
    set -- --workspace
fi
# Tests, no doctests: nextest skips them; a doctest leaks nothing its unit
# tests wouldn't show.
exec cargo "+$toolchain" nextest run --locked \
    --target "$target" --profile leak-check "$@"
