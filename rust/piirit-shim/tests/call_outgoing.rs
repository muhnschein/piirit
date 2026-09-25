//! Placing a call: the page it runs in, what the page is told, and what
//! the core is asked -- from the button to the hang-up.
//!
//! The page is upstream's calls-webapp, and the media is the browser
//! engine's, so neither runs here. What does run is everything between
//! them and the core: the `Call` object a window holds, the loopback host
//! it serves the page from, and the bridge the page talks to that host
//! through. The page's side of the conversation is played over a real
//! socket, request for request, the way `calls.js` makes it: the offer it
//! has gathered, the answer it waits for, the connection it reports.
//!
//! The call's row is watched too, since the row is what is left of a
//! call afterwards: a call placed here is a message in its chat, and its
//! state -- ringing, then over -- is read off the core rather than off
//! the English sentence the core writes into it.

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

use common::http::{api_of, authority_of, body_of, get, get_as, payload_of, post, status_of};

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        property string log: ''
        Call {
            id: call
            onRinging: log += 'ringing;'
            onEnded: log += 'ended:' + reason + ';'
            onError: log += 'error:' + message + ';'
        }
        ChatMessages { id: messages; account_id: 1; chat_id: 1 }
        Connections {
            target: core
            onCore_event: {
                call.handle_event(context_id, kind, payload_json)
                messages.handle_event(context_id, kind, payload_json)
            }
        }
        // The rows, read the way a view reads them.
        Repeater {
            id: rows
            model: messages.rows
            delegate: Item {
                property string viewType: model.view_type
                property string callState: model.call_state
                property int messageId: model.message_id
            }
        }
        function place() { return call.place(1, 1, false) ? 'placed' : 'refused' }
        function again() { return call.place(1, 1, false) ? 'placed' : 'refused' }
        function state() {
            return call.state + '|' + call.end_reason + '|' + call.message_id
                   + '|' + (call.connected_at > 0 ? 'clock' : 'no-clock')
        }
        function url() { return call.url }
        function mute(on) { call.mute(on); return '' + call.muted }
        function hangUp() { call.hang_up(); return call.state + '|' + call.url }
        function reset() { call.reset(); return call.state }
        function lastRow() {
            var row = rows.itemAt(rows.count - 1)
            return row ? row.viewType + '|' + row.callState + '|' + row.messageId : 'none'
        }
        function said() { return log }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_call_is_placed_answered_connected_and_hung_up() {
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

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);
    let mut served = String::new();
    let served_ptr: *mut String = std::ptr::addr_of_mut!(served);
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
    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        // No call, no microphone to switch.
        record!("mute-idle", call!("mute", true));
        record!("place", call!("place"));
        record!("placing", call!("state"));
        // Switched off before the page is up to be told.
        record!("mute-early", call!("mute", true));
        // One call at a time.
        record!("again", call!("again"));
    });

    // The page is up: what it is served, and what it may reach.
    single_shot(Duration::from_secs(3), move || unsafe {
        let url = call!("url");
        record!("url", url.clone());
        *url_ptr = url.clone();
        let authority = authority_of(&url);
        *served_ptr = authority.clone();
        let page = get(&authority, "/index.html");
        record!("page-status", status_of(&page).to_string());
        record!(
            "page-is-upstream",
            body_of(&page).contains("version: v0.12.1").to_string()
        );
        record!(
            "page-policy",
            page.lines()
                .find(|line| line.starts_with("Content-Security-Policy:"))
                .unwrap_or_default()
                .to_string()
        );
        let bridge = get(&authority, "/calls.js");
        record!("bridge-status", status_of(&bridge).to_string());
        let api = api_of(&url);
        record!("api", api.clone());
        // The key is in the page's own address and nowhere the host
        // serves: a page elsewhere that loads the bridge learns nothing.
        let key = api.trim_start_matches("/call-api/").to_string();
        record!(
            "bridge-secret",
            (!key.is_empty() && body_of(&bridge).contains(&key)).to_string()
        );
        record!(
            "bridge-ice",
            body_of(&get(&authority, &format!("{api}/ice")))
                .contains("turn:127.0.0.1:3478")
                .to_string()
        );
        record!(
            "webxdc-js",
            status_of(&get(&authority, "/webxdc.js")).to_string()
        );
        // A name that resolves to the same address is not the address.
        record!(
            "wrong-host",
            status_of(&get_as(&authority, "localhost", "/index.html")).to_string()
        );
        // Guessing is told nothing.
        record!(
            "wrong-token",
            status_of(&post(&authority, "/call-api/0000/start", "v=0")).to_string()
        );
        // The page has its offer: the core places the call. The fake
        // core's other end answers an offer that asks it to at once.
        record!(
            "start",
            status_of(&post(
                &authority,
                &format!("{api}/start"),
                "v=0 test-offer answer-me"
            ))
            .to_string()
        );
        // And a second offer from the same page is not a second call.
        record!(
            "start-again",
            status_of(&post(&authority, &format!("{api}/start"), "v=0 other")).to_string()
        );
    });

    // The other end's answer, delivered to the page as its hash command.
    single_shot(Duration::from_secs(5), move || unsafe {
        record!("answered", call!("state"));
        record!("row-ringing", call!("lastRow"));
        let authority = (*served_ptr).clone();
        let api = api_of(&(*url_ptr));
        let commands = get(&authority, &format!("{api}/commands"));
        let list: Vec<String> = serde_json::from_str(body_of(&commands)).unwrap_or_default();
        record!("page-muted", list.first().cloned().unwrap_or_default());
        record!(
            "answer",
            list.iter()
                .find(|command| command.starts_with("onAnswer="))
                .map_or_else(
                    || format!("no command: {commands}"),
                    |command| payload_of(command, "onAnswer")
                )
        );
        // And on again, told at once now that the page is up.
        record!("unmute", call!("mute", false));
        record!(
            "page-unmuted",
            body_of(&get(&authority, &format!("{api}/commands"))).to_string()
        );
        // The peer connection says it is through.
        record!(
            "connected",
            status_of(&post(&authority, &format!("{api}/state"), "connected")).to_string()
        );
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("in-call", call!("state"));
        record!("hang-up", call!("hangUp"));
    });

    single_shot(Duration::from_secs(8), move || unsafe {
        record!("after", call!("state"));
        let authority = (*served_ptr).clone();
        record!(
            "served-after",
            match std::net::TcpStream::connect(&authority) {
                Ok(_) => "yes".to_string(),
                Err(_) => "no".to_string(),
            }
        );
        record!("row-over", call!("lastRow"));
        record!("said", call!("said"));
        record!("reset", call!("reset"));
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

    assert_eq!(
        value("mute-idle"),
        "false",
        "a microphone was switched off with no call to switch it in. {context}"
    );
    assert_eq!(
        value("place"),
        "placed",
        "the call was not placed. {context}"
    );
    assert_eq!(value("mute-early"), "true", "{context}");
    assert!(
        value("placing").starts_with("starting|"),
        "a call being placed does not say so: {}. {context}",
        value("placing")
    );
    assert_eq!(
        value("again"),
        "refused",
        "a second call was placed over the first. {context}"
    );

    let url = value("url");
    assert!(
        url.starts_with("http://127.0.0.1:")
            && url.contains("/index.html?disableVideoCompletely&key=")
            && url.ends_with("#startCall"),
        "the page was not pointed at an audio-only call it is to start, with \
         its key: {url}. {context}"
    );
    assert!(
        value("page-status").starts_with("HTTP/1.1 200"),
        "{context}"
    );
    assert_eq!(
        value("page-is-upstream"),
        "true",
        "the page served is not the vendored calls-webapp. {context}"
    );
    let policy = value("page-policy");
    assert!(
        policy.contains("connect-src 'self'") && policy.contains("default-src 'self'"),
        "the page is not kept to its own origin for what a policy governs: {policy}. {context}"
    );
    assert!(
        value("bridge-status").starts_with("HTTP/1.1 200"),
        "{context}"
    );
    assert_eq!(
        value("bridge-secret"),
        "false",
        "the bridge the host serves carries the key to the core. {context}"
    );
    assert_eq!(
        value("bridge-ice"),
        "true",
        "the page cannot have the core's ICE servers. {context}"
    );
    assert!(
        value("api").starts_with("/call-api/") && value("api").len() > "/call-api/".len() + 16,
        "the bridge was not given an unguessable API path: {}. {context}",
        value("api")
    );
    assert!(value("webxdc-js").starts_with("HTTP/1.1 200"), "{context}");
    assert!(
        value("wrong-host").starts_with("HTTP/1.1 404"),
        "a request under another Host was served: {}. {context}",
        value("wrong-host")
    );
    assert!(
        value("wrong-token").starts_with("HTTP/1.1 404"),
        "a guessed API path reached the core: {}. {context}",
        value("wrong-token")
    );
    assert!(
        value("start").starts_with("HTTP/1.1 204"),
        "the page's offer was not taken: {}. {context}",
        value("start")
    );
    assert!(
        value("start-again").starts_with("HTTP/1.1 409"),
        "a second offer from the same page was taken: {}. {context}",
        value("start-again")
    );

    let answered = value("answered");
    assert!(
        answered.starts_with("connecting||") && !answered.contains("|0|"),
        "the call did not move on when the other end answered, or never \
         learnt its message: {answered}. {context}"
    );
    assert!(
        value("row-ringing").starts_with("Call|"),
        "the call is not a call's row in its chat: {}. {context}",
        value("row-ringing")
    );
    assert_eq!(
        value("page-muted"),
        "mute",
        "a microphone switched off before the page was up was not switched \
         off on it once it was. {context}"
    );
    assert_eq!(
        value("answer"),
        "v=0 fake-answer",
        "the other end's answer did not reach the page as sent. {context}"
    );
    assert_eq!(value("unmute"), "false", "{context}");
    assert_eq!(
        value("page-unmuted"),
        "[\"unmute\"]",
        "the microphone switched back on was not passed to the page. {context}"
    );
    assert!(value("connected").starts_with("HTTP/1.1 204"), "{context}");
    assert!(
        value("in-call").starts_with("connected||") && value("in-call").ends_with("|clock"),
        "a connection the page reported did not make the call connected, \
         with a clock: {}. {context}",
        value("in-call")
    );
    assert!(
        value("hang-up").starts_with("ended|"),
        "hanging up did not end the call: {}. {context}",
        value("hang-up")
    );
    assert_eq!(
        value("hang-up"),
        "ended|",
        "the page was left up after the call ended. {context}"
    );
    assert!(
        value("after").starts_with("ended|hung-up|"),
        "the call does not say it was hung up here: {}. {context}",
        value("after")
    );
    assert_eq!(
        value("served-after"),
        "no",
        "the page is still being served after the call ended. {context}"
    );
    assert!(
        value("row-over").starts_with("Call|Completed|"),
        "the call's row did not follow the call to its end: {}. {context}",
        value("row-over")
    );
    assert_eq!(
        value("said"),
        "ended:hung-up;",
        "the call said something other than that it ended. {context}"
    );
    assert_eq!(
        value("reset"),
        "",
        "an ended call was not let go. {context}"
    );

    let calls = common::calls(&journal);
    let named = |method: &str| -> Vec<serde_json::Value> {
        calls
            .iter()
            .filter(|(name, _)| name == method)
            .map(|(_, params)| params.clone())
            .collect()
    };
    assert_eq!(
        named("place_outgoing_call"),
        vec![json!([1, 1, "v=0 test-offer answer-me", false])],
        "the core was not asked to place exactly the one call, audio only, \
         with the page's offer. {context}"
    );
    let message_id: u32 = value("after")
        .split('|')
        .nth(2)
        .and_then(|id| id.parse().ok())
        .unwrap_or_default();
    assert_eq!(
        named("end_call"),
        vec![json!([1, message_id])],
        "the core was not told once that the call was hung up. {context}"
    );
    assert_eq!(
        named("ice_servers"),
        vec![json!([1])],
        "the page was not given the core's ICE servers. {context}"
    );
}
