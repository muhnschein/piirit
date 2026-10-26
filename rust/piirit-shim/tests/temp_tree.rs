//! `common::temp_tree`: test's temp dir starts empty, goes when test ends, failed too.
//!
//! Left behind, `/tmp` filled: `webxdc_app` alone left a 100 MB video a run.

#![allow(clippy::expect_used)]

use std::path::PathBuf;

mod common;

#[test]
fn a_test_temp_dir_starts_empty_and_goes_when_dropped_or_failed() {
    // Left by an earlier process with this pid.
    let path = std::env::temp_dir().join(format!("piirit-temp-tree-{}", std::process::id()));
    std::fs::create_dir_all(path.join("old")).expect("make a stale dir");

    let tree = common::temp_tree("temp-tree");
    assert_eq!(*tree, *path, "the dir is not named after the test and pid");
    assert_eq!(
        std::fs::read_dir(&*tree).expect("read the dir").count(),
        0,
        "the dir kept what an earlier run left"
    );
    std::fs::create_dir_all(tree.join("cache/outbox")).expect("make a subdir");
    std::fs::write(tree.join("cache/outbox/video.mp4"), "big").expect("write a file");
    drop(tree);
    assert!(!path.exists(), "the dir is still there once dropped");

    let mut root = PathBuf::new();
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let tree = common::temp_tree("temp-tree");
        root = tree.to_path_buf();
        std::fs::write(tree.join("journal.jsonl"), "{}").expect("write a file");
        panic!("a test failing with its temp dir in hand");
    }));
    assert!(failed.is_err());
    assert!(!root.as_os_str().is_empty(), "the dir was never made");
    assert!(!root.exists(), "a failing test left its temp dir behind");
}
