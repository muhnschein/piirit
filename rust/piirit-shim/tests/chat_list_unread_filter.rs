//! The unread filter on the chat list.
//!
//! It shows the chats with something unread, and keeps a chat read since
//! it went on until it is told to let go: the reader who opens a chat
//! from the filtered list and comes back should find the row where they
//! left it.

// Qt harness: see chat_list.rs.
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

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        ChatList { id: chats; account_id: 1 }
        function count() { return '' + chats.count }
        function filter(on) { chats.unread_only = on; return 'ok' }
        function markUnread(chatId) { chats.mark_unread(chatId); return 'ok' }
        function markRead(chatId) { chats.mark_read(chatId); return 'ok' }
        function forget() { chats.forget_read(); return 'ok' }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_filter_shows_unread_chats_and_keeps_the_ones_read_meanwhile() {
    let temp = std::env::temp_dir().join(format!("piirit-unread-filter-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
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
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    // The fake's two chats, neither of them unread.
    single_shot(Duration::from_secs(3), move || unsafe {
        record!("all", call!("count"));
        record!("filtered-on", call!("filter", true));
    });

    single_shot(Duration::from_millis(4000), move || unsafe {
        record!("nothing-unread", call!("count"));
        record!("marked-unread", call!("markUnread", 2u32));
    });

    single_shot(Duration::from_millis(5500), move || unsafe {
        record!("one-unread", call!("count"));
        record!("marked-read", call!("markRead", 2u32));
    });

    single_shot(Duration::from_millis(7000), move || unsafe {
        record!("read-meanwhile", call!("count"));
        record!("forgot", call!("forget"));
    });

    single_shot(Duration::from_millis(8000), move || unsafe {
        record!("after-forgetting", call!("count"));
        record!("filtered-off", call!("filter", false));
    });

    single_shot(Duration::from_millis(9000), move || unsafe {
        record!("all-again", call!("count"));
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
        value("all"),
        "2",
        "the fake's chats never loaded. {context}"
    );
    assert_eq!(
        value("nothing-unread"),
        "0",
        "with nothing unread the filter still shows chats. {context}"
    );
    assert_eq!(
        value("one-unread"),
        "1",
        "a chat marked unread did not appear under the filter. {context}"
    );
    assert_eq!(
        value("read-meanwhile"),
        "1",
        "a chat read while the filter was on vanished at once, so coming \
         back from it moves the list under the reader. {context}"
    );
    assert_eq!(
        value("after-forgetting"),
        "0",
        "a chat read while the filter was on stayed after being let go. \
         {context}"
    );
    assert_eq!(
        value("all-again"),
        "2",
        "turning the filter off did not bring every chat back. {context}"
    );

    let _ = std::fs::remove_dir_all(&temp);
}
