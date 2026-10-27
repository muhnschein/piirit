//! Reader's last-read point: where "new messages" line goes.
//!
//! Core asked once when chat opens; answer kept. Screen marks all read a moment
//! later, so a line tracking *current* unread would vanish unseen. Pinned: question
//! asked, answer reaches QML, reading chat leaves line put.

// Qt harness: see chat_actions.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        ChatMessages {
            id: messages
            account_id: 1
            chat_id: 1
        }
        function mark() { return '' + messages.unread_from }
        // The chat the fake has nothing unread in.
        function other() { messages.chat_id = 2; return 'ok' }
        function back() { messages.chat_id = 1; return 'ok' }
        function readIt() { messages.mark_seen_all(); return 'ok' }
    }
";

#[test]
fn the_line_is_read_once_when_a_chat_opens_and_stays_where_it_was() {
    let temp = std::env::temp_dir().join(format!("piirit-unread-mark-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt.
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
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    plan.after(2, move || unsafe {
        record!("opened", call!("mark"));
        // Reading would move a line tracking unread state, not reader.
        record!("read", call!("readIt"));
    });

    plan.after(2, move || unsafe {
        record!("after-reading", call!("mark"));
        record!("leave", call!("other"));
    });

    plan.after(2, move || unsafe {
        record!("elsewhere", call!("mark"));
        record!("return", call!("back"));
    });

    plan.after(2, move || unsafe {
        record!("returned", call!("mark"));
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    // Fake chat 1: messages 1 and 2, 2 unseen. Same as `get_messages`.
    assert_eq!(
        value("opened"),
        "2",
        "the line is not above the first message the reader has not seen. \
         {context}"
    );
    assert_eq!(
        value("after-reading"),
        "2",
        "reading the chat moved the line, so it is gone by the time anyone \
         could see it. {context}"
    );
    assert_eq!(
        value("elsewhere"),
        "0",
        "a chat with nothing unread still has a line in it. {context}"
    );
    assert_eq!(
        value("returned"),
        "2",
        "coming back to the chat did not ask again where the reader left \
         off. {context}"
    );

    // Once per opening, never with messages fetch still to come: answer must be the state
    // reader arrived to.
    let calls = common::calls(&journal);
    let asked = calls
        .iter()
        .filter(|(name, _)| name == "get_first_unread_message_of_chat")
        .count();
    assert_eq!(
        asked, 3,
        "the core was asked where the reader left off {asked} times for \
         three openings: {calls:?}"
    );
}
