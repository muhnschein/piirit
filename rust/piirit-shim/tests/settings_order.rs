//! Two settings choices back to back: core keeps later one.
//!
//! Each choice reads account list, then writes. First one's list read or write held back: later
//! choice must still write last, else core keeps older one (seen as `qml_auto_delete_flow`
//! flake). Restarted core: writes go to new one, not still queued for dead one.
//!
//! List reads look alike: unordered, held one is either choice's. Three rounds, so unordered
//! reads slip through 1 in 8; write held by value, always first one's.

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Component.onCompleted: core.download_limit = 1048576
        Timer {
            id: poll
            interval: 200; running: true; repeat: true
            onTriggered: {
                if (core.status === 'ready') {
                    poll.running = false
                    core.refresh_accounts()
                }
            }
        }
        // Core seen going away, then back.
        property bool wentAway: false
        Connections {
            target: core
            onStatus_changed: if (core.status === 'reconnecting') wentAway = true
        }
        function change(bytes) { core.download_limit = bytes; return 'ok' }
        function back() { return wentAway && core.status === 'ready' ? 'yes' : 'no' }
    }
";

/// Download limits written, in order server got them.
fn limits(journal: &std::path::Path) -> Vec<String> {
    common::calls(journal)
        .iter()
        .filter(|(method, params)| {
            method == "set_config"
                && params.pointer("/1").and_then(Value::as_str) == Some("download_limit")
        })
        .filter_map(|(_, params)| Some(params.pointer("/2").and_then(Value::as_str)?.to_string()))
        .collect()
}

#[test]
fn a_later_choice_is_written_last_even_when_the_first_one_is_answered_late() {
    let temp = std::env::temp_dir().join(format!("piirit-settings-order-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let slow = temp.join("slow-once");
    let exit = temp.join("exit-once");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1");
        std::env::set_var("PIIRIT_FAKE_SLOW_ONCE", &slow);
        std::env::set_var("PIIRIT_FAKE_EXIT_ONCE", &exit);
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

    let mut plan = common::Plan::default();

    let back = move || unsafe {
        let result = (*engine_ptr).invoke_method("back".into(), &[]);
        QString::from_qvariant(result)
            .map(|value| value.to_string())
            .unwrap_or_default()
    };
    let change = move |bytes: i32| unsafe {
        (*engine_ptr).invoke_method("change".into(), &[QVariant::from(bytes)]);
    };

    // First limit landed on account: nothing else in flight.
    let landed = journal.clone();
    plan.until(1, 30, move |_| !limits(&landed).is_empty());
    // Two at once, first one's request held back: account list read, then write.
    let rounds = [
        ("\"get_all_accounts\"", 0, 262_144),
        ("\"get_all_accounts\"", 65_536, 131_072),
        ("\"get_all_accounts\"", 16_384, 32_768),
        ("\"524288\"", 524_288, 1024),
    ];
    for (count, (held, first, second)) in (1..).step_by(2).skip(1).zip(rounds) {
        let slow = slow.clone();
        plan.after(1, move || {
            std::fs::write(&slow, held).expect("drop slow-once file");
            change(first);
            change(second);
        });
        let written = journal.clone();
        plan.until(1, 30, move |_| limits(&written).len() >= count);
    }
    // Server dies on this choice's list read; next choice goes to its replacement.
    plan.after(1, move || {
        std::fs::write(&exit, "").expect("drop exit-once file");
        change(2048);
    });
    plan.until(1, 30, move |_| back() == "yes");
    plan.after(1, move || change(4096));
    let written = journal.clone();
    plan.until(1, 30, move |_| {
        limits(&written).last().map(String::as_str) == Some("4096")
    });
    plan.after(0, move || unsafe {
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let limits = limits(&journal);
    let in_order = [
        "1048576", "0", "262144", "65536", "131072", "16384", "32768", "524288", "1024",
    ]
    .map(String::from);
    assert_eq!(
        limits.get(..in_order.len()),
        Some(&in_order[..]),
        "expected each second choice written after the first, so the core keeps it, \
         even with the first one's account list or write answered late: {:?}",
        common::calls(&journal)
    );
    assert_eq!(
        limits.last().map(String::as_str),
        Some("4096"),
        "expected a choice made after the core restarted to reach the new core: {:?}",
        common::calls(&journal)
    );
}
