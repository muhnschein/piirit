//! An impatient thumb must not send the message twice.
//!
//! The compose state is cleared when the core answers, not when the button
//! is tapped, so that a send which fails leaves the reader holding what
//! they chose. That leaves a window in which the field still holds the text
//! and the bar still holds the file -- seconds wide for a large video the
//! core has to copy into its blob directory -- and a second tap in that
//! window must not send the whole thing again.
//!
//! The fake server is told to take its time answering, which is what makes
//! that window reproducible rather than a race.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Postivene 1.0
    Item {
        property string trail: ''
        ChatMessages { id: chat; account_id: 1; chat_id: 1 }
        Connections {
            target: chat
            onSending_changed: trail = trail + (chat.sending ? 'busy ' : 'free ')
        }
        // Three taps in the time one send takes.
        function hammerText() {
            chat.send('hello')
            chat.send('hello')
            chat.send('hello')
            return '' + chat.sending
        }
        function hammerFile() {
            chat.send_file('', '/tmp/postivene-fake/holiday photo.png')
            chat.send_file('', '/tmp/postivene-fake/holiday photo.png')
            return '' + chat.sending
        }
        function busy() { return '' + chat.sending }
        function seen() { return trail }
    }
";

#[test]
fn a_second_tap_while_the_first_send_is_in_flight_sends_nothing() {
    let temp = std::env::temp_dir().join(format!("postivene-send-twice-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_FAKE_JOURNAL", &journal);
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
        // Long enough that every tap in one callback lands inside one
        // send, and short enough to be answered before the next read.
        std::env::set_var("POSTIVENE_FAKE_SEND_DELAY_MS", "1500");
    }

    postivene_shim::register_qml_types();

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
        ($name:expr) => {{
            let result = (*engine_ptr).invoke_method($name.into(), &[]);
            QString::from_qvariant(result)
                .map(|value| value.to_string())
                .unwrap_or_default()
        }};
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("text-taps", call!("hammerText")));
    });
    // Still inside the first send, which the server has not answered.
    single_shot(Duration::from_secs(4), move || unsafe {
        (*steps_ptr).push(("busy-during", call!("busy")));
    });
    single_shot(Duration::from_secs(6), move || unsafe {
        (*steps_ptr).push(("busy-after", call!("busy")));
        (*steps_ptr).push(("file-taps", call!("hammerFile")));
    });
    single_shot(Duration::from_secs(9), move || unsafe {
        (*steps_ptr).push(("trail", call!("seen")));
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

    let sends = common::calls(&journal)
        .into_iter()
        .filter(|(method, _)| method == "misc_send_msg")
        .count();
    assert_eq!(
        sends, 2,
        "five taps produced {sends} sends where they should have produced \
         two -- one per send that was actually free to start. {context}"
    );

    assert_eq!(
        value("text-taps"),
        "true",
        "the model does not report itself busy while a send is outstanding, \
         so nothing can disable the button. {context}"
    );
    assert_eq!(
        value("busy-during"),
        "true",
        "the model stopped reporting itself busy before the core answered. {context}"
    );
    assert_eq!(
        value("busy-after"),
        "false",
        "the model is still busy after the send was answered, so the button \
         never comes back. {context}"
    );
    // Twice each way, and never twice in a row: a stuck flag and a flag
    // that never rises both fail this.
    assert_eq!(
        value("trail"),
        "busy free busy free ",
        "the busy flag did not go up and come down once per send. {context}"
    );
}
