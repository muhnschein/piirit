//! Blocking a contact; core's blocked-contacts list.
//!
//! Core hides blocked contact from `get_contacts`, returns it via
//! `get_blocked_contacts` only. Block = row moves between lists.
//! Checks both calls and other list hearing via core event.

// Qt harness allows: unsafe `env::set_var` pre-Qt, borrow_as_ptr,
// single_shot with whole-second Durations.
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

/// Writable contacts and blocked lists.
/// Only first reloaded by hand; second hears via core event.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        property string lastError: ''
        property string applied: ''
        property bool started: false
        property int victim: 0

        ContactList {
            id: everyone
            account_id: 1
            onError: lastError = message
            onBlocking_applied: applied = applied + contact_id + ':' + blocked + ','
            // Off the data rather than off a clock: under load the core
            // takes longer to come up, and a fixed tick then reads an
            // empty list.
            onRows_changed: {
                if (!started && everyone.count > 1) {
                    started = true
                    victim = ordinary.itemAt(0).cid
                    everyone.block(victim)
                }
            }
        }

        ContactList {
            id: blockList
            account_id: 1
            blocked: true
            onError: lastError = message
            onBlocking_applied: applied = applied + contact_id + ':' + blocked + ','
        }

        Connections {
            target: core
            // What an open page does with a core event, and the only way
            // the blocked list hears of a block made anywhere else.
            onCore_event: {
                everyone.handle_event(context_id, kind, payload_json)
                blockList.handle_event(context_id, kind, payload_json)
            }
            onStatus_changed: {
                if (core.status === 'ready') {
                    everyone.reload()
                    blockList.reload()
                }
            }
        }

        Repeater { id: ordinary; model: everyone.rows; Item { property int cid: model.contact_id } }
        Repeater { id: barred; model: blockList.rows; Item { property int cid: model.contact_id } }

        function ids(rows) {
            var out = ''
            for (var i = 0; i < rows.count; i++) { out += rows.itemAt(i).cid + ',' }
            return out
        }
        function ordinaryIds() { return ids(ordinary) }
        function blockedIds() { return ids(barred) }
        function letBackIn() { blockList.unblock(victim); return '' + victim }
        function report() { return applied + '#' + lastError }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn blocking_moves_a_contact_between_the_two_lists() {
    let temp = std::env::temp_dir().join(format!("piirit-blocking-{}", std::process::id()));
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

    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    // Block done by now.
    single_shot(Duration::from_secs(4), move || unsafe {
        record!("ordinary-after-block", call!("ordinaryIds"));
        record!("blocked-after-block", call!("blockedIds"));
        record!("unblock", call!("letBackIn"));
    });
    single_shot(Duration::from_secs(7), move || unsafe {
        record!("ordinary-after-unblock", call!("ordinaryIds"));
        record!("blocked-after-unblock", call!("blockedIds"));
        record!("report", call!("report"));
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
    let calls = common::calls(&journal);
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();
    let context = format!("steps: {steps:?}, calls: {names:?}");
    let params_of = |method: &str| -> Vec<serde_json::Value> {
        calls
            .iter()
            .filter(|(name, _)| name == method)
            .map(|(_, params)| params.clone())
            .collect()
    };

    // Core's two calls, account first, as deltachat-android sends.
    assert_eq!(
        params_of("block_contact"),
        vec![serde_json::json!([1, 10])],
        "blocking did not reach the core as block_contact(account, contact). {context}"
    );
    assert_eq!(
        params_of("unblock_contact"),
        vec![serde_json::json!([1, 10])],
        "unblocking did not reach the core as unblock_contact(account, contact). {context}"
    );
    // Blocked list: own method, account only.
    assert!(
        params_of("get_blocked_contacts").contains(&serde_json::json!([1])),
        "the blocked list was never read with get_blocked_contacts(account). {context}"
    );

    assert_eq!(
        (value("ordinary-after-block"), value("blocked-after-block")),
        ("11,".to_string(), "10,".to_string()),
        "blocking did not take the contact out of the ordinary list and put \
         them in the blocked one -- the second is reloaded by the core's \
         event alone. {context}"
    );
    assert_eq!(
        (
            value("ordinary-after-unblock"),
            value("blocked-after-unblock")
        ),
        ("10,11,".to_string(), String::new()),
        "unblocking did not put the contact back. {context}"
    );
    // Each way said once, on list call made on.
    assert_eq!(
        value("report"),
        "10:true,10:false,#",
        "the block and the unblock were not each announced once, or \
         something failed. {context}"
    );
}
