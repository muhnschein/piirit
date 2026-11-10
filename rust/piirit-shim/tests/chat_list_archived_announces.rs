//! Arrival in ordinary chat is announced while list shows archived chats.
//!
//! One list feeds notifications and shows either side: archived listing alone holds no row for
//! the arrival, reader would hear nothing until filter off.

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

// Fake announces sent message as arrival (real core doesn't): lands without network.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        property string announced: ''
        ChatList {
            id: chats
            account_id: 1
            archived: true
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
fn an_ordinary_chats_arrival_is_announced_under_the_archived_filter() {
    let temp =
        std::env::temp_dir().join(format!("piirit-archived-announces-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
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
        ($name:expr) => {{
            let result = (*engine_ptr).invoke_method($name.into(), &[]);
            QString::from_qvariant(result)
                .map(|value| value.to_string())
                .unwrap_or_default()
        }};
    }

    let mut plan = common::Plan::default();

    // Archived side: chat 3 alone.
    plan.until(1, 30, move |last| unsafe {
        if call!("loaded") != "1" && !last {
            return false;
        }
        (*steps_ptr).push(("loaded", call!("loaded")));
        (*steps_ptr).push(("quiet", call!("heard")));
        (*steps_ptr).push(("land", call!("land")));
        true
    });

    plan.until(1, 30, move |last| unsafe {
        if call!("heard").is_empty() && !last {
            return false;
        }
        (*steps_ptr).push(("heard", call!("heard")));
        (*steps_ptr).push(("shown", call!("loaded")));
        (*engine_ptr).quit();
        true
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

    assert_eq!(
        value("loaded"),
        "1",
        "the archived side did not load alone, so this proves nothing. {context}"
    );
    assert_eq!(
        value("quiet"),
        "",
        "something was announced before anything arrived. {context}"
    );
    assert_eq!(value("land"), "ok", "sending failed. {context}");
    // Fake row summary is "last in <chat>"; real core's is message text.
    assert_eq!(
        value("heard"),
        "2:chat 2::last in 2|",
        "an arrival in an ordinary chat was not announced while the list showed \
         archived chats. {context}"
    );
    assert_eq!(
        value("shown"),
        "1",
        "the ordinary chat's arrival put it among the archived chats. {context}"
    );
}
