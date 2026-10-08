//! A message edited or deleted in a chat is restated, once the list has
//! read the chat's row back: what a notification standing for it should
//! say now, and how much is still unread there.
//!
//! An edit reaches the app as `MsgsChanged` on the message, a deletion as
//! `MsgDeleted`; neither is an arrival, so neither is announced. Other
//! list traffic restates nothing.

// Qt harness: see qml_chat_list.rs.
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
        property string announced: ''
        property string restated: ''
        ChatList {
            id: chats
            account_id: 1
            onMessage_arrived: announced += chat_id + '|'
            onMessage_restated: {
                restated += chat_id + ':' + chat_name + ':' + sender + ':' + preview
                    + ':' + unread + '|'
            }
        }
        Connections {
            target: core
            onStatus_changed: {
                if (core.status === 'ready') { chats.reload() }
            }
        }
        function loaded() { return '' + chats.count }
        function feed(kind, payload) { chats.handle_event(1, kind, payload); return 'ok' }
        function heard() { return announced + '#' + restated }
    }
";

#[test]
fn an_edit_or_a_deletion_is_restated_and_nothing_else_is() {
    let temp = std::env::temp_dir().join(format!("piirit-list-restates-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
    }

    piirit_shim::register_qml_types();

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
                &[$(QVariant::from(QString::from($arg))),*],
            );
            QString::from_qvariant(result)
                .map(|value| value.to_string())
                .unwrap_or_default()
        }};
    }

    single_shot(Duration::from_secs(2), move || unsafe {
        (*steps_ptr).push(("loaded", call!("loaded")));
        // List traffic that is neither: nothing to restate.
        (*steps_ptr).push((
            "other",
            call!("feed", "ChatlistItemChanged", r#"{"chatId":2}"#),
        ));
        (*steps_ptr).push(("noticed", call!("feed", "MsgsNoticed", r#"{"chatId":2}"#)));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("quiet", call!("heard")));
        (*steps_ptr).push((
            "edit",
            call!("feed", "MsgsChanged", r#"{"chatId":2,"msgId":9}"#),
        ));
        (*steps_ptr).push((
            "delete",
            call!("feed", "MsgDeleted", r#"{"chatId":1,"msgId":3}"#),
        ));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        (*steps_ptr).push(("heard", call!("heard")));
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
        "2",
        "the list did not load, so this proves nothing. {context}"
    );
    assert_eq!(
        value("quiet"),
        "#",
        "list traffic that is neither an edit nor a deletion was restated \
         or announced. {context}"
    );
    let heard = value("heard");
    let (announced, restated) = heard.split_once('#').unwrap_or_default();
    assert_eq!(
        announced, "",
        "an edit or a deletion was announced as an arrival. {context}"
    );
    // The fake's row summary is "last in <chat>" and its rows have
    // nothing unread; the order is the set's, so compared as a set.
    let mut restated: Vec<&str> = restated
        .split('|')
        .filter(|part| !part.is_empty())
        .collect();
    restated.sort_unstable();
    assert_eq!(
        restated,
        vec!["1:chat 1::last in 1:0", "2:chat 2::last in 2:0"],
        "the edited and the deleted chat were not each restated once, \
         from their rows. {context}"
    );
}
