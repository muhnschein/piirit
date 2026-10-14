//! Mark-read from list sticks, whatever core says meanwhile.
//!
//! Refresh reads unfetched rows as they were at start; old answers dropped (`generation`).
//! So: mark read, message lands in another chat, newer refresh has pre-mark copy, older one
//! with cleared badge discarded; badge returns. Fake answers listing slowly so both are in flight.

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
use serde_json::Value;

mod common;

const PROBE_QML: &str = r#"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        ChatList { id: chats; account_id: 1 }
        Connections {
            target: core
            onCore_event: chats.handle_event(context_id, kind, payload_json)
        }
        // The seeded chats have nothing unread, so the chat this is about
        // is made unread first.
        function makeUnread() { chats.mark_unread(2) }
        function markRead() { chats.mark_read(2) }
        // A message in the other chat, arriving while the read chat is
        // being re-read: the core's own event, as the page feeds it in.
        function otherChatChanged() {
            chats.handle_event(1, 'ChatlistItemChanged', '{"chatId": 1}')
        }
        function unread() { return '' + chats.unread_total }
    }
"#;

#[test]
fn a_chat_marked_read_stays_read_when_another_chat_changes() {
    let temp = std::env::temp_dir().join(format!("piirit-mark-read-race-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        // Longer than mark-read-to-other-change gap, so first answer is still in flight at the
        // second question; short enough both land first. `single_shot` counts whole seconds.
        std::env::set_var("PIIRIT_FAKE_CHATLIST_DELAY_MS", "1500");
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

    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        call!("makeUnread");
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        (*steps_ptr).push(("unread", call!("unread")));
        call!("markRead");
    });

    // While re-read of marked chat is still out.
    single_shot(Duration::from_secs(6), move || unsafe {
        call!("otherChatChanged");
    });

    single_shot(Duration::from_secs(9), move || unsafe {
        (*steps_ptr).push(("after", call!("unread")));
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
    let records = common::records(&journal);
    let names: Vec<String> = common::methods(&journal);
    // Call times at fake core (ms since up): shows whether questions overlapped.
    let at = |call: &Value| call.get("at").and_then(Value::as_u64).unwrap_or_default();
    let method = |call: &Value| {
        call.get("method")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let marked = records
        .iter()
        .find(|call| method(call) == "marknoticed_chat")
        .map(at)
        .unwrap_or_default();
    let listings: Vec<u64> = records
        .iter()
        .filter(|call| method(call) == "get_chatlist_entries" && at(call) >= marked)
        .map(at)
        .collect();
    let context = format!("steps: {steps:?}, calls: {names:?}, listings: {listings:?}");

    // Second question must come while first in flight: fake holds listings back by delay.
    assert!(
        listings.len() >= 2 && listings[listings.len() - 1] < listings[0] + 1500,
        "the refresh for the other chat was not asked for while the one \
         from marking the chat read was still out, so nothing here is being \
         tested. {context}"
    );

    // Nothing unread to clear: nothing tested.
    assert_eq!(
        value("unread"),
        "1",
        "the chat was not unread before it was marked read, so this test is \
         no longer arranged the way the bug needs. {context}"
    );
    assert!(
        names.iter().any(|name| name == "marknoticed_chat"),
        "marking the chat read never reached the core. {context}"
    );
    assert_eq!(
        value("after"),
        "0",
        "the unread badge came back after the chat was marked read: a \
         refresh for another chat carried the chat's row from before it was \
         marked, and the refresh that would have cleared it was dropped for \
         being stale. {context}"
    );
}
