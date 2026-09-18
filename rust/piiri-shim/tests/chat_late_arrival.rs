//! A message that sorts into the middle of a chat lands there, in place.
//!
//! The core does not only append. It sorts a received message below the
//! newest *seen* message and no further, so while the reader is up in the
//! history -- where nothing arriving is marked seen -- a late message from
//! a busy group, or an older one synced from another device, goes in among
//! the unread ones rather than after them.
//!
//! Answering that by starting the model over would replace the rows, lose
//! the view's place, and blank the rows the reader had filled in. What is
//! pinned here is that an arrival anywhere in the chat is one more row
//! where it belongs, and that nothing else moves or empties.

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

mod common;

/// More than a page, so the top of the chat is somewhere a reload would
/// leave empty.
const MESSAGES: u32 = 130;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piiri 1.0
    Item {
        ChatMessages { id: chat; account_id: 1; chat_id: 1 }
        Repeater {
            id: rows
            model: chat.rows
            Item {
                property int mid: model.message_id
                property bool filled: model.loaded
            }
        }
        Connections {
            target: core
            onCore_event: chat.handle_event(context_id, kind, payload_json)
        }
        function count() { return '' + chat.count }
        function edges() {
            if (rows.count === 0) { return 'empty' }
            return rows.itemAt(0).mid + '..' + rows.itemAt(rows.count - 1).mid
        }
        function idAt(index) {
            if (index < 0 || index >= rows.count) { return 'no-row' }
            return '' + rows.itemAt(index).mid
        }
        function filledAt(index) {
            if (index < 0 || index >= rows.count) { return 'no-row' }
            return '' + rows.itemAt(index).filled
        }
        /// Reading the top of the chat, as the reader does after scrolling
        /// there.
        function readTop() { chat.hydrate(0, 20); return 'ok' }
    }
";

#[test]
fn a_message_arriving_mid_chat_is_one_more_row_where_it_belongs() {
    let temp = std::env::temp_dir().join(format!("piiri-late-arrival-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRI_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRI_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRI_FAKE_LONG_CHAT", MESSAGES.to_string());
        // After the top of the chat has been read, below.
        std::env::set_var("PIIRI_FAKE_LATE_ARRIVAL_MS", "5000");
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
        (*steps_ptr).push(("opened", call!("count")));
        (*steps_ptr).push(("opened-edges", call!("edges")));
        // Up in the history, with the rows there filled in.
        (*steps_ptr).push(("read-top", call!("readTop")));
    });
    single_shot(Duration::from_millis(4500), move || unsafe {
        (*steps_ptr).push(("top-filled", call!("filledAt", 0)));
    });
    single_shot(Duration::from_secs(8), move || unsafe {
        (*steps_ptr).push(("arrived", call!("count")));
        (*steps_ptr).push(("arrived-edges", call!("edges")));
        (*steps_ptr).push((
            "late-id",
            call!("idAt", i32::try_from(MESSAGES).unwrap_or(0) - 1),
        ));
        (*steps_ptr).push((
            "late-filled",
            call!("filledAt", i32::try_from(MESSAGES).unwrap_or(0) - 1),
        ));
        (*steps_ptr).push(("top-still-filled", call!("filledAt", 0)));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps, &journal);
}

/// What the run has to show for itself, out of the test body.
fn assert_outcome(steps: &[(&str, String)], journal: &std::path::Path) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("opened"),
        MESSAGES.to_string(),
        "the chat did not open with a row per message. {context}"
    );
    assert_eq!(
        value("opened-edges"),
        format!("1..{MESSAGES}"),
        "the rows do not run from the first message to the last. {context}"
    );
    assert_eq!(
        value("top-filled"),
        "true",
        "the top of the chat was not filled in when it was read, so this \
         run says nothing about what an arrival does to it. {context}"
    );

    // One more row, where the core put it: before the last message.
    assert_eq!(
        value("arrived"),
        (MESSAGES + 1).to_string(),
        "a message arriving mid-chat did not add a row. {context}"
    );
    assert_eq!(
        value("arrived-edges"),
        format!("1..{MESSAGES}"),
        "a message arriving mid-chat moved an end of the chat. {context}"
    );
    assert_eq!(
        value("late-id"),
        (MESSAGES + 1).to_string(),
        "the late message is not in the row the core sorted it into. \
         {context}"
    );
    assert_eq!(
        value("late-filled"),
        "true",
        "the late message was put in a row and left blank. {context}"
    );

    // And nothing else moved or emptied: the rows the reader had filled in
    // are still filled in, which a reload would have undone.
    assert_eq!(
        value("top-still-filled"),
        "true",
        "the first message went blank again when a message arrived \
         mid-chat: the model started over rather than taking the arrival \
         in place, and the reader lost their place with it. {context}"
    );
    let list_reads = common::methods(journal)
        .into_iter()
        .filter(|method| method == "get_message_list_items")
        .count();
    assert_eq!(
        list_reads, 2,
        "the id list was read {list_reads} times: once to open and once \
         for the arrival is the whole of it, and a third is the model \
         starting over. {context}"
    );
}
