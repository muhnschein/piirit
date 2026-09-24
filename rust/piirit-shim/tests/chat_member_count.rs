//! The conversation model counts who is in the group.
//!
//! What the header over the messages says under the name. It arrives with
//! the chat's shape, so a group opens with the count already on it rather
//! than filling in a moment later, and it follows someone joining or
//! leaving -- from this device or another -- on the same events the name
//! does. A one-to-one chat has no count: the number would say "2" about a
//! conversation with one other person, and the header would carry a line
//! that tells the reader nothing.

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

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        property int announcements: 0
        // The group's conversation, the one-to-one chat beside it, and
        // the group page's own model -- which is what adds a member.
        ChatMessages {
            id: messages
            account_id: 1
            onMember_count_changed: announcements += 1
        }
        ChatMessages { id: alone; account_id: 1 }
        ChatInfo { id: info; account_id: 1 }
        function open() {
            messages.chat_id = 2
            alone.chat_id = 1
            info.chat_id = 2
            return 'ok'
        }
        Connections {
            target: core
            onCore_event: {
                messages.handle_event(context_id, kind, payload_json)
                info.handle_event(context_id, kind, payload_json)
            }
        }
        function count() { return '' + messages.member_count }
        function groupness() { return '' + messages.is_group }
        function oneToOneCount() { return '' + alone.member_count }
        function announced() { return '' + announcements }
        function add(contact) { info.add_members([contact]); return 'ok' }
    }
";

#[test]
fn the_header_over_a_group_counts_its_members_and_follows_a_joiner() {
    let temp = std::env::temp_dir().join(format!("piirit-member-count-{}", std::process::id()));
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
    engine.load_data(QByteArray::from(PROBE_QML));

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

    single_shot(Duration::from_secs(2), move || unsafe {
        record!("open", call!("open"));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("group", call!("groupness"));
        record!("counted", call!("count"));
        record!("one-to-one", call!("oneToOneCount"));
        record!("add", call!("add", 11));
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        record!("after-joining", call!("count"));
        record!("announced", call!("announced"));
        (*engine_ptr).quit();
    });

    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("open"), "ok", "the chats were not opened. {context}");
    assert_eq!(
        value("group"),
        "true",
        "the chat the count is about is not a group. {context}"
    );
    assert_eq!(
        value("counted"),
        "2",
        "the group opened without the count its header shows. {context}"
    );
    assert_eq!(
        value("one-to-one"),
        "0",
        "a one-to-one chat was counted, so its header would carry a \
         number about a conversation with one other person. {context}"
    );
    assert_eq!(value("add"), "ok", "nobody was added. {context}");
    assert_eq!(
        value("after-joining"),
        "3",
        "somebody joined and the header did not follow. {context}"
    );
    // Once on the load and once on the joiner. The property is announced
    // only when it changed, so a count re-read and found the same must
    // not redraw the header.
    assert_eq!(
        value("announced"),
        "2",
        "the count was announced other than once per change. {context}"
    );
}
