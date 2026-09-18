//! What a message's context menu sends: a reply carries the quote, and
//! delete and resend name the message they act on.
//!
//! Delete is two calls rather than one -- the message goes from this
//! account's devices, or the other ends are asked to delete their copies
//! too -- and which of them the reader picked has to reach the core
//! intact. Which kind is asked on a page (`qml_delete_choice`); this is
//! the far end of it.

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

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        property string lastError: ''
        ChatMessages { id: chat; account_id: 1; chat_id: 1
                       onError: lastError = message }
        // A second view of the same chat, standing in for the other end:
        // what it sends arrives at `chat` as someone else's message.
        ChatMessages { id: other; account_id: 1; chat_id: 1 }
        Connections {
            target: core
            onCore_event: {
                chat.handle_event(context_id, kind, payload_json)
                other.handle_event(context_id, kind, payload_json)
            }
        }
        function reply() {
            chat.quoted_message_id = 1
            chat.send('answering that')
        }
        // A reply the core refuses. The quote has to survive it: the reader
        // picked a message to answer, and a send that never happened is no
        // reason to drop it.
        function replyAndFail() {
            chat.quoted_message_id = 2
            chat.send('please fail')
        }
        function stillQuoting() { return '' + chat.quoted_message_id }
        // The reader is up in the history, not looking at the newest. The
        // far end is too, so only catching up can mark this read.
        function arrive() {
            chat.reading_history = true
            other.reading_history = true
            other.send('from them')
        }
        function catchUp() { chat.mark_seen_all() }
        // A deletion changes the id list, which is what makes the model
        // reload rather than append -- the other path that marks messages
        // read, and must not while the reader is up in the history.
        function forceReload() { chat.delete_message(1) }
        function remove() { chat.delete_message(2) }
        // The other kind: this account's own message, gone from
        // everybody's phone rather than from this one. 101 is the reply
        // sent above -- the fake core numbers what it is sent from 101,
        // above the ids it seeds a chat with.
        function removeEverywhere() { chat.delete_message_for_all(101) }
        function retry() { chat.resend_message(1) }
        function count() { return '' + chat.count }
        function error() { return lastError }
    }
";

#[test]
fn a_reply_carries_its_quote_and_the_rest_name_their_message() {
    let temp = std::env::temp_dir().join(format!("piirit-message-actions-{}", std::process::id()));
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

    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("loaded", call!("count")));
        call!("reply");
    });
    single_shot(Duration::from_secs(5), move || unsafe {
        // Read once the send has been answered, not before: the quote is
        // cleared when the message is really gone, so that a send which
        // fails leaves the reader the reply they chose.
        (*steps_ptr).push(("quote-cleared", call!("stillQuoting")));
        (*steps_ptr).push(("after-reply", call!("count")));
        call!("replyAndFail");
    });
    single_shot(Duration::from_secs(6), move || unsafe {
        (*steps_ptr).push(("quote-after-failure", call!("stillQuoting")));
        call!("remove");
    });
    single_shot(Duration::from_secs(8), move || unsafe {
        (*steps_ptr).push(("after-delete", call!("count")));
        call!("retry");
        call!("arrive");
    });

    single_shot(Duration::from_secs(10), move || unsafe {
        call!("forceReload");
    });

    single_shot(Duration::from_secs(12), move || unsafe {
        call!("catchUp");
        call!("removeEverywhere");
    });
    single_shot(Duration::from_secs(14), move || unsafe {
        (*steps_ptr).push(("error", call!("error")));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&common::calls(&journal), &steps);
}

/// The wire calls, and what they did to the model.
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

    // misc_send_msg's last parameter is the message being quoted.
    let sends = params_of("misc_send_msg");
    assert_eq!(
        sends.first().and_then(|params| params.pointer("/6")),
        Some(&serde_json::json!(1)),
        "the reply did not carry the message it answers: {sends:?}. {context}"
    );
    assert_eq!(
        value("quote-cleared"),
        "0",
        "the quote outlived the reply that used it. {context}"
    );
    assert_eq!(
        value("quote-after-failure"),
        "2",
        "a send the core refused took the reply's quote with it, so trying \
         again sends an unquoted message and says nothing about it. {context}"
    );
    assert_eq!(
        value("after-reply"),
        "3",
        "the reply is not in the chat. {context}"
    );

    // Two deletions: the menu's, and the one that forces a reload later.
    assert!(
        params_of("delete_messages").contains(&serde_json::json!([1, [2]])),
        "deleting did not name its message. {context}"
    );
    assert_eq!(
        value("after-delete"),
        "2",
        "a deleted message stayed in the chat. {context}"
    );
    assert_eq!(
        params_of("resend_messages"),
        vec![serde_json::json!([1, [1]])],
        "resending did not name its message. {context}"
    );
    // The reply sent above, deleted the other way: the core's own
    // `delete_messages_for_all`, which is the call that asks the other
    // ends to delete their copies as well.
    assert_eq!(
        params_of("delete_messages_for_all"),
        vec![serde_json::json!([1, [101]])],
        "deleting for everyone did not reach the core as the call that \
         means it. {context}"
    );
    // A resend leaves the id list alone, so the sync a core event triggers
    // fetches nothing: without an explicit re-read the row keeps the failed
    // state it had, mark and "Send again" and all.
    let refetched_after_resend = calls
        .iter()
        .skip_while(|(name, _)| name != "resend_messages")
        .any(|(name, params)| {
            name == "get_messages" && params.pointer("/1") == Some(&serde_json::json!([1]))
        });
    assert!(
        refetched_after_resend,
        "the resent message was never re-read, so its row still shows as \
         failed and still offers Send again. {context}"
    );
    assert!(
        value("error").contains("could not send"),
        "the refused send did not reach the page, got {:?}. {context}",
        value("error")
    );

    // A message arriving while the reader is up in the history is not read
    // yet: marking it so loses its unread badge in the chat list too. The
    // marker step splits the calls into before and after catching up.
    let seen_ids: Vec<Vec<u64>> = calls
        .iter()
        .filter(|(name, _)| name == "markseen_msgs")
        .filter_map(|(_, params)| params.pointer("/1").and_then(Value::as_array).cloned())
        .map(|ids| ids.iter().filter_map(Value::as_u64).collect())
        .collect();
    // 102 is what the far end sent while the reader was up in the history.
    let arrived: Vec<&Vec<u64>> = seen_ids.iter().filter(|ids| ids.contains(&102)).collect();
    assert_eq!(
        arrived.len(),
        1,
        "a message arriving out of sight was marked read {} times rather than \
         once, when the reader caught up: {seen_ids:?}. {context}",
        arrived.len()
    );
}
