//! The app comes back to the profile it was closed on.
//!
//! The chat list tells the core which profile it is showing, the core
//! keeps that on disk, and the account list read at the next start names
//! it as the one to resume -- rather than whichever configured profile
//! the core happens to list first, which is what always came back before.
//! A selection that no longer points at a usable profile falls back to
//! the first that is.

// Qt harness: see qml_chat_list.rs.
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

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        // Every answer, in order: what the core said to resume each time
        // the list was read.
        property string resumed: ''
        Connections {
            target: core
            onAccounts_refreshed: {
                resumed = resumed + configured_count + ':' + resume_account_id + ','
            }
        }
        function refresh() { core.refresh_accounts(); return 'ok' }
        function select(id) { core.select_account(id); return 'ok' }
        function resumeAll() { core.start_all_account_io(); return 'ok' }
        function seen() { return resumed }
    }
";

#[test]
fn the_list_names_the_selected_profile_as_the_one_to_resume() {
    let temp = std::env::temp_dir().join(format!("piiri-resume-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRI_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRI_ACCOUNTS_DIR", temp.join("accounts"));
        // Two configured profiles, neither selected yet.
        std::env::set_var("PIIRI_FAKE_ACCOUNTS", "1,2");
    }

    piiri_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);

    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = (*engine_ptr).invoke_method(
                $name.into(),
                &[$(QVariant::from($arg)),*],
            );
            QString::from_qvariant(result)
                .map(|value| value.to_string())
                .unwrap_or_default()
        }};
    }

    // Nothing selected: the first configured profile.
    single_shot(Duration::from_secs(2), move || unsafe {
        call!("refresh");
    });
    // The chat list opened on the second profile, and the app was
    // started again.
    single_shot(Duration::from_secs(4), move || unsafe {
        call!("select", 2);
        call!("resumeAll");
    });
    single_shot(Duration::from_secs(6), move || unsafe {
        call!("refresh");
    });
    // A selection the core no longer has a profile for.
    single_shot(Duration::from_secs(8), move || unsafe {
        call!("select", 7);
    });
    single_shot(Duration::from_secs(10), move || unsafe {
        call!("refresh");
    });
    single_shot(Duration::from_secs(12), move || unsafe {
        (*engine_ptr).quit();
    });

    engine.exec();

    let seen = QString::from_qvariant(engine.invoke_method("seen".into(), &[]))
        .map(|value| value.to_string())
        .unwrap_or_default();
    let methods = common::methods(&journal);
    assert_eq!(
        seen, "2:1,2:2,2:2,",
        "the account list did not name the selected profile as the one to \
         resume: first nothing selected, then profile 2, then a selection \
         the core refused. Calls: {methods:?}"
    );
    assert!(
        methods.iter().any(|method| method == "select_account"),
        "the chat list's choice never reached the core, so it cannot be \
         remembered across a restart. Calls: {methods:?}"
    );
    assert!(
        methods
            .iter()
            .any(|method| method == "get_selected_account_id"),
        "the list was read without asking which profile is selected, so \
         the answer can only ever be the first. Calls: {methods:?}"
    );
    assert!(
        methods
            .iter()
            .any(|method| method == "start_io_for_all_accounts"),
        "IO was not resumed for every profile. Calls: {methods:?}"
    );
}
