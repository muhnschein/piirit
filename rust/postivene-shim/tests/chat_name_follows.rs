//! The conversation model's name follows a rename.
//!
//! The header over the messages is named from the chat list when the
//! page opens. Left to hear of a rename only from the group page beside
//! it, a contact given a name -- or a group renamed from another device --
//! would show the old name until the chat was reopened. The model re-reads
//! the name on the events that can change
//! it, and says so only when it did.

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

use postivene_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Postivene 1.0
    Item {
        property int changes: 0
        // The conversation's model, on the group; and the group page's,
        // which is what renames it. The chat is set once the core is up,
        // as it is in the app: a conversation is only ever opened from
        // a list the core filled.
        ChatMessages { id: messages; account_id: 1; onChat_name_changed: changes += 1 }
        ChatInfo { id: info; account_id: 1 }
        function open() { messages.chat_id = 2; info.chat_id = 2; return 'ok' }
        Connections {
            target: core
            onCore_event: {
                messages.handle_event(context_id, kind, payload_json)
                info.handle_event(context_id, kind, payload_json)
            }
        }
        function name() { return messages.chat_name }
        function changed() { return '' + changes }
        function rename(name) { info.rename(name); return 'ok' }
    }
";

#[test]
fn the_name_over_the_messages_follows_a_rename() {
    let temp = std::env::temp_dir().join(format!("postivene-chat-name-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_FAKE_JOURNAL", &journal);
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
    }

    postivene_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

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
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    single_shot(Duration::from_secs(2), move || unsafe {
        record!("open", call!("open"));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("loaded", call!("name"));
        record!("changes-loaded", call!("changed"));
        record!("rename", call!("rename", QString::from("Hikers")));
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        record!("renamed", call!("name"));
        record!("changes-renamed", call!("changed"));
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

    assert_eq!(value("open"), "ok", "the chat was not opened. {context}");
    assert_eq!(
        value("loaded"),
        "chat 2",
        "the model did not read the chat's name on loading. {context}"
    );
    assert_eq!(
        value("changes-loaded"),
        "1",
        "reading the name once should say so once. {context}"
    );
    assert_eq!(value("rename"), "ok", "no rename. {context}");
    assert_eq!(
        value("renamed"),
        "Hikers",
        "the rename beside the conversation did not reach its model. {context}"
    );
    assert_eq!(
        value("changes-renamed"),
        "2",
        "the name was announced other than once per change. {context}"
    );
}
