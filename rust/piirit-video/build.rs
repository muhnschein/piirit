//! Builds FFmpeg and x264 (`third_party/` submodules) plus C glue (`csrc/`)
//! into static libs.
//!
//! Static: Harbour allows one executable plus listed system libs; neither codec
//! is listed (`ci/harbour/allowed_libraries.conf`). Built in, they are plain
//! code; validator sees libc, libm, libpthread, which it allows. LAME same way
//! (mp3lame-sys). See docs/BUILDING.md, "Making a video smaller".
//!
//! Trimmed: FFmpeg reads MP4/`QuickTime`, decodes H.264/HEVC/AAC, scales and
//! resamples, encodes AAC, writes MP4; x264 does 8-bit 4:2:0 H.264. Nothing
//! else, so binary grows a few MB, not a few dozen.
//!
//! Compiler is cargo's for this target (`cc` crate), so configure builds for
//! what rest of binary targets. Under Sailfish SDK's scratchbox2 that is the
//! target compiler; configure sees a native build (as
//! LAME's does).
//!
//! Hand-written asm only on ARM, where the same compiler assembles it. x86 would
//! need nasm, which CI runners and SDK emulator target need for nothing else;
//! x86 only hosts tests.
//!
//! x264 also in own lib: H.264 encoder for calls, loaded by Gecko as media plugin
//! (`csrc/gmp_h264.cpp`). Gecko can't call into app binary, only a plugin on
//! disk, so laid out as Gecko looks: `gmp-piirit/<version>/libpiirit.so` beside
//! `piirit.info`, in `OUT_DIR` and beside binary (spec installs from there).
//! Harbour allows package libs under `/usr/share/<name>/lib/`, linked against
//! same system-lib list; x264 inside, symbols hidden.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "build_reuse.rs"]
mod reuse;

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
    println!("cargo:rerun-if-changed=build_reuse.rs");
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

    // Dependency order: each lib after its users.
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
    // FFmpeg's and x264's pkg-config libs, minus libatomic (Harbour forbids):
    // on built arches both inline their atomics. A target needing it fails at
    // link, not intake.
    for lib in ["m", "pthread"] {
        println!("cargo:rustc-link-lib={lib}");
    }
}

/// C glue between FFmpeg, x264 and Rust.
const GLUE: [&str; 2] = ["csrc/recode.c", "csrc/synth.c"];

/// Media plugin source; Gecko stand-in drives it in tests.
const GMP_SOURCE: &str = "csrc/gmp_h264.cpp";
const GMP_CHECK: &str = "tests/gmp_check.cpp";
/// Plugin dir name under Gecko's search dir, less `gmp-`. Gecko looks for
/// `lib<name>.so` and `<name>.info` in the version dir below it.
const GMP_NAME: &str = "piirit";
const GMP_VERSION: &str = "1.0";
/// What Gecko reads. H.264 encode only; decode stays with phone's decoder,
/// which Gecko prefers to any plugin.
const GMP_INFO: &str = "Name: piirit\n\
                        Description: H.264 encoder for Piirit's video calls, built on x264\n\
                        Version: 1.0\n\
                        APIs: encode-video[h264]\n";

/// Plugin and test harness: cargo's C++ compiler, `third_party/gmp-api`, x264.
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

    /// Builds plugin in `OUT_DIR`, copies beside binary, builds harness if tests
    /// can run it. Returns compile commands.
    fn build(&self, out: &Path, target: &Target) -> Vec<(PathBuf, Vec<String>)> {
        let dir = out.join(format!("gmp-{GMP_NAME}")).join(GMP_VERSION);
        std::fs::create_dir_all(&dir)
            .unwrap_or_else(|err| panic!("mkdir {}: {err}", dir.display()));
        let library = dir.join(format!("lib{GMP_NAME}.so"));
        let mut link = self.command();
        link.args([
            "-shared",
            "-fPIC",
            // Export three entry points only: not this file's classes, nor x264,
            // whose names another lib in Gecko's plugin process may share.
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
        // Tests run it: build only where they run (not cross-building for phone).
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

/// Cargo profile dir (`target/<triple>/release`), three above `OUT_DIR`; spec
/// installs binary and plugin from there.
fn profile_dir(out: &Path) -> Option<&Path> {
    let build = out.parent()?.parent()?;
    if build.file_name()? == "build" {
        build.parent()
    } else {
        None
    }
}

/// Copies plugin files via temp name: a build running alongside never sees
/// half a library.
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

/// One entry's args: compiler, target flags, `extra`, file.
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

/// Writes `OUT_DIR/compile_commands.json` for Sonar (`make sonar-reports`),
/// which needs it (include paths) to analyse C/C++ at all. Build never reads it.
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

/// Env var cargo always sets for build scripts.
fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("cargo did not set {name}"))
}

/// Target settings for configure scripts.
struct Target {
    /// C compiler, as cargo runs it for target.
    cc: PathBuf,
    /// Flags minus optimisation/debug levels: libs pick their own; FFmpeg won't
    /// build below -O1 (relies on optimiser to drop configured-out code).
    cflags: Vec<String>,
    ar: PathBuf,
    ranlib: PathBuf,
    /// `CARGO_CFG_TARGET_ARCH`: `aarch64`, `arm`, `x86_64`, `x86`.
    arch: String,
    /// Target triple, if not build machine.
    cross: Option<String>,
    /// Jobs cargo allows this build script.
    jobs: String,
    /// First line of compiler's `--version`, for reuse stamp; `None` if it
    /// won't say.
    version: Option<String>,
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
        let version = Command::new(compiler.path())
            .arg("--version")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| {
                let text = String::from_utf8_lossy(&output.stdout);
                text.lines().next().map(str::to_owned)
            });
        Self {
            cc: compiler.path().to_path_buf(),
            cflags,
            ar: build.get_archiver().get_program().into(),
            ranlib: build.get_ranlib().get_program().into(),
            arch: env("CARGO_CFG_TARGET_ARCH"),
            cross: (target != host).then_some(target),
            jobs: std::env::var("NUM_JOBS").unwrap_or_else(|_| "1".into()),
            version,
        }
    }

    /// Whether asm is left out; see module doc.
    fn without_asm(&self) -> bool {
        self.arch == "x86" || self.arch == "x86_64"
    }

    /// Flags as one argument, for `--extra-cflags`.
    fn cflags(&self) -> String {
        self.cflags.join(" ")
    }
}

/// Configures and builds x264 out of tree, installs under `OUT_DIR/x264`
/// (kept if there from same inputs: `build_reuse.rs`); returns prefix.
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
    let make = make(&build, target, "install-lib-static");
    configure_and_make(
        "x264",
        src,
        &prefix,
        configure,
        make,
        &build.join("config.log"),
        target,
    );
    prefix
}

/// Configures and builds FFmpeg out of tree, installs under `OUT_DIR/ffmpeg`
/// (kept if there from same inputs: `build_reuse.rs`); returns prefix.
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
            // Static PIC library; no programs or docs.
            "--enable-static",
            "--disable-shared",
            "--enable-pic",
            "--disable-programs",
            "--disable-doc",
            "--disable-debug",
            // Nothing used just because the build machine has it: only what's
            // named below.
            "--disable-autodetect",
            "--enable-pthreads",
            "--disable-network",
            "--disable-avdevice",
            "--disable-avfilter",
            "--disable-everything",
            // Read MP4/QuickTime (phone recordings), decode, write MP4.
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
    let make = make(&build, target, "install");
    configure_and_make(
        "FFmpeg",
        src,
        &prefix,
        configure,
        make,
        &build.join("ffbuild/config.log"),
        target,
    );
    prefix
}

/// `make <goal>` in `build` with the jobs cargo allows.
fn make(build: &Path, target: &Target, goal: &str) -> Command {
    let mut make = Command::new("make");
    make.current_dir(build)
        .arg(format!("-j{}", target.jobs))
        .arg(goal);
    make
}

/// Runs `configure` then `make` into `prefix`, unless an install from the same
/// inputs is already there (`build_reuse.rs`).
fn configure_and_make(
    name: &str,
    src: &Path,
    prefix: &Path,
    mut configure: Command,
    mut make: Command,
    log: &Path,
    target: &Target,
) {
    let inputs = reuse::inputs(
        src,
        &[&configure, &make],
        target.version.as_deref(),
        &|key| std::env::var_os(key),
    );
    if inputs
        .as_deref()
        .is_some_and(|inputs| reuse::is_current(prefix, inputs))
    {
        return;
    }
    reuse::forget(prefix);
    run(&mut configure, &format!("{name}'s configure"), log);
    run(&mut make, &format!("{name}'s make"), log);
    if let Some(inputs) = inputs {
        reuse::record(prefix, &inputs);
    }
}

/// Runs a step; on failure, names it and its log.
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
