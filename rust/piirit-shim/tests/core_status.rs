//! Core failure reaches app: `Error` event is a typed QML signal; dead server replaced.

// Qt harness: unsafe set_var pre-Qt; borrow_as_ptr.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
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
    let temp = std::env::temp_dir().join(format!("piirit-core-status-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let exit = temp.join("exit-once");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Server dies on next request once error seen; replacement lives.
        std::env::set_var("PIIRIT_FAKE_EXIT_ONCE", &exit);
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);

    let mut plan = common::Plan::default();

    let report = move || unsafe {
        let value = (*engine_ptr).invoke_method("report".into(), &[]);
        QString::from_qvariant(value)
            .map(|text| text.to_string())
            .unwrap_or_default()
    };

    plan.after(1, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    plan.until(1, 30, move |_| report().ends_with("#ready"));
    plan.after(0, move || unsafe {
        (*engine_ptr).invoke_method("resumeIo".into(), &[]);
        (*engine_ptr).invoke_method("fail".into(), &[]);
    });

    // Error in, then server dies on next request: resume asks one.
    let mut before_death = String::new();
    let before_ptr: *mut String = std::ptr::addr_of_mut!(before_death);
    plan.until(1, 30, move |last| unsafe {
        let now = report();
        if !now.starts_with("could not send") && !last {
            return false;
        }
        *before_ptr = now;
        std::fs::write(&exit, "").expect("drop exit-once file");
        (*engine_ptr).invoke_method("resumeIo".into(), &[]);
        true
    });

    let mut after_restart = String::new();
    let after_ptr: *mut String = std::ptr::addr_of_mut!(after_restart);
    let mut trail = String::new();
    let trail_ptr: *mut String = std::ptr::addr_of_mut!(trail);
    plan.until(1, 30, move |last| unsafe {
        let seen = (*engine_ptr).invoke_method("seen".into(), &[]);
        let seen = QString::from_qvariant(seen)
            .map(|text| text.to_string())
            .unwrap_or_default();
        let now = report();
        let back = seen.contains("reconnecting") && now.ends_with("#ready");
        if !back && !last {
            return false;
        }
        *after_ptr = now;
        *trail_ptr = seen;
        (*engine_ptr).quit();
        true
    });

    plan.run();
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

    // Fresh server: account files, no IO. Without resume app reads history, receives nothing:
    // looks like working.
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
