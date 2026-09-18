//! A profile already on this phone is not taken over a second time.
//!
//! Two accounts on one address are two copies of one mailbox, each
//! fetching it: the profiles page counts every unread message once per
//! copy, a notification arrives once per copy, and a reply is sent from
//! whichever copy the reader happened to open. Nothing in the core stops
//! it -- `import_backup` is happy to write the same backup into as many
//! accounts as it is given -- so the shim reads the address the import
//! brought over and refuses one it already has.
//!
//! The refusal is a `restore_refused` reason rather than the core's
//! words, as `not-a-backup` and `too-new` are: the page puts it into the
//! reader's language. And the account the second import wrote is
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

use piiri_shim::DeltaChatCore;
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
fn the_same_backup_is_taken_over_once_and_the_list_hears_about_it() {
    let temp = std::env::temp_dir().join(format!("piiri-restore-dup-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and
    // before the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRI_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRI_ACCOUNTS_DIR", temp.join("accounts"));
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

    // 1s: the backup is read in. 3s: the list knows about the profile
    // without anything here having asked, and the same file is offered
    // again. 5s: what came of the second one.
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
            .restore_from_file(QString::from("/tmp/holiday-backup.tar"));
    });
    single_shot(Duration::from_secs(5), move || unsafe {
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
        "1/already-here|/",
        "the same backup was not taken over exactly once, and refused the \
         second time as one this phone already has. {context}"
    );

    // The import ran both times -- there is nothing to compare addresses
    // with until it has -- and the second account went with it.
    let imported: Vec<u64> = calls
        .iter()
        .filter(|(method, _)| method == "import_backup")
        .filter_map(|(_, params)| params.get(0).and_then(Value::as_u64))
        .collect();
    assert_eq!(
        imported,
        vec![1, 2],
        "the backup was not read into an account of its own each time. \
         {context}"
    );
    let removed: Vec<u64> = calls
        .iter()
        .filter(|(method, _)| method == "remove_account")
        .filter_map(|(_, params)| params.get(0).and_then(Value::as_u64))
        .collect();
    assert_eq!(
        removed,
        vec![2],
        "the second copy was left on the phone. {context}"
    );

    // The list the profiles page draws, neither asked for by this test
    // nor by any page: one profile after the import, and still one after
    // the copy was refused.
    assert_eq!(
        value("listed"),
        "1",
        "the profile that arrived is not in the account list, so the \
         profiles page has nothing to show until the app is restarted. \
         {context}"
    );
    assert_eq!(
        value("after"),
        "1",
        "the refused copy left a row behind. {context}"
    );
}
