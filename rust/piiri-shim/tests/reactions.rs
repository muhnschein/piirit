//! Reacting to a message: what the tap sends, and what the row shows once
//! the core has answered.
//!
//! A second tap on the same emoji takes the reaction off, and a different
//! emoji replaces it -- one reaction per person, as the reference clients
//! have it. The other end's reaction is seeded by the fake, so a chip that
//! is not ours and the count our own adds to it are both on show.

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

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piiri 1.0
    Item {
        property string lastError: ''
        ChatMessages { id: chat; account_id: 1; chat_id: 1
                       onError: lastError = message }
        Connections {
            target: core
            onCore_event: chat.handle_event(context_id, kind, payload_json)
        }
        // The rows, read the way a delegate reads them.
        Repeater {
            id: rows
            model: chat.rows
            Item {
                property int mid: model.message_id
                property string chips: model.reactions
                property string mine: model.my_reaction
            }
        }
        function count() { return '' + chat.count }
        function chipsOf(index) {
            var row = rows.itemAt(index)
            return row ? row.chips : 'no-row'
        }
        function mineOn(index) {
            var row = rows.itemAt(index)
            return row ? row.mine : 'no-row'
        }
        function thumbsUp() { chat.react(1, '👍') }
        function heart() { chat.react(1, '❤️') }
        // Nothing to send: neither an empty reaction nor one on no message.
        function nothing() { chat.react(1, '  '); chat.react(0, '👍') }
        function error() { return lastError }
    }
";

#[test]
fn a_tap_reacts_a_second_takes_it_off_and_another_emoji_replaces_it() {
    let temp = std::env::temp_dir().join(format!("piiri-reactions-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRI_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRI_ACCOUNTS_DIR", temp.join("accounts"));
        // Ada has already reacted to the first message.
        std::env::set_var("PIIRI_FAKE_REACTED", "1");
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
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    single_shot(Duration::from_secs(3), move || unsafe {
        record!("loaded", call!("count"));
        record!("theirs", call!("chipsOf", 0));
        record!("mine-before", call!("mineOn", 0));
        record!("untouched", call!("chipsOf", 1));
        call!("nothing");
        call!("thumbsUp");
    });
    single_shot(Duration::from_secs(5), move || unsafe {
        record!("joined", call!("chipsOf", 0));
        record!("mine-after", call!("mineOn", 0));
        // The same emoji again.
        call!("thumbsUp");
    });
    single_shot(Duration::from_secs(7), move || unsafe {
        record!("taken-off", call!("chipsOf", 0));
        record!("mine-gone", call!("mineOn", 0));
        call!("heart");
    });
    single_shot(Duration::from_secs(9), move || unsafe {
        record!("replaced", call!("chipsOf", 0));
        record!("mine-heart", call!("mineOn", 0));
        record!("error", call!("error"));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&common::calls(&journal), &steps);
}

fn assert_outcome(calls: &[(String, Value)], steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();
    let context = format!("steps: {steps:?}, calls: {names:?}");
    let chips =
        |label: &str| -> Vec<Value> { serde_json::from_str(&value(label)).unwrap_or_default() };

    assert_eq!(value("loaded"), "2", "the chat did not load. {context}");
    assert_eq!(
        chips("theirs"),
        vec![json!({"emoji": "👍", "count": 1, "self": false})],
        "the other end's reaction is not on the row, or is marked as ours. {context}"
    );
    assert_eq!(
        value("mine-before"),
        "",
        "a row nobody here reacted to claims a reaction of ours. {context}"
    );
    assert_eq!(
        value("untouched"),
        "",
        "a message nobody reacted to shows reactions. {context}"
    );

    let sent: Vec<Value> = calls
        .iter()
        .filter(|(name, _)| name == "send_reaction")
        .map(|(_, params)| params.clone())
        .collect();
    assert_eq!(
        sent,
        vec![
            json!([1, 1, ["👍"]]),
            json!([1, 1, []]),
            json!([1, 1, ["❤️"]]),
        ],
        "the taps did not send what they meant: a reaction, then none, then \
         another -- and nothing for an empty emoji or no message. {context}"
    );

    assert_eq!(
        chips("joined"),
        vec![json!({"emoji": "👍", "count": 2, "self": true})],
        "our reaction did not join the other end's on the same chip. {context}"
    );
    assert_eq!(
        value("mine-after"),
        "👍",
        "the row does not know the reaction is ours. {context}"
    );
    assert_eq!(
        chips("taken-off"),
        vec![json!({"emoji": "👍", "count": 1, "self": false})],
        "a second tap on the same emoji did not take ours off. {context}"
    );
    assert_eq!(value("mine-gone"), "", "{context}");
    // Most frequent first, and equal counts by emoji: the core's own
    // order, which the fake keeps, and in which the heart sorts before
    // the thumb.
    assert_eq!(
        chips("replaced"),
        vec![
            json!({"emoji": "❤️", "count": 1, "self": true}),
            json!({"emoji": "👍", "count": 1, "self": false}),
        ],
        "a different emoji did not go on beside the other end's. {context}"
    );
    assert_eq!(value("mine-heart"), "❤️", "{context}");

    // The row is re-read after each answer, so the tap shows at once
    // rather than when the event gets round to it.
    let refetches = calls
        .iter()
        .filter(|(name, params)| {
            name == "get_messages" && params.pointer("/1") == Some(&json!([1]))
        })
        .count();
    assert!(
        refetches >= 3,
        "message 1 was re-read {refetches} times after three reactions, so a \
         tap does not show until the core's event arrives. {context}"
    );
    assert_eq!(value("error"), "", "{context}");
}
