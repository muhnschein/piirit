//! When build.rs keeps FFmpeg and x264 from an earlier run (`build_reuse.rs`):
//! only on the same inputs, never when it can't tell.

#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../build_reuse.rs"]
mod reuse;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const COMMIT: &str = "18c174a80fc080e2e3f09f9acdebe40e15b7ddd7";
const COMPILER: &str = "aarch64-meego-linux-gnu-gcc (GCC) 8.3.0";

/// Scratch dir, removed when dropped (panic too).
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("piirit-build-reuse-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Submodule layout as `git submodule update` leaves it: `src/.git` file
/// pointing at `modules/src`, HEAD there.
fn submodule(scratch: &Path, head: &str) -> PathBuf {
    let src = scratch.join("src");
    let git_dir = scratch.join("modules/src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&git_dir).unwrap();
    std::fs::write(src.join(".git"), "gitdir: ../modules/src\n").unwrap();
    std::fs::write(git_dir.join("HEAD"), head).unwrap();
    src
}

fn no_env(_: &str) -> Option<OsString> {
    None
}

fn configure(flag: &str) -> Command {
    let mut command = Command::new("/src/configure");
    command.env("CC", "cc").arg("--enable-static").arg(flag);
    command
}

fn make(jobs: u32) -> Command {
    let mut command = Command::new("make");
    command.arg(format!("-j{jobs}")).arg("install");
    command
}

fn stamp(src: &Path, steps: &[&Command]) -> String {
    reuse::inputs(src, steps, Some(COMPILER), &no_env).expect("stamp")
}

#[test]
fn a_submodule_names_its_commit() {
    let scratch = Scratch::new("submodule");
    let src = submodule(&scratch.0, &format!("{COMMIT}\n"));
    assert_eq!(reuse::commit(&src).as_deref(), Some(COMMIT));
}

#[test]
fn a_clone_of_its_own_names_its_commit() {
    let scratch = Scratch::new("clone");
    std::fs::create_dir_all(scratch.0.join(".git")).unwrap();
    std::fs::write(scratch.0.join(".git/HEAD"), COMMIT).unwrap();
    assert_eq!(reuse::commit(&scratch.0).as_deref(), Some(COMMIT));
}

#[test]
fn no_commit_when_it_cant_be_read() {
    let scratch = Scratch::new("unreadable");
    // Tarball: no .git at all.
    assert_eq!(reuse::commit(&scratch.0), None);
    // On a branch, not a commit.
    let src = submodule(&scratch.0, "ref: refs/heads/release/7.1\n");
    assert_eq!(reuse::commit(&src), None);
    // Cut short.
    std::fs::write(scratch.0.join("modules/src/HEAD"), &COMMIT[..39]).unwrap();
    assert_eq!(reuse::commit(&src), None);
    // Not hex.
    std::fs::write(scratch.0.join("modules/src/HEAD"), "z".repeat(40)).unwrap();
    assert_eq!(reuse::commit(&src), None);
    // `.git` file that points nowhere useful.
    std::fs::write(src.join(".git"), "not a gitdir line").unwrap();
    assert_eq!(reuse::commit(&src), None);
}

#[test]
fn no_stamp_without_commit_or_compiler() {
    let scratch = Scratch::new("no-stamp");
    let configure = configure("--disable-asm");
    assert_eq!(
        reuse::inputs(&scratch.0, &[&configure], Some(COMPILER), &no_env),
        None
    );
    let src = submodule(&scratch.0, COMMIT);
    assert_eq!(reuse::inputs(&src, &[&configure], None, &no_env), None);
}

#[test]
fn same_inputs_same_stamp_whatever_the_job_count() {
    let scratch = Scratch::new("same");
    let src = submodule(&scratch.0, COMMIT);
    let configure = configure("--disable-asm");
    assert_eq!(
        stamp(&src, &[&configure, &make(1)]),
        stamp(&src, &[&configure, &make(4)])
    );
}

#[test]
fn each_input_changes_the_stamp() {
    let scratch = Scratch::new("each");
    let src = submodule(&scratch.0, COMMIT);
    let configure_asm = configure("--disable-asm");
    let make = make(1);
    let base = stamp(&src, &[&configure_asm, &make]);

    // Other configure flag.
    let other_flag = configure("--enable-asm");
    assert_ne!(base, stamp(&src, &[&other_flag, &make]));

    // Other configure script.
    let mut other_script = Command::new("/elsewhere/configure");
    other_script
        .env("CC", "cc")
        .arg("--enable-static")
        .arg("--disable-asm");
    assert_ne!(base, stamp(&src, &[&other_script, &make]));

    // Other env given to configure.
    let mut other_cc = configure("--disable-asm");
    other_cc.env("CC", "clang");
    assert_ne!(base, stamp(&src, &[&other_cc, &make]));

    // Other make target.
    let mut install_lib = Command::new("make");
    install_lib.arg("-j1").arg("install-lib-static");
    assert_ne!(base, stamp(&src, &[&configure_asm, &install_lib]));

    // Other compiler.
    let other_compiler = reuse::inputs(
        &src,
        &[&configure_asm, &make],
        Some("aarch64-meego-linux-gnu-gcc (GCC) 10.3.1"),
        &no_env,
    );
    assert_ne!(Some(base.clone()), other_compiler);

    // Env configure reads itself.
    let cflags = |key: &str| (key == "CFLAGS").then(|| OsString::from("-O3"));
    let with_cflags = reuse::inputs(&src, &[&configure_asm, &make], Some(COMPILER), &cflags);
    assert_ne!(Some(base.clone()), with_cflags);

    // Other source commit.
    std::fs::write(
        scratch.0.join("modules/src/HEAD"),
        "0123456789abcdef0123456789abcdef01234567",
    )
    .unwrap();
    assert_ne!(base, stamp(&src, &[&configure_asm, &make]));
}

#[test]
fn an_install_is_kept_only_once_recorded_and_for_its_own_inputs() {
    let scratch = Scratch::new("record");
    let prefix = &scratch.0;
    assert!(!reuse::is_current(prefix, "inputs a"));
    reuse::record(prefix, "inputs a");
    assert!(reuse::is_current(prefix, "inputs a"));
    assert!(!reuse::is_current(prefix, "inputs b"));
    // A build starting forgets it, so one cut short is built again.
    reuse::forget(prefix);
    assert!(!reuse::is_current(prefix, "inputs a"));
    // Nothing to forget: fine.
    reuse::forget(prefix);
}

/// This build's own install: stamped with the submodule's commit when the
/// checkout has one, unstamped (built every time) when it hasn't.
#[test]
fn this_build_stamped_its_libs_by_commit() {
    let third_party = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third_party");
    let out = Path::new(env!("OUT_DIR"));
    for lib in ["ffmpeg", "x264"] {
        let stamp = std::fs::read_to_string(out.join(lib).join(reuse::STAMP)).ok();
        match reuse::commit(&third_party.join(lib)) {
            Some(commit) => {
                let stamp = stamp.unwrap_or_else(|| panic!("{lib}: no stamp"));
                assert!(
                    stamp.starts_with(&format!("commit {commit}\n")),
                    "{lib}: {stamp}"
                );
            }
            None => assert_eq!(stamp, None, "{lib}: stamped without a commit"),
        }
    }
}
