//! Export `main()` as dynamic symbol; compile C++ of `cpp!` in src/translations.rs.
//!
//! `main` in `.symtab` only, stripped by rpmbuild. `silica-qt5` booster reads `.dynsym`;
//! Harbour checks it (`ci/harbour-check.sh` 1.7.3).
//!
//! `--dynamic-list`, not `--export-dynamic-symbol` (needs binutils 2.35) or `--export-dynamic`
//! (exports every symbol).

fn main() {
    let list = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("main.dynlist");
    println!("cargo:rerun-if-changed={}", list.display());
    println!(
        "cargo:rustc-link-arg-bins=-Wl,--dynamic-list={}",
        list.display()
    );

    // qttypes links Qt5Widgets always. Unused once qmetaobject drops QApplication.
    // --as-needed drops its DT_NEEDED, else Harbour allowed-libraries check fails. Prunes only
    // libraries no symbol needs, so safe for the rest.
    println!("cargo:rustc-link-arg-bins=-Wl,--as-needed");

    // C++ built against Qt qttypes found (same Qt as all links). Sailfish SDK scratchbox: target's,
    // via spec's QT_INCLUDE_PATH; here: qmake's. As vendored qmetaobject's build.rs.
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
