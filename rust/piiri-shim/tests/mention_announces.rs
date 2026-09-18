//! A muted group stays quiet, except for a reply to the reader.
//!
//! Muting a chat is what stops its arrivals being announced. The
//! reference clients make one exception, when the reader asks for it: a
//! message in a muted *group* that quotes one of the reader's own is for
//! them, and is announced all the same. A plain message in the muted
//! group is not, nor is anything with the setting off, nor a reply in a
//! muted one-to-one chat -- that chat was muted with the one person in
//! it in mind.
//!
//! The fake core reads a message sent from here as the account's own
//! under `PIIRI_FAKE_SELF_SENT`, which is what makes a reply to it a
//! reply to the reader; and it announces every send as an arrival, which
//! is how the reply can land without a network.

// Qt harness: see chat_list_announces.rs.
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

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piiri 1.0
    Item {
        property string announced: ''
        property int lastSent: 0
        ChatList {
            id: chats
            account_id: 1
            notify_mentions: true
            onMessage_arrived: announced += chat_id + ':' + preview + '|'
        }
        // Chat 2 is the fake's group; chat 1 is one-to-one.
        ChatMessages { id: group; account_id: 1; chat_id: 2
                       onSent: lastSent = message_id }
        ChatMessages { id: single; account_id: 1; chat_id: 1
                       onSent: lastSent = message_id }
        Connections {
            target: core
            onCore_event: {
                chats.handle_event(context_id, kind, payload_json)
                group.handle_event(context_id, kind, payload_json)
                single.handle_event(context_id, kind, payload_json)
            }
            onStatus_changed: {
                if (core.status === 'ready') {
                    chats.reload(); group.reload(); single.reload()
                }
            }
        }
        function loaded() { return chats.count + '/' + group.count + '/' + single.count }
        function say(text) { group.send(text); return 'ok' }
        function replyTo(text) {
            group.quoted_message_id = lastSent
            group.send(text)
            return 'ok'
        }
        function sayAlone(text) { single.send(text); return 'ok' }
        function replyAlone(text) {
            single.quoted_message_id = lastSent
            single.send(text)
            return 'ok'
        }
        function mute(chatId) { chats.set_muted(chatId, true); return 'ok' }
        function setMentions(on) { chats.notify_mentions = on; return 'ok' }
        function heard() { return announced }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_reply_to_the_reader_is_announced_from_a_muted_group_and_nothing_else_is() {
    let temp = std::env::temp_dir().join(format!("piiri-mentions-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRI_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRI_FAKE_SELF_SENT", "1");
    }

    piiri_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
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

    // Something of the reader's own in the group, while it is still
    // loud, so there is a message to reply to.
    single_shot(Duration::from_secs(2), move || unsafe {
        record!("loaded", call!("loaded"));
        record!("say", call!("say", QString::from("mine")));
    });
    single_shot(Duration::from_secs(4), move || unsafe {
        record!("heard-loud", call!("heard"));
        record!("mute", call!("mute", 2u32));
    });
    // Muted: a reply to the reader gets through, a plain message does not.
    single_shot(Duration::from_secs(6), move || unsafe {
        record!("reply", call!("replyTo", QString::from("answering you")));
    });
    single_shot(Duration::from_secs(8), move || unsafe {
        record!("heard-reply", call!("heard"));
        record!("plain", call!("say", QString::from("to everyone")));
    });
    // With the setting off, not even the reply.
    single_shot(Duration::from_secs(10), move || unsafe {
        record!("heard-plain", call!("heard"));
        record!("mentions-off", call!("setMentions", false));
        record!(
            "reply-off",
            call!("replyTo", QString::from("answering again"))
        );
    });
    // And a one-to-one chat is not a group: muted is muted.
    single_shot(Duration::from_secs(12), move || unsafe {
        record!("heard-off", call!("heard"));
        record!("mentions-on", call!("setMentions", true));
        record!("say-alone", call!("sayAlone", QString::from("just us")));
    });
    single_shot(Duration::from_secs(14), move || unsafe {
        record!("heard-alone", call!("heard"));
        record!("mute-alone", call!("mute", 1u32));
    });
    single_shot(Duration::from_secs(16), move || unsafe {
        record!(
            "reply-alone",
            call!("replyAlone", QString::from("still us"))
        );
    });
    single_shot(Duration::from_secs(18), move || unsafe {
        record!("heard-end", call!("heard"));
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

    assert_eq!(
        value("loaded"),
        "2/1/2",
        "the list and the chats did not load, so this proves nothing. {context}"
    );
    // The fake's row summary is "last in <chat>" whatever was sent.
    assert_eq!(
        value("heard-loud"),
        "2:last in 2|",
        "a message in a chat nobody muted was not announced. {context}"
    );
    assert_eq!(
        value("heard-reply"),
        "2:last in 2|2:last in 2|",
        "a reply to one of the reader's own messages, in a muted group, \
         was not announced -- which is the one thing muting is meant to \
         let through when the reader asks. {context}"
    );
    assert_eq!(
        value("heard-plain"),
        "2:last in 2|2:last in 2|",
        "a plain message in a muted group was announced. {context}"
    );
    assert_eq!(
        value("heard-off"),
        "2:last in 2|2:last in 2|",
        "with mention notifications off, a reply in a muted group was \
         announced all the same. {context}"
    );
    assert_eq!(
        value("heard-alone"),
        "2:last in 2|2:last in 2|1:last in 1|",
        "a message in a one-to-one chat nobody muted was not announced. \
         {context}"
    );
    assert_eq!(
        value("heard-end"),
        "2:last in 2|2:last in 2|1:last in 1|",
        "a reply in a muted one-to-one chat was announced: only a group \
         counts, since a muted one-to-one chat was muted with that one \
         person in mind. {context}"
    );
}
