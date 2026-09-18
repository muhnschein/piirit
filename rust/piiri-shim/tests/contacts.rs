//! Starting a conversation: the three ways in.
//!
//! Until this existed nothing in the app created a chat, so these assertions
//! are about calls that had no caller before.

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

use piiri_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piiri 1.0
    Item {
        property string opened: ''
        property string lastError: ''
        property string myInvite: ''
        property bool started: false
        ContactList {
            id: contacts
            account_id: 1
            onChat_ready: opened = opened + chat_id + ','
            onError: lastError = message
            // Drive the scenario off the data arriving rather than off a
            // clock: under load the core takes longer to come up, and a
            // fixed tick then reads an empty list.
            onRows_changed: {
                if (!started && rows.count > 1) {
                    started = true
                    contacts.open_chat_with(rows.itemAt(0).cid)
                    contacts.create_group('Team', [rows.itemAt(0).cid, rows.itemAt(1).cid], '')
                    contacts.join_by_invite('https://i.delta.chat/#ABC&a=them%40example.org')
                    contacts.join_by_invite('just some text')
                    contacts.fetch_invite()
                }
            }
            onInvite_ready: myInvite = link
        }
        Connections {
            target: core
            onStatus_changed: {
                if (core.status === 'ready') { contacts.reload() }
            }
        }
        Repeater {
            id: rows
            model: contacts.rows
            Item {
                property int cid: model.contact_id
                property string addr: model.address
            }
        }
        function names() {
            var out = ''
            for (var i = 0; i < rows.count; i++) { out += rows.itemAt(i).addr + ',' }
            return out
        }
        function firstId() { return rows.count > 0 ? rows.itemAt(0).cid : 0 }
        function report() { return opened + '#' + lastError + '#' + myInvite }
    }
";

#[test]
fn a_chat_can_be_started_three_ways() {
    let temp = std::env::temp_dir().join(format!("piiri-contacts-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRI_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRI_ACCOUNTS_DIR", temp.join("accounts"));
    }

    piiri_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut listed = String::new();
    let listed_ptr: *mut String = std::ptr::addr_of_mut!(listed);
    let mut report = String::new();
    let report_ptr: *mut String = std::ptr::addr_of_mut!(report);

    // SAFETY for every block: these fire only while `exec()` runs on this
    // thread, and `engine` outlives it.
    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        let value = (*engine_ptr).invoke_method("names".into(), &[]);
        *listed_ptr = QString::from_qvariant(value)
            .map(|text| text.to_string())
            .unwrap_or_default();
        let value = (*engine_ptr).invoke_method("report".into(), &[]);
        *report_ptr = QString::from_qvariant(value)
            .map(|text| text.to_string())
            .unwrap_or_default();
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_routes(&common::calls(&journal), &listed, &report);
}

/// Each way in produced the calls it should, and no others.
fn assert_routes(calls: &[(String, Value)], listed: &str, report: &str) {
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();

    assert_eq!(
        listed, "ada@example.org,grace@example.org,",
        "the contact list did not load. Calls were: {names:?}"
    );

    let mut parts = report.splitn(3, '#');
    let opened = parts.next().unwrap_or_default();
    let error = parts.next().unwrap_or_default();
    let invite = parts.next().unwrap_or_default();
    assert_eq!(
        opened.split(',').filter(|part| !part.is_empty()).count(),
        3,
        "expected a chat from each of the three ways in, got {opened:?}"
    );

    // Following an invite: classified first, then joined. Plain text is
    // refused before any join is attempted.
    assert!(
        names.contains(&"secure_join"),
        "the invite link was never followed: {names:?}"
    );
    assert_eq!(
        names.iter().filter(|name| **name == "secure_join").count(),
        1,
        "plain text should not have been sent to secure_join: {names:?}"
    );
    assert!(
        error.contains("not a contact or group invite"),
        "pasting plain text should have said so, got {error:?}"
    );
    assert!(
        invite.starts_with("https://i.delta.chat/"),
        "the account's own invite link was not fetched, got {invite:?}"
    );

    // Contact tapped: straight to a chat, no contact created.
    assert!(
        names.contains(&"create_chat_by_contact_id"),
        "tapping a contact did not open a chat: {names:?}"
    );

    // Group: created encrypted, then both members added. Encrypted because
    // of which method this is, not because of any argument to it -- the
    // third one is upstream's deprecated `protect`, which it says to pass
    // `false` and then reads not at all.
    let group = calls
        .iter()
        .find(|(name, _)| name == "create_group_chat")
        .expect("the group must be created");
    assert_eq!(group.1.pointer("/1").and_then(Value::as_str), Some("Team"));
    assert_eq!(
        group.1.pointer("/2").and_then(Value::as_bool),
        Some(false),
        "the deprecated `protect` argument is not being passed as upstream asks"
    );
    assert!(
        !names.contains(&"create_group_chat_unencrypted"),
        "the group was created unencrypted: {names:?}"
    );
    assert_eq!(
        names
            .iter()
            .filter(|name| **name == "add_contact_to_chat")
            .count(),
        2,
        "both members should have been added: {names:?}"
    );
}
