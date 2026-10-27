//! Forwarding a message into another chat.
//!
//! Copy asked of core with destination. Picker lists only chats core accepts forwards into.

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
        ChatMessages { id: messages; account_id: 1; chat_id: 2 }
        // What the picker page builds: only chats a forward can land in.
        ChatList { id: picker; account_id: 1; for_forwarding: true }
        function forward() { messages.forward_to(11, 5) }
    }
";

#[test]
fn a_forward_names_its_destination_and_the_picker_asks_for_valid_ones() {
    let temp = std::env::temp_dir().join(format!("piirit-forwarding-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set pre-Qt.
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
        (*engine_ptr).invoke_method("forward".into(), &[]);
    });

    plan.after(3, move || unsafe {
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let calls = common::calls(&journal);

    let forward = calls
        .iter()
        .find(|(method, _)| method == "forward_messages")
        .map(|(_, params)| params.clone());
    let Some(params) = forward else {
        panic!(
            "forwarding asked the core nothing. calls: {:?}",
            calls.iter().map(|(name, _)| name).collect::<Vec<_>>()
        );
    };
    // (accountId, messageIds, destinationChatId).
    assert_eq!(
        params.get(0).and_then(Value::as_u64),
        Some(1),
        "the forward was sent for the wrong account: {params:?}"
    );
    assert_eq!(
        params.get(1).and_then(Value::as_array).map(Vec::len),
        Some(1),
        "the forward did not carry exactly the message asked for: {params:?}"
    );
    assert_eq!(
        params
            .get(1)
            .and_then(Value::as_array)
            .and_then(|ids| ids.first())
            .and_then(Value::as_u64),
        Some(11),
        "the forward carried the wrong message: {params:?}"
    );
    assert_eq!(
        params.get(2).and_then(Value::as_u64),
        Some(5),
        "the forward did not name the chat it was going to, so it would \
         land back where it started: {params:?}"
    );

    // `DC_GCL_FOR_FORWARDING`: picker can't offer a chat core would refuse.
    let asked_for_forwarding = calls
        .iter()
        .filter(|(method, _)| method == "get_chatlist_entries")
        .any(|(_, params)| {
            params
                .get(1)
                .and_then(Value::as_u64)
                .is_some_and(|flags| flags & 0x08 != 0)
        });
    assert!(
        asked_for_forwarding,
        "the picker asked for an ordinary chat list, so it can offer \
         destinations a forward would be refused by"
    );

    let _ = std::fs::remove_dir_all(&temp);
}
