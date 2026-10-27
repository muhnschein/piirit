//! Unread event never leaves shim.
//!
//! Each page and cover list re-parses every `core_event`; core sends mostly own log, thousands in
//! bulk sync, screen off. So `relay` drops kinds not in `HANDLED_EVENT_KINDS` before serialising.
//!
//! List vs reader: `event_kinds.rs`. Here: 4 sent, 2 read, only those arrive.

// Qt harness: see qml_share.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        property string seen: ''
        Connections {
            target: core
            // Qt 5.6 handler syntax; see WelcomePage.qml.
            onCore_event: seen = seen + kind + ','
        }
        function begin(path) { core.start(path); return 'ok' }
        function heard() { return '' + seen }
    }
";

#[test]
fn only_the_kinds_something_reads_are_handed_out() {
    let temp = std::env::temp_dir().join(format!("piirit-fanout-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Two app reads, two core log lines; `Info` is most (one per step of each
        // connection/message).
        std::env::set_var(
            "PIIRIT_FAKE_SEED_EVENTS",
            "Info,IncomingMsg,SmtpConnected,MsgsChanged",
        );
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let server = env!("CARGO_BIN_EXE_fake-core-server").to_string();

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

    let steps: common::Steps = common::Steps::default();

    let mut plan = common::Plan::default();

    let starting = steps.clone();
    plan.after(1, move || unsafe {
        common::record(
            &starting,
            "begin",
            call!("begin", QString::from(server.clone())).into(),
        );
    });

    let heard = steps.clone();
    plan.after(4, move || unsafe {
        common::record(&heard, "seen", call!("heard").into());
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let steps = steps.borrow().clone();
    assert_eq!(
        common::value_of(&steps, "begin"),
        "ok",
        "the core did not start"
    );
    let seen = common::value_of(&steps, "seen");

    assert!(
        seen.contains("IncomingMsg"),
        "a message arriving never reached the app, so the gate in `relay` \
         drops what the whole app is for: {seen:?}"
    );
    assert!(
        seen.contains("MsgsChanged"),
        "a change to a chat's messages never reached the app: {seen:?}"
    );
    assert!(
        !seen.contains("Info"),
        "the core's own log reached every listener in the app, which is the \
         work this gate exists to not do: {seen:?}"
    );
    assert!(
        !seen.contains("SmtpConnected"),
        "an event nothing in the app reads was serialised and handed to \
         every page and every cover list: {seen:?}"
    );
}
