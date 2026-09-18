//! The per-chat message model: what it asks the core for, and when.
//!
//! Two instances are driven at once, which is the case the old single shared
//! model got wrong -- opening a second conversation reset the first.

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

use piiri_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::{json, Value};

mod common;

/// Two models over different chats, plus a way to read their row counts and
/// to send. Written in the Qt 5.6 dialect with the shim's `snake_case`
/// names.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piiri 1.0
    Item {
        property string lastError: ''
        ChatMessages { id: first; account_id: 1; chat_id: 1
                       onError: lastError = message }
        ChatMessages { id: second; account_id: 1; chat_id: 2
                       onError: lastError = message }
        Connections {
            target: core
            onCore_event: {
                first.handle_event(context_id, kind, payload_json)
                second.handle_event(context_id, kind, payload_json)
            }
        }
        function counts() { return first.count + '/' + second.count }
        function error() { return lastError }

        // Both loads finished, so what follows is not racing them.
        property bool loaded: first.count > 0 && second.count > 0
        property string countsAtLoad: ''
        property bool haveSent: false

        // Retried rather than fired at a fixed moment: the models load
        // over a process, and under a loaded `make check` that is not
        // always done by any particular second. Waiting for the thing
        // itself is what makes this reliable.
        Timer {
            interval: 200; running: true; repeat: true
            onTriggered: {
                if (!loaded || haveSent) { return }
                countsAtLoad = counts()
                haveSent = true
                first.send('hello there')
            }
        }
        function countsBeforeSend() { return countsAtLoad }
        function sentYet() { return haveSent ? 'yes' : 'no' }
    }
";

#[test]
fn each_chat_has_its_own_model_and_loads_in_one_batch() {
    let temp = std::env::temp_dir().join(format!("piiri-chat-model-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRI_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRI_ACCOUNTS_DIR", temp.join("accounts"));
        // This test is about the ordering where a send's own reply is
        // dealt with before the event the same send produced: the row is
        // already in the model by the time the event arrives, so the
        // refresh it triggers has nothing left to fetch. Which of the two
        // wins is otherwise a race between queued callbacks on the Qt
        // thread, and it went the other way on a loaded CI runner --
        // where the model correctly fetched a message it did not have
        // yet, and the assertions below, which only describe this
        // ordering, failed. The other ordering has tests of its own:
        // chat_send_race.rs and chat_sync_race.rs.
        std::env::set_var("PIIRI_FAKE_EVENT_DELAY_MS", "1500");
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
    let mut counts_after_load = String::new();
    let counts_ptr: *mut String = std::ptr::addr_of_mut!(counts_after_load);

    // The models load as soon as the QML sets their ids, which needs the
    // core to be up: load the probe a tick after start.
    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    let mut counts_after_send = String::new();
    let sent_ptr: *mut String = std::ptr::addr_of_mut!(counts_after_send);
    let mut ever_sent = String::new();
    let ever_ptr: *mut String = std::ptr::addr_of_mut!(ever_sent);
    single_shot(Duration::from_secs(8), move || unsafe {
        let read = |name: &str| {
            QString::from_qvariant((*engine_ptr).invoke_method(name.into(), &[]))
                .map(|text| text.to_string())
                .unwrap_or_default()
        };
        *counts_ptr = read("countsBeforeSend");
        *sent_ptr = read("counts");
        *ever_ptr = read("sentYet");
        (*engine_ptr).quit();
    });

    engine.exec();

    // If the send never happened the rest says nothing, and would say it
    // as a confusing count mismatch rather than as what went wrong.
    assert_eq!(
        ever_sent, "yes",
        "the models never both finished loading, so nothing was sent"
    );

    let calls = common::calls(&journal);
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();

    // The two chats are seeded with two messages and one. Separate models,
    // separate contents.
    assert_eq!(
        counts_after_load, "2/1",
        "the two models do not hold their own chats. Calls were: {names:?}"
    );

    // One call for the whole chat, not one per message.
    assert!(
        !names.contains(&"get_message"),
        "messages were fetched one at a time: {names:?}"
    );
    let batches: Vec<usize> = calls
        .iter()
        .filter(|(name, _)| name == "get_messages")
        .map(|(_, params)| {
            params
                .pointer("/1")
                .and_then(Value::as_array)
                .map_or(0, Vec::len)
        })
        .collect();
    // One load per model. The two race, so which lands first is not fixed --
    // assert on the set, not the order.
    assert_eq!(batches.len(), 2, "expected one batch per model: {names:?}");
    assert!(
        batches.contains(&2),
        "the two-message chat did not come back in one call: {batches:?}"
    );

    // Sending appends to the sending model only, and the IncomingMsg the
    // core answers with must not duplicate the row.
    assert_eq!(
        counts_after_send, "3/1",
        "a sent message did not land exactly once in its own chat"
    );

    // Opening a chat marks its unread messages read, which is what sends
    // the read receipt. Only the unread ones, and only once.
    let seen: Vec<Value> = calls
        .iter()
        .filter(|(name, _)| name == "markseen_msgs")
        .map(|(_, params)| params.pointer("/1").cloned().unwrap_or(Value::Null))
        .collect();
    assert_eq!(
        seen,
        vec![json!([2])],
        "the unread message was not marked seen exactly once: {names:?}"
    );

    // The event path fetched only what it did not have -- which, after a
    // send, is nothing: the row is already in the model. So the event costs
    // one id-list call and no message fetch at all.
    let after_send: Vec<&str> = names
        .iter()
        .skip_while(|name| **name != "misc_send_msg")
        .skip(1)
        .copied()
        .collect();
    assert!(
        after_send.contains(&"get_message_list_items"),
        "the event did not reach the model: {names:?}"
    );
    assert!(
        !after_send.contains(&"get_messages"),
        "the event refetched messages the model already had: {names:?}"
    );
}
