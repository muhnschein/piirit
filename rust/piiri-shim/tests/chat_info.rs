//! A group after it has been made: renamed, given a picture, added to,
//! removed from, and left -- and a one-to-one chat read the same way.
//!
//! Until this existed a group could be created and then never touched:
//! `add_contact_to_chat` was reachable only from `create_group`. Each call
//! here is asserted on the wire, and the model is watched to read back what
//! the core then says.

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

// Driven off the model answering rather than off a clock: each step waits
// for the last one's reload to land, which is the thing being tested.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piiri 1.0
    Item {
        property string log: ''
        property int step: 0
        ChatInfo {
            id: group
            account_id: 1
            chat_id: 2
            onError: log += 'error:' + message + ';'
            onSaved: log += 'saved;'
            onRenamed: log += 'renamed:' + name + ';'
            onLeft: log += 'left;'
        }
        Connections {
            target: core
            onStatus_changed: {
                if (core.status === 'ready') { group.reload() }
            }
            onCore_event: group.handle_event(context_id, kind, payload_json)
        }
        Repeater {
            id: rows
            model: group.members
            Item {
                property int cid: model.contact_id
                property bool self_: model.is_self
            }
        }
        function members() {
            var out = ''
            for (var i = 0; i < rows.count; i++) {
                out += rows.itemAt(i).cid + (rows.itemAt(i).self_ ? '*' : '') + ','
            }
            return out
        }
        Timer {
            interval: 50
            repeat: true
            running: true
            onTriggered: {
                if (step === 0 && group.loaded && group.member_count === 2) {
                    step = 1
                    // Refused here, before the core sees it.
                    group.rename('  ')
                    // The name it already has: nothing to send.
                    group.rename('chat 2')
                    group.rename('Hikers')
                } else if (step === 1 && group.name === 'Hikers') {
                    step = 2
                    group.add_members([11])
                } else if (step === 2 && group.is_member(11)) {
                    step = 3
                    group.remove_member(10)
                } else if (step === 3 && !group.is_member(10)) {
                    step = 4
                    group.set_picture('file:///tmp/piiri%20fake/pic.png')
                } else if (step === 4 && group.avatar_path.length > 0) {
                    step = 5
                    group.clear_picture()
                } else if (step === 5 && group.avatar_path.length === 0) {
                    step = 6
                    group.set_ephemeral_timer(3600)
                } else if (step === 6 && group.ephemeral_timer === 3600) {
                    step = 7
                    group.leave()
                } else if (step === 7 && !group.can_edit) {
                    step = 8
                }
            }
        }
        function report() {
            return step + '#' + group.name + '#' + group.can_edit + '#'
                + group.avatar_path + '#' + members() + '#' + log
        }
    }
";

#[test]
fn a_group_can_be_changed_after_it_is_made() {
    let temp = std::env::temp_dir().join(format!("piiri-group-info-{}", std::process::id()));
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
    let mut report = String::new();
    let report_ptr: *mut String = std::ptr::addr_of_mut!(report);

    // SAFETY for every block: these fire only while `exec()` runs on this
    // thread, and `engine` outlives it.
    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    // The backstop: reads whatever the steps got to and quits.
    single_shot(Duration::from_secs(8), move || unsafe {
        let value = (*engine_ptr).invoke_method("report".into(), &[]);
        *report_ptr = QString::from_qvariant(value)
            .map(|text| text.to_string())
            .unwrap_or_default();
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_changes(&common::calls(&journal), &report);
}

/// Every change went to the core as the one call it is, and the model read
/// back what the core then held.
#[allow(clippy::too_many_lines)]
fn assert_changes(calls: &[(String, Value)], report: &str) {
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();
    let context = format!("report: {report}\ncalls: {names:?}");

    let mut parts = report.split('#');
    let step = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    let can_edit = parts.next().unwrap_or_default();
    let avatar = parts.next().unwrap_or_default();
    let members = parts.next().unwrap_or_default();
    let log = parts.next().unwrap_or_default();

    assert_eq!(step, "8", "the scenario did not run to the end. {context}");

    // The members came from the id list, with the account's own contact
    // marked, and in the core's order.
    assert!(
        names.contains(&"get_full_chat_by_id") && names.contains(&"get_contacts_by_ids"),
        "the group was never read from the core. {context}"
    );
    let asked = calls
        .iter()
        .find(|(name, _)| name == "get_contacts_by_ids")
        .map(|(_, params)| params.pointer("/1").cloned().unwrap_or_default())
        .unwrap_or_default();
    assert_eq!(
        asked,
        serde_json::json!([1, 10]),
        "the members were not looked up by the ids the chat named. {context}"
    );

    // Renaming: refused before the core for an empty name, skipped for the
    // name it already has, and sent once for a real change.
    let renames: Vec<&Value> = calls
        .iter()
        .filter(|(name, _)| name == "set_chat_name")
        .map(|(_, params)| params)
        .collect();
    assert_eq!(
        renames.len(),
        1,
        "exactly one rename should have reached the core. {context}"
    );
    assert_eq!(
        renames[0].pointer("/2").and_then(Value::as_str),
        Some("Hikers"),
        "the rename carried the wrong name. {context}"
    );
    assert_eq!(
        name, "Hikers",
        "the model did not read the new name back. {context}"
    );
    assert!(
        log.contains("error:A group needs a name;"),
        "an empty name was not refused with a reason. {context}"
    );
    assert!(
        log.contains("renamed:Hikers;"),
        "the rename was not announced. {context}"
    );

    // Adding and removing, each as the one call the core has for it.
    let added = calls
        .iter()
        .find(|(name, _)| name == "add_contact_to_chat")
        .map(|(_, params)| params.clone())
        .unwrap_or_default();
    assert_eq!(
        added,
        serde_json::json!([1, 2, 11]),
        "the member was not added to this group. {context}"
    );
    let removed = calls
        .iter()
        .find(|(name, _)| name == "remove_contact_from_chat")
        .map(|(_, params)| params.clone())
        .unwrap_or_default();
    assert_eq!(
        removed,
        serde_json::json!([1, 2, 10]),
        "the member was not removed from this group. {context}"
    );

    // The picture: a picker's URL unwrapped to the path the core wants,
    // and null to clear it.
    let pictures: Vec<Value> = calls
        .iter()
        .filter(|(name, _)| name == "set_chat_profile_image")
        .map(|(_, params)| params.pointer("/2").cloned().unwrap_or_default())
        .collect();
    assert_eq!(
        pictures,
        vec![serde_json::json!("/tmp/piiri fake/pic.png"), Value::Null],
        "the picture was not set from the decoded path and then cleared. {context}"
    );
    assert_eq!(
        avatar, "",
        "the cleared picture is still showing. {context}"
    );

    // Disappearing messages: seconds to the core, and the model reads
    // the timer back off the chat.
    let timer = calls
        .iter()
        .find(|(name, _)| name == "set_chat_ephemeral_timer")
        .map(|(_, params)| params.clone())
        .unwrap_or_default();
    assert_eq!(
        timer,
        serde_json::json!([1, 2, 3600]),
        "the timer was not set on this chat. {context}"
    );

    // Leaving is its own call, after which the core allows no edits and
    // the model says so.
    assert!(
        names.contains(&"leave_group"),
        "leaving did not reach the core. {context}"
    );
    assert!(
        log.contains("left;"),
        "leaving was not announced. {context}"
    );
    assert_eq!(
        can_edit, "false",
        "a group that has been left still offers edits. {context}"
    );
    // Grace stayed; Ada was removed and the account itself left.
    assert_eq!(
        members, "11,",
        "the members did not end up as the calls should have left them. {context}"
    );
    assert_eq!(
        log.matches("saved;").count(),
        5,
        "adding, removing, setting and clearing the picture, and the timer \
         should each have been confirmed once. {context}"
    );
}
