//! The beginning of a chat stays where it is, whatever the core says.
//!
//! This started as the reproduction for a race: the model held a moving
//! window of loaded messages, going to the beginning replaced its contents,
//! and a reconciliation that overlapped the move compared the old window's
//! anchors against the new window's rows, agreed with neither, and reloaded
//! -- onto the newest page. The jump undid itself, and whether it did
//! depended on whether an event happened to be in the air. In a chat full
//! of media, where the core reports on every download, one usually was.
//!
//! There is no jump any more, and no window: the model holds a row for
//! every message in the chat, so the first message is row 0 from the moment
//! the id list arrives. What that race was about cannot happen, and what is
//! worth keeping is the guarantee underneath it -- events landing at any
//! moment, in any number, never move where the beginning of the chat is.
//!
//! The fetches are slowed down so the events really do land mid-flight; on
//! a device that costs nothing to arrange.

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

const MESSAGES: u32 = 117;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piiri 1.0
    Item {
        ChatMessages { id: chat; account_id: 1; chat_id: 1 }
        Repeater {
            id: rows
            model: chat.rows
            Item { property int mid: model.message_id }
        }
        function count() { return '' + chat.count }
        function firstRow() {
            return rows.count === 0 ? 'empty' : '' + rows.itemAt(0).mid
        }
        function edges() {
            if (rows.count === 0) { return 'empty' }
            return rows.itemAt(0).mid + '..' + rows.itemAt(rows.count - 1).mid
        }
        /// Reading the top of the chat, which is what the reader does after
        /// scrolling there.
        function readTop() { chat.hydrate(0, 20); return 'ok' }
        /// What the core says while it works through a chat's attachments.
        function stir() {
            chat.handle_event(1, 'MsgsChanged', '{}')
            return 'ok'
        }
    }
";

#[test]
fn the_first_message_stays_row_zero_however_much_the_core_talks() {
    let temp = std::env::temp_dir().join(format!("piiri-jump-race-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRI_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRI_FAKE_LONG_CHAT", MESSAGES.to_string());
        // Long enough that the events below land while something is still
        // in the air, which on a device they do without any help.
        std::env::set_var("PIIRI_FAKE_FETCH_DELAY_MS", "700");
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
    single_shot(Duration::from_secs(4), move || unsafe {
        (*steps_ptr).push(("opened", call!("edges")));
        // Reading the top of the chat, with the core talking over it.
        (*steps_ptr).push(("read-top", call!("readTop")));
        (*steps_ptr).push(("stir-1", call!("stir")));
    });
    single_shot(Duration::from_millis(4300), move || unsafe {
        (*steps_ptr).push(("stir-2", call!("stir")));
    });
    single_shot(Duration::from_millis(4600), move || unsafe {
        (*steps_ptr).push(("stir-3", call!("stir")));
    });
    single_shot(Duration::from_secs(8), move || unsafe {
        (*steps_ptr).push(("first", call!("firstRow")));
        (*steps_ptr).push(("count", call!("count")));
        (*steps_ptr).push(("edges", call!("edges")));
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

    assert_eq!(
        value("opened"),
        format!("1..{MESSAGES}"),
        "the chat did not open with a row per message. {context}"
    );
    assert_eq!(
        value("first"),
        "1",
        "three events landed while the top of the chat was being read, and \
         the first message is no longer row 0 -- which is what going to the \
         beginning of a chat depends on. {context}"
    );
    assert_eq!(
        value("count"),
        MESSAGES.to_string(),
        "the events changed how many rows the chat has. {context}"
    );
    assert_eq!(
        value("edges"),
        format!("1..{MESSAGES}"),
        "the ends of the chat moved. {context}"
    );
}
