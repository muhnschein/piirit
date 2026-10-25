//! Chat-list model: cost of a message arriving in one chat. One row moves, or rebuild?

// Qt harness allows: unsafe `env::set_var`, borrow_as_ptr.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// `Repeater` reads row order: `QAbstractListModel` gives JS nothing; a real view proves it.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        ChatList { id: chats; account_id: 1 }
        // Sending is how a chat gets bumped; the fake core moves the chat
        // it sent to and announces it, as the real one does.
        ChatMessages { id: chatTwo; account_id: 1; chat_id: 2 }
        Repeater {
            id: rows
            model: chats.rows
            Item { property int cid: model.chat_id }
        }
        Connections {
            target: core
            onCore_event: chats.handle_event(context_id, kind, payload_json)
        }
        function order() {
            var out = ''
            for (var i = 0; i < rows.count; i++) { out += rows.itemAt(i).cid + ',' }
            return out
        }
        function bump() { chatTwo.send('bump') }
        // What the core sends beside every message, every message sent
        // and every draft saved: the order may have changed, and no row.
        function reorder() { chats.handle_event(1, 'ChatlistChanged', '{}') }
    }
";

#[test]
fn a_message_moves_one_row_instead_of_rebuilding_the_list() {
    let temp = std::env::temp_dir().join(format!("piirit-chat-list-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
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
    let mut first_order = String::new();
    let first_ptr: *mut String = std::ptr::addr_of_mut!(first_order);
    let mut second_order = String::new();
    let second_ptr: *mut String = std::ptr::addr_of_mut!(second_order);

    let mut plan = common::Plan::default();

    // SAFETY for every block: fire only inside `exec()`; `engine` outlives it.
    plan.after(1, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    plan.after(2, move || unsafe {
        let value = (*engine_ptr).invoke_method("order".into(), &[]);
        *first_ptr = QString::from_qvariant(value)
            .map(|text| text.to_string())
            .unwrap_or_default();
        // Message in chat 2 moves it to top.
        (*engine_ptr).invoke_method("bump".into(), &[]);
    });

    plan.after(3, move || unsafe {
        let value = (*engine_ptr).invoke_method("order".into(), &[]);
        *second_ptr = QString::from_qvariant(value)
            .map(|text| text.to_string())
            .unwrap_or_default();
        (*engine_ptr).invoke_method("reorder".into(), &[]);
    });

    plan.after(2, move || unsafe {
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let calls = common::calls(&journal);
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();

    assert_eq!(
        first_order, "1,2,",
        "the list did not load in the core's order. Calls were: {names:?}"
    );
    assert_eq!(
        second_order, "2,1,",
        "a message in chat 2 did not move it to the top. Calls were: {names:?}"
    );

    // Reorder refetched only changed chat; rebuild would ask for both.
    let fetches: Vec<usize> = calls
        .iter()
        .filter(|(name, _)| name == "get_chatlist_items_by_entries")
        .map(|(_, params)| {
            params
                .pointer("/1")
                .and_then(Value::as_array)
                .map_or(0, Vec::len)
        })
        .collect();
    assert_eq!(
        fetches.first(),
        Some(&2),
        "the initial load did not fetch both chats in one call: {fetches:?}"
    );
    assert_eq!(
        fetches.last(),
        Some(&1),
        "the reorder refetched more than the chat that changed: {fetches:?}"
    );

    // Bare `ChatlistChanged` relisted chats, read none: rows held, none changed.
    let last_listing = names
        .iter()
        .rposition(|name| *name == "get_chatlist_entries");
    let last_fetch = names
        .iter()
        .rposition(|name| *name == "get_chatlist_items_by_entries");
    assert!(
        last_listing > last_fetch,
        "a change of order alone was not listed, or re-read rows: {names:?}"
    );
}
