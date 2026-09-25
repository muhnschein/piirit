//! An offer stopped and made again at once is the second offer's own.
//!
//! A stopped provider answers `Ok`, and that answer is held for a second
//! (`second_device.rs`'s `SETTLE`) in case the progress that says how it
//! ended is still on its way. The reader can ask for the code again in
//! that second. The held answer then ended the new offer on the page --
//! "No device copied the profile", Cancel gone -- while the new provider
//! went on running in the core, with the profile's IO paused and a live
//! code on the network, and nothing left that could stop it: `cancel`
//! saw an offer that was not running.

// Qt harness: see qml_pages.rs.
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
        property string log: ''
        SecondDevice {
            id: device
            account_id: 4
            onTaken: log += 'taken;'
            onStalled: log += 'stalled;'
            onError: log += 'error:' + message + ';'
        }
        Connections {
            target: core
            onCore_event: device.handle_event(context_id, kind, payload_json)
        }
        // Asked for again a moment after Cancel: after the stopped
        // provider has answered, and well inside the second its answer
        // is held for.
        Timer {
            id: again
            interval: 300
            onTriggered: device.offer()
        }
        function offer() { device.offer(); return state() }
        function cancelAndAgain() { device.cancel(); again.start(); return state() }
        function cancel() { device.cancel(); return state() }
        function state() { return device.running + '|' + device.code }
        function said() { return log }
    }
";

#[test]
fn an_offer_made_again_straight_after_cancel_is_not_ended_by_the_first() {
    let temp = std::env::temp_dir().join(format!("piirit-reoffer-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "4");
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
        ($name:expr) => {{
            let result = (*engine_ptr).invoke_method($name.into(), &[]);
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

    // SAFETY for every block: these fire only while `exec()` runs on this
    // thread, and everything they point at outlives it.
    single_shot(Duration::from_secs(2), move || unsafe {
        record!("offer", call!("offer"));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("first", call!("state"));
        record!("cancel", call!("cancelAndAgain"));
    });

    // Past the second the stopped provider's answer was held for.
    single_shot(Duration::from_secs(5), move || unsafe {
        record!("second", call!("state"));
        record!("said", call!("said"));
        // And the second offer can still be stopped.
        record!("cancel-second", call!("cancel"));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
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

    assert_eq!(value("offer"), "true|", "{context}");
    assert!(
        value("first").starts_with("true|DCBACKUP"),
        "the first offer put up no code. {context}"
    );
    assert_eq!(value("cancel"), "false|", "{context}");
    assert!(
        value("second").starts_with("true|DCBACKUP"),
        "the first offer's late answer ended the second one on the page. \
         {context}"
    );
    assert_eq!(
        value("said"),
        "",
        "the reader was told nobody copied a profile they had just asked \
         to offer again. {context}"
    );
    assert_eq!(value("cancel-second"), "false|", "{context}");

    let stop_requests = common::calls(&journal)
        .into_iter()
        .filter(|(name, _)| name == "stop_ongoing_process")
        .map(|(_, params)| params)
        .collect::<Vec<Value>>();
    assert_eq!(
        stop_requests.len(),
        2,
        "the second provider could not be stopped: it was running in the \
         core with nothing on the page to stop it. {context}"
    );
}
