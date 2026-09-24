//! A sent row must land once even when the core announces the message
//! before it answers the send.

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

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

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
fn a_send_announced_before_it_is_answered_lands_once() {
    let temp = std::env::temp_dir().join(format!("piirit-send-race-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Long enough for the queued event to be polled first.
        std::env::set_var("PIIRIT_FAKE_SEND_DELAY_MS", "300");
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

    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    single_shot(Duration::from_secs(3), move || unsafe {
        (*engine_ptr).invoke_method("send".into(), &[QVariant::from(QString::from("hello"))]);
    });

    let mut count_after_send = 0_u32;
    let count_ptr: *mut u32 = std::ptr::addr_of_mut!(count_after_send);
    single_shot(Duration::from_secs(6), move || unsafe {
        let value = (*engine_ptr).invoke_method("count".into(), &[]);
        *count_ptr = u32::from_qvariant(value).unwrap_or_default();
        (*engine_ptr).quit();
    });

    engine.exec();

    let calls = common::calls(&journal);
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();

    // The point of the test: the event won, so the model fetched the new
    // message itself before the send's reply carried it.
    let fetched_after_send = calls
        .iter()
        .skip_while(|(name, _)| name != "misc_send_msg")
        .any(|(name, params)| {
            name == "get_messages"
                && params
                    .pointer("/1")
                    .and_then(Value::as_array)
                    .is_some_and(|ids| !ids.is_empty())
        });
    assert!(
        fetched_after_send,
        "the send's reply won the race, so the duplicate path was never taken: {names:?}"
    );

    // Two seeded messages plus the one just sent.
    assert_eq!(
        count_after_send, 3,
        "the sent message was added twice: {names:?}"
    );
}
