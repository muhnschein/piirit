//! Where account state lives, and what happens to a profile written
//! under the app's old name.
//!
//! Sailjail grants write access to `~/.local/share/<Org>/<App>` only,
//! and the app was Postivene before it was Piirit, so a profile from
//! then sits under `postivene/postivene/accounts`, outside the grant;
//! this pins the move, and pins that it never overwrites a profile
//! already in the new place.

// `set_var` in a single-test binary; only unsafe from edition 2024 on.
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

    // A profile written by a build under the old name.
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
    // The directory holds the keys, the mail password and every message.
    // The adopted one arrived with the umask default, which is the case
    // that has to be tightened rather than merely created right.
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

    // Asking again is a no-op, not a second move.
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

    // A legacy directory reappearing next to a live profile must not
    // replace it: the live one wins and nothing is destroyed.
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
