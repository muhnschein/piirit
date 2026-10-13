//! Compile the C++ side of the `cpp!` blocks in `src/recorder.rs`,
//! `src/pictures.rs` and `src/mic_watch.rs`, and link the Qt module and
//! the sound server's client library they call into.
//!
//! The same arrangement as `piirit-app/build.rs` and the vendored
//! qmetaobject's own: the C++ is compiled against the Qt that qttypes
//! found, which is the one everything else links. Under the Sailfish
//! SDK's scratchbox that is the target's, through the `QT_INCLUDE_PATH`
//! and `QT_LIBRARY_PATH` the spec sets.

fn main() {
    // The C++ compiler's crate names the environment it watches, which
    // turns off cargo's own rule of re-running on any change in the
    // package; so the files with `cpp!` blocks are named here, or a
    // block added to them is not compiled until the next clean build.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/recorder.rs");
    println!("cargo:rerun-if-changed=src/pictures.rs");
    println!("cargo:rerun-if-changed=src/mic_watch.rs");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_SYSROOT_DIR");
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
    for dir in pulse_include_dirs() {
        config.include(dir);
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

    // libpulse, for the microphone watch: `libpulse.so.0`, which Harbour
    // allows. Linked by name, from wherever the toolchain looks.
    println!("cargo:rustc-link-lib=pulse");
}

/// Where `<pulse/pulseaudio.h>` is, when pkg-config knows of anywhere
/// beyond the usual places. Nothing when it does not, or is not there:
/// on a desktop the headers are in `/usr/include`, and under the Sailfish
/// SDK in the target's, both of which the compiler searches anyway.
fn pulse_include_dirs() -> Vec<String> {
    let pkg_config = std::env::var("PKG_CONFIG").unwrap_or_else(|_| "pkg-config".into());
    let Ok(output) = std::process::Command::new(pkg_config)
        .args(["--cflags-only-I", "libpulse"])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .filter_map(|flag| flag.strip_prefix("-I"))
        .map(ToString::to_string)
        .collect()
}
