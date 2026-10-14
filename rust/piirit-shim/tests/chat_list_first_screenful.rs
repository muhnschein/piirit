//! The chat list's first load: the rows a screen shows come first.
//!
//! The core builds chat-list items one after another, so asking for a
//! long list at once leaves the page empty until the last one is built.

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
        ChatList { id: chats; account_id: 1 }
        Repeater {
            id: rows
            model: chats.rows
            Item { property int cid: model.chat_id }
        }
        function count() { return '' + rows.count }
        function first() { return rows.count > 0 ? '' + rows.itemAt(0).cid : '' }
    }
";

#[test]
fn a_long_list_shows_its_first_screenful_before_the_rest() {
    let temp = std::env::temp_dir().join(format!("piirit-chat-list-first-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // 32 chats in all, and two seconds for each batch of rows.
        std::env::set_var("PIIRIT_FAKE_MANY_CHATS", "30");
        std::env::set_var("PIIRIT_FAKE_CHATLIST_ITEMS_DELAY_MS", "2000");
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
    let steps_ptr = std::ptr::addr_of_mut!(steps);

    macro_rules! call {
        ($name:expr) => {
            QString::from_qvariant((*engine_ptr).invoke_method($name.into(), &[]))
                .map(|text| text.to_string())
                .unwrap_or_default()
        };
    }

    // SAFETY for every block below: these callbacks fire only while `exec()`
    // is running on this thread, and `engine` and `steps` outlive it.
    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    // The first batch lands about two seconds after the load, the second
    // two seconds after that.
    single_shot(Duration::from_secs(4), move || unsafe {
        (*steps_ptr).push(("early-count", call!("count")));
        (*steps_ptr).push(("early-first", call!("first")));
    });
    single_shot(Duration::from_secs(7), move || unsafe {
        (*steps_ptr).push(("count", call!("count")));
        (*steps_ptr).push(("first", call!("first")));
        (*engine_ptr).quit();
    });

    engine.exec();

    let calls = common::calls(&journal);
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
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}, fetches: {fetches:?}");

    assert_eq!(
        fetches,
        vec![20, 12],
        "the first load did not ask for one screenful and then the rest. {context}"
    );
    assert_eq!(
        (value("early-count").as_str(), value("early-first").as_str()),
        ("20", "1"),
        "the first screenful was not shown, in order, before the rest was read. {context}"
    );
    assert_eq!(
        (value("count").as_str(), value("first").as_str()),
        ("32", "1"),
        "the rest of the list did not follow the first screenful. {context}"
    );
}
