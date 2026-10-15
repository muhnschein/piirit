//! Profile already on phone is not taken over again.
//!
//! Two accounts on one address = two copies of one mailbox (unread counted twice,
//! notifications twice, replies from whichever copy). Core lets `import_backup` write
//! same backup again, so shim reads imported relays (`list_transports`) and refuses a
//! profile any of whose relays another has; every relay, not own address (`configured_addr`
//! may differ per copy; no address in account list since 2.61). Refused reason is
//! `restore_refused`; page localizes it. Refused import's account is removed, as
//! on failure.
//!
//! Other half: account list repopulated by shim, not by asking page, so arrived profile
//! shows even if page gone. It was not: import finishing while file browser animated away
//! left profiles page missing it until restart.

// Qt harness: see restore.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// Records shim signals; mirrors account list (`Repeater`: list is a model).
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        property int restored: 0
        property string refusals: ''
        property string failures: ''
        Repeater { id: mirror; model: core.account_list; Item {} }
        Connections {
            target: core
            onProfile_restored: restored = restored + 1
            onRestore_refused: refusals = refusals + reason + '|'
            onRestore_failed: failures = failures + message + '|'
        }
        function summary() {
            return restored + '/' + refusals + '/' + failures
        }
        // What the profiles page would draw, unasked.
        function profiles() { return '' + mirror.count }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_same_backup_is_taken_over_once_and_the_list_hears_about_it() {
    let temp = std::env::temp_dir().join(format!("piirit-restore-dup-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt and server.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Profile on two relays: own address's, and first set-up. Fake core takes
        // the backup's address from file name, so capitalized name below is a copy.
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1");
        std::env::set_var("PIIRIT_FAKE_OLDER_RELAY", "summer-backup-tar@example.org");
    }

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let core_ptr = std::ptr::addr_of!(core_box);
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);

    macro_rules! ask {
        ($name:expr) => {{
            let result = (*engine_ptr).invoke_method($name.into(), &[]);
            QString::from_qvariant(result)
                .map(|value| value.to_string())
                .unwrap_or_default()
        }};
    }

    // 1s: unknown backup read in. 3s: list knows unasked; copy offered.
    // 5s: first backup again. 7s: outcome.
    // SAFETY: fire only while `exec()` runs on this thread; boxes outlive it.
    single_shot(Duration::from_secs(1), move || unsafe {
        (*core_ptr)
            .pinned()
            .borrow_mut()
            .restore_from_file(QString::from("/tmp/holiday-backup.tar"));
    });
    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("listed", ask!("profiles")));
        (*core_ptr)
            .pinned()
            .borrow_mut()
            .restore_from_file(QString::from("/tmp/Summer-Backup.tar"));
    });
    single_shot(Duration::from_secs(5), move || unsafe {
        (*core_ptr)
            .pinned()
            .borrow_mut()
            .restore_from_file(QString::from("/tmp/holiday-backup.tar"));
    });
    single_shot(Duration::from_secs(7), move || unsafe {
        (*steps_ptr).push(("after", ask!("profiles")));
        (*steps_ptr).push(("summary", ask!("summary")));
        (*engine_ptr).quit();
    });

    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let calls = common::calls(&journal);
    let context = format!("steps: {steps:?}\ncalls: {calls:?}");

    assert_eq!(
        value("summary"),
        "1/already-here|already-here|/",
        "the new backup was not taken over exactly once, or a copy of a \
         profile this phone already has was not refused: first the one \
         whose own address is another of its relays, then the same \
         backup a second time. {context}"
    );

    // Import ran each time (no relays to compare until then), into own account,
    // never this profile; refused accounts removed.
    let imported: Vec<u64> = calls
        .iter()
        .filter(|(method, _)| method == "import_backup")
        .filter_map(|(_, params)| params.get(0).and_then(Value::as_u64))
        .collect();
    assert!(
        imported.len() == 3 && imported[0] == 2 && !imported.contains(&1),
        "the backups were not each read into an account of their own. \
         {context}"
    );
    let removed: Vec<u64> = calls
        .iter()
        .filter(|(method, _)| method == "remove_account")
        .filter_map(|(_, params)| params.get(0).and_then(Value::as_u64))
        .collect();
    assert_eq!(
        removed,
        imported[1..].to_vec(),
        "a refused copy was left on the phone, or a profile it was a copy \
         of went instead. {context}"
    );

    // Account list, unasked: profile here plus arrived one; still two after both copies
    // refused.
    assert_eq!(
        value("listed"),
        "2",
        "the profile that arrived is not in the account list, so the \
         profiles page has nothing to show until the app is restarted. \
         {context}"
    );
    assert_eq!(
        value("after"),
        "2",
        "a refused copy left a row behind. {context}"
    );
}
