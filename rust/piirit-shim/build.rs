//! Compile C++ of `cpp!` in `src/recorder.rs`, `src/pictures.rs`, `src/mic_watch.rs`; link
//! `QtMultimedia` and libpulse they call.
//!
//! As `piirit-app/build.rs`: against Qt qttypes found. Sailfish: target's Qt via
//! `QT_INCLUDE_PATH`, `QT_LIBRARY_PATH`.

fn main() {
    // C++ compiler crate names env it watches, which turns off cargo's rerun-on-any-change. So
    // `cpp!` files listed, else a new block waits for clean build.
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

    // QtMultimedia for recorder. qttypes doesn't link it; it sits beside Qt libs qttypes found.
    if let Ok(library_path) = std::env::var("DEP_QT_LIBRARY_PATH") {
        println!("cargo:rustc-link-search=native={library_path}");
    }
    println!("cargo:rustc-link-lib=Qt5Multimedia");

    // libpulse (`libpulse.so.0`, Harbour-allowed) for mic watch; by name.
    println!("cargo:rustc-link-lib=pulse");
}

/// Include dirs for `<pulse/pulseaudio.h>` from pkg-config.
///
/// Empty if none or no pkg-config: compiler's default dirs (`/usr/include`, SDK target) cover it.
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
