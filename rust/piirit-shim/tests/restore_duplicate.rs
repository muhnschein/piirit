//! A profile already on this phone is not taken over a second time.
//!
//! Two accounts on one address are two copies of one mailbox, each
//! fetching it: the profiles page counts every unread message once per
//! copy, a notification arrives once per copy, and a reply is sent from
//! whichever copy the reader happened to open. Nothing in the core stops
//! it -- `import_backup` is happy to write the same backup into as many
//! accounts as it is given -- so the shim reads the relays the import
//! brought over (`list_transports`) and refuses a profile any of whose
//! relays another profile already has.
//!
//! Every relay, not the profile's own address alone. The core's account
//! list carries no address since 2.61, and the profile's own address
//! (`configured_addr`) need not be the same on two copies: a backup made
//! where the profile had been switched to another of its relays names
//! that one, and still fetches the same mailboxes.
//!
//! The refusal is a `restore_refused` reason rather than the core's
//! words, as `not-a-backup` and `too-new` are: the page puts it into the
//! reader's language. And the account a refused import wrote is
//! removed, the way a failed import's is -- a second copy is good for
//! nothing, and the copy already here is untouched.
//!
//! The other half of this file is the list every other page reads. A
//! profile that arrives is on the phone whether or not the page that
//! asked for it is still up, so the account list is repopulated by the
//! shim rather than by that page. It was not, and a reader whose import
//! finished while the file browser was still animating away found the
//! profiles page missing the profile until the app was next started.

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

/// Records what the shim signalled, and mirrors the account list so the
/// test can count what is in it. A `Repeater` rather than a property:
/// the list is a model, and a model is counted by something built from
/// it.
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
        // What the profiles page would draw, without having asked for it.
        function profiles() { return '' + mirror.count }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_same_backup_is_taken_over_once_and_the_list_hears_about_it() {
    let temp = std::env::temp_dir().join(format!("piirit-restore-dup-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and
    // before the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // A profile here from the start, on two relays: the one its own
        // address is on, and one it was set up on first. The fake core
        // makes a backup's address from its file name, so the one named
        // for that first relay below is a copy of this profile, written
        // in capitals the way another client might have.
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

    // 1s: a backup of a profile the phone does not have is read in. 3s:
    // the list knows about it without anything here having asked, and
    // the copy of the profile already here is offered. 5s: the first
    // backup again. 7s: what came of both.
    // SAFETY: these callbacks fire only while `exec()` is running on this
    // thread, and both boxes outlive it.
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

    // The import ran every time -- there are no relays to compare until
    // it has -- each into an account of its own, never the profile here,
    // and each refused account went with it.
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

    // The list the profiles page draws, neither asked for by this test
    // nor by any page: the profile here and the one that arrived after
    // the import, and still those two after both copies were refused.
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
