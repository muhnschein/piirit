//! Opening a chat has to clear its unread badge, even when the page was
//! not yet on screen at the moment the messages arrived.
//!
//! `reading_history` exists so a reader scrolled up in history, or an app
//! sitting in the background, does not send read receipts for messages
//! nobody has looked at. It is driven from `readerIsLooking`, which is
//! false until the page transition finishes -- and a local fetch can
//! easily finish first. Checking it once, when the load returns, means a
//! chat opened that way is never marked read at all: the badge stays
//! forever, because everything afterwards only marks messages that
//! *arrive*.

// Qt harness: see chat_actions.rs.
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
        // True at load: the page is still transitioning in, which is what
        // ConversationPage's readerIsLooking reports until it is Active.
        ChatMessages {
            id: messages
            account_id: 1
            chat_id: 1
            reading_history: true
        }
        function arrive() { messages.reading_history = false }
        function count() { return messages.count }
    }
";

#[test]
fn a_chat_opened_before_its_page_settles_is_still_marked_read() {
    let temp = std::env::temp_dir().join(format!("piirit-mark-read-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts.
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

    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    // The load has finished by now, and nothing should have been marked:
    // as far as the app knows nobody is looking yet.
    let mut during_load = Vec::new();
    let during_ptr: *mut Vec<String> = std::ptr::addr_of_mut!(during_load);
    single_shot(Duration::from_secs(3), move || unsafe {
        *during_ptr = common::methods(std::path::Path::new(
            &std::env::var("PIIRIT_FAKE_JOURNAL").unwrap_or_default(),
        ));
        // The page finishes its transition: the reader is now looking.
        (*engine_ptr).invoke_method("arrive".into(), &[]);
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        (*engine_ptr).quit();
    });

    engine.exec();

    let calls = common::calls(&journal);
    assert_outcome(&calls, &during_load);
}

fn assert_outcome(calls: &[(String, Value)], during_load: &[String]) {
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();
    let context = format!("calls: {names:?}");

    assert!(
        !during_load.iter().any(|name| name == "marknoticed_chat"),
        "the chat was marked read while the reader was still not looking, \
         which is what reading_history exists to prevent. \
         during load: {during_load:?}"
    );
    assert!(
        names.contains(&"marknoticed_chat"),
        "the reader started looking and the chat was never marked read, so \
         its unread badge never clears. {context}"
    );
}
