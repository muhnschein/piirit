//! Whether FFmpeg or x264 installed in `OUT_DIR` by an earlier run can stay.
//!
//! Cargo reruns build script when any file under `third_party/ffmpeg` is newer
//! than last run, by mtime. Fresh checkout (every CI run) = every file new, so
//! configure + make ran from zero each time with libs from cache sitting there:
//! ~8-12 min of a device build at one job. Decided on content instead: stamp
//! beside install names everything that makes its bytes; same stamp = keep.
//!
//! In stamp: submodule commit, configure and make command (program, args, env
//! set; `-j` left out, changes no byte), configure-read env, compiler's
//! `--version`. No commit readable (tarball build, OBS) = no stamp, always
//! built. Edits inside a submodule without a new commit not seen: rebuild with
//! `cargo clean -p piirit-video`.
//!
//! Own file: build.rs and `tests/build_reuse.rs` both include it.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

/// Stamp file in install prefix.
pub const STAMP: &str = "piirit-inputs";

/// Env both configure scripts read on top of their arguments.
const CONFIGURE_ENV: [&str; 4] = ["CFLAGS", "CPPFLAGS", "LDFLAGS", "ASFLAGS"];

/// Commit checked out in git checkout `src` (submodule or own clone). Read
/// from files, not `git`: under scratchbox2 a spawned `git` is the target's.
pub fn commit(src: &Path) -> Option<String> {
    let dot_git = src.join(".git");
    let git_dir = if dot_git.is_dir() {
        dot_git
    } else {
        // Submodule: `.git` is a file, `gitdir: <path>`, relative to `src`.
        let link = std::fs::read_to_string(&dot_git).ok()?;
        src.join(link.strip_prefix("gitdir:")?.trim())
    };
    // Submodules check out detached: HEAD is the commit itself. A branch
    // (`ref: ...`) = no stamp, built as before.
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    (head.len() == 40 && head.bytes().all(|b| b.is_ascii_hexdigit())).then(|| head.to_owned())
}

/// Everything that decides an install's bytes, as stamp text. `None`: can't
/// tell, so never reused. `env`: build script's environment (build.rs passes
/// `std::env::var_os`).
pub fn inputs(
    src: &Path,
    steps: &[&Command],
    compiler: Option<&str>,
    env: &dyn Fn(&str) -> Option<OsString>,
) -> Option<String> {
    let mut text = format!("commit {}\ncompiler {}\n", commit(src)?, compiler?);
    for step in steps {
        let _ = writeln!(text, "run {}", step.get_program().to_string_lossy());
        for arg in step.get_args() {
            let arg = arg.to_string_lossy();
            if !arg.starts_with("-j") {
                let _ = writeln!(text, "  arg {arg}");
            }
        }
        for (key, value) in step.get_envs() {
            let value = value.map_or_else(|| "(removed)".into(), |v| v.to_string_lossy());
            let _ = writeln!(text, "  env {}={value}", key.to_string_lossy());
        }
    }
    for key in CONFIGURE_ENV {
        if let Some(value) = env(key) {
            let _ = writeln!(text, "env {key}={}", value.to_string_lossy());
        }
    }
    Some(text)
}

/// Whether `prefix` holds a finished install from `inputs`.
pub fn is_current(prefix: &Path, inputs: &str) -> bool {
    std::fs::read_to_string(prefix.join(STAMP)).is_ok_and(|stamp| stamp == inputs)
}

/// Drops stamp before a build: one cut short leaves none, so next run builds.
pub fn forget(prefix: &Path) {
    match std::fs::remove_file(prefix.join(STAMP)) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
            panic!("remove {}: {err}", prefix.join(STAMP).display())
        }
        _ => {}
    }
}

/// Records `inputs` after a finished install.
pub fn record(prefix: &Path, inputs: &str) {
    let stamp = prefix.join(STAMP);
    std::fs::write(&stamp, inputs).unwrap_or_else(|err| panic!("write {}: {err}", stamp.display()));
}
