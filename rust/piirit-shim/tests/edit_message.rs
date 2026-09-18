//! Changing the text of a message already sent.
//!
//! The core's `send_edit_request` takes the message and the new text,
//! and the row is re-read so it shows the change -- and says it was
//! changed -- by the time the page hears `edited` and clears its field.
//! A refusal reaches the page the way a refused send does, and leaves
//! the row as it was. The model also says whether the chat takes
//! messages at all, which is what decides whether Edit is offered.

// Qt harness: see message_actions.rs.
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
        property string lastError: ''
        // Not `edited`: inside the handler that name is the signal itself.
        property string landed: ''
        ChatMessages { id: chat; account_id: 1; chat_id: 1
                       onError: lastError = message
                       onEdited: landed += message_id + ';' }
        Connections {
            target: core
            onCore_event: chat.handle_event(context_id, kind, payload_json)
        }
        // The rows, read through a view: a model's roles are not
        // reachable from outside one.
        Repeater {
            id: rows
            model: chat.rows
            Item {
                property string body: model.text
                property bool changed: model.is_edited
            }
        }
        function count() { return '' + chat.count }
        function shape() { return chat.can_send + '/' + chat.is_encrypted }
        // The first message, which is the one edited.
        function rowOf() {
            var index = chat.row_of(1)
            var row = index >= 0 ? rows.itemAt(index) : null
            return row ? row.body + '/' + row.changed : 'missing'
        }
        function edit() { chat.edit_message(1, '  fixed words  '); return 'ok' }
        function editAndFail() { chat.edit_message(2, 'please fail'); return 'ok' }
        function heard() { return landed }
        function error() { return lastError }
    }
";

#[test]
fn an_edit_reaches_the_core_and_the_row_shows_the_change() {
    let temp = std::env::temp_dir().join(format!("piirit-edit-message-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
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
        ($name:expr) => {{
            let result = (*engine_ptr).invoke_method($name.into(), &[]);
            QString::from_qvariant(result)
                .map(|value| value.to_string())
                .unwrap_or_default()
        }};
    }
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    single_shot(Duration::from_secs(3), move || unsafe {
        record!("loaded", call!("count"));
        record!("shape", call!("shape"));
        record!("before", call!("rowOf"));
    });
    single_shot(Duration::from_secs(4), move || unsafe {
        record!("edit", call!("edit"));
    });
    single_shot(Duration::from_secs(6), move || unsafe {
        record!("heard", call!("heard"));
        record!("after", call!("rowOf"));
        record!("edit-fail", call!("editAndFail"));
    });
    single_shot(Duration::from_secs(8), move || unsafe {
        record!("heard-after-failure", call!("heard"));
        record!("error", call!("error"));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&common::calls(&journal), &steps);
}

fn assert_outcome(calls: &[(String, Value)], steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let params_of = |method: &str| -> Vec<Value> {
        calls
            .iter()
            .filter(|(name, _)| name == method)
            .map(|(_, params)| params.clone())
            .collect()
    };
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();
    let context = format!("steps: {steps:?}, calls: {names:?}");

    assert_eq!(value("loaded"), "2", "the chat did not load. {context}");
    assert_eq!(
        value("shape"),
        "true/true",
        "the model does not say the chat takes messages and is encrypted, \
         which is what decides whether Edit is offered. {context}"
    );
    assert_eq!(
        value("before"),
        "message 1/false",
        "the row did not start as the message the fake seeds. {context}"
    );
    assert_eq!(value("edit"), "ok", "the edit was not asked for. {context}");
    // send_edit_request params: account, message, new text -- trimmed,
    // since the field's whitespace is not part of the message.
    assert_eq!(
        params_of("send_edit_request"),
        vec![
            serde_json::json!([1, 1, "fixed words"]),
            serde_json::json!([1, 2, "please fail"])
        ],
        "the edits did not reach the core in the shape it takes. {context}"
    );
    assert_eq!(
        value("heard"),
        "1;",
        "the model did not say the edit landed, so the page keeps the field \
         full of it. {context}"
    );
    assert_eq!(
        value("after"),
        "fixed words/true",
        "the row does not show the new text as an edit. {context}"
    );
    assert_eq!(
        value("heard-after-failure"),
        "1;",
        "a refused edit was reported as having landed. {context}"
    );
    assert!(
        value("error").contains("could not send"),
        "the refused edit did not reach the page, got {:?}. {context}",
        value("error")
    );
}
