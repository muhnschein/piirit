//! Incoming call: ringing, and each way it ends.
//!
//! `IncomingCall`, `CallEnded`, `IncomingCallAccepted` etc.: core's only `snake_case` events; a
//! reader asking for `msgId` gets nothing silently. Fed in core's shape: from fake where it can,
//! by hand otherwise.
//! Answer played page-side over real socket. Call taken on another device first: not ended here.

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
use serde_json::json;

mod common;

use common::http::{api_of, authority_of, payload_of, post, status_of};

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        property string log: ''
        // The window's call, which hears the core.
        Call {
            id: call
            onRinging: log += 'ringing;'
            onEnded: log += 'ended:' + reason + ';'
            onError: log += 'error:' + message + ';'
        }
        // One that hears nothing, for taking a call up from its row.
        Call {
            id: late
            onRinging: log += 'late-ringing;'
        }
        Connections {
            target: core
            onCore_event: call.handle_event(context_id, kind, payload_json)
        }
        function state() {
            return call.state + '|' + call.end_reason + '|' + call.incoming
                   + '|' + call.chat_id + '|' + call.message_id
        }
        function url() { return call.url }
        function answer() { call.answer(); return call.state }
        function hangUp() { call.hang_up(); return call.state }
        function reset() { call.reset(); return call.state }
        // The core's own shapes, snake_case and all.
        function ring(messageId) {
            call.handle_event(1, 'IncomingCall', JSON.stringify({
                kind: 'IncomingCall', msg_id: messageId, chat_id: 1,
                place_call_info: 'v=0 other-offer', has_video: false
            }))
            return call.state + '|' + call.message_id
        }
        function endedThere(messageId) {
            call.handle_event(1, 'CallEnded', JSON.stringify({
                kind: 'CallEnded', msg_id: messageId, chat_id: 1
            }))
            return call.state
        }
        function takenElsewhere(messageId) {
            call.handle_event(1, 'IncomingCallAccepted', JSON.stringify({
                kind: 'IncomingCallAccepted', msg_id: messageId, chat_id: 1,
                from_this_device: false
            }))
            return call.state + '|' + call.end_reason
        }
        // Another profile's event for the same number is not this call's.
        function otherProfile(messageId) {
            call.handle_event(2, 'CallEnded', JSON.stringify({
                kind: 'CallEnded', msg_id: messageId, chat_id: 1
            }))
            return call.state
        }
        function pickUp(messageId) { late.pick_up(1, 1, messageId); return late.state }
        function lateState() { return late.state + '|' + late.incoming + '|' + late.message_id }
        function said() { return log }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_call_rings_is_answered_and_ends_every_way_a_ringing_call_can() {
    let temp = std::env::temp_dir().join(format!("piirit-call-in-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Call rings from first chat once probe up.
        std::env::set_var("PIIRIT_FAKE_INCOMING_CALL_MS", "1500");
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

    // SAFETY for every block: fire only inside `exec()`; pointees outlive it.
    single_shot(Duration::from_secs(3), move || unsafe {
        record!("ringing", call!("state"));
        // Second call while one rings: rings nowhere.
        record!("second", call!("ring", 9000));
        // Taken up by object that heard nothing: core still rings it.
        let first: u32 = call!("state")
            .split('|')
            .nth(4)
            .and_then(|id| id.parse().ok())
            .unwrap_or_default();
        record!("pick-up", call!("pickUp", first));
        record!("answer", call!("answer"));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("picked-up", call!("lateState"));
        let url = call!("url");
        record!("url", url.clone());
        let authority = authority_of(&url);
        let hash = url.split_once('#').map_or("", |(_, hash)| hash).to_string();
        record!("offer", payload_of(&hash, "acceptCall"));
        let api = api_of(&url);
        record!(
            "accept",
            status_of(&post(
                &authority,
                &format!("{api}/accept"),
                "v=0 test-answer"
            ))
            .to_string()
        );
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!("accepted", call!("state"));
        let first: u32 = call!("state")
            .split('|')
            .nth(4)
            .and_then(|id| id.parse().ok())
            .unwrap_or_default();
        record!("other-profile", call!("otherProfile", first));
        // Other end hangs up.
        record!("ended-there", call!("endedThere", first));
        record!("after-first", call!("state"));
        record!("reset-first", call!("reset"));

        // Another device answers.
        record!("ring-2", call!("ring", 9002));
        record!("elsewhere", call!("takenElsewhere", 9002));
        call!("reset");

        // Declined here.
        record!("ring-3", call!("ring", 9003));
        record!("decline", call!("hangUp"));
        call!("reset");

        // Caller gives up, or nobody answers in time.
        record!("ring-4", call!("ring", 9004));
        record!("gone", call!("endedThere", 9004));
    });

    // Reason for rang-out call waits on core.
    single_shot(Duration::from_secs(7), move || unsafe {
        record!("missed", call!("state"));
        record!("said", call!("said"));
        call!("reset");

        // Taken elsewhere while page comes up: core drops this accept.
        record!("ring-5", call!("ring", 9005));
        record!("answer-5", call!("answer"));
    });

    single_shot(Duration::from_secs(8), move || unsafe {
        record!("elsewhere-5", call!("takenElsewhere", 9005));
        call!("reset");

        // Same before page up.
        call!("ring", 9006);
        call!("answer");
        record!("elsewhere-6", call!("takenElsewhere", 9006));
    });

    // Long enough for host still coming up to come and go.
    single_shot(Duration::from_secs(9), move || unsafe {
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

    let ringing = value("ringing");
    assert!(
        ringing.starts_with("ringing||true|1|") && !ringing.ends_with("|0"),
        "the core's IncomingCall did not make a ringing call, with its chat \
         and message, read from the event's snake_case: {ringing}. {context}"
    );
    let first = ringing.rsplit('|').next().unwrap_or_default().to_string();
    assert_eq!(
        value("second"),
        format!("ringing|{first}"),
        "a second call rang over the first. {context}"
    );
    assert_eq!(
        value("answer"),
        "starting",
        "answering did not start the call's page. {context}"
    );
    assert_eq!(
        value("picked-up"),
        format!("ringing|true|{first}"),
        "a call still ringing could not be taken up from its row. {context}"
    );

    let url = value("url");
    assert!(
        url.starts_with("http://127.0.0.1:")
            && url.contains("/index.html?noOutgoingVideoInitially&key=")
            && url.contains("#acceptCall="),
        "the page was not pointed at the call it is to answer, camera off: \
         {url}. {context}"
    );
    assert_eq!(
        value("offer"),
        "v=0 fake-offer",
        "the page was not handed the caller's offer as the core has it. \
         {context}"
    );
    assert!(
        value("accept").starts_with("HTTP/1.1 204"),
        "the page's answer was not taken: {}. {context}",
        value("accept")
    );
    assert!(
        value("accepted").starts_with("connecting||true|1|"),
        "the call did not move on once answered: {}. {context}",
        value("accepted")
    );
    assert_eq!(
        value("other-profile"),
        "connecting",
        "another profile's event ended this profile's call. {context}"
    );
    assert_eq!(
        value("after-first"),
        format!("ended|remote|true|1|{first}"),
        "the other end hanging up did not end the call as theirs. {context}"
    );
    assert_eq!(value("reset-first"), "", "{context}");

    assert_eq!(value("ring-2"), "ringing|9002", "{context}");
    assert_eq!(
        value("elsewhere"),
        "ended|answered-elsewhere",
        "a call answered on another device kept ringing here. {context}"
    );
    assert_eq!(value("ring-3"), "ringing|9003", "{context}");
    assert_eq!(
        value("decline"),
        "ended",
        "declining did not end the call. {context}"
    );
    assert_eq!(value("ring-4"), "ringing|9004", "{context}");
    assert_eq!(
        value("gone"),
        "ended",
        "a call that stopped ringing kept ringing here. {context}"
    );
    assert!(
        value("missed").starts_with("ended|missed|"),
        "a call that rang out unanswered is not a missed call: {}. {context}",
        value("missed")
    );
    assert_eq!(
        value("said"),
        "ringing;ended:remote;ringing;ended:answered-elsewhere;ringing;\
         ended:declined;ringing;ended:missed;",
        "the call did not say, once each, that it rang and why it ended; \
         and one taken up from its row must not ring. {context}"
    );

    assert_eq!(value("ring-5"), "ringing|9005", "{context}");
    assert_eq!(value("answer-5"), "starting", "{context}");
    assert_eq!(
        value("elsewhere-5"),
        "ended|answered-elsewhere",
        "a call answered here and then on another device went on here, \
         on a call this device will never join. {context}"
    );
    assert_eq!(
        value("elsewhere-6"),
        "ended|answered-elsewhere",
        "a call taken on another device before this one's page was up \
         went on here. {context}"
    );

    let calls = common::calls(&journal);
    let named = |method: &str| -> Vec<serde_json::Value> {
        calls
            .iter()
            .filter(|(name, _)| name == method)
            .map(|(_, params)| params.clone())
            .collect()
    };
    let first: u32 = first.parse().unwrap_or_default();
    assert_eq!(
        named("accept_incoming_call"),
        vec![json!([1, first, "v=0 test-answer"])],
        "the core was not handed the page's answer to the call. {context}"
    );
    // Only declining is told to core from here. Hangup and other-device answer are core's news.
    assert_eq!(
        named("end_call"),
        vec![json!([1, 9003])],
        "the core was told to end calls it had ended itself, or not told \
         of the one declined here. {context}"
    );
}
