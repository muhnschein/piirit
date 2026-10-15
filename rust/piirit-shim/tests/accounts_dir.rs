//! Old-name profile moved into account dir.
//!
//! Sailjail grants `~/.local/share/<Org>/<App>` only. App was Postivene; old
//! `postivene/postivene/accounts` is outside. Never overwrites new profile.

// `set_var`: unsafe from edition 2024 only.
#![allow(unsafe_code, unused_unsafe, clippy::expect_used)]

use std::fs;
use std::path::Path;

use piirit_shim::DeltaChatCore;

fn marker(dir: &Path, name: &str) {
    fs::create_dir_all(dir).expect("make dir");
    fs::write(dir.join(name), b"x").expect("write marker");
}

#[test]
fn a_profile_from_the_old_name_is_adopted_but_never_overwrites_one() {
    let root = std::env::temp_dir().join(format!("piirit-accounts-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let data = root.join("share");

    // SAFETY: single-threaded test binary.
    unsafe {
        std::env::remove_var("PIIRIT_ACCOUNTS_DIR");
        std::env::set_var("XDG_DATA_HOME", &data);
    }

    // Old-name build profile.
    let legacy = data.join("postivene/postivene/accounts");
    marker(&legacy, "old.db");

    let chosen = DeltaChatCore::accounts_dir().expect("accounts dir");
    let wanted = data.join("piirit/piirit/accounts");
    assert_eq!(
        Path::new(&chosen),
        wanted,
        "accounts must sit inside the directory sailjail grants"
    );
    assert!(
        wanted.join("old.db").is_file(),
        "the profile from under the old name was not carried across"
    );
    assert!(
        !legacy.exists(),
        "the old directory is still there, so the profile now exists twice"
    );
    // Dir holds keys, mail password, all messages. Adopted dir came with umask default: must be
    // tightened, not just created right.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&wanted)
            .expect("stat the accounts dir")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o700,
            "the accounts directory is readable by others (mode {mode:o})"
        );
    }

    // Repeat: no-op.
    let again = DeltaChatCore::accounts_dir().expect("accounts dir");
    assert_eq!(
        Path::new(&again),
        wanted,
        "the answer changed on a second ask"
    );
    assert!(
        wanted.join("old.db").is_file(),
        "the second ask lost the profile"
    );

    // Legacy dir beside live profile never replaces it.
    marker(&legacy, "stale.db");
    let third = DeltaChatCore::accounts_dir().expect("accounts dir");
    assert_eq!(Path::new(&third), wanted);
    assert!(
        wanted.join("old.db").is_file() && !wanted.join("stale.db").exists(),
        "a stale legacy directory overwrote the profile in use"
    );
    assert!(
        legacy.join("stale.db").is_file(),
        "the stale directory was consumed rather than left alone"
    );

    let _ = fs::remove_dir_all(&root);
}
