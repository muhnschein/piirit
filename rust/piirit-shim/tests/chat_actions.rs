//! Chat-list context menu calls, with parameter shapes core accepts (pinned by `real_server.rs`).

// Qt harness allows: unsafe `env::set_var`, borrow_as_ptr, whole-second single_shot.
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
        property string lastError: ''
        ChatList { id: chats; account_id: 1; onError: lastError = message }
        Connections {
            target: core
            onCore_event: chats.handle_event(context_id, kind, payload_json)
        }
        function act() {
            chats.mark_read(1)
            chats.mark_unread(2)
            chats.set_pinned(1, true)
            chats.set_mute_duration(1, 28800)
            chats.set_mute_duration(2, -1)
            chats.archive(1)
        }
        function remove() {
            chats.set_mute_duration(1, 0)
            chats.delete_chat(2)
        }
        function count() { return chats.count }
        function error() { return lastError }
    }
";

#[test]
fn the_context_menu_sends_what_the_core_expects() {
    let temp = std::env::temp_dir().join(format!("piirit-chat-actions-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
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
    single_shot(Duration::from_secs(3), move || unsafe {
        (*engine_ptr).invoke_method("act".into(), &[]);
    });

    let mut before_delete = String::new();
    let before_ptr: *mut String = std::ptr::addr_of_mut!(before_delete);
    single_shot(Duration::from_secs(5), move || unsafe {
        let value = (*engine_ptr).invoke_method("count".into(), &[]);
        *before_ptr = QString::from_qvariant(value)
            .map(|text| text.to_string())
            .unwrap_or_default();
        (*engine_ptr).invoke_method("remove".into(), &[]);
    });

    let mut report = String::new();
    let report_ptr: *mut String = std::ptr::addr_of_mut!(report);
    single_shot(Duration::from_secs(7), move || unsafe {
        let count = (*engine_ptr).invoke_method("count".into(), &[]);
        let error = (*engine_ptr).invoke_method("error".into(), &[]);
        *report_ptr = format!(
            "{}#{}",
            QString::from_qvariant(count)
                .map(|text| text.to_string())
                .unwrap_or_default(),
            QString::from_qvariant(error)
                .map(|text| text.to_string())
                .unwrap_or_default(),
        );
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&common::calls(&journal), &before_delete, &report);
}

/// Every action reaches core in its shape; deleted chat leaves list.
fn assert_outcome(calls: &[(String, Value)], before_delete: &str, report: &str) {
    let params_of = |method: &str| -> Vec<Value> {
        calls
            .iter()
            .filter(|(name, _)| name == method)
            .map(|(_, params)| params.clone())
            .collect()
    };
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();
    let context = format!("calls: {names:?}");

    assert!(
        params_of("marknoticed_chat").contains(&serde_json::json!([1, 1])),
        "mark_read did not reach the core as marknoticed_chat(1, 1): {report}"
    );
    assert!(
        params_of("markfresh_chat").contains(&serde_json::json!([1, 2])),
        "mark_unread did not reach the core as markfresh_chat(1, 2). {context}"
    );
    // One method with variant name; four actions race, so assert on the set.
    let visibility = params_of("set_chat_visibility");
    assert!(
        visibility.contains(&serde_json::json!([1, 1, "Pinned"]))
            && visibility.contains(&serde_json::json!([1, 1, "Archived"]))
            && visibility.len() == 2,
        "pinning and archiving did not send the core's own variant names: {visibility:?}. \
         {context}"
    );
    // Timed, forever (both race), then unmute a step later.
    let mutes = params_of("set_chat_mute_duration");
    assert!(
        mutes.len() == 3
            && mutes[..2]
                .contains(&serde_json::json!([1, 1, {"kind": "Until", "duration": 28800}]))
            && mutes[..2].contains(&serde_json::json!([1, 2, {"kind": "Forever"}]))
            && mutes[2] == serde_json::json!([1, 1, {"kind": "NotMuted"}]),
        "muting for a while, forever, and unmuting did not send the core's tagged \
         durations: {mutes:?}. {context}"
    );
    assert_eq!(
        params_of("delete_chat"),
        vec![serde_json::json!([1, 2])],
        "deleting a chat did not reach the core. {context}"
    );

    assert_eq!(before_delete, "2", "the list did not load. {context}");
    assert_eq!(
        report, "1#",
        "a deleted chat stayed in the list, or an action failed. {context}"
    );
}
