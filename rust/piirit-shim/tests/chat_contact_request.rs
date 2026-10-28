//! Conversation model answers a contact request.
//!
//! Request-ness arrives with chat shape (page swaps field for accept/decline at once). Accept makes
//! it an ordinary chat: shape re-read, field and sending back. Block and delete leave nothing to
//! show: model says `closed`, page leaves. Each a whole-chat call core takes as `(account, chat)`.

// Qt harness allows: unsafe `env::set_var` pre-Qt, borrow_as_ptr.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::json;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        property int closedOneToOne: 0
        property int closedGroup: 0
        property int closedPlain: 0
        // One-to-one request (accepted), group request (deleted), another
        // one-to-one request (blocked), and an ordinary chat.
        ChatMessages { id: request; account_id: 1 }
        ChatMessages { id: group; account_id: 1; onClosed: closedGroup += 1 }
        ChatMessages { id: blocked; account_id: 1; onClosed: closedOneToOne += 1 }
        ChatMessages { id: plain; account_id: 1; onClosed: closedPlain += 1 }
        function open() {
            request.chat_id = 1
            group.chat_id = 2
            blocked.chat_id = 3
            plain.chat_id = 201
            return 'ok'
        }
        Connections {
            target: core
            onCore_event: {
                request.handle_event(context_id, kind, payload_json)
                group.handle_event(context_id, kind, payload_json)
                blocked.handle_event(context_id, kind, payload_json)
                plain.handle_event(context_id, kind, payload_json)
            }
        }
        function shape(model) {
            return model.is_contact_request + '/' + model.can_send + '/' + model.can_call
        }
        function requestShape() { return shape(request) }
        function groupShape() { return shape(group) + '/' + group.is_group }
        function plainShape() { return shape(plain) }
        function accept() { request.accept_chat(); return 'ok' }
        function decline() { group.delete_chat(); blocked.block_chat(); return 'ok' }
        function closings() {
            return closedGroup + '/' + closedOneToOne + '/' + closedPlain
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_contact_request_is_accepted_blocked_or_deleted_from_its_conversation() {
    let temp = std::env::temp_dir().join(format!("piirit-contact-request-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_REQUESTS", "1,2,3");
        // Chat 201 exists: ordinary chat beside requests.
        std::env::set_var("PIIRIT_FAKE_MANY_CHATS", "1");
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

    let mut plan = common::Plan::default();

    plan.after(2, move || unsafe {
        record!("open", call!("open"));
    });

    plan.after(2, move || unsafe {
        record!("request", call!("requestShape"));
        record!("group", call!("groupShape"));
        record!("plain", call!("plainShape"));
        record!("accept", call!("accept"));
        record!("decline", call!("decline"));
    });

    plan.after(3, move || unsafe {
        record!("accepted", call!("requestShape"));
        record!("closings", call!("closings"));
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

    assert_eq!(value("open"), "ok", "the chats were not opened. {context}");
    // request / can send / can call.
    assert_eq!(
        value("request"),
        "true/false/false",
        "a contact request opened as an ordinary chat, so the page offers a \
         field nothing can be sent from and no way to answer it. {context}"
    );
    assert_eq!(
        value("group"),
        "true/false/false/true",
        "a group the account was added to is not shown as a request. {context}"
    );
    assert_eq!(
        value("plain"),
        "false/true/true",
        "an ordinary chat is taken for a contact request. {context}"
    );
    assert_eq!(
        value("accepted"),
        "false/true/true",
        "accepting a request did not turn it into a chat the reader can \
         write in. {context}"
    );
    // Group deleted, one-to-one blocked: both leave; ordinary chat untouched.
    assert_eq!(
        value("closings"),
        "1/1/0",
        "a request that was turned down did not close its conversation. {context}"
    );

    let calls = common::calls(&journal);
    for (method, chat) in [("accept_chat", 1), ("delete_chat", 2), ("block_chat", 3)] {
        assert!(
            calls
                .iter()
                .any(|(name, params)| name == method && *params == json!([1, chat])),
            "{method} was not asked of core for chat {chat} as (account, chat). calls: {calls:?}"
        );
    }
}
