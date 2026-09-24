//! A webxdc app in the chat's realtime channel.
//!
//! Games played together send their moves over `joinRealtimeChannel`
//! rather than as status updates, and an app that finds no such call does
//! not start at all. The channel is the core's; what is the host's is the
//! way there and back, which is driven here over a real socket against
//! the fake core: joining, sending, the others' data arriving as a core
//! event and reaching the app's waiting request, and leaving -- by the
//! app, and by the page closing with the app still in.
//!
//! The fake core's peer answers every piece of data with the same bytes
//! backwards, so what comes back is proof of the round trip through the
//! core rather than the host handing the app its own bytes.

// Qt harness: see qml_pages.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        WebxdcApp {
            id: app
            account_id: 1
            message_id: 5
        }
        function open() {
            if (core.status === 'ready') {
                app.reload()
                app.start()
            }
        }
        Component.onCompleted: open()
        Connections {
            target: core
            onStatus_changed: open()
        }
        function url() { return app.url }
        function stop() { app.stop(); return app.url }
    }
";

/// One request, its whole answer. `Err` is the client's own failure.
fn ask(authority: &str, head: &str, body: &[u8]) -> Result<String, String> {
    let mut stream =
        std::net::TcpStream::connect(authority).map_err(|err| format!("connect: {err}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|err| err.to_string())?;
    stream
        .write_all(head.as_bytes())
        .map_err(|err| format!("write: {err}"))?;
    stream
        .write_all(body)
        .map_err(|err| format!("write-body: {err}"))?;
    let mut answer = Vec::new();
    stream
        .read_to_end(&mut answer)
        .map_err(|err| format!("read: {err}"))?;
    Ok(String::from_utf8_lossy(&answer).into_owned())
}

fn get(authority: &str, path: &str) -> String {
    ask(
        authority,
        &format!("GET {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n"),
        &[],
    )
    .unwrap_or_else(|err| err)
}

/// A POST the way the bridge sends one: the bytes as they are.
fn post(authority: &str, path: &str, body: &[u8]) -> String {
    ask(
        authority,
        &format!(
            "POST {path} HTTP/1.1\r\nHost: {authority}\r\n\
             Content-Type: application/octet-stream\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n",
            body.len()
        ),
        body,
    )
    .unwrap_or_else(|err| err)
}

/// Ask for realtime data on a thread of its own, since the host holds the
/// request until there is some. The answer and how long it took arrive
/// on the channel.
fn receive(authority: &str, api: &str) -> mpsc::Receiver<(String, Duration)> {
    let (tx, rx) = mpsc::channel();
    let authority = authority.to_string();
    let path = format!("{api}/realtime/receive");
    std::thread::spawn(move || {
        let started = Instant::now();
        let answer = get(&authority, &path);
        let _ = tx.send((answer, started.elapsed()));
    });
    rx
}

/// What a waiting request answered, or why there was nothing to read.
fn answered(rx: &mpsc::Receiver<(String, Duration)>) -> (String, Duration) {
    rx.recv_timeout(Duration::from_secs(10))
        .unwrap_or_else(|_| ("no answer within ten seconds".to_string(), Duration::MAX))
}

fn body_of(answer: &str) -> &str {
    answer.split_once("\r\n\r\n").map_or("", |(_, body)| body)
}

fn authority_of(url: &str) -> String {
    let rest = url.strip_prefix("http://").unwrap_or(url);
    rest.split('/').next().unwrap_or_default().to_string()
}

fn api_of(bridge: &str) -> String {
    bridge
        .split("var API = \"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_default()
        .to_string()
}

#[test]
#[allow(clippy::too_many_lines)]
fn an_app_joins_the_realtime_channel_hears_the_others_and_leaves() {
    let temp = std::env::temp_dir().join(format!("piirit-webxdc-rt-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let cache = temp.join("cache");
    std::fs::create_dir_all(&cache).expect("create cache dir");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("XDG_CACHE_HOME", &cache);
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
    // thread, and `engine` outlives it.
    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    // Everything below blocks the Qt thread while it waits, which is
    // part of what is checked: realtime data reaches the app without
    // passing through Qt at all.
    single_shot(Duration::from_secs(3), move || unsafe {
        let url = call!("url");
        let authority = authority_of(&url);
        let api = api_of(&get(&authority, "/webxdc.js"));
        record!("api", api.clone());
        if api.is_empty() {
            return;
        }
        let pause = || std::thread::sleep(Duration::from_millis(300));

        record!(
            "join",
            post(&authority, &format!("{api}/realtime/join"), b"")
        );

        // Waiting before anything is sent: the data has to wake a request
        // that is already asleep, not just be there for the next one.
        let first = receive(&authority, &api);
        pause();
        record!(
            "send",
            post(&authority, &format!("{api}/realtime/send"), &[1, 2, 3])
        );
        record!("received", answered(&first).0);

        // Past the specification's limit: refused here, never sent.
        record!(
            "too-big",
            post(
                &authority,
                &format!("{api}/realtime/send"),
                &vec![0; 128_001]
            )
        );

        // Leaving answers a request that was waiting on the channel, at
        // once rather than when its wait runs out.
        let second = receive(&authority, &api);
        pause();
        record!(
            "leave",
            post(&authority, &format!("{api}/realtime/leave"), b"")
        );
        let (left, waited) = answered(&second);
        record!("receive-left", left);
        record!(
            "receive-left-quickly",
            (waited < Duration::from_secs(5)).to_string()
        );
        // And a channel the app is not in takes nothing to send.
        record!(
            "send-after-leave",
            post(&authority, &format!("{api}/realtime/send"), &[9])
        );

        // In again, and then the page closes with the app still in: the
        // request waiting answers, and the core is told the app has gone.
        record!(
            "rejoin",
            post(&authority, &format!("{api}/realtime/join"), b"")
        );
        let third = receive(&authority, &api);
        pause();
        record!("stopped-url", call!("stop"));
        let (closed, waited) = answered(&third);
        record!("receive-closed", closed);
        record!(
            "receive-closed-quickly",
            (waited < Duration::from_secs(5)).to_string()
        );
    });

    // Leaving on close is spawned as the host goes; give it its turn.
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

    assert!(
        value("api").starts_with("/webxdc-api/"),
        "the app was never served. {context}"
    );
    assert!(
        value("join").starts_with("HTTP/1.1 204"),
        "the app could not join the realtime channel: {}. {context}",
        value("join")
    );
    assert!(
        value("send").starts_with("HTTP/1.1 204"),
        "the app could not send realtime data: {}. {context}",
        value("send")
    );
    let received = value("received");
    assert!(
        received.starts_with("HTTP/1.1 200"),
        "the waiting request was not answered: {received}. {context}"
    );
    assert_eq!(
        body_of(&received),
        "[[3,2,1]]",
        "what the peer sent back did not reach the app as it was sent. \
         {context}"
    );
    assert!(
        value("too-big").starts_with("HTTP/1.1 413"),
        "data past 128000 bytes was taken: {}. {context}",
        value("too-big")
    );
    assert!(
        value("leave").starts_with("HTTP/1.1 204"),
        "the app could not leave the channel: {}. {context}",
        value("leave")
    );
    assert_eq!(
        (
            body_of(&value("receive-left")),
            value("receive-left-quickly").as_str()
        ),
        ("[]", "true"),
        "a request waiting on the channel was not answered when the app \
         left it. {context}"
    );
    assert!(
        value("send-after-leave").starts_with("HTTP/1.1 409"),
        "a channel the app had left still took data: {}. {context}",
        value("send-after-leave")
    );
    assert!(
        value("rejoin").starts_with("HTTP/1.1 204"),
        "the app could not join again after leaving: {}. {context}",
        value("rejoin")
    );
    assert_eq!(
        value("stopped-url"),
        "",
        "the app was not stopped. {context}"
    );
    assert_eq!(
        (
            body_of(&value("receive-closed")),
            value("receive-closed-quickly").as_str()
        ),
        ("[]", "true"),
        "a request waiting on the channel held on after the app was \
         closed. {context}"
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
        named("send_webxdc_realtime_advertisement"),
        vec![serde_json::json!([1, 5]), serde_json::json!([1, 5])],
        "the chat was not told each time the app joined. {context}"
    );
    // One: the data too big to send, and the data sent after leaving,
    // never reached the core.
    assert_eq!(
        named("send_webxdc_realtime_data"),
        vec![serde_json::json!([1, 5, [1, 2, 3]])],
        "the core was not handed exactly the one piece of data the app \
         was allowed to send. {context}"
    );
    // Twice: once when the app left, once when the page closed with the
    // app back in. A page closing with an app in the channel and not
    // telling the core leaves the phone in it.
    assert_eq!(
        named("leave_webxdc_realtime"),
        vec![serde_json::json!([1, 5]), serde_json::json!([1, 5])],
        "the core was not told each time the app left the channel. \
         {context}"
    );
}
