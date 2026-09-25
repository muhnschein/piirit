//! Closing the app mid-call ends the call, and the other end hears it.
//!
//! The app stops the core as soon as its window has closed, and the call's
//! host -- whose going is what tells the core a call is over -- goes only
//! after that, with the window. So the core used to hear nothing: the
//! other end rang on until its own timeout, or sat in a silent call. Now
//! stopping the core ends every call still hosted first, and waits for the
//! core to have sent the message that says so.
//!
//! A call that only rings is left alone: closing the app is not a
//! decline, and another of the account's devices may yet answer it.

// Qt harness: see qml_pages.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::{Duration, Instant};

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::json;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        // The window's call, which hears the core and is answered.
        Call { id: call }
        // Another, left ringing.
        Call { id: ringing }
        Connections {
            target: core
            onCore_event: call.handle_event(context_id, kind, payload_json)
        }
        function state() { return call.state + '|' + call.message_id }
        function answer() { call.answer(); return call.state }
        function url() { return call.url }
        function ring() {
            ringing.handle_event(1, 'IncomingCall', JSON.stringify({
                kind: 'IncomingCall', msg_id: 9300, chat_id: 1,
                place_call_info: 'v=0 other-offer', has_video: false
            }))
            return ringing.state
        }
    }
";

#[test]
fn a_call_up_when_the_app_closes_is_ended_before_the_core_goes() {
    let temp = std::env::temp_dir().join(format!("piirit-call-exit-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_INCOMING_CALL_MS", "500");
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
        record!("ringing", call!("state"));
        record!("answer", call!("answer"));
        record!("other", call!("ring"));
    });

    // The page is up, and the reader closes the app.
    single_shot(Duration::from_secs(3), move || unsafe {
        record!("url", call!("url"));
        (*engine_ptr).quit();
    });

    engine.exec();

    // What the app does once its window has closed.
    let stopping = Instant::now();
    piirit_shim::shutdown();
    let stopped_in = stopping.elapsed();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}, stopped in {stopped_in:?}");

    let ringing = value("ringing");
    assert!(ringing.starts_with("ringing|"), "{context}");
    let message_id: u32 = ringing
        .rsplit('|')
        .next()
        .and_then(|id| id.parse().ok())
        .unwrap_or_default();
    assert_eq!(value("answer"), "starting", "{context}");
    assert_eq!(value("other"), "ringing", "{context}");
    assert!(
        value("url").starts_with("http://127.0.0.1:"),
        "the call's page never came up. {context}"
    );

    let ends: Vec<serde_json::Value> = common::calls(&journal)
        .into_iter()
        .filter(|(name, _)| name == "end_call")
        .map(|(_, params)| params)
        .collect();
    assert_eq!(
        ends,
        vec![json!([1, message_id])],
        "closing the app did not end the call it was in -- or declined \
         the one only ringing, which another device may yet answer. \
         {context}"
    );
    // The core said it had sent the message ending the call, and the app
    // stopped it then rather than at the end of its wait.
    assert!(
        stopped_in < Duration::from_secs(2),
        "the app did not stop the core once the call's end was sent. \
         {context}"
    );
}
