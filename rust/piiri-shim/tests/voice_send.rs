//! A voice message goes out as one: `send_msg` with the Voice view type,
//! which is the one kind the core has to be told, and the row the reader
//! sees is the one the core then holds.

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
use serde_json::Value;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piiri 1.0
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
    let temp = std::env::temp_dir().join(format!("piiri-voice-send-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRI_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRI_ACCOUNTS_DIR", temp.join("accounts"));
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

    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    single_shot(Duration::from_secs(3), move || unsafe {
        // As the recorder hands it over: a plain path in the captures
        // directory.
        call!(
            "send",
            QString::from("/tmp/piiri-fake/captures/voice-20260904-151212.ogg")
        );
    });
    single_shot(Duration::from_secs(6), move || unsafe {
        (*steps_ptr).push(("sent", call!("sent")));
        (*steps_ptr).push(("error", call!("error")));
        let sent = call!("sent").parse::<i32>().unwrap_or(0);
        (*steps_ptr).push(("type", call!("typeOf", sent)));
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
                    // A recording carries neither a caption nor a name
                    // anyone sees; both slots go over as null, which is
                    // what the core reads as absent.
                    "text": null,
                    "file": "/tmp/piiri-fake/captures/voice-20260904-151212.ogg",
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
