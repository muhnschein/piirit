//! Searching chat list, archived list, one contact's chats. All must reach core.
//!
//! Filtering loaded rows finds only on-screen chats; archived is a disjoint list.

// Qt harness: see chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        ChatList { id: chats; account_id: 1 }
        function search(text) { chats.query = text }
        function showArchived() { chats.archived = true }
        function accept(chatId) { chats.accept_chat(chatId) }
        function block(chatId) { chats.block_chat(chatId) }
        function byContact(contactId) { chats.contact_id = contactId }
    }
";

/// Parameters: (`accountId`, `listFlags`, `query`, `contactId`).
fn query_of(params: &Value) -> Option<&str> {
    params.get(2).and_then(Value::as_str)
}

fn contact_of(params: &Value) -> Option<u64> {
    params.get(3).and_then(Value::as_u64)
}

/// See [`query_of`].
fn flags_of(params: &Value) -> Option<u64> {
    params.get(1).and_then(Value::as_u64)
}

/// Every `get_chatlist_entries` call's parameters, in order.
fn entry_calls(journal: &std::path::Path) -> Vec<Value> {
    common::calls(journal)
        .into_iter()
        .filter(|(method, _)| method == "get_chatlist_entries")
        .map(|(_, params)| params)
        .collect()
}

#[test]
fn a_search_and_the_archived_list_are_asked_of_the_core() {
    let temp = std::env::temp_dir().join(format!("piirit-chat-filters-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt starts.
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

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    plan.after(2, move || unsafe {
        (*engine_ptr).invoke_method("search".into(), &[QVariant::from(QString::from("anna"))]);
    });

    plan.after(2, move || unsafe {
        (*engine_ptr).invoke_method("showArchived".into(), &[]);
        (*engine_ptr).invoke_method("accept".into(), &[QVariant::from(7u32)]);
        (*engine_ptr).invoke_method("block".into(), &[QVariant::from(9u32)]);
    });

    plan.after(1, move || unsafe {
        (*engine_ptr).invoke_method("byContact".into(), &[QVariant::from(10u32)]);
    });

    plan.after(3, move || unsafe {
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let entries = entry_calls(&journal);
    assert!(
        !entries.is_empty(),
        "the model never asked the core for a chat list at all"
    );

    assert!(
        entries
            .iter()
            .any(|params| query_of(params) == Some("anna")),
        "the search text never reached the core, so a search can only find \
         chats already on screen. calls: {entries:?}"
    );
    assert!(
        entries.iter().any(|params| flags_of(params) == Some(1)),
        "the archived list was never asked for with DC_GCL_ARCHIVED_ONLY, \
         so it would show the ordinary chats again. calls: {entries:?}"
    );
    assert!(
        entries.iter().any(|params| contact_of(params) == Some(10)),
        "a list of one contact's chats was never asked of the core by \
         contact, so it could only filter chats already loaded. calls: \
         {entries:?}"
    );
    // First load: neither search nor archive nor contact.
    let first = entries.first().cloned().unwrap_or(Value::Null);
    assert_eq!(
        query_of(&first),
        None,
        "the ordinary list was loaded with a query. calls: {entries:?}"
    );
    assert_eq!(
        flags_of(&first),
        None,
        "the ordinary list was loaded with list flags. calls: {entries:?}"
    );
    assert_eq!(
        contact_of(&first),
        None,
        "the ordinary list was loaded for one contact. calls: {entries:?}"
    );

    let methods = common::methods(&journal);
    assert!(
        methods.iter().any(|name| name == "accept_chat"),
        "accepting a contact request asked the core nothing. calls: {methods:?}"
    );
    assert!(
        methods.iter().any(|name| name == "block_chat"),
        "blocking a contact request asked the core nothing. calls: {methods:?}"
    );

    let _ = std::fs::remove_dir_all(&temp);
}
