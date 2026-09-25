//! A call that rings here with nothing more from the core rings out.
//!
//! The core ends a ringing call after 120 seconds with one `CallEnded`,
//! from a timer that lives only in the server that heard the call. An
//! event channel that overflows, or a server that dies and is started
//! again, loses it -- and a call that waited for it would ring until
//! somebody declined it. So a call keeps a count of its own, a little
//! longer than the core's, and ends as one that rang out when that is up.
//!
//! Shortened here from minutes to a second and a half, and fed by hand:
//! the fake core never says anything about these calls at all, which is
//! the case being guarded against.

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

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        property string log: ''
        // Left ringing.
        Call {
            id: unanswered
            ring_limit_ms: 1500
            onEnded: log += 'unanswered:' + reason + ';'
        }
        // Answered before its count is up.
        Call {
            id: answered
            ring_limit_ms: 1500
            onEnded: log += 'answered:' + reason + ';'
        }
        // Ended by the core, and then another call rings: the first
        // call's count is not the second's.
        Call {
            id: again
            ring_limit_ms: 1500
            onEnded: log += 'again:' + reason + ';'
        }
        function ring(call, messageId) {
            call.handle_event(1, 'IncomingCall', JSON.stringify({
                kind: 'IncomingCall', msg_id: messageId, chat_id: 1,
                place_call_info: 'v=0 other-offer', has_video: false
            }))
        }
        function start() {
            ring(unanswered, 9100)
            ring(answered, 9101)
            answered.answer()
            ring(again, 9102)
            return unanswered.state + '|' + answered.state + '|' + again.state
        }
        function next() {
            again.handle_event(1, 'CallEnded', JSON.stringify({
                kind: 'CallEnded', msg_id: 9102, chat_id: 1
            }))
            again.reset()
            ring(again, 9103)
            return again.state + '|' + again.message_id
        }
        function state(which) {
            var call = which === 'unanswered' ? unanswered
                     : which === 'answered' ? answered : again
            return call.state + '|' + call.end_reason + '|' + call.message_id
        }
        function hangUp() { answered.hang_up(); return answered.state }
        function said() { return log }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_call_the_core_never_ends_stops_ringing_by_itself() {
    let temp = std::env::temp_dir().join(format!("piirit-call-out-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts.
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

    // SAFETY for every block: these fire only while `exec()` runs on this
    // thread, and everything they point at outlives it.
    single_shot(Duration::from_secs(2), move || unsafe {
        record!("start", call!("start"));
    });

    // Past the first calls' count, which was due at 3.5 s; the call that
    // rings now is due at 4.5 s.
    single_shot(Duration::from_secs(3), move || unsafe {
        record!("next", call!("next"));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("unanswered", call!("state", QString::from("unanswered")));
        record!("answered", call!("state", QString::from("answered")));
        record!("again-ringing", call!("state", QString::from("again")));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("again-out", call!("state", QString::from("again")));
        record!("hang-up", call!("hangUp"));
        record!("said", call!("said"));
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

    assert_eq!(value("start"), "ringing|starting|ringing", "{context}");
    assert_eq!(value("next"), "ringing|9103", "{context}");
    assert_eq!(
        value("unanswered"),
        "ended|missed|9100",
        "a call the core never ended rang on past its count. {context}"
    );
    assert_eq!(
        value("answered"),
        "starting||9101",
        "a call answered before its count was up was ended as rung out. \
         {context}"
    );
    assert_eq!(
        value("again-ringing"),
        "ringing||9103",
        "an earlier call's count ended the call ringing after it. {context}"
    );
    assert_eq!(
        value("again-out"),
        "ended|missed|9103",
        "the second call did not ring out on a count of its own. {context}"
    );
    assert_eq!(value("hang-up"), "ended", "{context}");
    let said = value("said");
    assert_eq!(
        said.matches("unanswered:missed;").count(),
        1,
        "the call that rang out did not say so once: {said}. {context}"
    );
    assert!(
        said.contains("again:missed;") && said.contains("answered:hung-up;"),
        "{said}. {context}"
    );

    // Nothing is said to the core about a call that rang out: it counts
    // it as over itself.
    let calls = common::calls(&journal);
    assert!(
        !calls
            .iter()
            .any(|(name, params)| name == "end_call" && params[1] != 9101),
        "the core was told to end a call that rang out. {context}"
    );
}
