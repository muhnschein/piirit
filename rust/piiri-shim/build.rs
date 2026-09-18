//! Compile the C++ side of the `cpp!` blocks in `src/recorder.rs`, and
//! link the Qt module they call into.
//!
//! The same arrangement as `piiri-app/build.rs` and the vendored
//! qmetaobject's own: the C++ is compiled against the Qt that qttypes
//! found, which is the one everything else links. Under the Sailfish
//! SDK's scratchbox that is the target's, through the `QT_INCLUDE_PATH`
//! and `QT_LIBRARY_PATH` the spec sets.

fn main() {
    // The C++ compiler's crate names the environment it watches, which
    // turns off cargo's own rule of re-running on any change in the
    // package; so the two files with `cpp!` blocks are named here, or a
    // block added to them is not compiled until the next clean build.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/recorder.rs");
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
    config.include(&include).build("src/lib.rs");

    // QtMultimedia, for the recorder. qttypes links QtCore, QtGui, QtQml
    // and QtQuick (and QtWidgets, which the app's link drops again) and
    // knows nothing of this module; the library path it found is where
    // the rest of Qt is, and Multimedia sits beside them.
    if let Ok(library_path) = std::env::var("DEP_QT_LIBRARY_PATH") {
        println!("cargo:rustc-link-search=native={library_path}");
    }
    println!("cargo:rustc-link-lib=Qt5Multimedia");
}
