//! `common::qml_tree_without_enter_key`: copy loads like app tree, costs little, leaves nothing.
//!
//! 30 tests take one each, so a copy left behind or copied whole filled `/tmp` (12 MB a test,
//! ~800 MB a `make check`).

#![allow(clippy::expect_used)]

use std::path::{Path, PathBuf};

mod common;

fn app_tree() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml")
}

/// Bytes of real files below `dir`; links not followed.
fn bytes_below(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .expect("read the copy")
        .map(|entry| {
            let entry = entry.expect("read a copy entry");
            let kind = entry.file_type().expect("stat a copy entry");
            if kind.is_dir() {
                bytes_below(&entry.path())
            } else if kind.is_symlink() {
                0
            } else {
                entry.metadata().expect("stat a copy entry").len()
            }
        })
        .sum()
}

#[test]
fn the_copy_reaches_every_app_file_holds_only_qml_and_goes_when_dropped_or_failed() {
    let tree = common::qml_tree_without_enter_key();
    let root = tree.to_path_buf();

    // Same pages, components and pictures as app tree, by same relative paths.
    for file in [
        "piirit.qml",
        "pages/ChatListPage.qml",
        "components/Settings.qml",
        "js/Emoji.js",
        "art/intro-profile.png",
    ] {
        assert!(root.join(file).is_file(), "{file} is missing from the copy");
    }
    let page = std::fs::read_to_string(root.join("pages/ChatListPage.qml")).expect("read a page");
    assert!(
        !page
            .lines()
            .any(|line| line.trim_start().starts_with("EnterKey.")),
        "the copy kept an `EnterKey.` line"
    );

    for copied in ["pages", "pages/ChatListPage.qml"] {
        let kind = std::fs::symlink_metadata(root.join(copied))
            .expect("stat a copied page")
            .file_type();
        assert!(
            !kind.is_symlink(),
            "{copied} is linked, so `EnterKey.` lines would reach the engine"
        );
    }

    // Pictures linked, not copied: `art/` alone is 11 MB.
    let copied = bytes_below(&root);
    assert!(
        copied < 2 * 1024 * 1024,
        "the copy holds {copied} bytes of files; only .qml files should be copied"
    );

    drop(tree);
    assert!(!root.exists(), "the copy is still there once dropped");
    assert!(
        app_tree().join("art/intro-profile.png").is_file(),
        "removing the copy reached into the app tree"
    );

    // Failing test: copy goes too. Same test, not a second: copy is per process, `cargo test`
    // runs a binary's tests side by side.
    let mut root = PathBuf::new();
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let tree = common::qml_tree_without_enter_key();
        root = tree.to_path_buf();
        panic!("a test failing with the copy in hand");
    }));
    assert!(failed.is_err());
    assert!(!root.as_os_str().is_empty(), "the copy was never made");
    assert!(!root.exists(), "a failing test left its copy behind");
}
