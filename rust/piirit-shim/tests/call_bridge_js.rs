//! Call page bridge and map's opening view (`map_view.js`), tested in Node: `tests/js/*.test.js`.
//!
//! Bridge is JS run in browser engine beside upstream page; Rust can't run it.
//! Node runs it against stand-ins for page and engine: camera, peer connection,
//! page and host messages.
//!
//! Node on every CI runner. No Node on laptop: says so, passes. In CI (`CI` set)
//! missing Node fails.

#![allow(clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn the_bridge_does_what_its_node_tests_say() {
    let root = repo_root();
    let mut files: Vec<PathBuf> = std::fs::read_dir(root.join("tests/js"))
        .expect("tests/js is there")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".test.js"))
        })
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no tests under tests/js");

    let ran = Command::new("node")
        .arg("--test")
        .args(&files)
        .current_dir(&root)
        .output();
    let output = match ran {
        Ok(output) => output,
        Err(err) => {
            assert!(
                std::env::var_os("CI").is_none(),
                "node is not installed, and CI must run the bridge's tests: {err}"
            );
            eprintln!("call_bridge_js: SKIP (node is not installed: {err})");
            return;
        }
    };
    assert!(
        output.status.success(),
        "the bridge's tests failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
