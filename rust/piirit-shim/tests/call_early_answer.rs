//! The other end answers before this end knows what it placed.
//!
//! The core sends the call and then says which message it placed it as;
//! the other end's answer is an event, and nothing orders the two. The
//! answer can come first -- while the call has no message to be matched
//! against -- and a call that dropped it would ring on, answered, until
//! the core gave up on it. So the answer is kept, and applied once the
//! call knows its message.
//!
//! Forced here in that order: the answer is fed to the call while the
//! page is still up and has sent no offer, and only then does the page
//! send one.

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

use common::http::{api_of, authority_of, body_of, get, payload_of, post, status_of};

/// The fake core numbers its messages from above what it seeds, and a
/// call is the first message this test adds.
const PLACED_AS: u32 = 101;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        property string log: ''
        Call {
            id: call
            onEnded: log += 'ended:' + reason + ';'
            onError: log += 'error:' + message + ';'
        }
        function place() { return call.place(1, 1, false) ? 'placed' : 'refused' }
        function state() { return call.state + '|' + call.message_id }
        function url() { return call.url }
        // The core's own shape, before the call has its message.
        function answeredEarly(messageId) {
            call.handle_event(1, 'OutgoingCallAccepted', JSON.stringify({
                kind: 'OutgoingCallAccepted', msg_id: messageId, chat_id: 1,
                accept_call_info: 'v=0 early-answer'
            }))
            return call.state + '|' + call.message_id
        }
        // Another call's end, early too: not this call's.
        function otherEnded() {
            call.handle_event(1, 'CallEnded', JSON.stringify({
                kind: 'CallEnded', msg_id: 4242, chat_id: 1
            }))
            return call.state
        }
        function hangUp() { call.hang_up(); return call.state }
        function said() { return log }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn an_answer_that_comes_before_the_call_is_placed_is_not_lost() {
    let temp = std::env::temp_dir().join(format!("piirit-call-early-{}", std::process::id()));
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
    let mut page_url = String::new();
    let url_ptr: *mut String = std::ptr::addr_of_mut!(page_url);

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
        record!("place", call!("place"));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        // The page is up and has not sent its offer: the call has no
        // message yet, and the answer arrives anyway.
        record!("before", call!("state"));
        record!("early", call!("answeredEarly", PLACED_AS));
        record!("other", call!("otherEnded"));
        let url = call!("url");
        *url_ptr = url.clone();
        let authority = authority_of(&url);
        let api = api_of(&url);
        // An offer the fake core's other end does not answer by itself:
        // the only answer there is is the early one.
        record!(
            "start",
            status_of(&post(&authority, &format!("{api}/start"), "v=0 test-offer")).to_string()
        );
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!("placed", call!("state"));
        let url = (*url_ptr).clone();
        let authority = authority_of(&url);
        let api = api_of(&url);
        let commands = get(&authority, &format!("{api}/commands"));
        let list: Vec<String> = serde_json::from_str(body_of(&commands)).unwrap_or_default();
        record!(
            "answer",
            list.first().map_or_else(
                || format!("no command: {commands}"),
                |command| payload_of(command, "onAnswer")
            )
        );
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

    assert_eq!(value("place"), "placed", "{context}");
    assert_eq!(
        value("before"),
        "starting|0",
        "the call was not still waiting on its page's offer. {context}"
    );
    assert_eq!(
        value("early"),
        "starting|0",
        "an answer to a call not yet placed was acted on at once. {context}"
    );
    assert_eq!(
        value("other"),
        "starting",
        "another call's end ended this one. {context}"
    );
    assert!(
        value("start").starts_with("HTTP/1.1 204"),
        "the page's offer was not taken: {}. {context}",
        value("start")
    );
    assert_eq!(
        value("placed"),
        format!("connecting|{PLACED_AS}"),
        "the answer that came before the call was placed was lost. {context}"
    );
    assert_eq!(
        value("answer"),
        "v=0 early-answer",
        "the early answer did not reach the page. {context}"
    );
    assert_eq!(value("hang-up"), "ended", "{context}");
    assert_eq!(
        value("said"),
        "ended:hung-up;",
        "the call said something other than that it was hung up. {context}"
    );
}
