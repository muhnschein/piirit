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
//!
//! The same x264 is also linked into a library of its own: the H.264
//! encoder video calls send through, which Gecko loads as a media plugin
//! (`csrc/gmp_h264.cpp`). Gecko cannot call into the app's binary, only
//! into a plugin it finds on disk, so this one is laid out as Gecko looks
//! for one -- `gmp-piirit/<version>/libpiirit.so` beside `piirit.info` --
//! in `OUT_DIR` and again beside the binary, where the package's spec
//! installs it from. Harbour allows a package's own libraries under
//! `/usr/share/<name>/lib/`, linked against the same list of system
//! libraries; x264 is inside it, and its symbols are hidden there.

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
    println!("cargo:rerun-if-changed={GMP_CHECK}");
    println!("cargo:rerun-if-changed={}", root.join("gmp-api").display());

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
    let compiler = glue.get_compiler();
    let mut database: Vec<(PathBuf, Vec<String>)> = GLUE
        .iter()
        .map(|file| {
            let path = manifest.join(file);
            (path.clone(), compile_command(&compiler, &[], &path))
        })
        .collect();
    glue.compile("piirit_video");

    let plugin = Plugin::new(&manifest, &root.join("gmp-api"), &x264);
    database.extend(plugin.build(&out, &target));
    write_compile_commands(&database, &manifest, &out);

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

/// The media plugin's one source file, and the stand-in for Gecko the
/// tests drive it with.
const GMP_SOURCE: &str = "csrc/gmp_h264.cpp";
const GMP_CHECK: &str = "tests/gmp_check.cpp";
/// The plugin's directory under a directory Gecko is pointed at: Gecko
/// takes its name from this, less `gmp-`, and looks for `lib<name>.so`
/// and `<name>.info` in the version directory below it.
const GMP_NAME: &str = "piirit";
const GMP_VERSION: &str = "1.0";
/// What Gecko reads to know the plugin. It encodes H.264 and nothing
/// else: decoding stays with the phone's own decoder, which Gecko
/// prefers to any plugin.
const GMP_INFO: &str = "Name: piirit\n\
                        Description: H.264 encoder for Piirit's video calls, built on x264\n\
                        Version: 1.0\n\
                        APIs: encode-video[h264]\n";

/// How the media plugin and its test harness are built: by the C++
/// compiler cargo would use for this target, against the Gecko headers
/// in `third_party/gmp-api` and the x264 built above.
struct Plugin {
    compiler: cc::Tool,
    flags: Vec<String>,
    source: PathBuf,
    check: PathBuf,
    x264: PathBuf,
}

impl Plugin {
    fn new(manifest: &Path, gmp_api: &Path, x264: &Path) -> Self {
        let compiler = cc::Build::new().cpp(true).get_compiler();
        let flags = vec![
            "-std=c++17".to_owned(),
            "-O2".to_owned(),
            "-Wall".to_owned(),
            "-Wextra".to_owned(),
            "-fno-exceptions".to_owned(),
            "-fno-rtti".to_owned(),
            format!("-I{}", gmp_api.display()),
            format!("-I{}", x264.join("include").display()),
        ];
        Self {
            compiler,
            flags,
            source: manifest.join(GMP_SOURCE),
            check: manifest.join(GMP_CHECK),
            x264: x264.to_path_buf(),
        }
    }

    /// The plugin under `OUT_DIR`, copied beside the binary, and the
    /// harness when the tests can run what this builds. Returns how each
    /// file was compiled, for the compilation database.
    fn build(&self, out: &Path, target: &Target) -> Vec<(PathBuf, Vec<String>)> {
        let dir = out.join(format!("gmp-{GMP_NAME}")).join(GMP_VERSION);
        std::fs::create_dir_all(&dir)
            .unwrap_or_else(|err| panic!("mkdir {}: {err}", dir.display()));
        let library = dir.join(format!("lib{GMP_NAME}.so"));
        let mut link = self.command();
        link.args([
            "-shared",
            "-fPIC",
            // The three entry points and nothing else: not this file's
            // own classes, and not x264, which another library in Gecko's
            // plugin process could have under the same names.
            "-fvisibility=hidden",
            "-Wl,--exclude-libs,ALL",
            "-Wl,-z,defs",
            "-Wl,--as-needed",
            "-s",
        ])
        .arg(&self.source)
        .arg(self.x264.join("lib/libx264.a"))
        .args(["-lpthread", "-lm", "-o"])
        .arg(&library);
        run(
            &mut link,
            "the media plugin's build",
            Path::new("the compiler's output above"),
        );
        let info = dir.join(format!("{GMP_NAME}.info"));
        std::fs::write(&info, GMP_INFO)
            .unwrap_or_else(|err| panic!("write {}: {err}", info.display()));
        println!("cargo:rustc-env=PIIRIT_GMP_PLUGIN={}", dir.display());
        if let Some(profile) = profile_dir(out) {
            install(
                &dir,
                &profile.join(format!("gmp-{GMP_NAME}")).join(GMP_VERSION),
            );
        }

        let mut database = vec![(
            self.source.clone(),
            compile_command(&self.compiler, &self.flags, &self.source),
        )];
        // Run by the tests, so built only where they run: not for a
        // phone, from a machine that is not one.
        if target.cross.is_none() {
            let check = out.join("gmp-check");
            let mut build = self.command();
            build.arg(&self.check).args(["-ldl", "-o"]).arg(&check);
            run(
                &mut build,
                "the media plugin's test harness",
                Path::new("the compiler's output above"),
            );
            println!("cargo:rustc-env=PIIRIT_GMP_CHECK={}", check.display());
            database.push((
                self.check.clone(),
                compile_command(&self.compiler, &self.flags, &self.check),
            ));
        }
        database
    }

    fn command(&self) -> Command {
        let mut command = self.compiler.to_command();
        command.args(&self.flags);
        command
    }
}

/// The directory cargo builds this profile into -- `target/<triple>/
/// release` -- which `OUT_DIR` is three below. The package's spec
/// installs the binary from there, and the plugin with it.
fn profile_dir(out: &Path) -> Option<&Path> {
    let build = out.parent()?.parent()?;
    if build.file_name()? == "build" {
        build.parent()
    } else {
        None
    }
}

/// Copy the plugin's files to `to`, each through a temporary name, so a
/// build running beside this one never finds half a library there.
fn install(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap_or_else(|err| panic!("mkdir {}: {err}", to.display()));
    let files =
        std::fs::read_dir(from).unwrap_or_else(|err| panic!("read {}: {err}", from.display()));
    for file in files {
        let file = file.unwrap_or_else(|err| panic!("read {}: {err}", from.display()));
        let name = file.file_name();
        let partial = to.join(format!(
            ".{}.{}",
            name.to_string_lossy(),
            std::process::id()
        ));
        std::fs::copy(file.path(), &partial)
            .unwrap_or_else(|err| panic!("copy to {}: {err}", partial.display()));
        std::fs::rename(&partial, to.join(&name))
            .unwrap_or_else(|err| panic!("rename {}: {err}", partial.display()));
    }
}

/// One entry's arguments: the compiler, its flags for this target less
/// none, `extra`, and the file.
fn compile_command(compiler: &cc::Tool, extra: &[String], file: &Path) -> Vec<String> {
    let mut arguments = vec![compiler.path().to_string_lossy().into_owned()];
    arguments.extend(
        compiler
            .args()
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned()),
    );
    arguments.extend(extra.iter().cloned());
    arguments.push("-c".into());
    arguments.push(file.to_string_lossy().into_owned());
    arguments
}

/// How each C and C++ file is compiled, as a compilation database at
/// `OUT_DIR/compile_commands.json`. Sonar analyses C and C++ only from
/// one -- it has to know the include paths to read a file at all -- and
/// `make sonar-reports` hands it this. Nothing in the build reads it.
fn write_compile_commands(entries: &[(PathBuf, Vec<String>)], manifest: &Path, out: &Path) {
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
    let entries: Vec<String> = entries
        .iter()
        .map(|(path, arguments)| {
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
