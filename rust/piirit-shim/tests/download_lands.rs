//! A message the download limit held back fills in where it stands.
//!
//! The rest of such a message is asked for with `download_full_message`,
//! and the core announces the download landing as `MsgsChanged` with the
//! message's id -- and nothing else changed: the id list is the same,
//! so a sync that only reconciles ids found nothing to do, and the row
//! said "Downloading…" until the chat was opened again. The row has to
//! be re-read on that event, in the chat the reader is looking at.

// Qt harness: see qml_chat_list.rs.
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

mod common;

/// The seeded message the fake core holds back.
const HELD_BACK: u32 = 2;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        ChatMessages { id: chat; account_id: 1; chat_id: 1 }
        Connections {
            target: core
            onCore_event: chat.handle_event(context_id, kind, payload_json)
        }
        // The rows as a delegate reads them, so a change to one reaches
        // here the way it reaches the screen.
        Repeater {
            id: rows
            model: chat.rows
            delegate: Item {
                property int messageId: model.message_id
                property string downloadState: model.download_state
            }
        }
        function stateOf(messageId) {
            for (var i = 0; i < rows.count; i++) {
                var row = rows.itemAt(i)
                if (row && row.messageId === messageId) {
                    return row.downloadState
                }
            }
            return 'no-row'
        }
        function loaded() { return '' + chat.loaded }
        function download() { chat.download_full(2); return 'ok' }
    }
";

#[test]
fn a_finished_download_reaches_the_row_without_reopening_the_chat() {
    let temp = std::env::temp_dir().join(format!("piirit-download-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_HELD_BACK_MSG", HELD_BACK.to_string());
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

    // Loaded once the core is up, so the model has something to load from.
    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

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

    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("loaded", call!("loaded")));
        (*steps_ptr).push(("before", call!("stateOf", HELD_BACK)));
        call!("download");
    });
    single_shot(Duration::from_secs(6), move || unsafe {
        (*steps_ptr).push(("after", call!("stateOf", HELD_BACK)));
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
    assert_eq!(value("loaded"), "true", "the chat did not load. {context}");
    assert_eq!(
        value("before"),
        "Available",
        "the fake core did not hold the message back, so this proves nothing. {context}"
    );
    assert_eq!(
        value("after"),
        "Done",
        "the download landed and the row never heard: the core announces it \
         as MsgsChanged naming the message, with the id list unchanged, and \
         only reopening the chat re-read the row. {context}"
    );
    let methods = common::methods(&journal);
    assert!(
        methods
            .iter()
            .any(|method| method == "download_full_message"),
        "the download was never asked for. Calls: {methods:?}"
    );
}
