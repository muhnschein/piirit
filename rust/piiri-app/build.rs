//! Export `main()` as a dynamic symbol, and compile the C++ side of the
//! `cpp!` block in `src/translations.rs`.
//!
//! Rust emits `main` as an ordinary global symbol, which lives only in
//! `.symtab` and is gone once rpmbuild strips the binary. The
//! `silica-qt5` booster looks it up in `.dynsym`, and Harbour's validator
//! rejects a Silica app where it is missing (`ci/harbour-check.sh`, check
//! 1.7.3).
//!
//! `--dynamic-list` rather than `--export-dynamic-symbol`, which needs
//! binutils 2.35, or `--export-dynamic`, which would export every symbol
//! in the binary.

fn main() {
    let list = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("main.dynlist");
    println!("cargo:rerun-if-changed={}", list.display());
    println!(
        "cargo:rustc-link-arg-bins=-Wl,--dynamic-list={}",
        list.display()
    );

    // qttypes links Qt5Widgets unconditionally, with no feature to turn it
    // off. Once the vendored qmetaobject stops using QApplication nothing
    // refers to it, and --as-needed drops the DT_NEEDED entry that would
    // otherwise fail Harbour's allowed-libraries check. It prunes only
    // libraries no symbol needs, so it is right for the others too.
    println!("cargo:rustc-link-arg-bins=-Wl,--as-needed");

    // The C++ in src/translations.rs, compiled against the Qt that qttypes
    // found -- the same Qt everything else links, since qttypes tells its
    // dependents about exactly one. Under the Sailfish SDK's scratchbox
    // that is the target's, through the QT_INCLUDE_PATH the spec sets;
    // here it is whatever qmake answers for. The same arrangement as the
    // vendored qmetaobject's own build.rs.
    let Ok(include) = std::env::var("DEP_QT_INCLUDE_PATH") else {
        panic!("qttypes found no Qt; its build script says why above");
    };
    let mut config = cpp_build::Config::new();
    for flag in std::env::var("DEP_QT_COMPILE_FLAGS")
        .unwrap_or_default()
        .split_terminator(';')
    {
        config.flag(flag);
    }
    config.include(include).build("src/main.rs");
}
