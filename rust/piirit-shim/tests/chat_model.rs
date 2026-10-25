//! Per-chat message model: what it asks core for, and when.
//!
//! Two instances at once: old shared model reset the first on second open.

// Qt harness allows: unsafe `env::set_var`, borrow_as_ptr.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::{json, Value};

mod common;

/// Two models on different chats; read row counts, send. Qt 5.6 dialect, `snake_case` names.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
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
    let temp = std::env::temp_dir().join(format!("piirit-chat-model-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Event delayed so send's reply lands first: row already held, refresh fetches nothing.
        // Undelayed, order raced and flipped on loaded CI. Other order's tests: chat_send_race.rs,
        // chat_sync_race.rs.
        std::env::set_var("PIIRIT_FAKE_EVENT_DELAY_MS", "1500");
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
    let mut counts_after_load = String::new();
    let counts_ptr: *mut String = std::ptr::addr_of_mut!(counts_after_load);

    let mut plan = common::Plan::default();

    // Models load once QML sets ids: load probe a tick after core is up.
    plan.after(1, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    let mut counts_after_send = String::new();
    let sent_ptr: *mut String = std::ptr::addr_of_mut!(counts_after_send);
    let mut ever_sent = String::new();
    let ever_ptr: *mut String = std::ptr::addr_of_mut!(ever_sent);
    plan.after(7, move || unsafe {
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

    plan.run();
    engine.exec();

    // Send never happened: rest would show as count mismatch, not cause.
    assert_eq!(
        ever_sent, "yes",
        "the models never both finished loading, so nothing was sent"
    );

    let calls = common::calls(&journal);
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();

    // Two chats seeded with two and one messages; separate models, separate contents.
    assert_eq!(
        counts_after_load, "2/1",
        "the two models do not hold their own chats. Calls were: {names:?}"
    );

    // One call for whole chat, not per message.
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
    // One load per model; they race, so assert on set not order.
    assert_eq!(batches.len(), 2, "expected one batch per model: {names:?}");
    assert!(
        batches.contains(&2),
        "the two-message chat did not come back in one call: {batches:?}"
    );

    // Send appends to sending model only; reply must not duplicate row.
    assert_eq!(
        counts_after_send, "3/1",
        "a sent message did not land exactly once in its own chat"
    );

    // Opening chat marks unread read (read receipt). Only unread, once.
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

    // Event fetched only what it lacked: nothing after a send. One id-list call.
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
