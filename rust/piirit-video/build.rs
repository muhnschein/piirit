//! Build FFmpeg and x264 from the submodules under `third_party/`, and the
//! C between them and Rust (`csrc/`), into static libraries this crate
//! links.
//!
//! Static, because Harbour allows a package one executable and a list of
//! system libraries, and neither codec is on that list
//! (`ci/harbour/allowed_libraries.conf`). Built into the binary, they are
//! code like any other: the validator sees libc, libm and libpthread,
//! which it allows. LAME reaches the binary the same way, through
//! mp3lame-sys. See docs/BUILDING.md, "Making a video smaller".
//!
//! Both are trimmed to what making a video smaller needs. FFmpeg reads an
//! MP4 or a QuickTime file, decodes H.264, HEVC and AAC, scales and
//! resamples, encodes AAC, and writes an MP4; x264 encodes 8-bit 4:2:0
//! H.264. Nothing else is configured in, which is what keeps the binary
//! a few megabytes heavier rather than a few dozen.
//!
//! The compiler is the one cargo would use for this target, from the `cc`
//! crate, so both configure scripts build for what the rest of the binary
//! is built for. Under the Sailfish SDK's scratchbox2 that is the target's
//! own compiler, and the configure scripts see a native build, which is
//! how LAME's configure runs there too.
//!
//! Hand-written assembly is used where it is free: on ARM, where the
//! compiler that builds everything else assembles it. On x86 it would
//! need nasm, which neither CI's runners nor the SDK's emulator target
//! need otherwise, and x86 is only ever the host the tests run on.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(env("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../../third_party");
    let ffmpeg_src = root.join("ffmpeg");
    let x264_src = root.join("x264");
    for (src, marker) in [(&ffmpeg_src, "configure"), (&x264_src, "x264.h")] {
        assert!(
            src.join(marker).is_file(),
            "{} is empty. FFmpeg and x264 are git submodules: run\n    \
             git submodule update --init --depth 1\nfrom the repository root.",
            src.display()
        );
        println!("cargo:rerun-if-changed={}", src.display());
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=csrc");

    let out = PathBuf::from(env("OUT_DIR"));
    let target = Target::from_env();

    let x264 = build_x264(&x264_src, &out, &target);
    let ffmpeg = build_ffmpeg(&ffmpeg_src, &out, &target);

    let mut glue = cc::Build::new();
    glue.files(GLUE)
        .include(x264.join("include"))
        .include(ffmpeg.join("include"))
        .warnings(true)
        .extra_warnings(true);
    write_compile_commands(&glue, &manifest, &out);
    glue.compile("piirit_video");

    // In dependency order, each after what uses it.
    println!(
        "cargo:rustc-link-search=native={}",
        ffmpeg.join("lib").display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        x264.join("lib").display()
    );
    for lib in [
        "avformat",
        "avcodec",
        "swscale",
        "swresample",
        "avutil",
        "x264",
    ] {
        println!("cargo:rustc-link-lib=static={lib}");
    }
    // What FFmpeg's and x264's own pkg-config files ask for, less
    // libatomic: Harbour does not allow it, and neither library calls
    // into it on the architectures this is built for -- the atomics
    // they use are compiled inline there. A target that did need it
    // would fail to link here rather than at intake.
    for lib in ["m", "pthread"] {
        println!("cargo:rustc-link-lib={lib}");
    }
}

/// The C between FFmpeg, x264 and Rust.
const GLUE: [&str; 2] = ["csrc/recode.c", "csrc/synth.c"];

/// How `glue` compiles each of its files, as a compilation database at
/// `OUT_DIR/compile_commands.json`. Sonar analyses C only from
/// one -- it has to know the include paths to read a file at all -- and
/// `make sonar-reports` hands it this. Nothing in the build reads it.
fn write_compile_commands(glue: &cc::Build, manifest: &Path, out: &Path) {
    let compiler = glue.get_compiler();
    let quote = |text: &str| {
        let mut quoted = String::from("\"");
        for c in text.chars() {
            match c {
                '"' | '\\' => {
                    quoted.push('\\');
                    quoted.push(c);
                }
                c if u32::from(c) < 0x20 => {
                    let _ = write!(quoted, "\\u{:04x}", u32::from(c));
                }
                c => quoted.push(c),
            }
        }
        quoted.push('"');
        quoted
    };
    let entries: Vec<String> = GLUE
        .iter()
        .map(|file| {
            let path = manifest.join(file);
            let mut arguments = vec![compiler.path().to_string_lossy().into_owned()];
            arguments.extend(
                compiler
                    .args()
                    .iter()
                    .map(|arg| arg.to_string_lossy().into_owned()),
            );
            arguments.push("-c".into());
            arguments.push(path.to_string_lossy().into_owned());
            let arguments: Vec<String> = arguments.iter().map(|arg| quote(arg)).collect();
            format!(
                "{{\"directory\":{},\"file\":{},\"arguments\":[{}]}}",
                quote(&manifest.to_string_lossy()),
                quote(&path.to_string_lossy()),
                arguments.join(",")
            )
        })
        .collect();
    let database = out.join("compile_commands.json");
    std::fs::write(&database, format!("[{}]\n", entries.join(",\n")))
        .unwrap_or_else(|err| panic!("write {}: {err}", database.display()));
}

/// An environment variable cargo always sets for a build script.
fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("cargo did not set {name}"))
}

/// What the configure scripts are told about the machine they build for.
struct Target {
    /// The C compiler, as cargo would run it for this target.
    cc: PathBuf,
    /// Its flags, less the optimisation and debug levels: both libraries
    /// pick their own, and FFmpeg does not build below -O1, since it
    /// relies on the optimiser to drop code for what is configured out.
    cflags: Vec<String>,
    ar: PathBuf,
    ranlib: PathBuf,
    /// `CARGO_CFG_TARGET_ARCH`: `aarch64`, `arm`, `x86_64`, `x86`.
    arch: String,
    /// The target triple, when it is not the machine running the build.
    cross: Option<String>,
    /// How many jobs cargo allows this build script.
    jobs: String,
}

impl Target {
    fn from_env() -> Self {
        let build = cc::Build::new();
        let compiler = build.get_compiler();
        let cflags = compiler
            .args()
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .filter(|arg| !arg.starts_with("-O") && !arg.starts_with("-g"))
            .collect();
        let target = env("TARGET");
        let host = env("HOST");
        Self {
            cc: compiler.path().to_path_buf(),
            cflags,
            ar: build.get_archiver().get_program().into(),
            ranlib: build.get_ranlib().get_program().into(),
            arch: env("CARGO_CFG_TARGET_ARCH"),
            cross: (target != host).then_some(target),
            jobs: std::env::var("NUM_JOBS").unwrap_or_else(|_| "1".into()),
        }
    }

    /// Whether the hand-written assembly is left out; see the module doc.
    fn without_asm(&self) -> bool {
        self.arch == "x86" || self.arch == "x86_64"
    }

    /// The flags as one argument, for `--extra-cflags`.
    fn cflags(&self) -> String {
        self.cflags.join(" ")
    }
}

/// Configure and build x264 out of its tree, and install it under
/// `OUT_DIR/x264`. Returns that prefix.
fn build_x264(src: &Path, out: &Path, target: &Target) -> PathBuf {
    let build = out.join("x264-build");
    let prefix = out.join("x264");
    std::fs::create_dir_all(&build)
        .unwrap_or_else(|err| panic!("mkdir {}: {err}", build.display()));

    let mut configure = Command::new(src.join("configure"));
    configure
        .current_dir(&build)
        .env("CC", &target.cc)
        .env("AR", &target.ar)
        .env("RANLIB", &target.ranlib)
        .arg(format!("--prefix={}", prefix.display()))
        .arg(format!("--extra-cflags={}", target.cflags()))
        .args([
            "--enable-static",
            "--enable-pic",
            "--disable-cli",
            "--disable-opencl",
            "--disable-avs",
            "--disable-swscale",
            "--disable-lavf",
            "--disable-ffms",
            "--disable-gpac",
            "--disable-lsmash",
            "--bit-depth=8",
            "--chroma-format=420",
        ]);
    if target.without_asm() {
        configure.arg("--disable-asm");
    }
    if let Some(triple) = &target.cross {
        configure.arg(format!("--host={triple}"));
    }
    run(
        &mut configure,
        "x264's configure",
        &build.join("config.log"),
    );
    run(
        Command::new("make")
            .current_dir(&build)
            .arg(format!("-j{}", target.jobs))
            .arg("install-lib-static"),
        "x264's make",
        &build.join("config.log"),
    );
    prefix
}

/// Configure and build FFmpeg out of its tree, and install it under
/// `OUT_DIR/ffmpeg`. Returns that prefix.
fn build_ffmpeg(src: &Path, out: &Path, target: &Target) -> PathBuf {
    let build = out.join("ffmpeg-build");
    let prefix = out.join("ffmpeg");
    std::fs::create_dir_all(&build)
        .unwrap_or_else(|err| panic!("mkdir {}: {err}", build.display()));

    let mut configure = Command::new(src.join("configure"));
    configure
        .current_dir(&build)
        .arg(format!("--prefix={}", prefix.display()))
        .arg(format!("--cc={}", target.cc.display()))
        .arg(format!("--ar={}", target.ar.display()))
        .arg(format!("--ranlib={}", target.ranlib.display()))
        .arg(format!("--extra-cflags={}", target.cflags()))
        .args([
            // A static library of position-independent code, and nothing
            // that is a program or a document.
            "--enable-static",
            "--disable-shared",
            "--enable-pic",
            "--disable-programs",
            "--disable-doc",
            "--disable-debug",
            // Nothing found on the build machine and used because it was
            // there: what this needs is named below, and nothing else.
            "--disable-autodetect",
            "--enable-pthreads",
            "--disable-network",
            "--disable-avdevice",
            "--disable-avfilter",
            "--disable-everything",
            // Read an MP4 or a QuickTime file, which is what a phone
            // records; decode what is in one; write an MP4 back.
            "--enable-protocol=file",
            "--enable-demuxer=mov",
            "--enable-muxer=mp4",
            "--enable-decoder=h264,hevc,aac",
            "--enable-parser=h264,hevc,aac",
            "--enable-encoder=aac",
        ]);
    if target.without_asm() {
        configure.arg("--disable-asm");
    }
    if let Some(triple) = &target.cross {
        configure
            .arg("--enable-cross-compile")
            .arg("--target-os=linux")
            .arg(format!("--arch={}", target.arch))
            .arg(format!("--cross-prefix={triple}-"));
    }
    run(
        &mut configure,
        "FFmpeg's configure",
        &build.join("ffbuild/config.log"),
    );
    run(
        Command::new("make")
            .current_dir(&build)
            .arg(format!("-j{}", target.jobs))
            .arg("install"),
        "FFmpeg's make",
        &build.join("ffbuild/config.log"),
    );
    prefix
}

/// Run one step, and on failure say which and where its log is.
fn run(command: &mut Command, what: &str, log: &Path) {
    let status = command
        .status()
        .unwrap_or_else(|err| panic!("{what} could not be started: {err}"));
    assert!(
        status.success(),
        "{what} failed ({status}); see {}",
        log.display()
    );
}
