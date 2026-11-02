//! Builds whisper.cpp and ggml (`third_party/whisper.cpp`, CPU backend only) plus
//! glue (`csrc/speech.cpp`) into shared libs `src/lib.rs` loads at run time.
//!
//! Loaded, not linked: aarch64 phones differ (Xperia 10 II: no dotprod), so one
//! lib per CPU level, picked at run time; app binary runs on all. Harbour
//! allows `.so` under `/usr/share/<name>/lib/` linking only `libc`, `libm`,
//! `libpthread`, `libstdc++`, `libgcc_s`: whisper.cpp, ggml inside, hidden; only
//! `csrc/speech.h`'s three functions exported.
//!
//! `cc` crate's compiler, no `CMake`: under scratchbox2 that is target's own
//! (as piirit-video); configure would probe host and fetch (`KleidiAI`).
//! Defines mirror whisper.cpp's `CMake` at default options minus `OpenMP`
//! (libgomp not Harbour-allowed: ggml's own thread pool), BLAS, GPU backends.
//!
//! whisper.cpp + ggml objects kept across builds while inputs same
//! (piirit-video's `build_reuse.rs`); glue built every time. Libs land in
//! `OUT_DIR/speech` (`PIIRIT_SPEECH_LIBS`, for tests) and beside binary (spec
//! installs from there).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

#[path = "../piirit-video/build_reuse.rs"]
mod reuse;

/// Lib file name stem; `src/lib.rs` adds CPU level and `.so`.
const LIB: &str = "libpiirit-speech";
const GLUE: &str = "csrc/speech.cpp";
/// Only these leave the lib.
const EXPORTS: &str =
    "{ global: piirit_speech_load; piirit_speech_free; piirit_speech_run; local: *; };\n";

/// ggml base, registry, CPU backend: C, C++.
const GGML_C: [&str; 5] = [
    "ggml/src/ggml.c",
    "ggml/src/ggml-alloc.c",
    "ggml/src/ggml-quants.c",
    "ggml/src/ggml-cpu/ggml-cpu.c",
    "ggml/src/ggml-cpu/quants.c",
];
const GGML_CXX: [&str; 20] = [
    "ggml/src/ggml.cpp",
    "ggml/src/ggml-backend.cpp",
    "ggml/src/ggml-backend-meta.cpp",
    "ggml/src/ggml-opt.cpp",
    "ggml/src/ggml-threading.cpp",
    "ggml/src/gguf.cpp",
    "ggml/src/ggml-backend-reg.cpp",
    "ggml/src/ggml-backend-dl.cpp",
    "ggml/src/ggml-cpu/ggml-cpu.cpp",
    "ggml/src/ggml-cpu/repack.cpp",
    "ggml/src/ggml-cpu/hbm.cpp",
    "ggml/src/ggml-cpu/traits.cpp",
    "ggml/src/ggml-cpu/amx/amx.cpp",
    "ggml/src/ggml-cpu/amx/mmq.cpp",
    "ggml/src/ggml-cpu/tiled/tiled.cpp",
    "ggml/src/ggml-cpu/tiled/tiled-kernel.cpp",
    "ggml/src/ggml-cpu/binary-ops.cpp",
    "ggml/src/ggml-cpu/unary-ops.cpp",
    "ggml/src/ggml-cpu/vec.cpp",
    "ggml/src/ggml-cpu/ops.cpp",
];
/// whisper.cpp; ANE file is an empty stub off Apple, still referenced.
const WHISPER_CXX: [&str; 2] = ["src/whisper.cpp", "src/aneforge/whisper-aneforge.cpp"];

fn main() {
    let manifest = PathBuf::from(env("CARGO_MANIFEST_DIR"));
    let src = manifest.join("../../third_party/whisper.cpp");
    assert!(
        src.join("include/whisper.h").is_file(),
        "{} is empty. whisper.cpp is a git submodule: run\n    \
         git submodule update --init --depth 1\nfrom the repository root.",
        src.display()
    );
    println!("cargo:rerun-if-changed={}", src.display());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../piirit-video/build_reuse.rs");
    println!("cargo:rerun-if-changed=csrc");
    println!("cargo:rerun-if-env-changed=PIIRIT_MAKE_JOBS");

    let out = PathBuf::from(env("OUT_DIR"));
    let arch = env("CARGO_CFG_TARGET_ARCH");
    let tools = Tools::from_env();
    let libs = out.join("speech");
    std::fs::create_dir_all(&libs).unwrap_or_else(|err| panic!("mkdir {}: {err}", libs.display()));
    let exports = out.join("speech.map");
    std::fs::write(&exports, EXPORTS)
        .unwrap_or_else(|err| panic!("write {}: {err}", exports.display()));

    let mut database = Vec::new();
    for (suffix, level) in levels(&arch) {
        let archive = build_whisper(&src, &out, &tools, (&arch, suffix, level));
        let glue = manifest.join(GLUE);
        let glue_flags = glue_flags(&src, level);
        let object = out.join(format!("speech{suffix}.o"));
        let mut compile = tools.cxx.to_command();
        compile
            .args(&glue_flags)
            .arg("-c")
            .arg(&glue)
            .arg("-o")
            .arg(&object);
        run(&mut compile, "the speech glue's build");
        if database.is_empty() {
            database.push((
                glue.clone(),
                compile_command(&tools.cxx, &glue_flags, &glue),
            ));
        }

        let library = libs.join(format!("{LIB}{suffix}.so"));
        let mut link = tools.cxx.to_command();
        link.args(level)
            .args(["-shared", "-fPIC", "-pthread", "-s"])
            .arg(format!("-Wl,--version-script={}", exports.display()))
            .args(["-Wl,--exclude-libs,ALL", "-Wl,-z,defs", "-Wl,--as-needed"])
            .arg(&object)
            .arg(&archive)
            .args(tools.filesystem_lib())
            .args(["-ldl", "-lm", "-o"])
            .arg(&library);
        run(&mut link, "the speech engine's link");
    }
    write_compile_commands(&database, &manifest, &out);
    println!("cargo:rustc-env=PIIRIT_SPEECH_LIBS={}", libs.display());
    if let Some(profile) = profile_dir(&out) {
        install(&libs, &profile.join("speech"));
    }
}

/// Lib per CPU level: name suffix, flags. aarch64: 8.0 (Xperia 10 II; `OnePlus` 6, no
/// dotprod) and 8.2 with dotprod + fp16 (Jolla Phone 2026). `x86_64` (test hosts only): one,
/// AVX2 + F16C level every CI runner has, not `native` (runners differ past it);
/// plain SSE2: ~1 min per test-model run.
fn levels(arch: &str) -> Vec<(&'static str, &'static [&'static str])> {
    match arch {
        "aarch64" => vec![
            ("-v8", &["-march=armv8-a"]),
            ("-v82", &["-march=armv8.2-a+dotprod+fp16"]),
        ],
        "x86_64" => vec![("", &["-march=x86-64-v3"])],
        _ => vec![("", &[])],
    }
}

/// Compilers and archiver, as cargo runs them for target.
struct Tools {
    cc: cc::Tool,
    cxx: cc::Tool,
    ar: PathBuf,
    /// Target flags minus cc crate's optimisation and debug levels.
    cflags: Vec<String>,
    cxxflags: Vec<String>,
    /// `--version`, for reuse stamp.
    version: Option<String>,
    /// GCC major, if GCC.
    gcc_major: Option<u32>,
    jobs: usize,
}

impl Tools {
    fn from_env() -> Self {
        let build = cc::Build::new();
        let cc = build.get_compiler();
        let cxx = cc::Build::new().cpp(true).get_compiler();
        let flags = |tool: &cc::Tool| -> Vec<String> {
            tool.args()
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .filter(|arg| !arg.starts_with("-O") && !arg.starts_with("-g"))
                .collect()
        };
        let output = |arg: &str| {
            Command::new(cxx.path())
                .arg(arg)
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        };
        let version = output("--version").and_then(|text| text.lines().next().map(str::to_owned));
        let gcc_major = version
            .as_deref()
            .filter(|version| !version.contains("clang"))
            .and_then(|_| output("-dumpversion"))
            .and_then(|text| text.split('.').next().and_then(|major| major.parse().ok()));
        let jobs = std::env::var("PIIRIT_MAKE_JOBS")
            .or_else(|_| std::env::var("NUM_JOBS"))
            .ok()
            .and_then(|jobs| jobs.parse().ok())
            .unwrap_or(1);
        Self {
            cflags: flags(&cc),
            cxxflags: flags(&cxx),
            cc,
            cxx,
            ar: build.get_archiver().get_program().into(),
            version,
            gcc_major,
            jobs,
        }
    }

    /// GCC before 9 keeps `std::filesystem` (ggml-backend-reg.cpp) in a static lib.
    fn filesystem_lib(&self) -> Option<&'static str> {
        self.gcc_major
            .is_some_and(|major| major < 9)
            .then_some("-lstdc++fs")
    }
}

/// Compiles whisper.cpp + ggml for one CPU level into `OUT_DIR/whisper<suffix>/`,
/// archived; kept if there from same inputs. Returns archive.
fn build_whisper(
    src: &Path,
    out: &Path,
    tools: &Tools,
    (arch, suffix, level): (&str, &str, &[&str]),
) -> PathBuf {
    let dir = out.join(format!("whisper{suffix}"));
    let objects = dir.join("objects");
    let archive = dir.join("libwhisper.a");
    std::fs::create_dir_all(&objects)
        .unwrap_or_else(|err| panic!("mkdir {}: {err}", objects.display()));
    let (generated, whisper_version) = version_header(src, &dir);

    let mut units = Vec::new();
    let mut names = Vec::new();
    let defines = (generated.as_path(), whisper_version.as_str());
    for (tool, flags, file) in sources(src, tools, (arch, level), defines) {
        let name = format!("{}.o", file.replace('/', "_"));
        let mut command = tool.to_command();
        command
            .args(&flags)
            .arg("-c")
            .arg(src.join(&file))
            .arg("-o")
            .arg(objects.join(&name));
        names.push(name);
        units.push(command);
    }
    let mut pack = Command::new(&tools.ar);
    pack.current_dir(&objects)
        .arg("crs")
        .arg(&archive)
        .args(&names);

    let steps: Vec<&Command> = units.iter().chain([&pack]).collect();
    let inputs = reuse::inputs(src, &steps, tools.version.as_deref(), &|key| {
        std::env::var_os(key)
    });
    if archive.is_file()
        && inputs
            .as_deref()
            .is_some_and(|inputs| reuse::is_current(&dir, inputs))
    {
        return archive;
    }
    reuse::forget(&dir);
    compile_all(units, tools.jobs);
    if archive.is_file() {
        std::fs::remove_file(&archive)
            .unwrap_or_else(|err| panic!("remove {}: {err}", archive.display()));
    }
    run(&mut pack, "whisper.cpp's archive");
    if let Some(inputs) = inputs {
        reuse::record(&dir, &inputs);
    }
    archive
}

/// `ggml-version.h`, which ggml's `CMake` generates, in `dir/include`; returns that dir
/// and whisper.cpp's version.
fn version_header(src: &Path, dir: &Path) -> (PathBuf, String) {
    let version = |cmake: &str, name: &str| {
        let text = std::fs::read_to_string(src.join(cmake))
            .unwrap_or_else(|err| panic!("read {cmake}: {err}"));
        ["MAJOR", "MINOR", "PATCH"]
            .iter()
            .map(|part| {
                let key = format!("set({name}_VERSION_{part} ");
                text.lines()
                    .find_map(|line| line.trim().strip_prefix(&key))
                    .and_then(|rest| rest.strip_suffix(')'))
                    .unwrap_or_else(|| panic!("no {name}_VERSION_{part} in {cmake}"))
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join(".")
    };
    let commit = reuse::commit(src)
        .and_then(|commit| commit.get(..7).map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_owned());
    let generated = dir.join("include");
    std::fs::create_dir_all(&generated)
        .unwrap_or_else(|err| panic!("mkdir {}: {err}", generated.display()));
    let header = generated.join("ggml-version.h");
    std::fs::write(
        &header,
        format!(
            "#pragma once\n#define GGML_VERSION \"{}\"\n#define GGML_COMMIT \"{commit}\"\n",
            version("ggml/CMakeLists.txt", "GGML")
        ),
    )
    .unwrap_or_else(|err| panic!("write {}: {err}", header.display()));
    (generated, version("CMakeLists.txt", "WHISPER"))
}

/// Every source with compiler and flags, as whisper.cpp's `CMake` would build it.
fn sources<'a>(
    src: &Path,
    tools: &'a Tools,
    (arch, level): (&str, &[&str]),
    (generated, whisper_version): (&Path, &str),
) -> Vec<(&'a cc::Tool, Vec<String>, String)> {
    let include = |path: &str| format!("-I{}", src.join(path).display());
    let common = |flags: &[String], standard: &str| -> Vec<String> {
        let mut all = flags.to_vec();
        all.extend(level.iter().map(|flag| (*flag).to_owned()));
        all.extend(
            [
                standard,
                "-O3",
                "-DNDEBUG",
                "-fPIC",
                "-pthread",
                "-fvisibility=hidden",
            ]
            .map(str::to_owned),
        );
        all
    };
    // ggml/src/CMakeLists.txt: POSIX level, copies; CPU backend in, repack on.
    let ggml = [
        "-D_XOPEN_SOURCE=600".to_owned(),
        "-D_GNU_SOURCE".to_owned(),
        "-DGGML_SCHED_MAX_COPIES=4".to_owned(),
        "-DGGML_USE_CPU".to_owned(),
        "-DGGML_USE_CPU_REPACK".to_owned(),
        include("ggml/include"),
        include("ggml/src"),
        include("ggml/src/ggml-cpu"),
        format!("-I{}", generated.display()),
    ];
    let arch_dir = match arch {
        "aarch64" | "arm" => "arm",
        "x86_64" | "x86" => "x86",
        other => panic!("no ggml CPU code picked for {other}"),
    };

    let mut c_flags = common(&tools.cflags, "-std=c11");
    c_flags.extend(ggml.iter().cloned());
    let mut cxx_flags = common(&tools.cxxflags, "-std=c++17");
    cxx_flags.extend(ggml.iter().cloned());
    let mut whisper_flags = common(&tools.cxxflags, "-std=c++17");
    whisper_flags.push(format!("-DWHISPER_VERSION=\"{whisper_version}\""));
    whisper_flags.extend([include("include"), include("src"), include("ggml/include")]);

    let c = GGML_C
        .iter()
        .map(|&file| file.to_owned())
        .chain([format!("ggml/src/ggml-cpu/arch/{arch_dir}/quants.c")])
        .map(|file| (&tools.cc, c_flags.clone(), file));
    let cxx = GGML_CXX
        .iter()
        .map(|&file| file.to_owned())
        .chain([format!("ggml/src/ggml-cpu/arch/{arch_dir}/repack.cpp")])
        .map(|file| (&tools.cxx, cxx_flags.clone(), file));
    let whisper = WHISPER_CXX
        .iter()
        .map(|&file| (&tools.cxx, whisper_flags.clone(), file.to_owned()));
    c.chain(cxx).chain(whisper).collect()
}

/// Runs compile commands, `jobs` at a time.
fn compile_all(units: Vec<Command>, jobs: usize) {
    let next = AtomicUsize::new(0);
    let units: Vec<Mutex<Command>> = units.into_iter().map(Mutex::new).collect();
    std::thread::scope(|scope| {
        for _ in 0..jobs.clamp(1, units.len().max(1)) {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(unit) = units.get(index) else {
                    break;
                };
                let mut command = unit
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                run(&mut command, "whisper.cpp's build");
            });
        }
    });
}

/// Glue's flags: warnings on (our code), whisper.cpp headers.
fn glue_flags(src: &Path, level: &[&str]) -> Vec<String> {
    let mut flags: Vec<String> = level.iter().map(|flag| (*flag).to_owned()).collect();
    flags.extend(
        [
            "-std=c++17",
            "-O2",
            "-fPIC",
            "-fvisibility=hidden",
            "-Wall",
            "-Wextra",
        ]
        .map(str::to_owned),
    );
    flags.push(format!("-I{}", src.join("include").display()));
    flags.push(format!("-I{}", src.join("ggml/include").display()));
    flags
}

/// Cargo profile dir (`target/<triple>/release`), three above `OUT_DIR`; spec
/// installs binary and libs from there.
fn profile_dir(out: &Path) -> Option<&Path> {
    let build = out.parent()?.parent()?;
    if build.file_name()? == "build" {
        build.parent()
    } else {
        None
    }
}

/// Copies libs via temp name: a build running alongside never sees half a library.
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

/// `OUT_DIR/compile_commands.json` for Sonar (`make sonar-reports` merges it with
/// piirit-video's). Build never reads it.
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

/// Runs a step; on failure, names it (compiler's message is above it).
fn run(command: &mut Command, what: &str) {
    let status = command
        .status()
        .unwrap_or_else(|err| panic!("{what} could not be started: {err}"));
    assert!(status.success(), "{what} failed ({status}): {command:?}");
}
