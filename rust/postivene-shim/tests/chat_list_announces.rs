//! A message that lands is announced, whatever else the core says about
//! the list in the same breath.
//!
//! The real core follows every `IncomingMsg` with a `ChatlistItemChanged`
//! within the same millisecond (and a `ChatlistChanged` besides when the
//! order changed), each starting a refresh of its own. An announcement
//! riding on the refresh the first event started is made stale by the one
//! behind it before it lands, so nothing reaches the reader at all. The
//! fake core sends the pair, which is what makes this test see it.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

// The fake core announces a sent message as an arrival (the real one does
// not), which is how a message can be made to land without a network.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Postivene 1.0
    Item {
        property string announced: ''
        ChatList {
            id: chats
            account_id: 1
            onMessage_arrived: {
                announced += chat_id + ':' + chat_name + ':' + sender + ':' + preview + '|'
            }
        }
        ChatMessages {
            id: messages
            account_id: 1
            chat_id: 2
        }
        Connections {
            target: core
            onCore_event: {
                chats.handle_event(context_id, kind, payload_json)
                messages.handle_event(context_id, kind, payload_json)
            }
            onStatus_changed: {
                if (core.status === 'ready') { chats.reload(); messages.reload() }
            }
        }
        function loaded() { return '' + chats.count }
        function land() { messages.send('ping'); return 'ok' }
        function heard() { return announced }
    }
";

#[test]
fn an_arrival_is_announced_once_the_list_has_settled() {
    let temp =
        std::env::temp_dir().join(format!("postivene-list-announces-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
    }

    postivene_shim::register_qml_types();

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
        ($name:expr) => {{
            let result = (*engine_ptr).invoke_method($name.into(), &[]);
            QString::from_qvariant(result)
                .map(|value| value.to_string())
                .unwrap_or_default()
        }};
    }

    single_shot(Duration::from_secs(2), move || unsafe {
        (*steps_ptr).push(("loaded", call!("loaded")));
        (*steps_ptr).push(("quiet", call!("heard")));
        (*steps_ptr).push(("land", call!("land")));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
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
        "",
        "something was announced before anything arrived. {context}"
    );
    assert_eq!(value("land"), "ok", "sending failed. {context}");
    // The fake's row summary is "last in <chat>" whatever was sent; the
    // real core's is the message text, pinned in real_server.rs.
    assert_eq!(
        value("heard"),
        "2:chat 2::last in 2|",
        "the arrival was not announced exactly once, with the chat's name \
         and its row's preview, after the list event that follows it. {context}"
    );
}
