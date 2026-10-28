//! Reverse of `chat_send_race`: send reply lands during event's reconciliation; must not append row
//! twice.

// Qt harness: unsafe set_var pre-Qt; borrow_as_ptr.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        ChatMessages { id: chat; account_id: 1; chat_id: 1 }
        Connections {
            target: core
            onCore_event: chat.handle_event(context_id, kind, payload_json)
        }
        function count() { return chat.count }
        function send(text) { chat.send(text) }
    }
";

#[test]
fn a_send_answered_mid_fetch_lands_once() {
    let temp = std::env::temp_dir().join(format!("piirit-sync-race-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Event out first, send reply next, event's fetch last.
        std::env::set_var("PIIRIT_FAKE_SEND_DELAY_MS", "150");
        std::env::set_var("PIIRIT_FAKE_FETCH_DELAY_MS", "400");
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

    plan.after(1, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    plan.after(2, move || unsafe {
        (*engine_ptr).invoke_method("send".into(), &[QVariant::from(QString::from("hello"))]);
    });

    let mut count_after_send = 0_u32;
    let count_ptr: *mut u32 = std::ptr::addr_of_mut!(count_after_send);
    plan.after(3, move || unsafe {
        let value = (*engine_ptr).invoke_method("count".into(), &[]);
        *count_ptr = u32::from_qvariant(value).unwrap_or_default();
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let names = common::methods(&journal);

    // Reconciliation must run or ordering untested. Id list read, not message: reply already put
    // row in, so nothing fetched.
    assert!(
        names
            .iter()
            .skip_while(|name| *name != "misc_send_msg")
            .any(|name| name == "get_message_list_items"),
        "the event never reconciled after the send: {names:?}"
    );

    // Two seeded + one sent.
    assert_eq!(
        count_after_send, 3,
        "the sent message was added twice: {names:?}"
    );
}
