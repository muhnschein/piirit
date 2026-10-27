//! Group after creation: renamed, picture, members, leaving. 1:1 too.
//! Was untouchable once made: `add_contact_to_chat` reachable only from `create_group`.
//! Each call asserted on wire; model reads back core state.

// Qt harness allows: unsafe `env::set_var`, borrow_as_ptr.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

// Model answers, not clock: each step waits for last reload.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        property string log: ''
        property int step: 0
        property string firstDescription: ''
        // How many of the rows marked at the first load were still the
        // same delegates after a member joined, and after one left.
        property int keptAfterAdd: -1
        property int keptAfterRemove: -1
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
                // Not from the model: set on a delegate by hand, so it
                // survives only as long as that delegate does.
                property bool marked: false
            }
        }
        function markRows() {
            for (var i = 0; i < rows.count; i++) {
                rows.itemAt(i).marked = true
            }
        }
        function markedRows() {
            var count = 0
            for (var i = 0; i < rows.count; i++) {
                if (rows.itemAt(i).marked) { count++ }
            }
            return count
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
                    firstDescription = group.description
                    markRows()
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
                    keptAfterAdd = markedRows()
                    group.remove_member(10)
                } else if (step === 3 && !group.is_member(10)) {
                    step = 4
                    keptAfterRemove = markedRows()
                    group.set_picture('file:///tmp/piirit%20fake/pic.png')
                } else if (step === 4 && group.avatar_path.length > 0) {
                    step = 5
                    group.clear_picture()
                } else if (step === 5 && group.avatar_path.length === 0) {
                    step = 6
                    group.set_ephemeral_timer(3600)
                } else if (step === 6 && group.ephemeral_timer === 3600) {
                    step = 7
                    // The text it already has, with the stray whitespace
                    // a field leaves: nothing to send.
                    group.set_description('Walks\nBring boots \n')
                    // A real change, kept multi-line and trimmed.
                    group.set_description('  Bring water\nand boots  \n')
                } else if (step === 7 && group.description === 'Bring water\nand boots') {
                    step = 8
                    // Empty clears.
                    group.set_description('')
                } else if (step === 8 && group.description === '') {
                    step = 9
                    // Refused by the core: said on `error`, not kept.
                    group.set_description('this will fail')
                } else if (step === 9 && log.indexOf('Cannot set chat description') >= 0) {
                    step = 10
                    group.leave()
                } else if (step === 10 && !group.can_edit) {
                    step = 11
                }
            }
        }
        function report() {
            return step + '#' + group.name + '#' + group.can_edit + '#'
                + group.avatar_path + '#' + members() + '#' + log
                + '#' + firstDescription + '#' + group.can_edit_description
                + '#' + keptAfterAdd + '#' + keptAfterRemove
        }
    }
";

#[test]
fn a_group_can_be_changed_after_it_is_made() {
    let temp = std::env::temp_dir().join(format!("piirit-group-info-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_DESCRIPTION", "Walks\nBring boots");
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
    let mut report = String::new();
    let report_ptr: *mut String = std::ptr::addr_of_mut!(report);

    let mut plan = common::Plan::default();

    // SAFETY for every block: fire only inside `exec()`; `engine` outlives it.
    plan.after(1, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    // Backstop: reads reached steps, quits.
    plan.after(11, move || unsafe {
        let value = (*engine_ptr).invoke_method("report".into(), &[]);
        *report_ptr = QString::from_qvariant(value)
            .map(|text| text.to_string())
            .unwrap_or_default();
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    assert_changes(&common::calls(&journal), &report);
}

/// One core call per change; model reads state back.
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
    let first_description = parts.next().unwrap_or_default();
    let can_edit_description = parts.next().unwrap_or_default();
    let kept_after_add = parts.next().unwrap_or_default();
    let kept_after_remove = parts.next().unwrap_or_default();

    assert_eq!(step, "11", "the scenario did not run to the end. {context}");

    // Members from id list, own contact marked, in core's order.
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

    // Reloads change rows in place; reset drops first-load marks.
    assert_eq!(
        kept_after_add, "2",
        "a member joining rebuilt the rows of the members already there. {context}"
    );
    assert_eq!(
        kept_after_remove, "1",
        "a member leaving rebuilt the rows of the members who stayed. {context}"
    );

    // Rename: empty refused, same skipped, real sent once.
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

    // Picture: picker URL to core path; null clears.
    let pictures: Vec<Value> = calls
        .iter()
        .filter(|(name, _)| name == "set_chat_profile_image")
        .map(|(_, params)| params.pointer("/2").cloned().unwrap_or_default())
        .collect();
    assert_eq!(
        pictures,
        vec![serde_json::json!("/tmp/piirit fake/pic.png"), Value::Null],
        "the picture was not set from the decoded path and then cleared. {context}"
    );
    assert_eq!(
        avatar, "",
        "the cleared picture is still showing. {context}"
    );

    // Disappearing: seconds to core; timer read back.
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

    assert!(
        names.contains(&"get_chat_description"),
        "the description was never read from the core. {context}"
    );
    assert_eq!(
        first_description, "Walks\nBring boots",
        "the model did not load the description. {context}"
    );
    // Setting: trimmed, multi-line kept, no-op if same, empty clears, refusal shown.
    let described: Vec<&Value> = calls
        .iter()
        .filter(|(name, _)| name == "set_chat_description")
        .map(|(_, params)| params)
        .collect();
    assert_eq!(
        described,
        vec![
            &serde_json::json!([1, 2, "Bring water\nand boots"]),
            &serde_json::json!([1, 2, ""]),
            &serde_json::json!([1, 2, "this will fail"]),
        ],
        "the description calls are not the trimmed change, the clear and the refused one, \
         with nothing sent for the unchanged text. {context}"
    );
    assert!(
        log.contains("Cannot set chat description"),
        "the core's refusal did not reach the error signal. {context}"
    );
    assert_eq!(
        can_edit_description, "false",
        "a group that has been left still offers a description edit. {context}"
    );

    // Leaving: own call; core then refuses edits.
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
    // Grace stayed; Ada removed, account itself left.
    assert_eq!(
        members, "11,",
        "the members did not end up as the calls should have left them. {context}"
    );
    assert_eq!(
        log.matches("saved;").count(),
        7,
        "adding, removing, setting and clearing the picture, the timer, and \
         setting and clearing the description should each have been confirmed once. {context}"
    );
}
