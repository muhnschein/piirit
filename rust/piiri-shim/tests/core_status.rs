//! What the app is told when the core fails: an `Error` event reaches QML as
//! a typed signal, and a server that dies is replaced rather than mourned.

// Qt harness: needs `unsafe` for `env::set_var` before Qt starts
// (`unused_unsafe` because it is only unsafe from edition 2024 on),
// `borrow_as_ptr` for the engine pointer, and `single_shot` with
// whole-second Durations.
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
    import Piiri 1.0
    Item {
        property string lastError: ''
        // Every status the app was told about, in order. Asserting on the
        // trail rather than on one reading: a restart is a sequence, and a
        // single sample cannot tell 'never left ready' from 'went away and
        // came back'.
        property string trail: ''
        ChatMessages { id: chat; account_id: 1; chat_id: 1 }
        Connections {
            target: core
            onCore_error: lastError = message
            onStatus_changed: trail = trail + core.status + ' '
        }
        function resumeIo() { core.start_account_io(1) }
        function fail() { chat.send('please fail') }
        function report() { return lastError + '#' + core.status }
        function seen() { return trail }
    }
";

#[test]
fn a_core_failure_reaches_qml_and_a_dead_server_is_replaced() {
    let temp = std::env::temp_dir().join(format!("piiri-core-status-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRI_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRI_ACCOUNTS_DIR", temp.join("accounts"));
        // After the failing send has been answered, and long enough before
        // the last read for the restart to have landed. Inherited by the
        // replacement too, which is why the reads below stop at 8s: the
        // second server dies around 10.
        std::env::set_var("PIIRI_FAKE_EXIT_AFTER_MS", "4500");
    }

    piiri_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);

    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    single_shot(Duration::from_secs(2), move || unsafe {
        (*engine_ptr).invoke_method("resumeIo".into(), &[]);
    });
    single_shot(Duration::from_secs(3), move || unsafe {
        (*engine_ptr).invoke_method("fail".into(), &[]);
    });

    let mut before_death = String::new();
    let before_ptr: *mut String = std::ptr::addr_of_mut!(before_death);
    single_shot(Duration::from_secs(4), move || unsafe {
        let value = (*engine_ptr).invoke_method("report".into(), &[]);
        *before_ptr = QString::from_qvariant(value)
            .map(|text| text.to_string())
            .unwrap_or_default();
    });

    let mut after_restart = String::new();
    let after_ptr: *mut String = std::ptr::addr_of_mut!(after_restart);
    let mut trail = String::new();
    let trail_ptr: *mut String = std::ptr::addr_of_mut!(trail);
    single_shot(Duration::from_secs(8), move || unsafe {
        let value = (*engine_ptr).invoke_method("report".into(), &[]);
        *after_ptr = QString::from_qvariant(value)
            .map(|text| text.to_string())
            .unwrap_or_default();
        let seen = (*engine_ptr).invoke_method("seen".into(), &[]);
        *trail_ptr = QString::from_qvariant(seen)
            .map(|text| text.to_string())
            .unwrap_or_default();
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_eq!(
        before_death, "could not send#ready",
        "the core's own Error event did not reach QML as a typed signal"
    );
    assert!(
        trail.contains("reconnecting"),
        "the server died and the app was never told: {trail:?}"
    );
    assert_eq!(
        after_restart, "could not send#ready",
        "the server died and nothing replaced it. Statuses seen: {trail:?}"
    );

    // A fresh server has the account files but no IO running. Without the
    // resume the app comes back able to read its history and unable to
    // receive anything -- which looks exactly like working.
    let resumes = common::methods(&journal)
        .into_iter()
        .filter(|method| method == "start_io")
        .count();
    assert!(
        resumes >= 2,
        "IO was not resumed on the replacement server: {resumes} start_io call(s). \
         Statuses seen: {trail:?}"
    );
}
