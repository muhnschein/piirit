//! The deletion period reaches every account the core has, and the core
//! says first how many messages it would take.
//!
//! It is one setting for the whole app, held in dconf, and the core's
//! `delete_device_after` is per account: so the core object writes it to
//! each account when it is set, and again whenever the account list is
//! read -- which is how a profile added later gets it too, the way the
//! download limit does. Before the settings page writes it, it asks the
//! core how many messages are already older than the period, across
//! every account, so the reader agrees to a number rather than an idea.

// Qt harness: see download_limit.rs.
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
    Item {
        property string estimated: ''
        // Handed over before the core is up, as the app's root does.
        Component.onCompleted: core.delete_device_after = 0
        Timer {
            id: poll
            interval: 200; running: true; repeat: true
            onTriggered: {
                if (core.status === 'ready') {
                    poll.running = false
                    core.add_account()
                }
            }
        }
        Connections {
            target: core
            onAccount_added: core.refresh_accounts()
            onAuto_deletion_estimated: estimated += seconds + ':' + count + ';'
        }
        function change(seconds) { core.delete_device_after = seconds; return 'ok' }
        function estimate(seconds) { core.estimate_auto_deletion(seconds); return 'ok' }
        function heard() { return estimated }
    }
";

#[test]
fn the_period_is_written_to_each_account_and_the_core_counts_first() {
    let temp = std::env::temp_dir().join(format!("piirit-auto-delete-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
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
    let mut heard = String::new();
    let heard_ptr: *mut String = std::ptr::addr_of_mut!(heard);

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

    single_shot(Duration::from_secs(4), move || unsafe {
        call!("estimate", 3600u32);
    });
    single_shot(Duration::from_secs(6), move || unsafe {
        call!("change", 3600u32);
    });
    single_shot(Duration::from_secs(8), move || unsafe {
        *heard_ptr = call!("heard");
        (*engine_ptr).quit();
    });

    engine.exec();

    let calls = common::calls(&journal);
    let periods: Vec<(u64, String)> = calls
        .iter()
        .filter(|(method, params)| {
            method == "set_config"
                && params.pointer("/1").and_then(Value::as_str) == Some("delete_device_after")
        })
        .filter_map(|(_, params)| {
            Some((
                params.pointer("/0").and_then(Value::as_u64)?,
                params.pointer("/2").and_then(Value::as_str)?.to_string(),
            ))
        })
        .collect();
    assert_eq!(
        periods,
        vec![(1, "0".to_string()), (1, "3600".to_string())],
        "expected the period written to account 1 when the list was read, and \
         again when it changed: {calls:?}"
    );
    // estimate_auto_deletion_count params: account, from_server, seconds.
    // From the device, since that is whose setting this is.
    let estimates: Vec<Value> = calls
        .iter()
        .filter(|(method, _)| method == "estimate_auto_deletion_count")
        .map(|(_, params)| params.clone())
        .collect();
    assert_eq!(
        estimates,
        vec![serde_json::json!([1, false, 3600])],
        "the count was not asked for in the shape the core takes: {calls:?}"
    );
    // The fake counts every seeded message as older than an hour.
    assert_eq!(
        heard, "3600:4;",
        "the count did not come back with the period it was asked for: {calls:?}"
    );
}
