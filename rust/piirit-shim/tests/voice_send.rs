//! Voice message goes out as one: `send_msg` with Voice view type (the one kind core
//! must be told); row shown is the one core holds.

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        property string lastError: ''
        property string sentRow: ''
        ChatMessages { id: chat; account_id: 1; chat_id: 1
                       onError: lastError = message
                       onSent: sentRow = '' + message_id }
        Connections {
            target: core
            onCore_event: chat.handle_event(context_id, kind, payload_json)
        }
        Repeater {
            id: rows
            model: chat.rows
            delegate: Item {
                property int messageId: model.message_id
                property string viewType: model.view_type
                property bool outgoing: model.is_outgoing
            }
        }
        function send(path) { chat.send_voice(path); return 'ok' }
        function sent() { return sentRow }
        function error() { return lastError }
        function typeOf(messageId) {
            for (var i = 0; i < rows.count; i++) {
                var row = rows.itemAt(i)
                if (row && row.messageId === messageId) {
                    return row.viewType + ':' + row.outgoing
                }
            }
            return 'no-row'
        }
    }
";

#[test]
fn a_recording_is_sent_as_a_voice_message() {
    let temp = std::env::temp_dir().join(format!("piirit-voice-send-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt and server.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
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
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);

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

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    plan.after(2, move || unsafe {
        // Recorder's path as handed over: plain path in captures dir.
        call!(
            "send",
            QString::from("/tmp/piirit-fake/captures/voice-20260904-151212.mp3")
        );
    });
    plan.after(3, move || unsafe {
        (*steps_ptr).push(("sent", call!("sent")));
        (*steps_ptr).push(("error", call!("error")));
        let sent = call!("sent").parse::<i32>().unwrap_or(0);
        (*steps_ptr).push(("type", call!("typeOf", sent)));
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");
    assert_eq!(value("error"), "", "the send failed. {context}");
    assert!(
        !value("sent").is_empty(),
        "the voice message was never reported sent. {context}"
    );
    assert_eq!(
        value("type"),
        "Voice:true",
        "the row is not this account's voice message. {context}"
    );

    let sends: Vec<(String, Value)> = common::calls(&journal)
        .into_iter()
        .filter(|(method, _)| method == "send_msg" || method == "misc_send_msg")
        .collect();
    assert_eq!(
        sends,
        vec![(
            "send_msg".to_string(),
            serde_json::json!([
                1,
                1,
                {
                    // Recording has no caption or name: both null (core reads as absent).
                    "text": null,
                    "file": "/tmp/piirit-fake/captures/voice-20260904-151212.mp3",
                    "filename": null,
                    "viewtype": "Voice",
                    "quotedMessageId": null
                }
            ])
        )],
        "a voice message has to go through send_msg with the Voice view type, \
         which is the one thing the core cannot decide from the file. {context}"
    );
}
